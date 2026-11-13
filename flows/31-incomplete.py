"""The Responses stream's failure transitions, each in its own session of one home: a reply split
at any byte parses the same; malformed arguments get an error result and no side effect; an early
EOF or a missing done item admits no response and executes no calls, and a turn failing after
its retries keeps its last attempt's words alone."""
import json

from driver import Reply, flow
from responses import PLAIN, call, done, message, response
import server

# Splitting UTF-8, JSON escapes and SSE framing never changes an admitted call.
name = 'quoted " 🪶.txt'
raw = json.dumps({"path": name}, ensure_ascii=False)
read = response([call("read", raw)])
feather = read.body.index("🪶".encode())
escape = read.body.index(b'\\\\\\"')
cuts = sorted({1, 6, feather + 1, feather + 2, feather + 3, escape + 1, escape + 2, len(read.body) - 1})
# A complete but malformed call gets its own error result, with no side effect.
malformed = '{"command":"printf changed > must-not-exist.txt"}broken'
# JSON text inside JSON text stays a string.
question = json.dumps({"type": "question", "to": "parent", "text": "Double?", "options": ["a", "b"]})
# GPT early EOF and missing done item admit no response and execute no calls; each early
# attempt's words differ.
items = [call("bash", json.dumps({"command": "printf first > first.txt"}), "call_first"),
         call("bash", json.dumps({"command": "printf second > second.txt"}), "call_second")]
replies = [reply for cut in cuts for reply in (Reply(read.body, (cut,)), done())]
replies += [response([call("bash", malformed)]), done()]
replies += [response([call("bash", '{"command":"true"}', "valid"), call("message", json.dumps(question), "double")]), done()]
replies += [response([message(f"Try {n}."), *items], completed=False) for n in range(6)]
replies += [response(items, missing=(1,)), done()]
with flow(replies, model="gpt-6.1-sol", config=PLAIN) as f:
    (f.cwd / name).write_text("payload 🪶\n")
    seen = []
    for cut in cuts:
        entries = list(f.record(f.ask("Read the named file.")["session"]))
        calls = [e for e in entries if e["kind"] == "call"]
        assert len(calls) == 1, (cut, calls)
        native = next(e["native"]["responses"]["items"] for e in entries if e["kind"] == "response")
        seen.append((calls[0]["name"], native[0]["arguments"]))
        outputs = [e for e in entries if e["kind"] == "result"]
        assert len(outputs) == 1 and outputs[0]["outcome"] == "success", (cut, outputs)
        assert "payload 🪶" in outputs[0]["output"], (cut, outputs)
    assert len(seen) == len(cuts) and len(set(seen)) == 1, seen
    assert seen[0] == ("read", raw), seen

    # A complete but malformed call gets its own error result, with no side effect.
    base = len(f.sent())
    entries = list(f.record(f.ask("Do not repair malformed arguments.")["session"]))
    calls = [e for e in entries if e["kind"] == "call"]
    assert len(calls) == 1 and calls[0]["input"] == malformed, calls
    outputs = [e for e in entries if e["kind"] == "result"]
    assert len(outputs) == 1 and outputs[0]["outcome"] == "error", outputs
    assert outputs[0]["call"] == calls[0]["call"] and calls[0]["id"] < outputs[0]["id"], entries
    assert not (f.cwd / "must-not-exist.txt").exists()
    assert len([e for e in entries if e["kind"] == "response"]) == 2, entries
    second = f.request(base + 2)
    assert any(i["type"] == "function_call_output" and i["call_id"] == calls[0]["call"] for i in second["input"]), second

    # Valid JSON arguments are recorded as objects. JSON text inside JSON text stays a string:
    # dispatch refuses it and the question markers must not reinterpret the refused call.
    session = f.ask("Check recorded arguments.")["session"]
    entries = list(f.record(session))
    calls = [e for e in entries if e["kind"] == "call"]
    assert calls[0]["input"] == {"command": "true"} and calls[1]["input"] == question, calls
    refused = next(e for e in entries if e.get("call") == "double" and e["kind"] == "result")
    assert refused["outcome"] == "error", refused
    marks = [json.loads(line) for line in server.send(f, "GET", f"/sessions/{session}/marks").read().splitlines()]
    assert "question" not in json.dumps(marks), marks

    # An early EOF exhausts five stream retries after the first attempt; a missing done item
    # recovers on its first retry. Neither cut attempt admits a response or runs a call.
    for case, attempts in (("early", 6), ("missing", 2)):
        base = len(f.sent())
        result = f.run("--print=stream", f"Exercise {case}.", timeout=90)
        entries = [json.loads(line) for line in result.stdout.splitlines()]
        ends = [e for e in entries if e.get("kind") == "turn_end"]
        assert len(ends) == 1, (case, entries)
        if case == "early":
            assert isinstance(ends[0]["end"], dict) and "error" in ends[0]["end"], entries
            assert not [e for e in entries if e.get("kind") in ("call", "result", "response")], entries
            # The failed turn keeps the words its last attempt streamed, once; retries void the others'.
            assert [e["text"] for e in entries if e.get("kind") == "text"] == ["Try 5."], entries
        else:
            assert ends[0]["end"] == "done", entries
            assert not [e for e in entries if e.get("kind") in ("call", "result")], entries
            responses = [e for e in entries if e.get("kind") == "response"]
            assert len(responses) == 1 and "call_first" not in json.dumps(responses), responses
        assert not (f.cwd / "first.txt").exists() and not (f.cwd / "second.txt").exists(), case
        assert len(f.sent()) - base == attempts, (case, list(f.requests.iterdir()))
