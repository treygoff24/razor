"""Thoughts stream as live lines on every route: numbered on through the session, `start` on each
one's first words, those words together its Thinking entry's text; never recorded or replayed; a
redacted thought sends nothing, and a dropped line never moves words to another thought's number.
A reply block's first shown words carry `start` too."""
from dataclasses import replace
from itertools import takewhile
import json

import chat
import claude
from driver import flow
from responses import PLAIN, call, done, message, response, retried, thought
import server
from standin import Reply


def turn(f, text, session=None):
    result = f.run("--print=stream", *(["--resume", session] if session else []), text)
    assert result.returncode == 0, result.stderr
    lines = [json.loads(line) for line in result.stdout.splitlines()]
    return lines, lines[0]["session"]


def check(lines, n):
    """Thoughts number on from `n`, `start` on each first line alone, a void line drops every
    thought through its number, and each Thinking entry is the words of the oldest thought left;
    a delta has `start` on a reply block's first words alone. Returns the last number and the
    order of thought (v for void), Thinking and reply lines."""
    begun, words = {}, False
    for line in lines:
        if line.get("void"):
            assert line == {"kind": "thought", "n": n, "void": True}, (line, n)
            begun, words = {}, False
        elif line["kind"] == "thought":
            assert line.get("start", True) is True, line  # omitted when false
            if "start" in line:
                n += 1
                begun[n] = ""
            assert line["n"] == n and n in begun, (line, n)
            begun[n] += line["text"]
        elif line["kind"] == "thinking":
            assert begun and begun.pop(min(begun)) == line["text"], (begun, line)
        assert line["kind"] != "delta" or line.get("start") is (None if words else True), (line, words)
        words = line["kind"] == "delta" or words and line["kind"] not in ("text", "turn_end")
    assert not begun, begun
    kinds = {"thought": "t", "thinking": "T", "delta": "d"}
    return n, "".join("v" if l.get("void") else kinds.get(l["kind"], "") for l in lines)


def unrecorded(f, session):
    """No thought is in the record, nor in a replay of the whole of it to a client attaching anew."""
    record = list(f.record(session))
    assert all(e["kind"] != "thought" for e in record)
    with server.send(f, "GET", f"/sessions/{session}/events?after=0") as stream:
        replayed = [json.loads(stream.readline()) for _ in range(len(record) + 1)]
    assert replayed[0]["kind"] == "attached" and [e["kind"] for e in replayed[1:]] == [e["kind"] for e in record], replayed


def attach(f, session):
    """The session's deltas from now until the turn ends: their words and `start`."""
    stream = server.send(f, "GET", f"/sessions/{session}/events?after={2**62}")
    assert json.loads(stream.readline())["kind"] == "attached"
    lines = takewhile(lambda l: l["kind"] != "turn_end", map(json.loads, stream))
    return ((l["text"], l.get("start")) for l in lines if l["kind"] == "delta")


# Claude: a thought in pieces with a pause between them lands as its entry right after; a redacted
# block beside the next one sends nothing; the resumed session's thought is the third. A message
# cut by an error mid-thought, or mid-reply after its thought landed, is tried again as a new one,
# after a void line.
error = 'event: error\ndata: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}\n\n'
cut = claude.reply(text="claude-cut-73", thinking=["ABAN", "DONED"]).body.decode()
said = claude.reply(text="SAID", thinking="Plan.").body.decode()
replies = [claude.reply((None, "mcp__razor__status", {}), thinking=["First ", "thought, ", "in pieces."], gap=0.2),
           claude.reply(text="claude-73", thinking="Second thought.", redacted=True),
           claude.reply(text="claude-again-73", thinking=["Third ", "one."]),
           Reply((cut[:cut.index("event: content_block_stop")] + error).encode()),
           claude.reply(text="claude-retried-73", thinking="COMPLETED"),
           Reply((said[:said.rindex("event: content_block_stop")] + error).encode()), claude.reply(text="RESAID", thinking="Replan.")]
