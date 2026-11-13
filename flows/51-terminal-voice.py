"""Real PCM through the terminal's recorder, server admission, saved rows and one player; audio
refusal and retry, live machine settings, SSH and a fast capture limit; answers kept addressed,
queued notes usable, and local programs reaped on quit and hangup, all in one home.
The audio programs stand in for devices, never for Razor code; audible playback is a live check.
"""
import base64
import json
import os
import signal
import socket
import tempfile
import threading
import time
import wave
from dataclasses import replace
from pathlib import Path
from driver import REPLIES, Reply, flow
from terminal import Terminal
import claude
from model import message, reply
import server

scratch = Path(tempfile.mkdtemp(prefix="razor-audio-flow-"))
capture, play, transcribe, mode, hold, played, heard, runs, release = [scratch / n for n in (
    'capture', 'play', 'transcribe', 'mode', 'hold', 'played', 'heard', 'runs', 'release')]
# One mode file drives every program: the recorder's sound, the player's length or failure, and
# the transcriber's refusal.
capture.write_text(f"""import os, struct, time
from pathlib import Path
Path('{scratch}/capture-pid').write_text(str(os.getpid()))
if Path('{mode}').read_text() == 'partial':
    os.write(1, struct.pack('<h', 5000) * 800)
    Path('{scratch}/partial').touch()
    time.sleep(60)
for n in range(6000):
    mode = Path('{mode}').read_text()
    if mode == 'none': time.sleep(10)
    if mode == 'exit': raise SystemExit(3)
    os.write(1, struct.pack('<h', {{'zero': 0, 'quiet': 1}}.get(mode, [1, 800, 8000, 30000][n % 4])) * 1600)
    if mode != 'fast': time.sleep(.1)
""")
play.write_text(f"""import json, os, sys, time
from pathlib import Path
with open('{played}', 'a') as f: f.write(json.dumps([os.getpid(), *sys.argv[1:]]) + '\\n')
with open('{heard}', 'a') as f: f.write(Path(sys.argv[-1]).read_bytes().hex() + '\\n')
mode = Path('{mode}').read_text()
if mode == 'bad-player': raise SystemExit(3)
time.sleep(.3 if mode == 'quiet' else 60)
""")
transcribe.write_text(f"""echo run >> {runs}
cat >/dev/null
while test -f {hold}; do sleep .05; done
if test "$(cat {mode})" = fail; then echo 'engine refused this recording.' >&2; exit 3; fi
echo spoken-words
""")
commands = {'capture': ['/usr/bin/python3', str(capture)], 'play': ['/usr/bin/python3', str(play), '{rate}'],
            'transcribe': ['/bin/sh', str(transcribe)], 'speech': ['/bin/sh', '-c', 'cat']}


def copied(t):
    """What Ctrl-Shift-C, as a terminal supporting modified keys sends it, copies through OSC 52."""
    before = len(t.output)
    t.send('\x1b[99;6u')
    t.until(lambda t: b'\x1b]52;c;' in t.output[before:])
    return base64.b64decode(t.output[before:].split(b'\x1b]52;c;', 1)[1].split(b'\x07', 1)[0])


def gone(pid):
    try: os.kill(pid, 0)
    except ProcessLookupError: return True
    return False


