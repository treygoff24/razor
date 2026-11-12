"""A paired child keeps git on its owner and facts/cards on its parent: mail, wait, cap,
continuation, stale-report rejection, local descendants/channels, stop, settle and restart recovery.
The two real homes pair by code over iroh, through a stand-in that counts their requests; only the
model is a stand-in.
Completed foreign helpers leave the live feed; their parent-owned launch/report facts remain
readable through the parent's children detail, and settled facts through direct detail."""
import json
import os
import queue
import re
import shutil
import signal
import threading
import time
from dataclasses import replace
from driver import flow
from model import agent, message, reply, tool
from server import Tap, listed, send, start, stop, until, wait
from terminal import Terminal

root, child = "FLOW_REMOTE_49", "You are oak49,"
replies = []
with flow(replies, config='[children]\nmax = 1\ngrace = "0s"') as f:
    local, other = f.home / ".razor", f.home / ".razor-b"
    other.mkdir()
    for name in ("config.toml", "auth.json"):
        shutil.copy(local / name, other / name)
    beta = replace(f, cwd=f.cwd / "target", env={**f.env, "RAZOR_HOME": str(other)})
    beta.cwd.mkdir()
    f.base()
    beta.base()
    start(f)
    assert beta.run("devices").returncode == 0
    # Count what the owner is asked on the real peer connection, including ordinary status traffic.
    tap = Tap(f, beta, "b", "a")

    def request(home, path, body=None, *, peer=False, status=200, method="POST"):
        answer = send((tap.to_a if home == local else tap.to_b) if peer else home, method, path, body)
        raw = answer.readline() if path == "/status" else answer.read()
        assert answer.status == status, (path, answer.status, raw)
        return json.loads(raw) if raw else None

    def ended(entries):
        return any(e["kind"] == "turn_end" for e in entries)

    def reports(session):
        return [e for e in f.record(session) if e["kind"] == "input" and "report" in e["from"]]

    # A top-level agent starts independent sessions, with the note's actual sender on both homes.
    top = "START_TOP_49"
    replies += [agent(top, "start_top", cwd=".", brief="LOCAL_TOP_49"),
                agent(top, "start_top", cwd=str(beta.cwd), server="b", brief="REMOTE_TOP_49"),
                reply(top, text="Started both."), reply("LOCAL_TOP_49", text="Local top ran."),
                reply("REMOTE_TOP_49", text="Remote top ran.")]
    initiator = f.ask(top)["session"]
    entries = list(f.record(initiator))
    calls = [e for e in entries if e["kind"] == "call" and e["input"].get("action") == "start_top"]
    results = [json.loads(next(e["output"] for e in entries if e["kind"] == "result" and e["call"] == c["call"])) for c in calls]
    for result, home, text, origin, suffix in ((results[0], f, "LOCAL_TOP_49", initiator, ""),
                                                (results[1], beta, "REMOTE_TOP_49", initiator + "@a", "@b")):
        session = result["session"]
        entries = until(home, session.removesuffix(suffix), lambda es: any(e.get("text") == ("Remote top ran." if suffix else "Local top ran.") for e in es))
        assert not entries[0].get("child") and result["address"] == result["name"] + suffix and (session.endswith(suffix) if suffix else "@" not in session)
        first = next(e for e in entries if e["kind"] == "input")
        assert first["text"] == text and first["from"]["mail"]["id"][0] == origin, first
    assert not any(e["kind"] == "child" for e in f.record(initiator))
    # A remote task is read through its owner and its change travels on the paired status stream.
    remote_session = results[1]["session"].removesuffix("@b")
    wait(lambda: any(s.get("name") == "b" and not s.get("silent") for s in request(f, "/status", method="GET")["servers"]))
    task_stream = send(f, "GET", "/status")
    task_stream.readline()
    received = queue.Queue()
    def collect_tasks():
        for line in task_stream:
            event = json.loads(line)
            if event.get("kind") == "tasks": received.put(event)
    threading.Thread(target=collect_tasks, daemon=True).start()
    replies += [tool("REMOTE_TASK_49", "task", action="write", title="Remote task"), reply("REMOTE_TASK_49", text="Saved.")]
    beta.ask("--resume", remote_session, "REMOTE_TASK_49")
    task_list = send(f, "GET", f"/sessions/{remote_session}@b/tasks?filter=open")
    rows = json.loads(task_list.read())
    detail = send(f, "GET", f"/sessions/{remote_session}@b/tasks/1")
    detail_status, item = detail.status, json.loads(detail.read())
    try: event = received.get(timeout=2)
    except queue.Empty: event = None
    assert rows.get("tasks", [{}])[0].get("title") == "Remote task" and detail_status == 200 and item.get("title") == "Remote task" and event == {"kind": "tasks", "project": str(beta.cwd.resolve()), "server": "b"}, (rows, detail_status, item, event)
    # A paired helper claims and comments in its own project.
    replies += [tool("TASK_OWNER_49", "task", action="write", title="Remote work"),
                reply("TASK_OWNER_49")]
    assert beta.run("--print=json", "TASK_OWNER_49").returncode == 0
    # Shared files expand at the source; paths, worktrees and commits resolve at the target.
    (f.cwd / "brief49").write_text("SHARED_49")
    replies += [agent(root, "start", server="b", name="oak49", cwd=str(beta.cwd), shared_brief="brief49", brief="Commit.", effort="medium"),
                agent(root, "start", server="b", name="over49", cwd=str(beta.cwd), brief="Over cap."),
                message(root, "note", to="oak49", text="PARENT_NOTE_49"), agent(root, "wait"), agent(root, "wait"), tool(root, "status"), reply(root, text="First report."),
                agent(child, "start_top", cwd=str(beta.cwd), brief="Refuse child."),
                reply(child, "Bash", {"command": "sleep 1; printf marker > marker49; git add -- marker49 && git -c user.name=Flow -c user.email=flow@example.invalid commit -m Marker"}),
                message(child, "note", text="CHILD_NOTE_49"),
                reply(child, "Bash", {"command": "sleep 60", "run_in_background": True}),
                tool(child, "task", action="write", id=2, claim=True, comment="Claim evidence."),
                tool(child, "task", action="write", id=2, claim=False),
                tool(child, "task", action="write", id=2, comment="Remote evidence."),
                agent(child, "finish", status="finished", report="REMOTE_REPORT_49")]
    parent = f.ask(root)["session"]
    records = until(f, parent, lambda es: any(e.get("text") == "First report." for e in es))
    started = next(e for e in records if e["kind"] == "child" and isinstance(e["event"], dict))
    qualified = started["session"]
    sid = qualified.split("@")[0]
    assert qualified.endswith("@b") and started["event"]["started"]["worktree"] is None
    there = list(beta.record(sid))
    assert any(e["kind"] == "result" and e["outcome"] == "error" and "Only a top-level agent" in e["output"] for e in there)
    calls = {e["call"] for e in there if e["kind"] == "call" and e["name"] == "mcp__razor__task"}
    writes = [e for e in there if e["kind"] == "result" and e["call"] in calls]
    assert [e["outcome"] for e in writes] == ["success", "success", "success"], writes
    task = next((other / "tasks").rglob("2.md"))
    assert "title: Remote work\n" in task.read_text() and "claim:" not in task.read_text() and sorted(p.read_text().split("---\n")[-1].strip() for p in task.parent.glob("comments/2/*.md")) == ["Claim evidence.", "Remote evidence."]
    assert there[0]["child"]["parent"] == parent + "@a", there[0]
    assert there[0]["child"]["worktree"]["origin"] == str(beta.cwd.resolve())
    first_input = next(e for e in there if e["kind"] == "input")
    assert first_input["child_task"] == started["id"] and "SHARED_49" in first_input["text"]
    assert any(e["kind"] == "result" and e["outcome"] == "error" and "children.max" in e["output"] for e in records)
    until(f, parent, lambda es: len(reports(parent)) == 1)
    old = next(e for e in beta.record(sid) if "report" in e)
    saved = old["report"]
    # The report is written with its turn's end, after the calls' results.
    assert old["kind"] == "turn_end" and old["id"] > max(e["id"] for e in there if e["kind"] == "result")
    assert any(e["kind"] == "job" and e["event"] == "ended" and e["id"] > old["id"] for e in beta.record(sid)), \
        "the report was not saved before closing the child's background job"
    assert any(e["kind"] == "input" and e["text"] == "PARENT_NOTE_49" and e["from"]["mail"]["id"][0] == parent + "@a" for e in there)
    assert any(e["kind"] == "input" and e["text"] == "CHILD_NOTE_49" and e["from"]["mail"]["id"][0] == qualified for e in records)
    row = wait(lambda: next((r for r in request(local, f"/sessions/{parent}/agent", method="GET")["children"] if r["session"] == qualified), None))
    assert row["parent"] == parent and row["launched"] == started["event"]["started"]["launched"]
    assert row["report"]["entry"] == reports(parent)[0]["id"] and row["state"] == "done" and row["changed_file_count"] == 0, row
    calls = {e["call"] for e in records if e["kind"] == "call" and e["name"].endswith("__status")}
    board = next(json.loads(e["output"])["agents"] for e in records if e["kind"] == "result" and e["call"] in calls)
    assert tap.detail.count(sid) == 1, tap.detail
    assert next(r for r in board if r["session"] == qualified)["changed_files"] == ["marker49"], board

    # A paired child's OSC id must fit the terminal protocol and survive reattachment.
    with Terminal(f, "--resume", parent, status=True) as t:
        t.until(lambda t: any(r.get("title") == "oak49" for r in t.records().values()))
        osc_id = next(k for k, r in t.records().items() if r.get("title") == "oak49")
        assert re.fullmatch(r"[A-Za-z0-9_.+-]{1,32}", osc_id), osc_id
    with Terminal(f, "--resume", parent, status=True) as t:
        t.until(lambda t: osc_id in t.records())
        assert t.records()[osc_id]["title"] == "oak49", t.records()

    # A continue fails while the child's server is down; the next one, once it is back, reaches it.
    stop(other)
    replies += [agent(root, "continue", child="oak49", brief="SECOND_TASK_49"), tool(root, "status"), reply(root, text="Not sent.")]
    assert f.run("--resume", parent, "--print=json", "Continue.").returncode == 0
    results = [e for e in f.record(parent) if e["kind"] == "result"]
    assert "it may have run" in results[-2]["output"]
    assert next(r for r in json.loads(results[-1]["output"])["agents"] if r["session"] == qualified)["changed_files"] is None
    assert beta.run("devices").returncode == 0
    # Hold the new task, then submit a real saved report from its older task through peer auth.
    release = beta.cwd / "release49"
    replies += [agent(root, "continue", child="oak49", brief="SECOND_TASK_49"), reply(root, text="Continued."), reply(root, text="Second report heard."),
                reply(child, "Bash", {"command": f"while [ ! -e {release} ]; do sleep 0.05; done"}),
                agent(child, "finish", status="partial", report="SECOND_REPORT_49")]
    assert f.run("--resume", parent, "--print=json", "Continue.").returncode == 0
    until(beta, sid, lambda es: any(e.get("text") == "SECOND_TASK_49" for e in es))
    request(local, "/children", {"action": "update", "parent": parent, "child": sid, "update": {"report": saved}}, peer=True)
    assert [e["text"] for e in reports(parent)] == ["REMOTE_REPORT_49"], "a stale report ended the new task"
    release.touch()
    until(f, parent, lambda es: len(reports(parent)) == 2)
    assert reports(parent)[-1]["text"] == "SECOND_REPORT_49"
    until(f, parent, lambda es: any(e.get("text") == "Second report heard." for e in es))
    request(local, "/children", {"action": "update", "parent": parent, "child": sid, "update": {"report": saved}}, peer=True)
    assert len(reports(parent)) == 2, "a duplicate report woke the parent"
    # A delayed continue of a task the child has moved past is refused, never run again.
    request(other, "/children", {"action": "continue", "session": sid, "input": first_input}, peer=True, status=400)

    # A person's continuation can exceed the parent's cap, as it can for a local child.
    replies += [agent(root, "start", server="b", name="hold49", cwd=str(beta.cwd), placement="in_place", brief="Hold the slot."),
                reply(root, text="Slot held."), reply("You are hold49,", "Bash", {"command": "sleep 60"})]
    assert f.run("--resume", parent, "--print=json", "Hold a slot.").returncode == 0
    hold = next(e["session"].split("@")[0] for e in f.record(parent) if e["kind"] == "child" and isinstance(e["event"], dict)
                and e["event"].get("started", {}).get("name") == "hold49")
    until(beta, hold, lambda es: any(e["kind"] == "call" and e["input"].get("command") == "sleep 60" for e in es))
    row = wait(lambda: next((r for r in request(local, "/status", method="GET")["agents"] if r["session"] == hold + "@b"), None))
    assert row["parent"] == parent and row["name"] == "hold49", row
    # The person reopens the remote child through its owning server, using the parent's fact id.
    replies += [agent(child, "start", server="a", name="third49", brief="Refuse the extra hop."),
                message(child, "note", to="channel", text="TARGET_CHANNEL_49"),
                agent(child, "start", name="local49", placement="in_place", brief="Local descendant."),
                agent(child, "wait"), agent(child, "settle", child="local49", decision="discard"),
                agent(child, "finish", status="finished", report="PERSON_REPORT_49"),
                agent("You are local49,", "finish", status="finished", report="LOCAL_49"), reply(root, text="Heard.")]
    request(other, f"/sessions/{sid}/input", {"text": "PERSON_TASK_49"})
    until(f, parent, lambda es: len(reports(parent)) == 3)
    assert reports(parent)[-1]["text"] == "PERSON_REPORT_49"
    until(f, parent, lambda es: any(e.get("text") == "Heard." for e in es))
    assert any(e["kind"] == "result" and e["outcome"] == "error" and "subtree" in e["output"] for e in beta.record(sid))
    assert not list((local / "channels").glob("*.jsonl")), "the channel crossed the machine boundary"
    assert "TARGET_CHANNEL_49" in (other / "channels" / f"{sid}.jsonl").read_text()
    continued = [e for e in f.record(parent) if e["kind"] == "child" and e["event"] == "continued"]
    person = next(e for e in beta.record(sid) if e["kind"] == "input" and e["text"] == "PERSON_TASK_49")
    assert person["from"] == "person" and person["child_task"] == continued[-1]["id"]
    replies += [agent(root, "stop", child="hold49"), agent(root, "settle", child="hold49", decision="discard"), reply(root, text="Slot free.")]
    assert f.run("--resume", parent, "--print=json", "Free the slot.").returncode == 0

    # Stop closes remote jobs; another continuation finishes while the parent's server is down.
    replies += [agent(root, "continue", child="oak49", brief="STOP_TASK_49"), reply(root, text="Running."),
                reply(child, "Bash", {"command": "sleep 60 # STOP_TASK_49"})]
    assert f.run("--resume", parent, "--print=json", "Run.").returncode == 0
    until(beta, sid, lambda es: any(e["kind"] == "call" and e["input"].get("command") == "sleep 60 # STOP_TASK_49" for e in es))
    replies += [agent(root, "stop", child="oak49"), reply(root, text="Stopped.")]
    assert f.run("--resume", parent, "--print=json", "Stop.").returncode == 0
    assert any(e["kind"] == "child" and e["event"] == "stopped" for e in f.record(parent))
    assert not listed(local, qualified), "a stopped helper stayed in its parent’s agents list"
    go = beta.cwd / "recover49"
    replies += [agent(root, "continue", child="oak49", brief="RECOVER_TASK_49"), reply(root, text="Recovering."),
                reply("RECOVER_TASK_49", "Bash", {"command": f"while [ ! -e {go} ]; do sleep 0.05; done"}),
                agent("RECOVER_TASK_49", "finish", status="partial", report="RECOVER_REPORT_49")]
    assert f.run("--resume", parent, "--print=json", "Recover.").returncode == 0
    until(beta, sid, lambda es: any(e["kind"] == "input" and e["text"] == "RECOVER_TASK_49" for e in es))
    stop(local)
    assert (local / "live" / parent).exists(), "a parent with an outstanding remote task lost its live marker"
    go.touch()
    until(beta, sid, lambda es: sum("report" in e for e in es) == 4)
    assert len(reports(parent)) == 3
    replies += [reply(root, text="Recovered.")]
    start(f)
    until(f, parent, lambda es: len(reports(parent)) == 4)
    assert reports(parent)[-1]["text"] == "RECOVER_REPORT_49"
    until(f, parent, lambda es: any(e.get("text") == "Recovered." for e in es))
    request(local, "/children", {"action": "update", "parent": parent, "child": sid,
            "update": {"report": next(e["report"] for e in reversed(list(beta.record(sid))) if "report" in e)}}, peer=True)
    assert len(reports(parent)) == 4, "push and recovery each admitted the report"
    replies += [agent(root, "settle", child="oak49", decision="merge"), reply(root, text="Merged.")]
    with Terminal(f, "--resume", parent, status=True) as t:
        t.until(lambda t: osc_id in t.records())
        assert f.run("--resume", parent, "--print=json", "Merge.").returncode == 0
        t.until(lambda t: osc_id not in t.records())
    assert (beta.cwd / "marker49").read_text() == "marker" and not (f.cwd / "marker49").exists()
    assert not (other / "sessions" / sid / "worktree").exists()
    row = request(local, f"/sessions/{qualified}/agent", method="GET")
    assert row["settled"] == "merge", row
    assert not listed(other, sid), "a remotely settled helper stayed in its owner’s agents list"
    reads = tap.detail.count(sid)
    with Terminal(f, "--resume", parent) as t:
        t.shown("Merged.")
        t.send("\x1b[1;5H")  # Ctrl-Home brings the original launch card into view.
        t.until(lambda t: any("oak49" in line and "Opus" in line and "medium" in line for line in t.screen.display))
        t.send("\x1b[1;5F\x1b[1;5H")  # Drawing the card again retains the owner's facts.
        t.until(lambda t: any("oak49" in line and "Opus" in line and "medium" in line for line in t.screen.display))
    assert tap.detail.count(sid) == reads + 1, tap.detail
    request(local, "/children", {"action": "report", "session": sid}, status=400)
    request(other, "/children", {"action": "report", "session": qualified}, peer=True, status=400)
    # An idle child has no report; unrelated peer activity must not cause recovery per line.
    replies += [agent(root, "start", server="b", name="idle49", cwd=str(beta.cwd), placement="in_place", brief="Ask."),
                agent(root, "wait"), reply(root, text="Idle held."),
                message("You are idle49,", "question", text="IDLE_QUESTION_49?"), reply("You are idle49,", text="Waiting.")]
    assert f.run("--resume", parent, "--print=json", "Hold idle.").returncode == 0
    idle = next(e["session"].split("@")[0] for e in f.record(parent) if e["kind"] == "child" and isinstance(e["event"], dict)
                and e["event"].get("started", {}).get("name") == "idle49")
    until(beta, idle, ended)
    before = len(tap.report)
    replies += [reply("BUSY_49", "Bash", {"command": "true"}) for _ in range(20)] + [reply("BUSY_49", text="Busy done.")]
    assert beta.run("--print=json", "BUSY_49").returncode == 0
    assert len(tap.report) - before <= 2, "ordinary peer updates repeatedly recovered an idle child"
    replies += [agent(root, "stop", child="idle49"), agent(root, "settle", child="idle49", decision="discard"), reply(root, text="Idle released.")]
    assert f.run("--resume", parent, "--print=json", "Release idle.").returncode == 0
    # Finishing and settling a local ancestor closes and retires its foreign leaf as well.
    leaf_started = beta.cwd / "leaf-started49"
    replies += [agent(root, "start", name="ancestor49", placement="in_place", brief="Close the remote leaf."),
                agent(root, "wait"), agent(root, "settle", child="ancestor49", decision="discard"), reply(root, text="Ancestor settled."),
                agent("You are ancestor49,", "start", server="b", name="leaf49", cwd=str(beta.cwd), brief="Wait for closure."),
                reply("You are ancestor49,", "Bash", {"command": f"while [ ! -e {leaf_started} ]; do sleep 0.05; done"}),
                agent("You are ancestor49,", "finish", status="finished", report="Ancestor finished."),
                reply("You are leaf49,", "Bash", {"command": f"touch {leaf_started}; sleep 60"})]
    f.ask("--resume", parent, "Close the ancestor.")
    ancestor = next(e["session"] for e in f.record(parent) if e["kind"] == "child" and isinstance(e["event"], dict)
                    and e["event"].get("started", {}).get("name") == "ancestor49")
    assert any(e["kind"] == "child" and e["event"] == "stopped" and e["session"].endswith("@b") for e in f.record(ancestor))
    assert not (local / "live" / ancestor).exists(), "the retired ancestor kept an outstanding remote fact"
    # A local child's report still reaches its parent when its remote descendant is down. The
    # parent's wait outlasts the outage steps below, however slow, so the report lands in it.
    ready, release = beta.cwd / "lost-ready49", beta.cwd / "lost-release49"
    replies += [agent(root, "start", name="outage49", placement="in_place", brief="Report during an outage."),
                agent(root, "wait", timeout_ms=50_000), reply(root, text="Outage report heard."),
                agent("You are outage49,", "start", server="b", name="lost49", cwd=str(beta.cwd), brief="Hold."),
                reply("You are outage49,", "Bash", {"command": f"while [ ! -e {release} ]; do sleep 0.05; done"}),
                agent("You are outage49,", "finish", status="finished", report="OUTAGE_REPORT_49"),
                reply("You are lost49,", "Bash", {"command": f"touch {ready}; while [ ! -e {release} ]; do sleep 0.05; done"})]
    result = {}
    worker = threading.Thread(target=lambda: result.setdefault("ran", f.run("--resume", parent, "--print=json", "Report during the outage.")))
    worker.start()
    wait(ready.exists)
    # A read leaves a connection open. The peer then goes silent on it and refuses fresh
    # ones: mail and child mutations must say nothing was sent.
    request(local, "/search?q=warm49", method="GET")
    tap.silence()
    replies += [agent("SILENT_49", "start", server="b", name="unsent49", cwd=str(beta.cwd), brief="Hold."),
                message("SILENT_49", "note", to=f"{sid}@b", text="UNSENT_49"), reply("SILENT_49")]
    began = time.monotonic()
    results = [e for e in f.record(f.ask("SILENT_49")["session"]) if e["kind"] == "result"]
    assert len(results) == 2 and all(e["outcome"] == "error" and ("nothing was sent" in e["output"] or "was not sent" in e["output"]) for e in results), results
    assert time.monotonic() - began < 4, "silent pooled connections delayed a mutation"
    assert not any(e.get("text") == "UNSENT_49" for e in beta.record(sid)), "refused mail arrived"
    stop(other)
    release.touch()
    worker.join(timeout=60)
    assert not worker.is_alive() and result["ran"].returncode == 0
    assert any("OUTAGE_REPORT_49" in e["text"] for e in reports(parent)), "the descendant outage lost its local parent's report"

