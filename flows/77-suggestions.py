"""Suggestions share the main request; projection never changes the native evidence.
Suggestion values are read directly; membership in the once-per-second feed is independent.
The stand-in establishes transport and parsing, not model quality or cache hits.
"""
import json

import chat
import claude
from clock import wall_clock
from driver import flow
from responses import PLAIN, call, message, response
import server
from standin import Reply

MARK = '[razor-suggestion]'
TEXT = 'Run the Linux build'
TRAILER = MARK + ' ' + json.dumps({'text': TEXT})
RAW = 'The change is ready.\n' + TRAILER
ON = '[ui]\nsuggestions = true\nmotion = false\n'
CHAT = '[providers.runinfra]\nbase_url = "{standin}"\n'


def turn(f, words, session=None):
    result = f.run('--print=stream', *(['--resume', session] if session else []), words)
    assert result.returncode == 0, result.stderr
    lines = [json.loads(line) for line in result.stdout.split('\n') if line]
    assert len([e for e in lines if e['kind'] == 'turn_end']) == 1, lines
    return lines[0]['session'], lines


def entries(f, session, native=False):
    return [json.loads(line) for line in server.send(
        f, 'GET', f'/sessions/{session}/entries?limit=4096' + ('&native=true' if native else '')).read().splitlines()]


def plain(value):
    """Without Claude's cache_control marks, which its CLI moves onto the newest blocks."""
    if isinstance(value, dict):
        return {k: plain(v) for k, v in value.items() if k != 'cache_control'}
    return [plain(v) for v in value] if isinstance(value, list) else value


def blocks(message):
    """Claude's CLI sends a message's lone text block as a string once it is no longer newest."""
    content = message['content']
    return {**message, 'content': [{'type': 'text', 'text': content}] if isinstance(content, str) else content}


# Same conversation, live off -> on -> on -> off, the third input from a program: the prefix,
# tools and all prior history stay the same, only the newest input differs, and no call is added.
for model, config, make in [('opus', '', lambda t: claude.reply(text=t)),
                            ('sol', PLAIN, lambda t: response([message(t)])),
                            ('glm-5.3-flash', CHAT, lambda t: chat.reply({'content': t}))]:
    with flow([make('Disabled reply.'), make(RAW), make('Replay checked.'), make('Off again.')], model=model,
              config=config, env={'RUNINFRA_API_KEY': 'flow-key'}) as f:
        session, _ = turn(f, 'OFF77 verbatim input')
        path = f.home / '.razor/config.toml'
        off = path.read_text()
        path.write_text(off + ON)
        server.wait(lambda: json.loads(server.send(f, 'GET', '/status').readline())['server'].get('suggestions'))
        _, live = turn(f, 'ON77 verbatim input', session)
        record = list(f.record(session))
        raw = next(e for e in record if e['kind'] == 'text' and e['text'] == RAW)
        projected = next(e for e in live if e['kind'] == 'text' and e['id'] == raw['id'])
        assert projected['text'] == 'The change is ready.' and projected['suggestion'] == TEXT, projected
        assert MARK not in ''.join(e['text'] for e in live if e['kind'] == 'delta'), live
        assert next(e for e in entries(f, session) if e['id'] == raw['id']) == projected
        assert next(e for e in entries(f, session, True) if e['id'] == raw['id']) == raw
        for native in (False, True):
            with server.send(f, 'GET', f'/sessions/{session}/events?after=0' + ('&native=true' if native else '')) as stream:
                replay = [json.loads(stream.readline()) for _ in range(len(record) + 1)]
            assert next(e for e in replay if e.get('id') == raw['id']) == (raw if native else projected)
        assert json.load(server.send(f, 'GET', f'/sessions/{session}/agent'))['suggestion'] == raw['id']
        admitted = server.post(f, f'/sessions/{session}/input', {'text': 'AGENT77', 'from': 'flow'})['entry']
        server.until(f, session, lambda es: any(e['kind'] == 'turn_end' and admitted in e['inputs'] for e in es))
        path.write_text(off)
        server.wait(lambda: 'suggestions' not in json.loads(server.send(f, 'GET', '/status').readline())['server'])
        turn(f, 'OFF77 again', session)
        sent = [json.loads(body) for body in f.sent()]
        assert len(sent) == 4, sent
        key = 'input' if model == 'sol' else 'messages'
        for field in set().union(*sent) - {key}:
            assert all(b.get(field) == sent[0].get(field) for b in sent), field
        for before, after in zip(sent, sent[1:]):
            old, new = before[key], after[key]
            assert len(new) == len(old) + 2, (old, new)
            if model == 'opus':
                old, new = plain(old), plain(new[:len(old)])
                old[-1], new[-1] = blocks(old[-1]), blocks(new[-1])
            assert new[:len(old)] == old, (old, new)
        newest = [json.dumps(b[key][-1]) for b in sent]
        assert MARK not in newest[0] and 'ON77 verbatim input' in newest[1] and MARK in newest[1], newest[:2]
        assert 'AGENT77' in newest[2] and MARK not in newest[2], newest[2]
        assert 'OFF77 again' in newest[3] and MARK not in newest[3], newest[3]
        assert RAW in json.dumps(sent[2], ensure_ascii=False).replace('\\n', '\n').replace('\\"', '"'), sent[2]
        recorded = list(f.record(session))
        assert [e['text'] for e in recorded if e['kind'] == 'input'] == [
            'OFF77 verbatim input', 'ON77 verbatim input', 'AGENT77', 'OFF77 again']
        assert len([e for e in recorded if e['kind'] == 'turn_end']) == 4
        for end in (e for e in recorded if e['kind'] == 'turn_end'):
            assert end['usage']['requests'] == 1 and end['usage']['output'] == (20 if model == 'opus' else 10), end
        assert not [e for e in recorded if e['kind'] in ('call', 'result')]


