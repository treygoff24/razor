"""Audio refusal/retry, live machine settings, SSH, a fast capture limit and quitting in flight.
Transport faults go through a proxy to the real server, which still owns transcription/admission.
"""
import base64
import json
import select
import socket
import socketserver
import tempfile
import threading
from dataclasses import replace
from pathlib import Path
from driver import flow
from terminal import Terminal
from claude import reply
import server

scratch = Path(tempfile.mkdtemp(prefix='razor-audio-edges-'))
capture, play, transcribe, hold, mode, played, heard, runs = [scratch / n for n in
    ('capture', 'play', 'transcribe', 'hold', 'mode', 'played', 'heard', 'runs')]
capture.write_text(f"""import os, struct, time
from pathlib import Path
for n in range(6000):
    mode = Path('{mode}').read_text()
    os.write(1, struct.pack('<h', 1 if mode == 'quiet' else [1, 5000, 30000][n % 3]) * 1600)
    if mode != 'fast': time.sleep(.1)
""")
play.write_text(f"""import json, os, sys, time
from pathlib import Path
with open('{played}', 'a') as f: f.write(json.dumps([os.getpid(), *sys.argv[1:]]) + '\\n')
with open('{heard}', 'a') as f: f.write(Path(sys.argv[-1]).read_bytes().hex() + '\\n')
if Path('{mode}').read_text() == 'bad-player': raise SystemExit(3)
time.sleep(.3)
""")
transcribe.write_text(f"""echo run >> {runs}
cat >/dev/null
while test -f {hold}; do sleep .05; done
if test "$(cat {mode})" = fail; then echo 'engine refused this recording.' >&2; exit 3; fi
echo words-from-the-note
""")
commands = {'capture': ['/usr/bin/python3', str(capture)],
    'play': ['/usr/bin/python3', str(play), '{rate}'], 'transcribe': ['/bin/sh', str(transcribe)],
    'speech': ['/bin/sh', '-c', 'cat']}


def config(commands=commands):
    return '\n'.join(f'{key} = {json.dumps(value)}' for key, value in commands.items())


class Proxy(socketserver.ThreadingUnixStreamServer):
    daemon_threads = True

    def __init__(self, f):
        self.upstream = f.home / '.razor/upstream.sock'
        path = f.home / '.razor/server.sock'
        path.rename(self.upstream)
        self.inputs, self.uploads = [], 0  # At most the four attempts this flow makes.
        proxy = self

        class Handler(socketserver.BaseRequestHandler):
            def handle(self):
                head = b''
                while b'\r\n\r\n' not in head:
                    part = self.request.recv(4096)
                    if not part: return
                    head += part
                    assert len(head) < 64 * 1024
                headers, body = head.split(b'\r\n\r\n', 1)
                # One request per connection keeps every retry visible to this proxy.
                headers = b'\r\n'.join(line for line in headers.split(b'\r\n')
                    if not line.lower().startswith(b'connection:')) + b'\r\nConnection: close'
                first = headers.split(b'\r\n', 1)[0]
                if first.startswith(b'POST') and b'/files/voice.wav ' in first:
                    proxy.uploads += 1
                if first.startswith(b'POST') and b'/input ' in first:
                    length = next(int(line.split(b':')[1]) for line in headers.split(b'\r\n')
                                  if line.lower().startswith(b'content-length:'))
                    while len(body) < length: body += self.request.recv(length - len(body))
                    proxy.inputs.append(json.loads(body))
                    assert len(proxy.inputs) <= 8
                    if len(proxy.inputs) == 1:
                        self.request.sendall(b'HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n')
                        return
                    if len(proxy.inputs) == 2:
                        # The real server starts its program; the client loses the answer.
                        with socket.socket(socket.AF_UNIX) as upstream:
                            upstream.connect(str(proxy.upstream))
                            upstream.sendall(headers + b'\r\n\r\n' + body)
                        return
                with socket.socket(socket.AF_UNIX) as upstream:
                    upstream.connect(str(proxy.upstream))
                    upstream.sendall(headers + b'\r\n\r\n' + body)
                    while True:
                        readable, _, _ = select.select([self.request, upstream], [], [], 1)
                        for source in readable:
                            try: data = source.recv(65536)
                            except OSError: return
                            if not data: return
                            try: (upstream if source is self.request else self.request).sendall(data)
                            except OSError: return

        super().__init__(str(path), Handler)
        self.thread = threading.Thread(target=self.serve_forever, daemon=True)
        self.thread.start()


