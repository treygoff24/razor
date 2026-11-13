"""`/done` through the API: it answers while the closing turn still runs; that turn writes the
project's state note; an input still waiting when it ends ends stopped and reaches no model; the session's job is stopped, its row reads done, nothing keeps it live, and
its next input reopens it, though an input hook rewrote the closing input and the server restarted.
A session closed with `/quit` reads done after that restart too, and neither row's activity moves;
that close settled its question and filed it in Earlier.
Closed rows are read from their direct detail endpoint, outside the live feed.
A second session in the same folder reads the note at its start, and its own `/done` keeps a separate note
(docs/design/done.md)."""
import json
import sys
from responses import PLAIN, call, reply, response
from driver import flow
import server
from terminal import Terminal

state = {"action": "write", "kind": "state"}
replies = [response([call("bash", json.dumps({"command": "sleep 600", "wait_ms": 0}), "toolu_job")]),
           reply(None, "Started."),
           response([call("bash", json.dumps({"command": "while [ ! -e go ]; do sleep 0.05; done", "wait_ms": 120000}), "toolu_wait")]),
           response([call("memory", json.dumps(dict(state, text="First closeout.")), "toolu_note")]),
           reply(None, "Closed out."), response([call("message", json.dumps({"type": "question", "text": "Go on?"}), "toolu_ask")]),
           reply(None, "Asked."), reply(None, "Reopened."), reply(None, "Hello."),
           response([call("memory", json.dumps(dict(state, text="Second closeout.")), "toolu_next")]),
           reply(None, "Closed out again."), reply(None, "Both notes read.")]


def answered(entry, end):
    """Until a turn end answers `entry`; then that it ended `end`."""
    def done(entries):
        ends = [e for e in entries if e["kind"] == "turn_end" and entry in e["inputs"]]
        assert not ends or ends[0]["end"] == end, ends
        return ends
    return done


# An input hook may rewrite any input, Razor's closing one included.
HOOK = f"""#!{sys.executable}
import json, sys
input = json.load(sys.stdin)
if input["from"] == "razor":
    print(json.dumps({{"text": input["text"] + "\\nREWRITTEN_20"}}))
"""
with flow(replies, model="sol", config=PLAIN + '[[plugins]]\nfile = "rewrite.py"\nbefore = ["input"]\n') as f:
    (f.home / ".razor/rewrite.py").write_text(HOOK)
    (f.home / ".razor/rewrite.py").chmod(0o755)
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    path = f"/sessions/{session}"
    server.until(f, session, answered(server.post(f, path + "/input", {"text": "Start a job."})["entry"], "done"))
    done = server.post(f, path + "/done", {})["entry"]
    # The closing turn's first call waits for `go`, so the answer came before the turn ended.
    waiting = server.until(f, session, lambda es: any(e["kind"] == "job" and e["job"] == 2 for e in es))
    assert not answered(done, "done")(waiting), waiting[-3:]
    with Terminal(f, "--resume", session, columns=160, rows=50) as t:
        t.shown("closing after this turn")
        t.send("\x1bOQ")
        t.until(lambda t: t.text().count("closing after this turn") >= 2)
    late = server.post(f, path + "/input", {"text": "Late program news.", "from": "flow"})["entry"]
    (f.cwd / "go").touch()
    entries = server.until(f, session, lambda es: any(
        e["kind"] == "job" and e["event"] == "ended" and e["job"] == 1 for e in es))
    assert answered(done, "done")(entries) and any(e["kind"] == "withdrawn" and late in e["inputs"] for e in entries), entries[-6:]
    assert not any(e["kind"] == "turn_end" and late in e["inputs"] for e in entries), "undelivered input ended a turn"
    closing = next(e for e in entries if e["id"] == done)
    assert closing["from"] == "razor" and closing["command"] == "done" and "state note" in closing["text"] \
        and closing["text"].endswith("\nREWRITTEN_20"), closing
    bodies = f.sent()
    assert json.dumps(closing["text"])[1:-1] in bodies[2], "the closing input missed the model"
    assert not any("Late program news." in body for body in bodies), "a stopped input was sent"
    first = json.loads(next(e["output"] for e in entries if e.get("call") == "toolu_note" and e["kind"] == "result"))
    killed = next(e for e in entries if e["kind"] == "job" and e["event"] == "ended" and e["job"] == 1)
    assert first["id"] and "signal" in killed["end"], (first, killed)
    assert not (f.home / ".razor/live" / session).exists(), "the closed session is still live"
    quit = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{quit}/input", {"text": "Ask me."})
    server.until(f, quit, lambda es: es[-1]["kind"] == "turn_end")
    # Set to end, a terminal's second Ctrl-C ends the session it addresses, as /quit does.
    with (f.home / ".razor/config.toml").open("a") as config:
        config.write('[ui]\nquit = "end"\n')
    with Terminal(f, "--resume", quit) as t:
        t.shown("Message the agent")
        t.quit("PRESS CTRL-C AGAIN TO END THIS SESSION")
    server.until(f, quit, lambda es: es[-1].get("closed"))
    # The person's own close settles its open question; the session unloads into Earlier.
    assert not json.loads(server.send(f, "GET", f"/sessions/{quit}/agent").read())["asks"], "the close left its question"
    server.wait(lambda: not (f.home / ".razor/live" / quit).exists() and quit in [
        row["session"] for row in json.loads(server.send(f, "GET", "/agents").read())["agents"]], 10, "it stayed loaded")
    # /exit leaves the terminal app as /detach does: the session stays open, to resume.
    left = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    with Terminal(f, "--resume", left) as t:
        t.shown("Message the agent")
        t.send("/exit\r")
        t.leaves("/exit did not leave the app")
    assert not any(e.get("closed") for e in f.record(left)), "/exit ended the session"
    closed = {s: list(f.record(s)) for s in (session, quit)}
    # After a restart, read unloaded as a phone does (its project), both rows read done and
    # their activity stays.
    server.stop(f)
    server.start(f)
    for s in closed:
        assert server.send(f, "GET", f"/sessions/{s}/project").status == 200, s
    assert not set(closed) & server.agents(f).keys(), "closed rows remained in the live feed"
    for s, entries in closed.items():
        row = json.loads(server.send(f, "GET", f"/sessions/{s}/agent").read())
        at = next(e["at"] for e in reversed(entries) if e.get("closed"))
        assert row["state"] == "done" and row["last_activity"] == at and not row["asks"], row
        assert list(f.record(s)) == entries, f"the restart wrote to closed session {s}"
    # Read again, the record says it closed, whatever the closing words were.
    assert server.send(f, "GET", f"/sessions/{session}/entries").status == 200, "the session did not load again"
    reopened = f.run("--print=json", "--resume", session, "Again.")
    assert reopened.returncode == 0 and json.loads(reopened.stdout)["message"] == "Reopened.", reopened
    assert not [e for e in f.record(session) if e["kind"] == "rebuild"], "the reopening rebuilt"
    second = f.run("--print=json", "Hello.")
    page = (f.requests / "008.body").read_text()
    assert second.returncode == 0 and "First closeout." in page, second
    other = json.loads(second.stdout)["session"]
    entries = server.until(f, other, answered(server.post(f, f"/sessions/{other}/done", {})["entry"], "done"))
    result = next(e for e in entries if e.get("call") == "toolu_next" and e["kind"] == "result")
    assert result["outcome"] == "success", result
    assert json.loads(result["output"])["id"] != first["id"], (first, result)
    f.ask("Read the project's state.")
    assert all(note in f.sent()[-1] for note in ("First closeout.", "Second closeout.")), "a closeout lost another session's state"
