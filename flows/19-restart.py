"""Restart recovers sessions, children, reports and jobs; unknown calls never run twice.
Closing sessions stay closed, expired turns stay held, and unread deliveries retain old owners.
The systemd unit preserves jobs; bridge cleanup survives SIGKILL with a wedged SDK."""
from contextlib import suppress
import json
import os
import signal
import subprocess
import time
import tomllib

from claude import reply
from driver import BINARY, REPLIES, Reply, flow
import server
from responses import PLAIN, reply as turn
from terminal import Terminal

# An unread delivery admits its owner even outside the restart's recent-record candidates.
with flow([turn("DELIVERY_19", "deliver", line="Read this later.", url="https://example.org/"), turn("DELIVERY_19", "Delivered.")],
          model="sol", config=PLAIN) as f:
    server.start(f)
    sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{sid}/input", {"text": "DELIVERY_19"})
    server.until(f, sid, lambda es: any(e["kind"] == "turn_end" for e in es))
    assert any(d["session"] == sid and not d["read"] for d in json.load(server.send(f, "GET", "/deliveries")))
    server.stop(f)
    path = f.home / ".razor/sessions" / sid / "record.jsonl"
    entries = list(f.record(sid))
    for entry in entries:
        entry["at"] -= 13 * 3_600_000
    path.write_text("".join(json.dumps(e) + "\n" for e in entries))
    old = time.time() - 13 * 3600
    os.utime(path, (old, old))
    server.start(f)
    server.wait(lambda: sid in server.agents(f), 5, "unread delivery's old owner missing after restart")

root, child = "FLOW_ROOT_19", "You are oak19,"
done = Reply((REPLIES / "done.sse").read_bytes(), match=root)
replies = [
    reply(("toolu_long", "Bash", {"command": "until [ -e release ]; do sleep 0.1; done",
                                  "run_in_background": True, "description": "Long job"}),
          ("toolu_start", "mcp__razor__agent", {"action": "start", "name": "oak19", "brief": "Count, then finish."}),
          match=root),
    reply(("toolu_wait", "mcp__razor__agent", {"action": "wait", "children": ["oak19"]}), match=root),
    reply(("toolu_wait_again", "mcp__razor__agent", {"action": "wait", "children": ["oak19"]}), match=root),
    done, done,
    # Its job, too, runs until the flow's release: ended on its own while a slow restart rebuilt
    # the child, its notice would join the child's turn and need a reply the flow does not have.
    reply(("toolu_count", "Bash", {"command": "printf x >> counted; until [ -e release ]; do sleep 0.1; done"}),
          match=child),
    reply(("toolu_finish", "mcp__razor__agent", {"action": "finish", "status": "finished", "report": "Counted once."}),
          match=child)]
# Razor dies after a command has done its work and before its result is recorded: on restart the
# call's outcome is unknown, its job ends unknown, and the command is never run again. The
# command's own last act kills the server, its parent, so no result can be recorded. It first
# waits for its job's start to be recorded, as any command that runs a moment does; on a busy
# machine the kill could otherwise land before it, leaving no job to end.
dies = "UNKNOWN_19"
command = ("printf 'ran\\n' >> runs.txt; until grep -qs '\"event\":\"started\"' "
           "\"$HOME/.razor/sessions/$RAZOR_SESSION/record.jsonl\"; do sleep 0.02; done; kill -KILL $PPID")
replies += [reply(("toolu_dies", "Bash", {"command": command}), match=dies),
            *[Reply((REPLIES / "done.sse").read_bytes(), match=dies)] * 2]


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def result(entries, call):
    return next(e for e in entries if e["kind"] == "result" and e["call"] == call)