mode.write_text('sound')
played.write_text('')
replies = []
with flow(replies, config='\n'.join(f'{k} = {json.dumps(v)}' for k, v in commands.items())) as f:
    f.base()
    cfg = f.home / '.razor/config.toml'

    def setting(key, value=None):
        """Sets one of the audio programs in the live config, as a person editing it would; none
        puts back the flow's own."""
        kept = [line for line in cfg.read_text().splitlines() if not line.startswith(f'{key} = ')]
        cfg.write_text(f'{key} = {json.dumps(commands[key] if value is None else value)}\n' + '\n'.join(kept) + '\n')

    def sessions():
        return {p.parent.name for p in (f.home / '.razor/sessions').glob('*/record.jsonl')}

    # Voice leaves answers addressed, queued notes usable, and local programs reaped on quit/hangup.
    for key, why in [('capture', 'No local capture program is set'), ('transcribe', 'This server has no transcriber')]:
        setting(key, [])
        with Terminal(f) as t:
            t.shown('Message the agent')
            assert 'Voice' not in t.text()
            # Before the server's status arrives /record says so, and is asked again until it has.
            for _ in range(100):
                t.send('/record\r')
                t.until(lambda t: 'NOT ANSWERED YET' in t.text().upper() or why.upper() in t.text().upper())
                if why.upper() in t.text().upper(): break
                t.pump(.2)
            t.until(lambda t: why.upper() in t.text().upper())
        setting(key)

    # A live shell keeps a note queued. Its waveform still plays, and Take Back restores its words.
    root = 'VOICE_QUEUE_ROOT'
    replies += [reply(root, 'Bash', {'command': f'while ! test -f {release}; do sleep .05; done'}),
                reply(root, text='queue-root-done')]
    session = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
    other = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
    assert server.send(f, 'PUT', f'/sessions/{other}/name', 'other').status == 200
    server.post(f, f'/sessions/{session}/input', {'text': root})
    server.until(f, session, lambda es: any(e['kind'] == 'job' for e in es))
    with Terminal(f, '--resume', session, '--model', 'opus', columns=160, rows=80) as t:
        # Voice shows once the server's status says it can transcribe; Ctrl-R waits for it.
        t.shown('Esc stops', 'Voice')
        t.send('\x12')
        t.shown('Recording on')
        t.pump(.2)
        t.send('\x1b')
        t.shown('Esc discards')
        t.send('\x1bOQ')
        t.shown('AGENTS', 'EARLIER')
        t.click('EARLIER')
        t.shown('other')
        t.click('other')
        t.shown('WAIT TO CHANGE THE ADDRESSED SESSION')
        t.send('\r')
        t.shown('queued', 'voice note', 'spoken-words')
        t.send('\x1bOQ')
        t.until(lambda t: '│ ↳ queued' in t.text())  # at rest, beside the context tape: the sliding list moves it
        t.click('▶ voice note')
        t.until(lambda _: played.read_text())
        pid = json.loads(played.read_text().splitlines()[-1])[0]
        t.send('\x1b')
        t.until(lambda _: gone(pid))
        t.click('take back')
        t.shown('spoken-words')
        t.until(lambda t: 'take back' not in t.text())
        assert not any(e['kind'] == 'turn_end' for e in f.record(session)), 'Esc stopped the agent'
        release.touch()
        t.shown('queue-root-done')

    # Real PCM through the terminal's recorder, server admission, saved rows and one player.
    replies += [Reply((REPLIES / 'hello.sse').read_bytes()), claude.reply(text='new reply arrived', match='another turn')]
    played.write_text('')
    before = sessions()
    with Terminal(f, columns=40) as t:
        t.shown('Voice Ctrl-R')
        t.send('my draft\x12')
        t.shown('Recording on', 'Enter sends')
        t.send('\x1b')
        t.shown('Esc discards', 'my draft')
        t.send('\x1b')
        t.until(lambda t: 'voice note' not in t.text())
        t.send('\x12')
        t.shown('Recording on')
        t.shown('0:01')
        hold.touch()
        t.send('\r')
        t.shown('Transcribing', 'my draft')
        t.send(' latest')
        hold.unlink()
        t.shown('voice note', 'spoken-words', 'flow reply')
        (session,) = sessions() - before
        voice = next(e['voice'] for e in f.record(session) if e['kind'] == 'input')
        assert len(voice['levels']) == 48 and max(voice['levels']) > min(voice['levels']), voice
        y, line = next((y, line) for y, line in enumerate(t.screen.display) if '▶' in line and '▁' in line)
        assert t.screen.buffer[y][line.index('▁')].fg == '3fd9f2', 'saved waveform lost its cyan'
        assert t.screen.buffer[y][line.index('▁')].bg == '123440', 'saved waveform lost the YOU band'
        saved = f.home / '.razor/sessions' / session / voice['file']
        with wave.open(str(saved), 'rb') as wav:
            assert (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()) == (1, 2, 16000, voice['duration_ms'] * 16)
        response = server.send(f, 'GET', f"/sessions/{session}/files/{voice['file']}", headers={'Range': 'bytes=0-3'})
        assert (response.status, response.getheader('Content-Type'), response.read()) == (206, 'audio/wav', b'RIFF')
        t.resize(40, 120)
        # Wait for the resized frame before clicking the play button that moved with it.
        t.until(lambda t: t.screen.display[0][119] == '┐')
        t.click('▶ ')
        t.shown('■')
        t.until(lambda _: played.read_text())
        first = json.loads(played.read_text().splitlines()[-1])
        assert first[1] == '1.0', first
        assert Path(first[-1]).read_bytes() == (f.home / '.razor/sessions' / session / voice['file']).read_bytes()
        t.send('\x1b')
        t.until(lambda _: not Path(first[-1]).exists())
        t.send('\x15')
        for _ in range(24):
            t.send('\t')
            t.pump(.05)
            if any('›' in line and '▶' in line and '▁' in line for line in t.screen.display): break
        else: raise AssertionError('Tab never reached the note waveform')
        t.send('\r')
        t.until(lambda _: len(played.read_text().splitlines()) == 2)
        note = json.loads(played.read_text().splitlines()[-1])
        assert note[1] == '1.0'
        t.send('/listen\r')
        t.shown('Reading aloud')
        t.until(lambda _: len(played.read_text().splitlines()) == 3)
        second = json.loads(played.read_text().splitlines()[-1])
        assert second[1] == '1.2', second
        try: os.kill(note[0], 0)
        except ProcessLookupError: pass
        else: raise AssertionError('replaced player survived')
        server.post(f, f'/sessions/{session}/input', {'text': 'another turn'})
        t.shown('new reply arrived', 'Reading aloud')
        os.kill(second[0], 0)  # A newly completed reply cannot interrupt the earlier one.
        t.send('\x1b')
        t.until(lambda t: 'Reading aloud' not in t.text())
        for value in ('zero', 'none', 'exit'):
            mode.write_text(value)
            t.send('\x12')
            t.shown('check', 'Esc discards')
            t.send('\x1b')
            t.until(lambda t: 'Esc discards' not in t.text())
        mode.write_text('partial')
        t.send('\x12')
        t.until(lambda _: (scratch / 'partial').exists())
        t.send('\x1b')
        t.shown('voice note', 'Esc discards')
        local = list((f.home / 'tmp').glob('razor-audio-*/*'))
        assert len(local) == 1 and local[0].stat().st_size == 1644, 'partial PCM window was lost'
        t.send('\x1b')
        t.until(lambda t: 'Esc discards' not in t.text())
        mode.write_text('sound')
        t.send('\x12')
        t.shown('Recording on')
        t.send('\x03')
        t.shown('DISCARD AUDIO')
        t.send('\x1b')
        t.until(lambda t: 'DISCARD AUDIO' not in t.text())

    # Audio refusal/retry, live machine settings, SSH, a fast capture limit and quitting in flight.
    mode.write_text('quiet')
    setting('speech', ['/bin/sh', '-c', 'echo speech-broken >&2; exit 3'])
    replies += [claude.reply(text='speech source'), claude.reply(text='typed reply')]
    before = sessions()
    with Terminal(f) as t:
        t.shown('Message the agent')
        t.send('hello\r')
        t.shown('speech source', '▶')
        (session,) = sessions() - before
        t.send('/listen\r')
        t.shown('speech-broken', 'Enter tries again')
        assert b'speech-broken' in copied(t)
        t.shown('COPIED')
        t.send('/not-a-command\r')
        t.shown('UNKNOWN COMMAND', 'speech-broken', 'Enter tries again')
        t.send('\x15')
        t.send('typed after error')
        t.shown('typed after error')
        assert copied(t) == b'typed after error'
        t.send('\r')
        t.shown('typed reply')
        assert any(e.get('kind') == 'input' and e.get('text') == 'typed after error' for e in f.record(session))
        assert 'Enter tries again' not in t.text()
    setting('speech')

    replies.append(claude.reply(text='ssh spoken reply'))
    unplayed = played.read_text()
    with Terminal(replace(f, env={**f.env, 'SSH_CONNECTION': 'flow'})) as t:
        t.shown('Message the agent')
        assert 'Voice' not in t.text()
        t.send('\x12')
        t.shown('RECORDING IS UNAVAILABLE OVER SSH')
        assert b'Recording is unavailable over SSH' in copied(t)
        # Read-aloud names the machine that would play it and plays nothing; the reply's ▶ lights
        # once its turn has ended, which /listen needs.
        t.send('hello\r')
        t.shown('ssh spoken reply', '▶')
        t.send('/listen\r')
        t.shown(f'OVER SSH, AUDIO WOULD PLAY ON {socket.gethostname().upper()}, NOT YOUR MACHINE')
        assert played.read_text() == unplayed and not list((f.home / 'tmp').glob('razor-audio-*'))

    # Quiet, nonzero PCM is valid; failed transcription keeps the same uploaded file for Enter.
    replies += [claude.reply(text=['first spoken reply', 'second spoken reply']), claude.reply(text='uncached reply')]
    before = sessions()
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
        (session,) = sessions() - before
        files = list((f.home / '.razor/sessions' / session).rglob('voice.wav'))
        assert len(files) == 1, files
        mode.write_text('quiet')
        t.send('\r')
        t.shown('spoken-words', '[Image #1]')
        assert list((f.home / '.razor/sessions' / session).rglob('voice.wav')) == files
        assert sum(e['kind'] == 'input' for e in f.record(session)) == 1
        t.resize(40, 120)
        # /listen reads the newest completed reply: its words can draw before its turn ends, its lit cap only after.
        t.until(lambda t: '└' + '─' * 118 + '┘' in t.text() and any('▶' in cap and 'first spoken reply' in words for cap, words in zip(t.screen.display, t.screen.display[1:])))
        played.write_text('')
        heard.write_text('')
        t.send('\x15/listen\r')
        t.until(lambda _: len(played.read_text().splitlines()) == 2)
        t.until(lambda _: len(heard.read_text().splitlines()) == 2)
        calls = [json.loads(line) for line in played.read_text().splitlines()]
        assert [bytes.fromhex(line) for line in heard.read_text().splitlines()] == [b'first spoken reply', b'second spoken reply']
        assert [call[1] for call in calls] == ['1.2', '1.2'], calls
        t.until(lambda t: 'Reading aloud' not in t.text())
        setting('play', commands['play'][:-1])
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
        setting('speech', [])
        # A new text entry has no cached speech, so missing speech remains a visible refusal.
        server.post(f, f'/sessions/{session}/input', {'text': 'need new speech'})
        t.shown('uncached reply')
        t.send('/listen\r')
        t.shown('No speech program is configured', 'Enter tries again')
        setting('speech', ['/bin/sh', '-c', 'exit 3'])
        t.send('\r')
        t.shown("Couldn't read it aloud", 'Enter tries again')
        t.send('\x1b')
        t.until(lambda t: 'Enter tries again' not in t.text())
    setting('play'), setting('speech')

    # The real server's four busy programs refuse a fifth; a live config change then refuses 503.
    hold.touch()
    runs.write_text('')
    replies += [claude.reply(text=f'busy note {n}') for n in range(5)]
    session = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
    threads, results, busy_sessions = [], [], []
    for n in range(4):
        busy = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
        busy_sessions.append(busy)
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
            setting('transcribe', [])
            t.send('\r')
            t.shown('This server has no transcriber', 'Enter tries again')
            setting('transcribe')
            hold.unlink()
            for thread in threads: thread.join()
            assert len(results) == 4
            for busy in busy_sessions:
                server.until(f, busy, lambda es: es[-1]['kind'] == 'turn_end')
            t.send('\r')
            t.shown('spoken-words')
            server.until(f, session, lambda es: sum(e['kind'] == 'turn_end' for e in es) >= 1)
            assert sum(e['kind'] == 'input' for e in f.record(session)) == 1
    finally:
        hold.unlink(missing_ok=True)
        for thread in threads: thread.join()

    # Limit is measured PCM duration; a fast device can fill ten minutes without a ten-minute test.
    mode.write_text('fast')
    before = sessions()
    with Terminal(f, columns=40) as t:
        t.shown('Voice Ctrl-R')
        t.send('\x12')
        t.shown('10:00', 'Esc discards')
        assert sessions() == before
        t.send('\x03')
        t.shown('DISCARD', 'AUDIO.', 'Esc stays')
        t.send('\x1b')
        t.until(lambda t: 'PRESS CTRL-C' not in t.text())
    assert not list((f.home / 'tmp').glob('razor-audio-*'))

    # A voice note settles a question being answered in words; the words wait for a second Enter.
    mode.write_text('sound')
    root = 'VOICE_QUESTION_ROOT'
    replies += [message(root, 'question', text='Keep this answer addressed?', options=['yes', 'no']),
                reply(root, text='question-open'), reply(root, text='voice-arrived'), reply(root, text='answer-arrived')]
    session = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
    server.post(f, f'/sessions/{session}/input', {'text': root})
    server.until(f, session, lambda es: es[-1]['kind'] == 'turn_end')
    with Terminal(f, '--resume', session, rows=80) as t:
        # Enter answers only the band a person sees, and the transcript can draw the question first.
        t.shown('Keep this answer addressed?', 'question-open', 'Enter to answer in words')
        t.send('\rtyped answer\x12')
        t.shown('Recording on')
        t.pump(.2)
        t.send('\r')
        t.shown('spoken-words', 'voice-arrived', 'typed answer')
        inputs = [e for e in f.record(session) if e['kind'] == 'input']
        assert inputs[-1]['text'] == 'spoken-words' and 'answer' not in inputs[-1]['from']
        t.until(lambda t: 'in words' not in t.text())
        t.send('\r')
        t.shown('QUESTION CLOSED', 'typed answer')  # Notices are drawn in capitals.
        assert [e for e in f.record(session) if e['kind'] == 'input'] == inputs
        t.send('\r')
        t.shown('answer-arrived')
        assert [(e['text'], 'answer' in e['from']) for e in f.record(session) if e['kind'] == 'input'][-1] == ('typed answer', False)

    # Starting capture reaps a player; quitting reaps capture and removes its private file.
    replies.append(reply(None, text='completed reply'))
    session = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
    server.post(f, f'/sessions/{session}/input', {'text': 'begin'})
    server.until(f, session, lambda es: es[-1]['kind'] == 'turn_end')
    with Terminal(f, '--resume', session) as t:
        t.shown('completed reply', 'Voice')
        before = len(played.read_text().splitlines())
        t.click('▶')
        t.shown('Reading aloud')
        t.until(lambda _: len(played.read_text().splitlines()) > before)
        pid = json.loads(played.read_text().splitlines()[-1])[0]
        t.send('\x12')
        t.shown('Recording on')
        assert gone(pid)
        capture_pid = int((scratch / 'capture-pid').read_text())
        directory = next((f.home / 'tmp').glob('razor-audio-*'))
        assert directory.stat().st_mode & 0o777 == 0o700
        assert all(p.stat().st_mode & 0o777 == 0o600 for p in directory.iterdir())
    assert gone(capture_pid) and not directory.exists()
    with Terminal(f, '--resume', session) as t:
        t.shown('completed reply')
        before = len(played.read_text().splitlines())
        t.send('/listen\r')
        t.shown('Reading aloud')
        t.until(lambda _: len(played.read_text().splitlines()) > before)
        pid = json.loads(played.read_text().splitlines()[-1])[0]
    assert gone(pid)
    # A real terminal hangup enters the same cleanup path, without another key to the app.
    with Terminal(f, '--resume', session) as t:
        t.shown('completed reply', 'Voice')
        t.send('\x12')
        t.shown('Recording on')
        pid = int((scratch / 'capture-pid').read_text())
        os.kill(t.pid, signal.SIGHUP)
        deadline = time.monotonic() + 10
        while True:
            waited, status = os.waitpid(t.pid, os.WNOHANG)
            if waited: break
            assert time.monotonic() < deadline, 'hangup did not exit'
            t.pump(.05)
        assert os.waitstatus_to_exitcode(status) == 0 and gone(pid)
        t.quit = lambda: None  # The hangup already reaped this terminal.
    assert not list((f.home / 'tmp').glob('razor-audio-*'))
