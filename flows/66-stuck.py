"""Within a tick, a working session whose record has sat past `stuck.after` with its input
unanswered shows as stuck, and one inside an `agent wait` before its deadline stays working.
Before it, on both routes, an input admitted while the model streams ends an `agent wait` at once."""
from dataclasses import replace
import json
import time
import claude
from driver import flow
from model import agent, reply
from responses import PLAIN, call, done, response
import server

# Streaming outlasts the stuck check even when its next tick is delayed.
replies = [agent("WAIT66", "wait", timeout_ms=600000),
           replace(reply("SLOW66", text="slow66" + "." * 400), cuts=tuple(range(1, 201)))]
wait = {"action": "wait", "timeout_ms": 20000}
routes = {"opus": [claude.reply(("toolu_wait67", "mcp__razor__agent", wait)), claude.reply(text="Heard.")],
          "gpt-6.1-sol": [response([call("agent", json.dumps(wait), "toolu_wait67")]), done()]}
for model, (first, heard) in routes.items():
    # About 3 s of streaming before the call, in which the person writes.
    replies += [replace(first, cuts=tuple(range(1, 100)), match=f"NEWS67 {model}"), replace(heard, match=f"NEWS67 {model}")]
with flow(replies, config='[stuck]\nafter = "1s"\n' + PLAIN) as f:
    server.start(f)
    waiting = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{waiting}/input", {"text": "WAIT66"})
    server.until(f, waiting, lambda es: any(e["kind"] == "call" for e in es))

    # The person's input admitted while the model streams goes to the model path at the next
    # call's admission and stays unread until that batch's results: an `agent wait` in the batch
    # answers null at once, and the input reaches the next request (3 November: the wait hung 86 s).
    for model in routes:
        marker = f"NEWS67 {model}"
        session = server.post(f, "/sessions", {"cwd": str(f.cwd), "model": model})["session"]
        path = f"/sessions/{session}"
        server.post(f, path + "/input", {"text": f"Wait for news. {marker}"})
        server.wait(lambda: any(marker in body for body in f.sent()))
        began, steer = time.monotonic(), server.post(f, path + "/input", {"text": "STEER67"})["entry"]
        entries = server.until(f, session, lambda es: es[-1]["kind"] == "turn_end")
        call_at, waited = (next(e for e in entries if e.get("call") == "toolu_wait67" and e["kind"] == k) for k in ("call", "result"))
        assert waited["output"] == "null" and time.monotonic() - began < 10, (model, waited)
        assert call_at["id"] > steer and steer in entries[-1]["inputs"], (model, entries)
        # The request carrying the wait's null, its last mention of the call, carries the input.
        body = [body for body in f.sent() if marker in body][1]
        assert body.rindex("toolu_wait67") < body.find("STEER67"), model

    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    started = time.monotonic()
    server.post(f, f"/sessions/{session}/input", {"text": "SLOW66"})
    while (rows := server.agents(f))[session]["state"] != "stuck":
        assert rows[session]["state"] == "working", rows
        assert time.monotonic() - started < 3, "stuck was not checked at the session's deadline"
        time.sleep(0.05)
    admitted = next(e["at"] for e in f.record(session) if e["kind"] == "input")
    assert int(time.time() * 1000) - admitted >= 1000, "stuck fired before its deadline"
    assert rows[waiting]["state"] == "working", rows
    server.post(f, f"/sessions/{waiting}/stop", {})
    server.until(f, session, lambda es: any(e["kind"] == "turn_end" for e in es))