# An idle handover's input, handover call, rebuild and turn end keep the offer for the person's return.
with wall_clock() as (_, env, advance):
    big = Reply(response([message(RAW)]).body.replace(b'"input_tokens": 100', b'"input_tokens": 50000'))
    note = response([call('handover', json.dumps({'note': 'Idle note77.'}), 'call_idle77')])
    with flow([big, note], model='sol', config=PLAIN + ON + '[sessions]\nidle = 0\n', env=env) as f:
        session, _ = turn(f, 'Idle77')
        offered = json.load(server.send(f, 'GET', f'/sessions/{session}/agent'))['suggestion']
        assert offered, json.load(server.send(f, 'GET', f'/sessions/{session}/agent'))
        advance(3_000_001)
        server.until(f, session, lambda es: es[-1]['kind'] == 'turn_end' and any(e['kind'] == 'rebuild' for e in es))
        assert [e['trigger'] for e in f.record(session) if e['kind'] == 'rebuild'] == ['idle']
        assert json.load(server.send(f, 'GET', f'/sessions/{session}/agent')).get('suggestion') == offered, json.load(server.send(f, 'GET', f'/sessions/{session}/agent'))


# The trailer ends the last reply, yet work after it, a failed or closing turn, or its own open
# question withholds the offer; in each case that one guard stands between the trailer and an offer.
hold = call('bash', json.dumps({'command': 'until [ -e release77 ]; do sleep 0.05; done'}))
question = call('message', json.dumps({'type': 'question', 'text': 'Which build77?', 'options': ['Linux', 'Mac']}))
unended = Reply(response([message(RAW)]).body.replace(b'"status": "completed"', b'"status": "completed", "end_turn": false'))
for case, replies in [('continued', [response([message(RAW), call('bash', '{"command": "true"}')]), response([])]),
                      ('failed', [unended, Reply(b'{"error":{"message":"failed77"}}', status=400)]),
                      ('asked', [response([question]), response([message(RAW)])]),
                      ('closed', [response([hold]), response([message(RAW)])])]:
    with flow(replies, model='sol', config=PLAIN + ON) as f:
        server.start(f)
        session = server.post(f, '/sessions', {'cwd': str(f.cwd)})['session']
        if case == 'closed':  # The person's words join the closing turn at its tool boundary.
            server.post(f, f'/sessions/{session}/done', {})
            server.until(f, session, lambda es: any(e['kind'] == 'job' for e in es))
        admitted = server.post(f, f'/sessions/{session}/input', {'text': case})['entry']
        (f.cwd / 'release77').touch()
        es = server.until(f, session, lambda es: any(e['kind'] == 'turn_end' and admitted in e['inputs'] for e in es))
        reply = max(e['id'] for e in es if e['kind'] == 'text' and e['text'] == RAW)
        later = [e['kind'] for e in es if e['id'] > reply]
        assert later == (['call', 'job', 'job', 'result', 'response', 'turn_end'] if case == 'continued' else ['turn_end']), es
        assert ('error' in es[-1]['end'], es[-1].get('closed', False)) == (case == 'failed', case == 'closed'), es[-1]
        row = json.load(server.send(f, 'GET', f'/sessions/{session}/agent'))
        assert not row.get('suggestion'), (case, row)


