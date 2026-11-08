"""The real composer shows the offer the server publishes; accepting edits, sending is separate."""
import json

from driver import flow
from responses import PLAIN, call, message, response
import server
from terminal import Terminal

OFFER = 'Run the Linux build'
RAW = 'Ready77.\n[razor-suggestion] ' + json.dumps({'text': OFFER})
HINT = 'Tab to edit'
CONFIG = '''capture = ["/bin/sh", "-c", "while :; do head -c 3200 /dev/zero; sleep 0.1; done"]
transcribe = ["/bin/sh", "-c", "cat >/dev/null; echo voice77"]
[ui]
suggestions = true
motion = false
'''


def inputs(f, s):
    return [e for e in f.record(s) if e['kind'] == 'input']


def new(f):
    return server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']


def send(f, s, words):
    admitted = server.post(f, f'/sessions/{s}/input', {'text': words})['entry']
    return server.until(f, s, lambda es: any(e['kind'] == 'turn_end' and admitted in e['inputs'] for e in es))


# Five separate replies exercise acceptance and dismissal without resurrecting a prior entry.
with flow([response([message(RAW)]) for _ in range(5)] + [response([message('Sent separately77.')])],
          model='sol', config=CONFIG + PLAIN) as f:
    server.start(f)
    s = new(f)
    send(f, s, 'Start77')
    with Terminal(f, '--resume', s) as t:
        t.shown('Ready77.', OFFER, HINT)
        assert '[razor-suggestion]' not in t.text(), t.text()
        # The proposed words use the empty composer's grey hint style.
        y = t.find(OFFER)[0]
        x = t.screen.display[y].index(OFFER)
        grey = t.screen.buffer[y][x].fg
        path = f.home / '.razor/config.toml'
        enabled = path.read_text()
        path.write_text(enabled.replace('suggestions = true', 'suggestions = false'))
        t.shown('Message the agent')
        assert OFFER not in t.text(), t.text()
        path.write_text(enabled)
        t.shown(OFFER, HINT)
        t.send('\r')
        t.pump(0.5)
        assert len(inputs(f, s)) == 1 and HINT in t.text(), (inputs(f, s), t.text())
        t.send('x')
        t.until(lambda t: HINT not in t.text())
        t.send('\x03')
        t.shown('Message the agent')
        assert OFFER not in t.text(), t.text()
        send(f, s, 'Paste77')
        t.shown(OFFER, HINT)
        t.send('\x1b[200~pasted77\x1b[201~')
        t.shown('pasted77')
        assert HINT not in t.text(), t.text()
        t.send('\x03')
        t.shown('Message the agent')
        assert OFFER not in t.text(), t.text()
        send(f, s, 'Voice77')
        t.shown(OFFER, HINT, 'Voice')
        t.send('\x12')
        t.shown('Recording on')
        assert HINT not in t.text(), t.text()
        t.send('\x1b')
        t.shown('Esc discards')
        t.send('\x1b')
        t.shown('Message the agent')
        assert OFFER not in t.text(), t.text()
        send(f, s, 'Escape77')
        t.shown(OFFER, HINT)
        # Esc closes the floating status pane first, and only then dismisses the offer.
        t.send('\x14')
        t.shown('STATUS', OFFER, HINT)
        t.send('\x1b')
        t.until(lambda t: 'STATUS' not in t.text())
        t.shown(OFFER, HINT)
        t.send('\x1b')
        t.shown('Message the agent')
        assert OFFER not in t.text(), t.text()
        send(f, s, 'Accept77')
        t.shown(OFFER, HINT)
        t.send('\t')
        t.until(lambda t: HINT not in t.text() and OFFER in t.text())
        y = t.find(OFFER)[0]
        x = t.screen.display[y].index(OFFER)
        assert t.screen.buffer[y][x].fg != grey, (grey, t.screen.buffer[y][x])
        assert len(inputs(f, s)) == 5, inputs(f, s)
        t.send('\r')
        t.shown('Sent separately77.')
        assert [e['text'] for e in inputs(f, s)] == [
            'Start77', 'Paste77', 'Voice77', 'Escape77', 'Accept77', OFFER]
    assert len(f.sent()) == 6


# A reply that is only its suggestion line draws no reply and keeps the earlier preview; another
# session's open question leaves this session's offer.
question = call('message', json.dumps({'type': 'question', 'text': 'Which build77?', 'options': ['Linux', 'Mac']}))
with flow([response([message('Earlier77.')]), response([message(RAW.split('\n')[1])]), response([question]),
           response([message('Asked77.')]), response([message('Answered77.')])], model='sol', config=CONFIG + PLAIN) as f:
    server.start(f)
    s, other = new(f), new(f)
    send(f, s, 'One77')
    spoke = json.loads(server.send(f, 'GET', f'/sessions/{s}/agent').read())['spoke_at']
    send(f, s, 'Two77')
    send(f, other, 'Ask77')
    row = json.loads(server.send(f, 'GET', f'/sessions/{s}/agent').read())
    assert row['suggestion'] and (row['last_reply'], row['spoke_at']) == ('Earlier77.', spoke), row
    with Terminal(f, '--resume', s) as t:
        t.shown('Which build77?', 'Earlier77.', 'Two77', OFFER, HINT)
        assert t.text().count('❖') == 1, t.text()
        t.send('\r')
        t.shown('Answer here')
        t.send('Linux')
        t.shown('Linux', 'Answer here')
        assert HINT not in t.text(), t.text()
        t.send('\r')
        t.shown(OFFER, HINT)
        server.until(f, other, lambda es: any(e['kind'] == 'input' and 'Linux' in e.get('text', '') for e in es))