mode.write_text('quiet')
failed_speech = dict(commands, speech=['/bin/sh', '-c', 'echo speech-broken >&2; exit 3'])
with flow([reply(text='speech source'), reply(text='typed reply')], config=config(failed_speech)) as f:
    with Terminal(f) as t:
        t.shown('Message the agent')
        t.send('hello\r')
        t.shown('speech source', '▶')
        session = next((f.home / '.razor/sessions').iterdir()).name
        t.send('/listen\r')
        t.shown('speech-broken', 'Enter tries again')
        before = len(t.output)
        t.send('\x1b[99;6u')  # Ctrl-Shift-C, as sent by a terminal supporting modified keys.
        t.until(lambda t: b'\x1b]52;c;' in t.output[before:])
        encoded = t.output[before:].split(b'\x1b]52;c;', 1)[1].split(b'\x07', 1)[0]
        assert b'speech-broken' in base64.b64decode(encoded)
        t.shown('COPIED')
        t.send('/not-a-command\r')
        t.shown('UNKNOWN COMMAND', 'speech-broken', 'Enter tries again')
        t.send('\x15')
        t.send('typed after error')
        t.shown('typed after error')
        before = len(t.output)
        t.send('\x1b[99;6u')
        t.until(lambda t: b'\x1b]52;c;' in t.output[before:])
        encoded = t.output[before:].split(b'\x1b]52;c;', 1)[1].split(b'\x07', 1)[0]
        assert base64.b64decode(encoded) == b'typed after error'
        t.send('\r')
        t.shown('typed reply')
        assert any(e.get('kind') == 'input' and e.get('text') == 'typed after error' for e in f.record(session))
        assert 'Enter tries again' not in t.text()

with flow([reply(text='ssh spoken reply')], config=config(), env={'SSH_CONNECTION': 'flow'}) as f:
    with Terminal(f) as t:
        t.shown('Message the agent')
        assert 'Voice' not in t.text()
        t.send('\x12')
        t.shown('RECORDING IS UNAVAILABLE OVER SSH')
        before = len(t.output)
        t.send('\x1b[99;6u')
        t.until(lambda t: b'\x1b]52;c;' in t.output[before:])
        encoded = t.output[before:].split(b'\x1b]52;c;', 1)[1].split(b'\x07', 1)[0]
        assert b'Recording is unavailable over SSH' in base64.b64decode(encoded)
        # Read-aloud names the machine that would play it and plays nothing; the reply's ▶ lights
        # once its turn has ended, which /listen needs.
        t.send('hello\r')
        t.shown('ssh spoken reply', '▶')
        t.send('/listen\r')
        t.shown(f'OVER SSH, AUDIO WOULD PLAY ON {socket.gethostname().upper()}, NOT YOUR MACHINE')
        assert not played.exists() and not list((f.home / 'tmp').glob('razor-audio-*'))

# A light client: [remote] names another server and the key it enrolled, so this home starts no
# server of its own, its own play and capture run, and new sessions start in [remote] folder.
there = config({k: commands[k] for k in ('speech', 'transcribe')}) + '\n[server]\naddress = "127.0.0.1:0"'
with flow([reply(text='printed remotely'), reply(text='heard here'), reply(text='launch folder')], config=there) as f:
    server.start(f)
    light, elsewhere = f.home / '.light', f.home / 'elsewhere'
    light.mkdir(), elsewhere.mkdir()
    (light / 'mac.key').write_text(f.run('devices', 'add', 'mac').stdout.splitlines()[1])
    remote = {'address': json.loads((f.home / '.razor/server.json').read_text())['address'], 'key': 'mac.key', 'folder': str(f.cwd)}
    here = config({k: commands[k] for k in ('capture', 'play')})
    (light / 'config.toml').write_text(f'{here}\n[server]\nname = "lightmac"\n[remote]\n{config(remote)}\n')
    mac = replace(f, cwd=elsewhere, env={**f.env, 'RAZOR_HOME': str(light)})
    remote['address'] += '/wrong'
    (light / 'config.toml').write_text(f'{here}\n[remote]\n{config(remote)}\n')
    bad = mac.run('--print', 'not sent')
    assert bad.returncode != 0 and 'without a path' in bad.stderr, bad
    remote['address'] = remote['address'].removesuffix('/wrong')
    (light / 'config.toml').write_text(f'{here}\n[server]\nname = "lightmac"\n[remote]\n{config(remote)}\n')
    printed = json.loads(mac.run('--print=json', 'from the mac').stdout)
    assert printed['message'] == 'printed remotely' and list(f.record(printed['session'])), printed
    with Terminal(mac, '--continue') as t:
        t.shown('printed remotely')
        t.send('/mcp import ~/missing.json\r')
        t.shown('USE AN ABSOLUTE PATH ON THE SERVER')
        t.send('\x15')  # clear the refused draft
        t.send('hello\r')
        t.shown('heard here', '▶')
        t.send('/listen\r')
        t.until(lambda _: heard.exists() and bytes.fromhex(heard.read_text().split()[-1]) == b'heard here')
        t.send('\x12')
        t.shown('Recording on lightmac')
        t.send('\x1b')
        t.shown('Esc discards')
        t.send('\x1b')
        t.until(lambda t: 'Esc discards' not in t.text())
    with Terminal(mac, '--resume', printed['session']) as t:
        t.shown('heard here')
    del remote['folder']
    (light / 'config.toml').write_text(f'{here}\n[server]\nname = "lightmac"\n[remote]\n{config(remote)}\n')
    at_server_folder = replace(mac, cwd=f.cwd)
    fallback = json.loads(at_server_folder.run('--print=json', 'folder fallback').stdout)
    assert fallback['message'] == 'launch folder' and list(f.record(fallback['session'])), fallback
    (light / 'mac.key').write_text('0' * 64)
    refused = mac.run('--print', 'wrong key')
    assert refused.returncode == 1 and 'needs a device key that works' in refused.stderr, refused
    assert not {'server.sock', 'server.json', 'server.log', 'server.lock'} & {p.name for p in light.iterdir()}
    heard.unlink()