with flow(replies) as f:
    pid = server.start(f)
    parent = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    ask = server.post(f, f"/sessions/{parent}/input", {"text": f"{root}: start the job and the child."})["entry"]
    server.until(f, parent, lambda es: any(e["kind"] == "call" and e["call"] == "toolu_wait" for e in es))
    sid = next(e["session"] for e in f.record(parent) if e["kind"] == "child")
    server.until(f, sid, lambda es: any(e["kind"] == "job" for e in es))
    job = next(e for e in f.record(parent) if e["kind"] == "job")
    killer = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    order = server.post(f, f"/sessions/{killer}/input", {"text": f"{dies}: run the command once."})["entry"]
    server.gone(pid)
    assert alive(job["pid"]), "the background job died with the server"
    before, early, unknown = list(f.record(parent)), list(f.record(sid)), list(f.record(killer))
    assert [e["kind"] for e in unknown if e["kind"] in ("call", "result")] == ["call"], unknown
    assert (f.cwd / "runs.txt").read_text() == "ran\n"
    server.start(f)
    entries = server.until(f, parent, lambda es: any(e["kind"] == "turn_end" and ask in e["inputs"] for e in es))
    assert entries[:len(before)] == before, "the restart rewrote the parent's record"
    assert len([e for e in entries if e["kind"] == "start"]) == 1, entries
    assert [e["trigger"] for e in entries if e["kind"] == "rebuild"] == ["restart"], entries
    assert result(entries, "toolu_wait")["outcome"] == "unknown", entries
    reports = [e for e in entries if e["kind"] == "input" and isinstance(e["from"], dict) and "report" in e["from"]]
    assert [e["text"] for e in reports] == ["Counted once."], reports
    taken = json.loads(result(entries, "toolu_wait_again")["output"])
    assert "report" in taken["from"] and taken["text"] == "Counted once.", taken
    later = list(f.record(sid))
    assert later[:len(early)] == early and [e["trigger"] for e in later if e["kind"] == "rebuild"] == ["restart"], later
    assert result(later, "toolu_count")["outcome"] == "unknown", later
    assert (f.cwd / "counted").read_text() == "x", "the child's command ran again, or not in its parent's folder"
    assert alive(job["pid"]) and not [e for e in entries if e["kind"] == "job" and e["event"] == "ended"], entries
    recovered = server.until(f, killer, lambda es: any(e["kind"] == "turn_end" and order in e["inputs"] for e in es))
    recovered = server.until(f, killer, lambda es: any(
        e["kind"] == "turn_end" and any(i["id"] in e["inputs"] for i in es if i.get("from") == "razor")
        for e in es))
    notices = [e for e in recovered if e.get("from") == "razor"]
    assert len(notices) == 1 and "toolu_dies" in notices[0]["text"], notices
    assert recovered[:len(unknown)] == unknown, "the restart rewrote the record"
    end = next(e for e in recovered if e["kind"] == "turn_end")
    assert end["end"] == "done", end
    calls = [e for e in recovered if e["kind"] == "call"]
    outputs = [e for e in recovered if e["kind"] == "result"]
    assert [e["call"] for e in calls] == ["toolu_dies"], calls
    assert len(outputs) == 1 and outputs[0]["call"] == "toolu_dies", outputs
    assert outputs[0]["outcome"] == "unknown" and outputs[0]["id"] > len(unknown), outputs
    jobs = [(e["event"], e.get("end")) for e in recovered if e["kind"] == "job"]
    assert jobs == [("started", None), ("ended", "unknown")], jobs
    rebuilds = [e for e in recovered if e["kind"] == "rebuild"]
    assert len(rebuilds) == 1 and rebuilds[0]["trigger"] == "restart", rebuilds
    assert (f.cwd / "runs.txt").read_text() == "ran\n", "the command ran again"
    fresh = json.loads([body for body in f.sent() if dies in body][1])
    assert "runs.txt" in json.dumps(fresh["messages"]), "the rebuilt context omits the call"
    assert "toolu_dies" not in json.dumps(fresh["messages"]), "the old call was replayed as a tool_use"
    (f.cwd / "release").touch()
    entries = server.until(f, parent, lambda es: any(
        e["kind"] == "turn_end" and any(i["id"] in e["inputs"] for i in es if i.get("from") == "razor")
        for e in es))
    ends = [e for e in entries if e["kind"] == "job" and e["event"] == "ended"]
    assert [e["job"] for e in ends] == [job["job"]] and not alive(job["pid"]), (ends, job)
    # Claude Code reports 1,000,000 for opus; with no row Razor fills 500,000 and hands over at 0.9.
    measured = {k: entries[-1][k] for k in ("context", "window", "threshold")}
    assert measured["context"] and (measured["window"], measured["threshold"]) == (500000, 0.9), measured
    # A wedged SDK child cannot cooperate with EOF or TERM. Its bridge must stay alive to
    # finish the SDK's KILL escalation even when the server dies without shutting down.
    pid = json.loads((f.home / ".razor/server.json").read_text())["pid"]
    parents = dict(map(int, line.split()) for line in subprocess.check_output(["ps", "-axo", "pid=,ppid="], text=True).splitlines())
    clis = [p for p, owner in parents.items() if parents.get(owner) == pid]
    assert clis, "the server had no SDK child to stop"
    try:
        for cli in clis:
            os.kill(cli, signal.SIGSTOP)
        server.stop(f, signal.SIGKILL)
    finally:
        for cli in clis:
            with suppress(ProcessLookupError):
                os.kill(cli, signal.SIGCONT)
    server.start(f)
    server.send(f, "GET", "/sessions").read()
    assert all(server.agents(f)[parent][k] == v for k, v in measured.items()), server.agents(f)[parent]
    with Terminal(f, "--resume", parent) as t:
        t.until(lambda _: (row := server.agents(f)[parent])["terminal"] and all(row[k] == v for k, v in measured.items()))


