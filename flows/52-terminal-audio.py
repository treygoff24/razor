"""A light client's own audio programs beside a paired server's, and notes' and typed inputs'
transport faults through a proxy to the real server, which owns transcription and admission.
"""
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
capture, play, transcribe, hold, heard = [scratch / n for n in ('capture', 'play', 'transcribe', 'hold', 'heard')]
capture.write_text("""import os, struct, time
while True:
    os.write(1, struct.pack('<h', 1) * 1600)
    time.sleep(.1)
""")
play.write_text(f"""import sys
from pathlib import Path
with open('{heard}', 'a') as f: f.write(Path(sys.argv[-1]).read_bytes().hex() + '\\n')
""")
transcribe.write_text(f"""cat >/dev/null
while test -f {hold}; do sleep .05; done
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
                    if len(proxy.inputs) == 1:  # A note's 502; a typed input's success naming no entry.
                        self.request.sendall(b'HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n' if proxy.inputs[0]['voice']
                                             else b'HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}')
                        return
                    if len(proxy.inputs) == 2:
                        # The real server admits it, or starts its program; the client loses the answer.
                        with socket.socket(socket.AF_UNIX) as upstream:
                            upstream.connect(str(proxy.upstream))
                            upstream.sendall(headers + b'\r\n\r\n' + body)
                            self.request.close()
                            while upstream.recv(65536): pass
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


# A light client: [remote] names a paired server, so new sessions and replies are that server's,
# while its own play and capture run here and new sessions start in the launch folder.
there = config({k: commands[k] for k in ('speech', 'transcribe')})
with flow([reply(text='printed remotely'), reply(text='heard here'), reply(text='launch folder')], config=there) as f:
    server.start(f)
    light, elsewhere = f.home / '.light', f.home / 'elsewhere'
    light.mkdir(), elsewhere.mkdir()
    here = config({k: commands[k] for k in ('capture', 'play')})
    network = f'[network]\nrelays = ["{server.RELAY}"]'
    mac = replace(f, cwd=elsewhere, env={**f.env, 'RAZOR_HOME': str(light)})
    (light / 'config.toml').write_text(f'{here}\n[server]\nname = "lightmac"\n[remote]\nserver = "main"\n{network}\n')
    bad = mac.run('--print', 'not sent')
    assert bad.returncode != 0 and 'That server is not paired; enroll it before addressing its sessions.' in bad.stderr, bad
    server.pair(f, mac, 'mac', 'main')
    printed = json.loads(mac.run('--print=json', 'from the mac').stdout)
    assert printed['message'] == 'printed remotely' and list(f.record(printed['session'].removesuffix('@main'))), printed
    with Terminal(mac, '--continue') as t:
        t.shown('printed remotely')
        t.send('/mcp import ~/missing.json\r')
        t.shown('USE AN ABSOLUTE PATH ON THE SERVER')
        t.send('\x15')  # clear the refused draft
        t.send('hello\r')
        t.shown('heard here', '▶')
        t.send('/listen\r')
        t.until(lambda _: heard.exists() and bytes.fromhex(heard.read_text().split()[-1]) == b'heard here')
        t.shown('Voice  ')  # the toolbar offers it again, once the paired server's status has been read
        t.send('\x12')
        t.shown('Recording on lightmac')
        t.send('\x1b')
        t.shown('Esc discards')
        t.send('\x1b')
        t.until(lambda t: 'Esc discards' not in t.text())
    with Terminal(mac, '--resume', printed['session']) as t:
        t.shown('heard here')
    at_server_folder = replace(mac, cwd=f.cwd)
    fallback = json.loads(at_server_folder.run('--print=json', 'folder fallback').stdout)
    assert fallback['message'] == 'launch folder' and list(f.record(fallback['session'].removesuffix('@main'))), fallback
    assert f.run('devices', 'remove', 'mac').returncode == 0
    refused = mac.run('--print', 'unpaired')
    assert refused.returncode != 0, refused

# A dropped/paired-style answer re-asks one id/file, while drafts and quit stay responsive.
hold.touch()
# Ctrl-C quits with words in the draft, which it would empty were the clear key bound.
with flow([reply(text='late note admitted'), reply(text='typed admitted'), reply(text='held admitted')], config=config() + '\n[ui.keys.composer]\nclear = []\n') as f:
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
        # A typed input whose first answer names no entry and whose second is lost stays out of
        # the editor under its request id; the empty editor's Send button, then Enter behind words
        # typed since, send that request again, and the server admits it once.
        proxy.inputs.clear()
        with Terminal(f, '--resume', session) as t:
            t.shown('before latest')
            t.send('\x01\x7ftyped once\r')
            t.shown('answer is invalid', 'Enter tries again')
            t.click('Send ⏎')
            t.shown('request failed', 'Enter tries again')
            t.send('later\r')
            t.shown('typed admitted', '❯ later')
            # The admission cleared the draft; the words typed since are saved as any edit is.
            saved = lambda: server.send(f, 'GET', f'/sessions/{session}/draft').read()
            server.wait(lambda: saved() == b'later', why=t.text())
        assert proxy.inputs[0]['request'] and proxy.inputs == proxy.inputs[:1] * 3, proxy.inputs
        assert [e['text'] for e in f.record(session) if e['kind'] == 'input'] == ['words-from-the-note', 'typed once']
        # Quitting while a send is held says where its words go, and puts them in the draft ahead
        # of those typed since, though the server took them and cleared the draft.
        proxy.inputs.clear()
        with Terminal(f, '--resume', session) as t:
            t.shown('❯ later')
            t.send('\x01\x7fheld twice\r')
            t.shown('answer is invalid', 'Enter tries again')
            t.click('Send ⏎')
            t.shown('request failed', 'Enter tries again')
            server.until(f, session, lambda es: any(e['kind'] == 'input' and e['text'] == 'held twice' for e in es))
            t.send('after')
            t.quit('Quitting puts the unanswered words in the draft.')
        assert saved() == b'held twice\nafter' and proxy.inputs == proxy.inputs[:1] * 2, (saved(), proxy.inputs)
    finally:
        proxy.shutdown()
        proxy.server_close()
