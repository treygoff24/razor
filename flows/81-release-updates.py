"""A real release installation updates through a local HTTP release, keeps its running server,
announces readiness once in a newly launched terminal, and starts current on restart.
A release marked as needing action is held, unless this server already meets its mark.
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
import sys
import tarfile
import tempfile
import threading
import time

from driver import BINARY, flow
from terminal import Terminal
import server
import terminal

INSTALLER = Path(__file__).resolve().parents[1] / 'scripts/install-release.sh'
RELEASE = INSTALLER.with_name('release.sh')
# release.sh's own line that writes install.sh, its action mark included.
WRITE = next(line for line in RELEASE.read_text().splitlines() if line.startswith('{ head -n 2 scripts/install-release.sh'))
# Beside the flows' scratch folders, so a release links its archive rather than copying it.
ARCHIVES = tempfile.TemporaryDirectory(prefix='razor-flow-81-')
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

    def publish(tag, *, damaged=False, mark=''):
        archive = files / f'razor-{tag}-{PLATFORM}.tar.gz'
        archive.unlink(missing_ok=True)
        os.link(bundled(), archive)
        digest = '0' * 64 if damaged else hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest()
        archive.with_suffix('.gz.sha256').write_text(f'{digest}  {archive.name}\n')
        subprocess.run(['bash', '-c', WRITE], cwd=RELEASE.parents[1], check=True, env={
            'PATH': os.environ['PATH'], 'base': base, 'version': tag, 'out': str(files), 'RAZOR_ACTION': mark})

    publish.pause = None
    try:
        yield publish, requests
    finally:
        http.shutdown()
        http.server_close()
        thread.join()


def bundled():
    """The release archive, built once a run: its contents do not depend on the tag."""
    archive = Path(ARCHIVES.name) / 'razor.tar.gz'
    if archive.exists():
        return archive
    # Actual binary and real npm installation, with no third-party package downloads.
    bundle = Path(ARCHIVES.name) / 'bundle'
    dependency = bundle / 'bridge/dependency'
    dependency.mkdir(parents=True)
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
        tar.add(BINARY, arcname='razor')
        tar.add(bundle / 'bridge', arcname='bridge')
    return archive


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


def notes(f):
    """Each notification's text, then which sender variables it had."""
    log = f.home / 'notify.py.log'
    return [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []


def held(f, root, requests, seen, checks):
    """The action status shows once the server's `checks`th release check ends, nothing downloaded
    since request `seen` and 0.2.0 still selected and running."""
    log = f.home / '.razor/server.log'
    server.wait(lambda: log.read_text().count('razor: Razor ') >= checks, why='release check did not end')
    assert not any(r.endswith('.tar.gz') for r in requests[seen:]), requests
    assert (root / 'current').readlink() == Path('0.2.0')
    if platform.system() == 'Linux':
        pid = json.loads((f.home / '.razor/server.json').read_text())['pid']
        assert Path(f'/proc/{pid}/exe').resolve() == (root / '0.2.0/razor').resolve()
    status = json.loads(server.send(f, 'GET', '/status').readline())['server']
    assert 'release_ready' not in status, status
    return status['release_action']


def config(f, enabled):
    path = f.home / '.razor/config.toml'
    path.write_text(f'release_updates = {str(enabled).lower()}\n' + path.read_text())


with flow([], model='gpt-6.1-sol') as f, releases(f) as (publish, requests):
    root, command = installed(f)
    old = root / '0.1.0/razor'
    script = f.home / 'notify.py'
    script.write_text('import json, os, sys\nwith open(sys.argv[0] + ".log", "a") as log:\n    print(json.dumps([sys.argv[1], *(k for k in ("RAZOR_SESSION", "RAZOR_NAME", "RAZOR_PROJECT", "RAZOR_ASK") if k in os.environ)]), file=log)\n')
    settings = f.home / '.razor/config.toml'
    settings.write_text(f'notify = {json.dumps([sys.executable, str(script)])}\n' + settings.read_text())
    publish('0.2.0', mark='0.1.1 Selected already meets this.')  # Not newer than the selected 0.1.1.
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
                # A views change sends the server line a heartbeat sends, at once.
                views = server.send(f, 'PUT', '/views', json.load(server.send(f, 'GET', '/views')))
                assert views.status == 200, views.read()
                t.quiet()
                assert 'RAZOR 0.2.0 IS READY' not in t.text(), 'heartbeat repeated the notice'
        finally:
            terminal.BINARY = original
        (root / 'release-action').write_text('broken Stale action.')
        cached = run(f, command, 'update')
        assert not (root / 'release-action').exists(), 'latest-installed check kept a malformed action'
        assert cached.returncode == 0 and '0.2.0, the latest release, is installed' in cached.stdout, cached
        assert 'run razor restart if your server started before it' in cached.stdout, cached
        assert requests.count('/install.sh') == 2, requests
        assert requests.count(f'/razor-0.2.0-{PLATFORM}.tar.gz') == 1, requests
        # 0.2.0 predates this release's mark: each check holds it, and the person is told once.
        first, broken, second = 'Pair every device again.', 'broken Pair by hand.', 'Sign in again.'
        publish('0.3.0', mark=f'0.3.0 {first}')
        seen = len(requests)
        restart = run(f, command, 'restart')
        assert restart.returncode == 0, restart.stderr
        running.wait(timeout=30)
        assert running.returncode == 0, running.returncode
        assert first in held(f, root, requests, seen, 1)
        server.wait(lambda: notes(f), why='the held release was not notified')
        assert len(notes(f)) == 1 and first in notes(f)[0][0] and notes(f)[0][1:] == [], notes(f)
        doctor = subprocess.run([str(command), 'doctor'], env={**f.env, 'RAZOR_RELEASE_URL': 'http://127.0.0.1:9'},
                                cwd=f.cwd, capture_output=True, text=True, timeout=60)  # The release server unreachable.
        assert first in doctor.stdout, doctor
        # A malformed mark holds too, shown as written; its new sentence is told once more.
        publish('0.4.0', mark=broken)
        reader = (root / 'release-action').open()
        assert run(f, command, 'restart').returncode == 0
        raw = f'RAZOR_ACTION="{broken}"'
        assert raw in held(f, root, requests, seen, 2)
        with reader:
            assert reader.read() == f'0.3.0 {first}', 'action rewrite changed an in-progress read'
        server.wait(lambda: len(notes(f)) >= 2, why='the changed action was not notified')
        assert [raw in note[0] for note in notes(f)] == [False, True], notes(f)
        # The person's update selects the latest and says what to do; its download is already there,
        # as in the cached case below: the first check above proves installing.
        publish('0.4.0', mark=f'0.4.0 {second}')
        (root / '0.4.0').mkdir()
        (root / '0.4.0/razor').symlink_to(BINARY)
        updated = run(f, command, 'update')
        assert updated.returncode == 0 and second in updated.stdout, updated
        assert (root / 'current').readlink() == Path('0.4.0') and not (root / 'release-action').exists()
        status = json.loads(server.send(f, 'GET', '/status').readline())['server']
        assert status['release_ready'] == '0.4.0' and 'release_action' not in status, status
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
    # release.sh refuses a mark the updater would misread; a good one goes on to the build.
    for mark, refused in (('0.2.0 Say "hi".', True), ('0.2.0 Pay $5.', True), ('0.2.0 ' + 'x' * 201, True),
                          ('0.2.1 Too new.', True), ('0.2.0 ' + 'x' * 200, False)):
        made = subprocess.run([str(RELEASE), '0.2.0', str(f.home / 'out'), 'https://example.com'], capture_output=True, text=True,
                              env={'PATH': os.environ['PATH'], 'RAZOR_ACTION': mark, 'RAZOR_PLATFORM': 'none'}, timeout=30)
        assert made.returncode == 1 and ('Invalid RAZOR_ACTION' in made.stderr) == refused, (mark, made.stderr)