# Quiet, nonzero PCM is valid; failed transcription keeps the same uploaded file for Enter.
with flow([reply(text=['first spoken reply', 'second spoken reply']), reply(text='uncached reply')], config=config()) as f:
    f.base()
    with Terminal(f, columns=40) as t:
        t.shown('Voice Ctrl-R')
        image = f.cwd / 'pixel.png'
        image.write_bytes(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aMk0AAAAASUVORK5CYII='))
        t.send('parked draft \x1b[200~' + str(image) + '\x1b[201~')
        t.shown('[Image #1]')
        t.send('\x12')
        t.shown('Recording on', '0:00')
        t.send('\x12')
        t.pump(.15)
        assert 'Recording on' in t.text()
        t.send('\x1b')
        t.shown('Esc discards', '[Image #1]', 'parked draft')
        mode.write_text('fail')
        t.send('\r')
        t.shown("Couldn't transcribe it:", 'refused this recording.', 'Enter tries again')
        assert "Couldn't transcribe it: engine refused this recording." in " ".join(t.text().split())
        session = next((f.home / '.razor/sessions').iterdir()).name
        files = list((f.home / '.razor/sessions' / session).rglob('voice.wav'))
        assert len(files) == 1, files
        mode.write_text('quiet')
        t.send('\r')
        t.shown('words-from-the-note', '[Image #1]')
        assert list((f.home / '.razor/sessions' / session).rglob('voice.wav')) == files
        assert sum(e['kind'] == 'input' for e in f.record(session)) == 1
        t.resize(40, 120)
        t.shown('└' + '─' * 118 + '┘', 'first spoken reply', 'second spoken reply')
        played.write_text('')
        t.send('\x15/listen\r')
        t.until(lambda _: len(played.read_text().splitlines()) == 2)
        t.until(lambda _: heard.exists() and len(heard.read_text().splitlines()) == 2)
        calls = [json.loads(line) for line in played.read_text().splitlines()]
        assert [bytes.fromhex(line) for line in heard.read_text().splitlines()] == [b'first spoken reply', b'second spoken reply']
        assert [call[1] for call in calls] == ['1.2', '1.2'], calls
        t.until(lambda t: 'Reading aloud' not in t.text())
        cfg = f.home / '.razor/config.toml'
        cfg.write_text(cfg.read_text().replace(json.dumps(commands['play']), json.dumps(commands['play'][:-1])))
        t.send('/listen\r')
        t.until(lambda _: len(played.read_text().splitlines()) >= 3)
        assert len(json.loads(played.read_text().splitlines()[2])) == 2  # pid and appended path, no rate.
        t.send('\x1b')
        t.until(lambda t: 'Reading aloud' not in t.text())
        mode.write_text('bad-player')
        t.send('/listen\r')
        t.shown('local player failed', 'Enter tries again')
        mode.write_text('quiet')
        t.send('\r')
        t.shown('Reading aloud')
        t.send('\x1b')
        t.until(lambda t: 'Reading aloud' not in t.text())
        cfg.write_text(cfg.read_text().replace(json.dumps(commands['speech']), '[]'))
        # A new text entry has no cached speech, so missing speech remains a visible refusal.
        server.post(f, f'/sessions/{session}/input', {'text': 'need new speech'})
        t.shown('uncached reply')
        t.send('/listen\r')
        t.shown('No speech program is configured', 'Enter tries again')
        cfg.write_text(cfg.read_text().replace('speech = []', 'speech = ["/bin/sh", "-c", "exit 3"]'))
        t.send('\r')
        t.shown("Couldn't read it aloud", 'Enter tries again')
        t.send('\x1b')
        t.until(lambda t: 'Enter tries again' not in t.text())

# The real server's four busy programs refuse a fifth; a live config change then refuses 503.
mode.write_text('quiet')
hold.touch()
runs.write_text('')
with flow([reply(text=f'busy note {n}') for n in range(5)], config=config()) as f:
    server.start(f)
    session = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
    threads, results, sessions = [], [], []
    for n in range(4):
        busy = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
        sessions.append(busy)
        path = json.loads(server.send(f, 'POST', f'/sessions/{busy}/files/voice.wav', b'audio').read())['path']
        body = {'text': '', 'request': f'busy-{n}', 'voice': {'file': path, 'duration_ms': 100, 'levels': [1]}}
        thread = threading.Thread(target=lambda body=body, busy=busy: results.append(server.post(f, f'/sessions/{busy}/input', body)))
        thread.start()
        threads.append(thread)
    try:
        with Terminal(f, '--resume', session) as t:
            t.shown('Voice')
            t.until(lambda _: len(runs.read_text().splitlines()) == 4)
            t.send('\x12')
            t.shown('Recording on')
            t.pump(.2)
            t.send('\r')
            t.shown('Four voice notes and replies are being processed', 'Enter tries again')
            cfg = f.home / '.razor/config.toml'
            original = cfg.read_text()
            cfg.write_text(original.replace(json.dumps(commands['transcribe']), '[]'))
            t.send('\r')
            t.shown('This server has no transcriber', 'Enter tries again')
            cfg.write_text(original)
            hold.unlink()
            for thread in threads: thread.join()
            assert len(results) == 4
            for busy in sessions:
                server.until(f, busy, lambda es: es[-1]['kind'] == 'turn_end')
            t.send('\r')
            t.shown('words-from-the-note')
            server.until(f, session, lambda es: sum(e['kind'] == 'turn_end' for e in es) >= 1)
            assert sum(e['kind'] == 'input' for e in f.record(session)) == 1
    finally:
        hold.unlink(missing_ok=True)
        for thread in threads: thread.join()

# Limit is measured PCM duration; a fast device can fill ten minutes without a ten-minute test.
mode.write_text('fast')
with flow([], config=config()) as f:
    with Terminal(f, columns=40) as t:
        t.shown('Voice Ctrl-R')
        t.send('\x12')
        t.shown('10:00', 'Esc discards')
        assert not list((f.home / '.razor/sessions').glob('*/record.jsonl'))
        t.send('\x03')
        t.shown('DISCARD', 'AUDIO.', 'Esc stays')
        t.send('\x1b')
        t.until(lambda t: 'PRESS CTRL-C' not in t.text())
    assert not list((f.home / 'tmp').glob('razor-audio-*'))

# A dropped/paired-style answer re-asks one id/file, while drafts and quit stay responsive.
mode.write_text('quiet')
hold.touch()
# Ctrl-C quits with words in the draft, which it would empty were the clear key bound.
with flow([reply(text='late note admitted')], config=config() + '\n[ui.keys.composer]\nclear = []\n') as f:
    server.start(f)
    session = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
    proxy = Proxy(f)
    try:
        with Terminal(f, '--resume', session) as t:
            t.shown('Voice')
            t.send('before\x12')
            t.shown('Recording on')
            t.send('\r')
            t.shown('Transcribing')
            t.send(' latest')
            draft = f.home / '.razor/sessions' / session / 'draft'
            t.until(lambda _: draft.exists() and draft.read_text() == 'before latest')
            t.until(lambda _: len(proxy.inputs) >= 3)
            assert proxy.uploads == 1
            assert len({json.dumps((i['request'], i['voice']), sort_keys=True) for i in proxy.inputs}) == 1
            t.send('\x1b')
            t.shown('THE VOICE NOTE MAY STILL ARRIVE')
            t.until(lambda t: 'Transcribing' not in t.text())
            t.until(lambda _: not list((f.home / 'tmp').glob('razor-audio-*/*')))
        hold.unlink()
        server.until(f, session, lambda es: any(e['kind'] == 'turn_end' for e in es))
        draft = server.send(f, 'GET', f'/sessions/{session}/draft')
        assert (draft.status, draft.read()) == (200, b'before latest')
        assert sum(e['kind'] == 'input' for e in f.record(session)) == 1
        assert not list((f.home / 'tmp').glob('razor-audio-*'))
    finally:
        proxy.shutdown()
        proxy.server_close()