# An expired cache must leave both turns held, even when recovery appends a lost-job entry.
root, child = "LONG_GAP_19", "You are held19,"
replies = [turn(root, "bash", command="until [ -e release19 ]; do sleep 0.05; done", wait_ms=0),
           turn(root, "agent", action="start", name="held19", brief="Wait for continuation."),
           turn(root, "agent", action="wait", children=["held19"]),
           turn(child, "bash", command="until [ -e release19 ]; do sleep 0.05; done")]
with flow(replies, model="gpt-6.1-sol", config=PLAIN) as f:
    pid = server.start(f)
    parent = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    ask = server.post(f, f"/sessions/{parent}/input", {"text": root})["entry"]
    server.until(f, parent, lambda es: any(e["kind"] == "call" and e["input"].get("action") == "wait"
                                         for e in es))
    sid = next(e["session"] for e in f.record(parent) if e["kind"] == "child")
    early = server.until(f, sid, lambda es: any(e["kind"] == "job" for e in es))
    job = next(e for e in early if e["kind"] == "job")
    os.kill(pid, signal.SIGKILL)
    server.gone(pid)
    os.killpg(job["pid"], signal.SIGKILL)
    before = {}
    for session in (parent, sid):
        entries = list(f.record(session))
        for entry in entries:
            entry["at"] -= 2 * 3_600_000
        path = f.home / ".razor/sessions" / session / "record.jsonl"
        path.write_text("".join(json.dumps(e) + "\n" for e in entries))
        before[session] = entries
    requests = len(f.sent())
    server.start(f)
    for session in (parent, sid):
        entries = server.until(f, session, lambda es: any(e["kind"] == "turn_end" for e in es)
                               or len(f.sent()) != requests)
        assert len(f.sent()) == requests, "a long restart sent a model request"
        end = entries[-1]
        assert end["kind"] == "turn_end" and end["closed"] and end["inputs"] == [], end
        assert "2 hours" in end["end"]["error"], end
        assert entries[:len(before[session])] == before[session], "restart rewrote history"
        assert not [e for e in entries if e["kind"] == "rebuild"], entries
    assert any(e["kind"] == "job" and e["event"] == "ended" for e in f.record(sid))
    assert not [e for e in f.record(parent) if e["kind"] == "input" and e["id"] != ask], "the child woke its parent"

    rows = {}
    def marked():  # the status rows follow the record's closing entries
        rows.clear()
        rows.update(server.agents(f))
        return all(rows.get(s, {}).get("state") == "needs_you" and rows[s].get("interrupted") for s in (parent, sid)) and rows.copy()
    held = server.wait(marked, 10, rows)
    assert all((f.home / ".razor/live" / s).exists() for s in (parent, sid))
    (f.cwd / "release19").touch()
    server.until(f, parent, lambda es: any(e["kind"] == "job" and e["event"] == "ended" for e in es))
    time.sleep(0.25)
    assert len(f.sent()) == requests, "a job end woke an interrupted turn"
    counts = {s: len(list(f.record(s))) for s in (parent, sid)}
    server.stop(f, signal.SIGKILL)
    server.start(f)
    server.wait(marked, 10, rows)
    time.sleep(0.25)
    assert counts == {s: len(list(f.record(s))) for s in (parent, sid)}, "a held turn ended twice"
    assert len(f.sent()) == requests, "a second restart sent a model request"
    replies += [turn(root, "agent", action="continue", child="held19", brief="Continue the held work."),
                turn(root, "agent", action="wait", children=["held19"]),
                turn(root, "continued19"),
                turn(child, "agent", action="finish", status="finished", report="Held work complete.")]
    # The terminal command sends only to roots and restores their saved drafts after admission.
    saved = f.home / ".razor/sessions" / parent / "draft"
    saved.write_text("keep this draft")
    viewer = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    with Terminal(f, "--resume", viewer, columns=140, rows=45) as t:
        t.shown("Message the agent")
        t.send("/resume\r")
        t.shown("AGENTS ·")  # The agents list includes the held child under its root.
        reason = held[parent]["error"][:24]  # Cards clip long sentences to the pane.
        t.shown("NEEDS YOU", reason, "IDLE")
        lines = t.screen.display
        needs = next(y for y, line in enumerate(lines) if "NEEDS YOU" in line)
        why = next(y for y, line in enumerate(lines) if reason in line)
        idle = next(y for y, line in enumerate(lines) if "IDLE" in line)
        assert needs < why < idle, t.text()
        assert "▲" in lines[why - 1] and "ERROR" in lines[why - 1], t.text()  # A failed turn's mark.
        t.send("/continue all\r")
        entries = server.until(f, parent, lambda es: any(e["kind"] == "turn_end" and ask in e["inputs"] for e in es))
        continued = next(e["id"] for e in entries if e["kind"] == "input" and e.get("from") == "person" and e["id"] != ask)
        assert any(ask in e.get("inputs", []) and continued in e["inputs"] for e in entries if e["kind"] == "turn_end"), entries
        t.until(lambda _: saved.read_text() == "keep this draft")
    later = list(f.record(sid))
    held_input = next(e["id"] for e in before[sid] if e["kind"] == "input")
    assert any(held_input in e.get("inputs", []) for e in later if e["kind"] == "turn_end"), later
    assert not [e for e in later if e["kind"] == "input" and e.get("from") == "person"], "continue all sent to a child"
    row = server.agents(f)[parent]
    assert not row.get("interrupted") and not row.get("error"), "continuation did not clear the reason"
    rebuilt = next(e["context"] for e in entries if e["kind"] == "rebuild")
    assert "held19" in rebuilt and sid in rebuilt and "started" in rebuilt, rebuilt