with flow(replies) as f:
    lines, session = turn(f, "Think, look, reply.")
    assert check(lines, 0) == (2, "tttTtTd"), lines
    lines, _ = turn(f, "Again.", session)
    assert check(lines, 2) == (3, "ttTd"), lines
    lines, _ = turn(f, "Once more.", session)
    assert check(lines, 3) == (5, "ttvtTd"), lines
    lines, _ = turn(f, "And again.", session)
    assert check(lines, 5) == (7, "tTdvtTd"), lines
    unrecorded(f, session)

# GPT: two reasoning items, the first in two summary parts, and the reply's words all stream
# before the response's entries; a later part begins with the line break its entry joins with.
streamed = response([thought(["Plan ", "the call."], ["Then ", "look."], id="rs_1"), message("Looking ", "now.", id="msg_1"),
                     thought(["Second ", "item."], id="rs_2"), call("status", "{}")], gap=0.2)
# A cut response tried again sends a void line between the cut attempt's thoughts and the retry's.
retry = retried([thought(["ABAN", "DONED"], id="rs_cut")], [thought(["COMPLETED"], id="rs_ok"), message("done")])
with flow([streamed, done(), *retry], model="gpt-6.1-sol", config=PLAIN) as f:
    lines, session = turn(f, "Think, look, reply.")
    assert check(lines, 0) == (2, "ttttddttTT"), lines
    lines, _ = turn(f, "Again.", session)
    assert check(lines, 2) == (4, "ttvtdT"), lines
    unrecorded(f, session)

# A block whose first words are held as a possible suggestion starts on the words it shows; a
# client that attaches mid-block gets no `start` until the next block.
held = response([message("[razor", "-ish. ", "More.", id="msg_1"), call("status", "{}")], gap=0.5)
with flow([done(), held, response([message("Then ", "this.", id="msg_2")])], model="gpt-6.1-sol", config=PLAIN) as f:
    session = turn(f, "Hello.")[1]
    early = attach(f, session)
    server.post(f, f"/sessions/{session}/input", {"text": "Look twice."})
    first, late = next(early), attach(f, session)
    assert [first, *early] == [("[razor-ish. ", True), ("More.", None), ("Then ", True), ("this.", None)], first
    assert [*late] == [("More.", None), ("Then ", True), ("this.", None)]

# A burst the display channel cannot hold drops lines, first ones among them: a thought's later
# words still carry its own number, so a client sees a new n without start and skips it.
burst = response([thought([f"{i}:{j} " for j in range(4)], id=f"rs_{i}") for i in range(1500)])
burst = replace(burst, cuts=burst.cuts[200::201])  # bursts of 201 lines 30 ms apart, each ending mid-thought
with flow([burst], model="gpt-6.1-sol", config=PLAIN) as f:
    lines, _ = turn(f, "Think a lot.")
    items, starts = {}, {l["n"] for l in lines if l.get("start")}
    for line in (l for l in lines if l["kind"] == "thought"):
        items.setdefault(line["n"], set()).add(line["text"].split(":")[0])
    assert all(len(i) == 1 for i in items.values()) and set(items) - starts, items


# A chat host's reasoning deltas are one thought per response, and a cut one is voided too.
replies = [chat.reply({"reasoning_content": "Look "}, {"reasoning_content": "first."}, {"content": "glm-73"}),
           chat.reply({"reasoning_content": "ABANDONED"}, finish=None),
           chat.reply({"reasoning_content": "DONE"}, {"content": "glm"})]
GLM = '[providers.runinfra]\nbase_url = "{standin}"\n'
with flow(replies, model="glm-5.3-flash", config=GLM, env={"RUNINFRA_API_KEY": "flow-key"}) as f:
    lines, session = turn(f, "Think and reply.")
    assert check(lines, 0) == (1, "ttdT"), lines
    assert check(turn(f, "Again.", session)[0], 1) == (3, "tvtdT")
