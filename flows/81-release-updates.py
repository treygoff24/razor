"""A real release installation updates through a local HTTP release, keeps its running server,
announces readiness once in a newly launched terminal, and starts current on restart.
Source installs, opt-out, older releases and damaged downloads never select a new version.
"""
from contextlib import contextmanager
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import hashlib
import json
import os
import platform
import shutil
import signal
import subprocess
import tarfile
import threading
import time

from driver import BINARY, flow
from terminal import Terminal
import server
import terminal

INSTALLER = Path(__file__).resolve().parents[1] / 'scripts/install-release.sh'
PLATFORM = {('Linux', 'x86_64'): 'linux-x64', ('Linux', 'aarch64'): 'linux-arm64',
            ('Darwin', 'arm64'): 'darwin-arm64'}[(platform.system(), platform.machine())]


@contextmanager
def releases(f):
    files = f.home / 'releases'
    files.mkdir()
    requests = []

    class Handler(SimpleHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_GET(self):
            requests.append(self.path)
            if self.path.endswith('.tar.gz') and publish.pause is not None:
                arrived, resume = publish.pause
                publish.pause = None
                arrived.set()
                resume.wait(60)
            try:
                super().do_GET()
            except (BrokenPipeError, ConnectionResetError):
                pass  # The cancellation check closes a pending download.

    http = ThreadingHTTPServer(('127.0.0.1', 0), partial(Handler, directory=str(files)))
    thread = threading.Thread(target=http.serve_forever, daemon=True)
    thread.start()
    base = f'http://127.0.0.1:{http.server_port}'
    f.env['RAZOR_RELEASE_URL'] = base

    def publish(tag, *, damaged=False):
        archive = files / f'razor-{tag}-{PLATFORM}.tar.gz'
        # Actual binary and real npm installation, with no third-party package downloads.
        bundle = f.home / 'bundle'
        (bundle / 'bridge').mkdir(parents=True, exist_ok=True)
        shutil.copy2(BINARY, bundle / 'razor')
        dependency = bundle / 'bridge/dependency'
        dependency.mkdir(exist_ok=True)
        (dependency / 'package.json').write_text('{"name":"release-dependency","version":"1.0.0"}')
        (bundle / 'bridge/package.json').write_text(json.dumps({
            'name': 'release-flow', 'version': '1.0.0',
            'dependencies': {'release-dependency': 'file:dependency'}}))
        (bundle / 'bridge/package-lock.json').write_text(json.dumps({
            'name': 'release-flow', 'version': '1.0.0', 'lockfileVersion': 3,
            'packages': {'': {'name': 'release-flow', 'version': '1.0.0',
                              'dependencies': {'release-dependency': 'file:dependency'}},
                         'dependency': {'name': 'release-dependency', 'version': '1.0.0'},
                         'node_modules/release-dependency': {'resolved': 'dependency', 'link': True}}}))
        with tarfile.open(archive, 'w:gz', compresslevel=1) as tar:
            tar.add(bundle / 'razor', arcname='razor')
            tar.add(bundle / 'bridge', arcname='bridge')
        digest = '0' * 64 if damaged else hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest()
        archive.with_suffix('.gz.sha256').write_text(f'{digest}  {archive.name}\n')
        (files / 'install.sh').write_text(
            '#!/bin/sh\n' + f'RAZOR_BASE_URL=${{RAZOR_BASE_URL:-{base}}}\n'
            + f'RAZOR_VERSION=${{RAZOR_VERSION:-{tag}}}\n' + INSTALLER.read_text())

    publish.pause = None
    try:
        yield publish, requests
    finally:
        http.shutdown()
        http.server_close()
        thread.join()


def installed(f, tag='0.1.0'):
    root = f.home / '.local/share/razor'
    bundle = root / tag
    bundle.mkdir(parents=True)
    shutil.copy2(BINARY, bundle / 'razor')
    (root / 'current').symlink_to(tag)
    link = f.home / '.local/bin/razor'
    link.parent.mkdir(parents=True)
    link.symlink_to(root / 'current/razor')
    return root, link


def run(f, binary, *args):
    return subprocess.run([str(binary), *args], env=f.env, cwd=f.cwd,
                          capture_output=True, text=True, timeout=60)


def start(f, binary):
    log = (f.home / 'release-server.log').open('a')
    child = subprocess.Popen([str(binary), 'serve'], env=f.env, cwd=f.cwd,
                             stdout=log, stderr=log, start_new_session=True)
    log.close()
    server.wait(lambda: (f.home / '.razor/server.json').exists(), why='release server did not start')
    return child


def config(f, enabled):
    path = f.home / '.razor/config.toml'
    path.write_text(f'release_updates = {str(enabled).lower()}\n' + path.read_text())


with flow([], model='gpt-6.1-sol') as f, releases(f) as (publish, requests):
    root, command = installed(f)
    old = root / '0.1.0/razor'
    publish('0.2.0')
    (root / '0.1.1').mkdir()
    (root / '0.1.1/razor').symlink_to(BINARY)
    (root / 'current').unlink()
    (root / 'current').symlink_to('0.1.1')
    # Make the running version old enough that only executable retention protects it.
    os.utime(old.parent, (time.time() - 3 * 86400,) * 2)
    running = start(f, old)
    try:
        server.wait(lambda: (root / 'current').readlink() == Path('0.2.0'), timeout=60,
                    why='daily/start check did not install and select the newer release')
        assert requests.count('/install.sh') == 1, requests
        assert (root / '0.2.0/bridge/node_modules/release-dependency/package.json').exists(), 'npm did not install the bridge'
        assert running.poll() is None and old.exists(), 'update disturbed the running server'
        assert json.loads((f.home / '.razor/server.json').read_text())['pid'] == running.pid
        status = json.loads(server.send(f, 'GET', '/status').readline())
        assert status['server']['release_ready'] == '0.2.0', status
        # Launch through newly selected current while the server still runs the previous folder.
        original = terminal.BINARY
        terminal.BINARY = str(command)
        try:
            with Terminal(f) as t:
                t.shown('RAZOR 0.2.0 IS READY ON', 'RUN RAZOR RESTART TO USE IT')
                assert 'invalid stream line' not in t.text(), t.text()
                # A heartbeat after the transient notice expires must not show it again.
                t.until(lambda t: 'RAZOR 0.2.0 IS READY' not in t.text(), timeout=8)
                t.pump(16)
                assert 'RAZOR 0.2.0 IS READY' not in t.text(), 'heartbeat repeated the notice'
        finally:
            terminal.BINARY = original
        cached = run(f, command, 'update')
        assert cached.returncode == 0 and '0.2.0, the latest release, is installed' in cached.stdout, cached
        assert 'run razor restart if your server started before it' in cached.stdout, cached
        assert requests.count('/install.sh') == 2, requests
        assert requests.count(f'/razor-0.2.0-{PLATFORM}.tar.gz') == 1, requests
        restart = run(f, command, 'restart')
        assert restart.returncode == 0, restart.stderr
        running.wait(timeout=30)
        assert running.returncode == 0, running.returncode
        if platform.system() == 'Linux':
            pid = json.loads((f.home / '.razor/server.json').read_text())['pid']
            assert Path(f'/proc/{pid}/exe').resolve() == (root / '0.2.0/razor').resolve()
        status = json.loads(server.send(f, 'GET', '/status').readline())
        assert 'release_ready' not in status['server'], status
    finally:
        if running.poll() is None:
            running.terminate()
            running.wait(timeout=30)


with flow([], model='gpt-6.1-sol') as f, releases(f) as (publish, requests):
    root, command = installed(f)
    old = root / '0.1.0/razor'
    config(f, False)
    publish('0.2.0', damaged=True)
    running = start(f, old)
    try:
        assert '/install.sh' not in requests, 'opted-out server checked releases at start'
        failed = run(f, command, 'update')  # Manual update works despite background opt-out.
        assert failed.returncode != 0 and 'checksum mismatch' in failed.stderr, failed
        assert (root / 'current').readlink() == Path('0.1.0') and old.exists()
        publish('0.2.0')
        arrived, resume = threading.Event(), threading.Event()
        publish.pause = (arrived, resume)
        cancelled = subprocess.Popen([str(command), 'update'], env=f.env, cwd=f.cwd,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                     start_new_session=True)
        try:
            assert arrived.wait(30), 'manual update did not begin its archive download'
            cancelled.send_signal(signal.SIGINT)
            # EOF proves the installer and curl also closed their inherited output pipes.
            out, err = cancelled.communicate(timeout=10)
            assert cancelled.returncode == 130, (cancelled.returncode, out, err)
            assert (root / 'current').readlink() == Path('0.1.0')
            assert not (root / '0.2.0').exists(), 'cancelled update installed a release'
            # A retry must get the lock even while the first HTTP request remains blocked.
            manual = run(f, command, 'update')
            assert manual.returncode == 0 and '0.2.0 is ready' in manual.stdout, manual
            assert (manual.stdout + manual.stderr).lower().count('run razor restart') == 1, manual
        finally:
            resume.set()
            if cancelled.poll() is None:
                cancelled.kill()
                cancelled.communicate(timeout=10)
        assert running.poll() is None
        publish('0.1.0')
        older = run(f, command, 'update')
        assert older.returncode == 0 and (root / 'current').readlink() == Path('0.2.0'), older
        # The person's own update runs under provider_only, which stops only Razor's own reach.
        (f.home / '.razor/config.toml').write_text('release_updates = false\nprovider_only = true\nmodel = "p81/m"\n[providers.p81]\nbase_url = "http://127.0.0.1:9"\nmodels = ["m"]\n')
        # Installer reruns select a cached newer folder without a second archive request.
        (root / 'current').unlink()
        (root / 'current').symlink_to('0.1.0')
        publish('0.2.0')
        before = requests.count(f'/razor-0.2.0-{PLATFORM}.tar.gz')
        cached = run(f, command, 'update')
        assert cached.returncode == 0 and (root / 'current').readlink() == Path('0.2.0'), cached
        assert requests.count(f'/razor-0.2.0-{PLATFORM}.tar.gz') == before
    finally:
        running.terminate()
        running.wait(timeout=30)


with flow([], model='gpt-6.1-sol') as f, releases(f) as (publish, requests):
    publish('0.2.0')
    server.start(f)  # Cargo build outside the data root: no network, no auto-update.
    refused = f.run('update')
    assert refused.returncode != 0 and 'source installation' in refused.stderr, refused
    root, source = installed(f, '123456789abc')  # scripts/install.sh naming.
    refused = run(f, source, 'update')
    assert refused.returncode != 0 and 'source installation' in refused.stderr, refused
    assert '/install.sh' not in requests, requests