# Claim, settle, and retry all use the owning server's project and its real tasks remote.
replies = []
with flow(replies) as f:
    f.base()
    beta = f.home_at("claims49")
    other = beta.home / ".razor"
    f.git("config", "user.name", "Flow")
    f.git("config", "user.email", "flow@example.invalid")
    remote = f.home / "tasks.git"
    beta.git("init", "--bare", str(remote))
    beta.git("remote", "add", "origin", str(remote))
    (beta.cwd / ".razor").mkdir()
    (beta.cwd / ".razor/config.local.toml").write_text('[tasks]\nremote = "origin"\n')
    f.git("remote", "add", "origin", str(remote))
    (f.cwd / ".razor").mkdir(exist_ok=True)
    (f.cwd / ".razor/config.local.toml").write_text('[tasks]\nremote = "origin"\n')
    start(f)
    beta.run("devices")
    tap = Tap(f, beta, "b", "a")
    replies += [tool("OWNER_49", "task", action="write", title=t) for t in ("Assigned", "Extra", "Assigned", "Extra")] + [reply("OWNER_49")]
    beta.ask("OWNER_49")
    for decision, named, extra in (("merge", 1, 2), ("discard", 3, 4)):
        tag, name = "CLAIM_49_" + decision, "claim49-" + decision
        full = f"Claim report {decision}.\nAll evidence is retained."
        replies += [tool(tag, "task", action="write", id=named, claim=True),
                    agent(tag, "start", server="b", name=name, cwd=str(beta.cwd), task=named, brief="Work."),
                    agent(tag, "wait"), reply(tag),
                    tool("You are " + name + ",", "task", action="write", id=extra, claim=True),
                    reply("You are " + name + ",", "Bash", {"command": "touch claim-marker; git add claim-marker; git -c user.name=Flow -c user.email=flow@example.invalid commit --allow-empty -m Claim"}),
                    agent("You are " + name + ",", "finish", status="finished", report=full)]
        parent = f.ask(tag)["session"]
        result = json.loads([e["output"] for e in f.record(parent) if e["kind"] == "result"][1])
        sid = result["session"].split("@")[0]
        if decision == "merge":
            for closer in ("parent\nclosed: injected", "parent\rclosed: injected"):
                refused = send(tap.to_b, "POST", "/children", {"action": "settle", "session": sid, "decision": "merge", "report": [full, named, closer]})
                raw = refused.read().decode()
                assert refused.status == 400 and "one line" in raw, (refused.status, raw)
        folder = next((other / "tasks").rglob(f"{named}.md")).parent
        claim = (folder / f"{named}.md").read_text()
        assert result.get("task") == {"id": named, "title": "Assigned", "claimed": True} and sid in claim, (result, claim)
        # Make the tasks remote unreachable after the child has finished; the git merge still runs.
        if decision == "merge":
            remote.rename(remote.with_name("tasks-away.git"))
        replies += [agent(tag, "settle", child=name, decision=decision), reply(tag)]
        f.ask("--resume", parent, tag)
        settled = [e for e in f.record(parent) if e["kind"] == "result"][-1]
        if decision == "merge":
            path = other / "sessions" / sid / "worktree"
            events = [e["event"] for e in f.record(parent) if e["kind"] == "child"]
            assert (settled["outcome"] == "error" and "Merged; the task step failed:" in settled["output"] and
                    "settle again" in settled["output"] and path.exists() and sid in (folder / f"{named}.md").read_text() and
                    not any(isinstance(e, dict) and "settled" in e for e in events)), (settled, events)
            remote.with_name("tasks-away.git").rename(remote)
            # The owner finishes, but the parent never receives that answer: its retry must work.
            completed = send(tap.to_b, "POST", "/children", {"action": "settle", "session": sid, "decision": decision, "report": [full, named, f"{parent} {(f.home / '.razor/server.id').read_text().strip()} parent on flow"]})
            assert completed.status == 200, completed.read()
            completed.read()
            replies += [agent(tag, "settle", child=name, decision=decision), reply(tag)]
            f.ask("--resume", parent, tag)
            retried = [e for e in f.record(parent) if e["kind"] == "result"][-1]
            assert retried["outcome"] == "success", retried
            assert (folder / f"closed/{named}.md").exists(), "merged task stayed open"
            closed = (folder / f"closed/{named}.md").read_text()
            assert "claim:" not in closed
            assert f"closed-by: {parent} {(f.home / '.razor/server.id').read_text().strip()} " in closed, closed
            assert full in "".join(p.read_text() for p in folder.glob(f"comments/{named}/*.md"))
        else:
            assert (folder / f"{named}.md").exists() and "claim:" not in (folder / f"{named}.md").read_text()
        assert "claim:" not in (folder / f"{extra}.md").read_text()
        comments = [p.read_text().split("---\n")[-1].strip() for p in folder.glob(f"comments/{extra}/*.md")]
        assert full in comments, comments
        assert not (other / "sessions" / sid / "worktree").exists()