# A server killed during `/done`'s closing turn still closes the session once it runs again.
with flow([turn(None, "bash", command="until [ -e go ]; do sleep 0.05; done", wait_ms=1000),
           turn(None, "Closed.")], model="gpt-6.1-sol", config=PLAIN) as f:
    pid = server.start(f)
    sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    done = server.post(f, f"/sessions/{sid}/done", {})["entry"]
    server.until(f, sid, lambda es: any(e["kind"] == "job" for e in es))
    os.kill(pid, signal.SIGKILL)
    server.gone(pid)
    server.start(f)
    end = lambda es: [e for e in es if e["kind"] == "turn_end" and done in e["inputs"]]
    assert end(server.until(f, sid, end))[0].get("closed"), "the crash lost the closing intent"
    server.until(f, sid, lambda es: json.load(server.send(f, "GET", f"/sessions/{sid}/agent"))["state"] == "done")


# The unit `razor install` writes, run by the user's systemd: its stop signals the server alone, so
# the bridge outlives it, the open turn stays open and resumes at the next start, and its job lives
# to be taken back. A stop of the whole control group killed the job and could end the turn on the
# bridge's exit (2026-11-03). macOS's launchd already signals only the server.
if subprocess.run("systemctl --user show-environment", shell=True, capture_output=True).returncode:
    print("19-restart: no systemd user manager here, so the service's stop was not checked")
else:
    with flow([reply(("toolu_svc", "Bash", {"command": "until [ -e release ]; do sleep 0.1; done"})), reply(text="Resumed."), reply(text="Noticed.")]) as f:
        unit = tomllib.loads((REPLIES.parents[1] / "src/residents/text.toml").read_text())["unit"].split("[Service]\n")[1].split("\n[")[0]
        kept = [f"--property={line}" for line in unit.splitlines() if not line.startswith(("ExecStart=", "Environment="))]
        subprocess.run(["systemd-run", "--user", "--quiet", "--collect", f"--unit=razor-flow-19-{os.getpid()}", f"--working-directory={f.cwd}", *kept, *(f"--setenv={k}={v}" for k, v in f.env.items()), BINARY, "serve"], check=True)
        subprocess.run(["timeout", "30", "sh", "-c", f"until [ -e '{f.home}/.razor/server.json' ]; do sleep 0.05; done"], check=True)
        sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        server.post(f, f"/sessions/{sid}/input", {"text": "Wait for the release."})
        job = next(e for e in server.until(f, sid, lambda es: any(e["kind"] == "job" for e in es)) if e["kind"] == "job")
        subprocess.run(["systemctl", "--user", "stop", f"razor-flow-19-{os.getpid()}"], check=True)
        assert alive(job["pid"]) and not [e for e in f.record(sid) if e["kind"] == "turn_end"], "the service's stop ended the turn or its job"
        server.start(f)
        server.until(f, sid, lambda es: [e["end"] for e in es if e["kind"] == "turn_end"] == ["done"])
        (f.cwd / "release").touch()
        server.until(f, sid, lambda es: len([e for e in es if e["kind"] == "turn_end"]) == 2)
