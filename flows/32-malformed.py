"""A complete but malformed call gets its own Error Result, with no side effect."""
import json

from driver import flow
from responses import PLAIN, call, done, response
import server

raw = '{"command":"printf changed > must-not-exist.txt"}broken'
with flow([response([call("bash", raw)]), done()], model="gpt-6.1-sol", config=PLAIN) as f:
    answer = f.ask("Do not repair malformed arguments.")
    entries = list(f.record(answer["session"]))
    calls = [e for e in entries if e["kind"] == "call"]
    assert len(calls) == 1 and calls[0]["input"] == raw, calls
    outputs = [e for e in entries if e["kind"] == "result"]
    assert len(outputs) == 1 and outputs[0]["outcome"] == "error", outputs
    assert outputs[0]["call"] == calls[0]["call"] and calls[0]["id"] < outputs[0]["id"], entries
    assert not (f.cwd / "must-not-exist.txt").exists()
    assert len([e for e in entries if e["kind"] == "response"]) == 2, entries
    second = f.request(2)
    assert any(i["type"] == "function_call_output" and i["call_id"] == calls[0]["call"] for i in second["input"]), second

# Valid JSON arguments are recorded as objects. JSON text inside JSON text stays a string:
# dispatch refuses it and the question markers must not reinterpret the refused call.
question = json.dumps({"type": "question", "to": "parent", "text": "Double?", "options": ["a", "b"]})
with flow([response([call("bash", '{"command":"true"}', "valid"),
                     call("message", json.dumps(question), "double")]), done()], model="sol", config=PLAIN) as f:
    session = f.ask("Check recorded arguments.")["session"]
    entries = list(f.record(session))
    calls = [e for e in entries if e["kind"] == "call"]
    assert calls[0]["input"] == {"command": "true"} and calls[1]["input"] == question, calls
    refused = next(e for e in entries if e.get("call") == "double" and e["kind"] == "result")
    assert refused["outcome"] == "error", refused
    marks = [json.loads(line) for line in server.send(f, "GET", f"/sessions/{session}/marks").read().splitlines()]
    assert "question" not in json.dumps(marks), marks