# Complete entries and their live deltas: invalid metadata is ordinary answer text.
# One character per delta splits the valid marker at every possible boundary.
unicode_text = '🙂' * 120
WORDS = 'Compilá el café\u0301 日本語 ✓'
cases = [
    (list(RAW), 'The change is ready.', TEXT, 'The change is ready.\n'),
    (['Answer\n' + MARK + ' {broken}\n'], None, None, None),
    (['Answer\n> ' + TRAILER], None, None, None),
    (['Answer\n```json\n', TRAILER, '\n```'], None, None, None),
    # A fenced marker streams as a candidate; its committed entry shows it as text.
    (['Answer\n~~~\n', TRAILER], None, None, 'Answer\n~~~\n'),
    (['Answer\n', TRAILER, '\nMore answer'], None, None, None),
    # At EOF these are still candidates; their committed entries restore the invalid text.
    (['Answer\n' + MARK + ' ' + json.dumps({'text': 'x' * 121})], None, None, 'Answer\n'),
    (['Answer\n' + MARK + ' ' + json.dumps({'text': 'line\nbreak'})], None, None, 'Answer\n'),
    (['Answer\n' + MARK + ' ' + json.dumps({'text': '/done'})], None, None, 'Answer\n'),
    (['Answer\n' + MARK + ' ' + json.dumps({'text': TEXT, 'extra': True})], None, None, 'Answer\n'),
    (['Answer\n' + MARK + ' ' + json.dumps({'text': unicode_text}, ensure_ascii=False)], 'Answer', unicode_text, 'Answer\n'),
    (['Answer\n' + MARK + ' ' + json.dumps({'text': '🙂' * 121}, ensure_ascii=False)], None, None, 'Answer\n'),
    # Bidi and zero-width format characters and Unicode line breaks could make the shown words
    # differ from the sent ones; accents, combining marks and other scripts are words.
    *[(['Answer\n' + MARK + ' ' + json.dumps({'text': 'Run ' + c + ' build'}, ensure_ascii=False)], None, None, 'Answer\n')
      for c in ('\u202edliub', '\u200b', '\u2028', '\u2029')],
    (['Answer\n' + MARK + ' ' + json.dumps({'text': WORDS}, ensure_ascii=False)], 'Answer', WORDS, 'Answer\n'),
    (['Answer\n' + MARK + ' ' + json.dumps({'text': '  ' + TEXT + '  '})], 'Answer', TEXT, 'Answer\n'),
    ([TRAILER], '', TEXT, ''),
    # A candidate over 1 KiB flushes before its entry, even without a newline.
    (['Answer\n' + MARK + ' ' + ' ' * 1100, '{"text":"short"}'], None, None, None),
    (['Answer\n', TRAILER, '\n' * 1100], None, None, None),
    (['x' * 5000, '\n', TRAILER], 'x' * 5000, TEXT, 'x' * 5000 + '\n'),
    # Long opening fence and long lines must not lose the fence's state.
    (['`' * 1100, '\n', TRAILER], None, None, '`' * 1100 + '\n'),
    (['```', 'x' * 1100, '\n', TRAILER], None, None, '```' + 'x' * 1100 + '\n'),
    (['```', 'x' * 1100, '`\n', TRAILER], '```' + 'x' * 1100 + '`', TEXT, '```' + 'x' * 1100 + '`\n'),
]
with flow([response([message(*pieces)], gap=0.002) for pieces, *_ in cases], model='sol', config=PLAIN + ON) as f:
    server.start(f)
    for pieces, clean, suggestion, deltas in cases:
        session, lines = turn(f, 'Exercise the trailer parser.')
        raw = ''.join(pieces)
        projected = next(e for e in lines if e['kind'] == 'text')
        assert projected['text'] == (raw if clean is None else clean), projected
        assert projected.get('suggestion') == suggestion, projected
        live = ''.join(e['text'] for e in lines if e['kind'] == 'delta')
        assert live == (raw if deltas is None else deltas), (live, pieces)
        assert next(e for e in f.record(session) if e['kind'] == 'text')['text'] == raw
        assert bool(json.load(server.send(f, 'GET', f'/sessions/{session}/agent')).get('suggestion')) == bool(suggestion)
