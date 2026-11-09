"""On-demand handover uses the same rebuild and preserves the complete old ask."""
import base64
import json
import os
from pathlib import Path
import signal
import time
from apns import apple
from claude import reply
from clock import wall_clock
from driver import REPLIES, Reply, flow
from mcp import PNG
from responses import PLAIN, call, done, response
import server

# Skills are current text once per rebuilt context on Claude, Responses plain and code mode.
from terminal import Terminal
from itertools import count
for model, code_mode in (("opus", False), ("sol", False), ("sol", True)):
    ids18 = count()
    def skill_step(*calls, text="skills18 ready"):
        if model == "opus":
            return reply(*[(None, "mcp__razor__" + name, args) for name, args in calls], text=text)
        if code_mode and calls:
            code = "\n".join(f"text(await tools.{name}({json.dumps(args)}));" for name, args in calls)
            return response([{"type": "custom_tool_call", "id": "ct_skills18", "call_id": "skills18_" + str(next(ids18)), "name": "exec", "input": code}])
        return response([call(name, json.dumps(args), "skills18_" + str(next(ids18))) for name, args in calls]) if calls else done()

    note18 = "Continue checking restored skills."
    replies18 = [skill_step(("skill", {"name": "kept18"}), ("skill", {"name": "gone18"})), skill_step(), skill_step(),
                 skill_step(("handover", {"note": note18})), skill_step(),
                 skill_step(("handover", {"note": note18})), skill_step()]
    if model == "sol" and not code_mode:
        replies18 += [response([call("read", json.dumps({"path": "tiny18"}), f"history18_{i}") for i in range(140)]), done()]
    with flow(replies18, model=model, config="" if code_mode else PLAIN) as f:
        root = f.cwd / ".agents/skills"
        for name in ("kept18", "gone18", "handed18", "broken18"):
            (root / name).mkdir(parents=True)
            (root / name / "SKILL.md").write_text(f"---\ndescription: Flow skill.\n---\n{name} original instructions\n")
        server.start(f)
        sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        path = f"/sessions/{sid}"
        server.post(f, path + "/input", {"text": "Check skills18.", "skill": "handed18"})
        server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end")
        handed18 = server.post(f, path + "/input", {"text": "Check broken18.", "skill": "broken18"})["entry"]
        server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end" and handed18 in es[-1]["inputs"])
        (root / "broken18/SKILL.md").write_bytes(b"\xff\xfe")
        (root / "kept18/SKILL.md").write_text("kept18 updated instructions")
        (root / "gone18").rename(root / "gone18-away")
        (root / "handed18").rename(root / "handed18-away")
        failures18 = []
        for turn18 in (1, 2):
            asked18 = server.post(f, path + "/handover", {})["entry"]
            entries18 = server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end" and asked18 in es[-1]["inputs"])
            rebuild18 = [e for e in entries18 if e["kind"] == "rebuild"][-1]
            context18 = rebuild18["context"]
            request18 = str(json.loads(f.sent()[-1]))
            row18 = json.loads(server.send(f, "GET", path + "/agent").read())
            states18 = {s["name"]: s for s in row18.get("skills", [])}
            checks18 = {
                "current disk skill in rebuild and provider request": all(context18.count(t) == 1 and t in request18 for t in ('<skill name="kept18">', 'kept18 updated instructions')),
                "handed fallback appears once": context18.count('<skill name="handed18">') == 1 and "handed18 original instructions" in context18 and '<skill name="handed18">' in request18 and context18.index('<skill name="handed18">') < context18.index("## The latest asks") and any(s["name"] == "handed18" and s["how"] == "handed" for s in rebuild18.get("skills", [])),
                "restore sources recorded": {s["name"]: s["outcome"] for s in rebuild18.get("skills", []) if s["name"] in ("kept18", "handed18")} == {"kept18": {"restored": {"from": "disk"}}, "handed18": {"restored": {"from": "record"}}},
                "skill spacing in captured request": note18 + '\n\n<skill' in context18 and '</skill>\n\n<skill' in context18 and json.dumps(context18)[1:-1] in f.sent()[-1],
                "feed retains worn for one rebuild": list(states18) == (["handed18", "kept18", "gone18", "broken18"] if turn18 == 1 else ["handed18", "kept18"]) and states18["kept18"]["worn"] is None and states18["handed18"]["worn"] is None and (states18["gone18"]["worn"] == rebuild18["at"] if turn18 == 1 else "gone18" not in states18),
                "unreadable handed file never falls back": '<skill name="broken18">' not in context18 and (states18.get("broken18", {}).get("worn") == rebuild18["at"] and any(s["name"] == "broken18" and isinstance(s["outcome"], dict) and "missing" in s["outcome"] and str(root / "broken18/SKILL.md") in s["outcome"]["missing"] for s in rebuild18.get("skills", [])) if turn18 == 1 else "broken18" not in states18 and all(s["name"] != "broken18" for s in rebuild18.get("skills", []))),
                "missing never restores or retries": '<skill name="gone18">' not in context18 and ("There is no skill named `gone18`" in context18 if turn18 == 1 else all(s["name"] != "gone18" for s in rebuild18.get("skills", []))),
            }
            failures18.extend(f"{model} code={code_mode} rebuild={turn18}: {name}" for name, passed in checks18.items() if not passed)
        assert not failures18, failures18
        if model == "sol" and not code_mode:
            (f.cwd / "tiny18").write_text("tiny")
            f.ask("--resume", sid, "Fill the recent history so the skill source needs paging.")
        with Terminal(f, "--resume", sid, rows=40, columns=120) as t:
            t.until(lambda _: "◉" in t.text() and "○ gone18" not in t.text())
            t.shown("kept18", "handed18")
            assert "worn off" not in next(line for line in t.text().splitlines() if "kept18" in line), t.text()
            t.send("\x1b2")
            t.until(lambda _: any("kept18" in line and "in context" in line for line in t.text().splitlines()))

# Each large skill uses its own spill file; the total keeps the newest skills and never retries the rest.
large18 = {f"large18-{i}": f"begin {i}\n" + "é" * 31_000 + f"\nend {i}" for i in range(5)}
with flow([response([call("skill", json.dumps({"name": name}), f"large18_{i}") for i, name in enumerate(large18)]), done(),
           response([call("skill", json.dumps({"name": "large18-0"}), "large18_reload")]), done(),
           response([call("handover", json.dumps({"note": "large skills"}), "large18_note")]), done()], model="sol", config=PLAIN) as f:
    for name, text in large18.items():
        where18 = f.cwd / ".agents/skills" / name
        where18.mkdir(parents=True)
        (where18 / "SKILL.md").write_text(text)
    sid = f.ask("Check large18 skill bounds.")["session"]
    f.ask("--resume", sid, "Reload large18-0 so the cap keeps its latest load.")
    (f.cwd / ".agents/skills/large18-1/SKILL.md").write_bytes(b"\xff\xfe")  # over cap: never read
    asked18 = server.post(f, f"/sessions/{sid}/handover", {})["entry"]
    entries18 = server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end" and asked18 in es[-1]["inputs"])
    rebuild18 = next(e for e in entries18 if e["kind"] == "rebuild")
    outcomes18 = {s["name"]: s["outcome"] for s in rebuild18.get("skills", [])}
    assert list(outcomes18.values()) == [{"restored": {"from": "disk"}}, "over_cap", "over_cap"] + [{"restored": {"from": "disk"}}] * 2, outcomes18
    assert len(rebuild18["context"]) < 4 * 30_000 + 4_000 and "skill text limit" in rebuild18["context"], len(rebuild18["context"])
    kept18 = ["large18-0", "large18-3", "large18-4"]
    for name in kept18:
        spilled18 = Path(f.env["RAZOR_HOME"]) / "sessions" / sid / "spill" / str(rebuild18["id"]) / name
        assert str(spilled18) in rebuild18["context"] and spilled18.read_text() == large18[name], spilled18
    assert [s["worn"] is not None for s in server.agents(f)[sid]["skills"]] == [False, True, True, False, False]
    assert sorted(p.name for p in spilled18.parent.iterdir()) == kept18, list(spilled18.parent.iterdir())

ask = "Keep this ask verbatim: the silver kingfisher visits at 06:43."
first_note = "Preserve the user's exact ask; recall entry 2 next."
handover = reply(("toolu_changed", "Bash", {"command": "printf kingfisher > kingfisher.txt"}),
                 ("toolu_handover", "mcp__razor__handover", {"note": first_note}))
with flow([handover] + [Reply((REPLIES / f"{name}.sse").read_bytes()) for name in ("recall", "done")]) as f:
    f.base()
    answer = f.ask(ask)
    assert answer["message"] == "flow done", answer
    entries = list(f.record(answer["session"]))
    assert entries[1]["kind"] == "input" and entries[1]["text"] == ask, entries[1]
    rebuilds = [e for e in entries if e["kind"] == "rebuild"]
    assert len(rebuilds) == 1 and rebuilds[0]["trigger"] == "handover", rebuilds
    assert ask in rebuilds[0]["context"], rebuilds[0]
    assert "?? kingfisher.txt" in rebuilds[0]["context"], rebuilds[0]["context"]
    assert first_note + "\n\n## Files changed on disk" in rebuilds[0]["context"], rebuilds[0]["context"]
    assert rebuilds[0]["note"] == first_note, rebuilds[0]
    calls = [e for e in entries if e["kind"] == "call"]
    assert [e["call"] for e in calls] == ["toolu_changed", "toolu_handover", "toolu_recall"], calls
    outputs = [e for e in entries if e["kind"] == "result"]
    assert [e["call"] for e in outputs] == [e["call"] for e in calls], outputs
    assert calls[1]["id"] < rebuilds[0]["id"] < calls[2]["id"], entries
    assert json.loads(outputs[2]["output"]) == entries[1], outputs[2]
    conversations = {e["native"]["claude"]["conversation"] for e in entries if e.get("native")}
    assert len(conversations) == 2, conversations
    assert len([e for e in entries if e["kind"] == "start"]) == 1, entries
    fresh = f.request(2)
    assert ask in json.dumps(fresh["messages"]), fresh
    assert rebuilds[0]["note"].strip() in json.dumps(fresh["messages"]), fresh

# While a turn waits on news, the person's message ends the wait at once with null and joins
# after that Result; so does the person's /handover, sent as the command it is, and a second is
# refused {}. On GPT, the threshold that the waiting response itself reaches ends that wait too.
wait = {"action": "wait", "timeout_ms": 30000}
wait_note = "Hand over from the wait."
replies = [reply(("toolu_wait", "mcp__razor__agent", wait)), reply(("toolu_again", "mcp__razor__agent", wait)),
           reply(("toolu_wait_note", "mcp__razor__handover", {"note": wait_note})), reply(text="Rebuilt.")]
with flow(replies) as f:
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    path = f"/sessions/{session}"
    server.post(f, path + "/input", {"text": "Wait for news."})
    server.until(f, session, lambda es: any(e.get("call") == "toolu_wait" for e in es))
    time.sleep(0.5)
    server.post(f, path + "/input", {"text": "Steer now."})
    server.until(f, session, lambda es: any(e.get("call") == "toolu_again" for e in es), timeout=10)
    time.sleep(0.5)
    asked = server.post(f, path + "/handover", {})["entry"]
    assert server.post(f, path + "/handover", {}) == {}, "a second handover"
    entries = server.until(f, session, lambda es: any(
        e["kind"] == "turn_end" and asked in e["inputs"] for e in es), timeout=10)
    results = [e["output"] for e in entries if e["kind"] == "result"]
    request = next(e for e in entries if e["id"] == asked)
    assert results[:2] == ["null", "null"] and request["command"] == "handover", (results, request)
    assert "Steer now." in (f.requests / "002.body").read_text()
    assert json.dumps(request["text"])[1:-1] in (f.requests / "003.body").read_text()
    rebuild = next(e for e in entries if e["kind"] == "rebuild")
    assert rebuild["trigger"] == "handover" and rebuild["note"].strip() == wait_note, rebuild
large = response([call("agent", json.dumps(wait), "call_wait")]).body
replies = [Reply(large.replace(b'"input_tokens": 100', b'"input_tokens": 200000')),
           response([call("handover", json.dumps({"note": wait_note}), "call_note")]), done()]
with flow(replies, model="gpt-6.1-sol", config=PLAIN + "[rebuild]\nthreshold = 0.5") as f:
    entries = list(f.record(f.ask(ask, timeout=10)["session"]))
    waited = next(e for e in entries if e["kind"] == "result" and e["call"] == "call_wait")
    rebuilds = [e["trigger"] for e in entries if e["kind"] == "rebuild"]
    assert waited["output"] == "null" and rebuilds == ["threshold"], (waited, rebuilds)

# The person's API request opens an idle note turn on both model paths. Files hold the
# note batch and the rebuilt turn open, so duplicate requests cannot race their endings.
for model in ("opus", "gpt-6.1-sol"):
    def message(*calls, text="flow done"):
        if model == "opus":
            names = {"bash": "Bash", "handover": "mcp__razor__handover"}
            return reply(*[(id, names[name], args) for id, name, args in calls], text=text)
        item = {"type": "message", "id": "msg_note", "role": "assistant",
                "content": [{"type": "output_text", "text": text}]}
        return response([item, *[call(name, json.dumps(args), id) for id, name, args in calls]])

    note = "Keep working on the silver kingfisher ask."
    replies = [message(),
               message(("note_wait", "bash", {"command": "while [ ! -e note_go ]; do sleep 0.05; done"}),
                       ("note_handover", "handover", {"note": note})),
               message(("rebuilt_wait", "bash", {"command": "while [ ! -e rebuilt_go ]; do sleep 0.05; done"})),
               message(), message()]
    with flow(replies, model=model, config=PLAIN) as f:
        session = f.ask(ask)["session"]
        path = f"/sessions/{session}"
        admitted = server.post(f, path + "/handover", {})["entry"]
        waiting = server.until(f, session, lambda es: any(
            e["kind"] == "job" and e["event"] == "started" for e in es))
        assert not any(e["kind"] == "rebuild" for e in waiting), waiting
        assert server.post(f, path + "/handover", {}) == {}, "duplicate note request"
        (f.cwd / "note_go").touch()
        server.until(f, session, lambda es: any(
            e["kind"] == "job" and e["event"] == "started" and e["job"] == 2 for e in es))
        (f.cwd / "rebuilt_go").touch()
        entries = server.until(f, session, lambda es: any(
            e["kind"] == "turn_end" and admitted in e["inputs"] for e in es))
        requests = [e for e in entries if e["kind"] == "input" and e["from"] == "razor"]
        assert len(requests) == 1 and requests[0]["id"] == admitted, requests
        rebuilds = [e for e in entries if e["kind"] == "rebuild"]
        assert len(rebuilds) == 1 and rebuilds[0]["trigger"] == "handover", rebuilds
        rebuild = rebuilds[0]
        assert rebuild["note"].strip() == note and ask in rebuild["context"], rebuild
        bodies = f.sent()
        assert json.dumps(requests[0]["text"])[1:-1] in bodies[1], bodies[1]
        next_turn = f.ask("--resume", session, "Continue the kingfisher work.")
        assert next_turn["message"] == "flow done", next_turn
        last = f.sent()[-1]
        assert note in last and "Continue the kingfisher work." in last, last
        assert len([e for e in f.record(session) if e["kind"] == "rebuild"]) == 1

# Recall never hands a model its own thinking back, and neither its answer nor a handover's ever
# names thinking: Claude refuses a request that asks for it as reasoning extraction. The note is
# the handover call's own field, so a model's thinking can never hide it.
thought = "The kingfisher note sits only in this reasoning."
note = "Recall the kingfisher ask next."
reasoning = {"type": "reasoning", "id": "rs_note", "summary": [{"type": "summary_text", "text": thought}]}
replies = [response([reasoning, call("recall", json.dumps({"id": 4}), "call_reasoning")]),
           response([call("handover", json.dumps({"note": note}), "call_noted")]), done()]
with flow(replies, model="gpt-6.1-sol", config=PLAIN) as f:
    entries = list(f.record(f.ask(ask)["session"]))
    assert entries[3]["kind"] == "thinking" and entries[3]["text"] == thought, entries[:5]
    outputs = {e["call"]: e for e in entries if e["kind"] == "result"}
    assert outputs["call_reasoning"]["outcome"] == "error", outputs["call_reasoning"]
    for said in (outputs["call_reasoning"]["output"], outputs["call_noted"]["output"]):
        assert thought not in said and not any(w in said.lower() for w in ("thinking", "reasoning")), said
    rebuilds = [e for e in entries if e["kind"] == "rebuild"]
    assert len(rebuilds) == 1 and rebuilds[0]["note"].strip() == note, rebuilds


def sized(tokens, *calls, text="flow done"):
    events = reply(*calls, text=text).body.decode().splitlines()
    for i, line in enumerate(events):
        if line.startswith("data: "):
            event = json.loads(line[6:])
            if event["type"] == "message_start":
                event["message"]["usage"].update(input_tokens=13, cache_read_input_tokens=tokens - 13,
                                               cache_creation_input_tokens=0)
                events[i] = "data: " + json.dumps(event)
    return Reply(("\n".join(events) + "\n").encode())


def completed(es):
    return es[-1]["kind"] == "turn_end"


def unloaded(f, session):
    pid = json.loads((f.home / ".razor/server.json").read_text())["pid"]
    record = f.home / ".razor/sessions" / session / "record.jsonl"
    return all(fd.resolve() != record for fd in Path(f"/proc/{pid}/fd").iterdir())


with wall_clock() as (clock, env, advance):
    env |= {"CLAUDE_CODE_PROMPT_CACHE_TTL": "5m", "CLAUDE_CODE_SUBAGENT_PROMPT_CACHE_TTL": "5m"}
    note = "Recall the silver kingfisher ask after idleness."
    child = "You are idle-helper18,"
    replies = [sized(100, ("idle_child", "mcp__razor__agent", {"action": "start", "name": "idle-helper18", "brief": "Wait.", "placement": "in_place"})),
               reply(("child_wait", "Bash", {"command": "until [ -e child_go ]; do sleep 0.05; done"}), match=child),
               reply(("child_finish", "mcp__razor__agent", {"action": "finish", "status": "finished", "report": "Child done."}), match=child),
               sized(100), reply(("idle_wait", "Bash", {"command": "until [ -e idle_go ]; do sleep 0.05; done"}),
                     ("idle_note", "mcp__razor__handover", {"note": note})),
               reply(text="flow done"), reply(text="Child heard."), reply(text="Continued.")]
    with flow(replies, config="[sessions]\nidle = 0\nhandover_tokens = 0\nclose_minutes = 60", env=env) as f:
        session = f.ask(ask)["session"]
        child_id = next(e["session"] for e in f.record(session) if e["kind"] == "child")
        server.until(f, child_id, lambda es: any(e["kind"] == "job" for e in es))
        advance(2_999_999)
        time.sleep(1.2)
        assert not any(e["kind"] == "input" and e["from"] == "razor" for e in f.record(session)), "handover before 50 minutes"
        advance(3_000_000)
        entries = server.until(f, session, lambda es: any(
            e["kind"] == "job" and e["event"] == "started" for e in es), timeout=8)
        requests = [e for e in entries if e["kind"] == "input" and e["from"] == "razor"]
        assert len(requests) == 1, requests  # The child's running job did not block its parent.
        (f.cwd / "child_go").touch()
        reported = server.until(f, session, lambda es: any("report" in e.get("from", "") for e in es))
        report_id = next(e["id"] for e in reported if "report" in e.get("from", ""))
        late = "Keep the late kingfisher instruction verbatim."
        admitted = server.post(f, f"/sessions/{session}/input", {"text": late})["entry"]
        (f.cwd / "idle_go").touch()
        entries = server.until(f, session, lambda es: any(
            e["kind"] == "turn_end" and admitted in e["inputs"] for e in es))
        entries = server.until(f, session, lambda es: any(e["kind"] == "turn_end" and report_id in e["inputs"] for e in es))
        rebuilds = [e for e in entries if e["kind"] == "rebuild"]
        assert len(rebuilds) == 1 and rebuilds[0]["trigger"] == "idle", rebuilds
        assert rebuilds[0]["note"].strip() == note and ask in rebuilds[0]["context"], rebuilds
        assert late in rebuilds[0]["context"]
        for request in map(json.loads, f.sent()):
            blocks = request.get("system", []) + request.get("tools", [])
            blocks += [b for m in request["messages"] for b in m["content"] if isinstance(b, dict)]
            writes = [b["cache_control"] for b in blocks if "cache_control" in b]
            assert writes and all(w.get("ttl") == "1h" for w in writes), writes
        activity = json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())["last_activity"]
        advance(9_000_000)
        server.until(f, session, lambda es: any(e.get("close_reason") == "idle" for e in es))
        server.until(f, session, lambda es: any("settled" in e.get("event", {}) for e in es if e["kind"] == "child"))
        assert json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())["last_activity"] == activity, "idle close's child settlement moved activity"
        resumed = f.run("--resume", session, "--print=json", "Continue the Claude conversation.")
        assert resumed.returncode == 0 and note in f.sent()[-1], resumed.stderr

    # Running jobs hold off maintenance; answering their notice starts a new idle interval.
    advance(0)
    replies = [sized(50_000, ("idle_background", "Bash", {
                   "command": "until [ -e background_go ]; do sleep 0.05; done", "run_in_background": True})),
               sized(50_000), sized(50_000), reply(("background_note", "mcp__razor__handover", {"note": note})),
               sized(50_000)]
    with flow(replies, config="[sessions]\nidle = 0\nclose_minutes = 60", env=env) as f:
        session = f.ask(ask)["session"]
        advance(3_600_000)
        time.sleep(1.2)
        assert not any(e.get("closed") or e["kind"] == "rebuild" for e in f.record(session)), "working job handed over or closed"
        (f.cwd / "background_go").touch()
        server.until(f, session, lambda es: completed(es) and any(
            e["kind"] == "job" and e["event"] == "ended" for e in es))
        os.kill(pid := server.start(f), signal.SIGTERM)  # Due maintenance survives a restart.
        server.gone(pid)
        server.start(f)
        advance(6_600_000)
        server.until(f, session, lambda es: completed(es) and any(e["kind"] == "rebuild" for e in es))
        settings = f.home / ".razor/config.toml"
        settings.write_text(settings.read_text().replace("close_minutes = 60", "close_minutes = 0"))
        server.until(f, session, lambda _: unloaded(f, session) and not (f.home / ".razor/live" / session).exists())
        assert not any(note in body for body in f.sent()), "the idle rebuild sent a request"
        resumed = f.run("--resume", session, "--print=json", "Back on Claude.")  # The note opens its first request.
        assert resumed.returncode == 0 and all(t in f.sent()[-1] for t in (note, "Back on Claude.")), resumed.stderr

    # A restart inside the idle note turn ends it at its rebuild, with no request nobody asked for.
    advance(0)
    replies = [sized(50_000), reply(("cut_note", "Bash", {"command": "until [ -e cut_go ]; do sleep 0.05; done"})),
               reply(text="Job ended.")]
    with flow(replies, config="[sessions]\nidle = 0\nclose_minutes = 60", env=env) as f:
        session = f.ask(ask)["session"]
        advance(3_000_000)
        server.until(f, session, lambda es: any(e["kind"] == "job" for e in es))
        sent = len(f.sent())
        server.stop(f, signal.SIGKILL)
        server.start(f)
        entries = server.until(f, session, completed)
        assert [e["trigger"] for e in entries if e["kind"] == "rebuild"] == ["idle"], entries[-3:]
        assert len(f.sent()) == sent, "the restart asked the model again"
        (f.cwd / "cut_go").touch()  # The job's end notice opens the next turn.
        server.until(f, session, lambda es: completed(es) and es[-2].get("text") == "Job ended.")

    # Each route's smaller request clears eligibility against the new 50,000-token floor.
    for model in ("opus", "gpt-6.1-sol"):
        advance(0)
        replies = [sized(n) if model == "opus" else Reply(done().body.replace(
            b'"input_tokens": 100', f'"input_tokens": {n}'.encode())) for n in (50_000, 49_999)]
        with flow(replies, model=model, config=PLAIN + "[sessions]\nidle = 0", env=env) as f:
            session = f.ask(ask)["session"]
            f.ask("--resume", session, "One smaller request.")
            if model == "opus":
                server.until(f, session, lambda _: unloaded(f, session))
            advance(3_000_001)
            time.sleep(1.2)
            assert not any(e["kind"] == "input" and e["from"] == "razor" for e in f.record(session))

    # A running tool turn holds off idle close.
    advance(0)
    with flow([sized(150_000, ("running_idle", "Bash", {
            "command": "until [ -e running_go ]; do sleep 0.05; done"})), reply(text="flow done")],
              config="[sessions]\nhandover_minutes = 0\nclose_minutes = 50", env=env) as f:
        server.start(f)
        session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        server.post(f, f"/sessions/{session}/input", {"text": ask})
        server.until(f, session, lambda es: any(e["kind"] == "job" for e in es))
        advance(3_000_001)
        time.sleep(1.2)
        assert not any(e.get("closed") for e in f.record(session))
        (f.cwd / "running_go").touch()
        server.until(f, session, completed)

    # An open question holds off idle handover; an idle request whose turn fails is not repeated.
    advance(0)
    model = "gpt-6.1-sol"
    big = lambda r: Reply(r.body.replace(b'"input_tokens": 100', b'"input_tokens": 50000'))
    refusal = Reply(b'{"error": {"type": "invalid_request_error", "message": "Refused."}}', status=400)
    with flow([big(message(("idle_ask", "message", {"to": "parent", "type": "question", "text": "Which path?"}))),
               big(message()), big(message()), refusal], model=model, config=PLAIN + "[sessions]\nidle = 0", env=env) as f:
        session = f.ask(ask)["session"]
        asked = next(e["id"] for e in f.record(session) if e["kind"] == "call")
        advance(3_000_001)
        time.sleep(1.2)
        assert not any(e.get("trigger") == "idle" for e in f.record(session)), "handover with a question open"
        answer = server.post(f, f"/sessions/{session}/asks/{asked}", {"text": "Left."})["entry"]
        server.until(f, session, lambda es: any(e["kind"] == "turn_end" and answer in e["inputs"] for e in es))
        advance(6_000_002)
        server.until(f, session, lambda es: completed(es) and any(e.get("trigger") == "idle" for e in es))
        time.sleep(1.2)
        assert sum(e["kind"] == "input" and e.get("trigger") == "idle" for e in f.record(session)) == 1, "the idle request repeated"

    # An idle note turn the host refuses as too long ends at its overflow rebuild, with no request.
    overflow = Reply(json.dumps({"error": {"type": "invalid_request_error", "code": "context_length_exceeded",
                     "message": "prompt is too long: 201000 tokens > 200000 maximum"}}).encode(), status=400)
    for model in ("opus", "gpt-6.1-sol"):
        advance(0)
        with flow([sized(50_000) if model == "opus" else big(message()), overflow], model=model,
                  config=PLAIN + "[sessions]\nidle = 0", env=env) as f:
            session = f.ask(ask)["session"]
            advance(3_000_000)
            entries = server.until(f, session, lambda es: completed(es) and any(e["kind"] == "rebuild" for e in es))
            triggers = [e["trigger"] for e in entries if e["kind"] == "rebuild"]
            assert triggers == ["overflow"] and len(f.sent()) == 2, entries[-3:]

    # GPT's default handover, resident report, stable phone history and notification silence.
    advance(0)
    replies = [message(text="Home ready."), big(message(text="Initial reply.")),
               message(("configured_note", "handover", {"note": note})),
               message(text="Report received."), message(text="Continued.")]
    config = PLAIN + "[sessions]\nidle = 0\nclose_minutes = 60\n"
    with apple(clock / model) as apns, flow(replies, model=model, env=env, config=config + f"""[push]
team = "FLOWTEAM48"
bundle = "com.example.razor"
endpoint = "{apns.url}"
[push.development]
key_id = "FLOWKEY48"
key = {json.dumps(str(apns.key))}
""") as f:
        with (f.home / ".razor/config.toml").open("a") as config:
            config.write(f"[residents.ember18]\ncwd = {json.dumps(str(f.cwd))}\n")
        server.start(f)
        server.post(f, "/sessions/ember18/input", {"text": "Home check."})
        server.until(f, "ember18", completed)
        session = server.post(f, "/sessions", {"resident": "ember18"})["session"]
        path = f"/sessions/{session}"
        server.post(f, path + "/input", {"text": ask})
        server.until(f, session, completed)
        server.post(f, "/sessions", {"resident": "ember18"})
        history = lambda: json.loads(server.send(f, "GET", "/sessions?resident=ember18").read())
        activity = lambda: (history(), json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())["last_activity"])
        original_activity = activity()
        advance(3_000_000)
        entries = server.until(f, session, lambda es: completed(es) and any(e["kind"] == "rebuild" for e in es))
        maintenance = [e for e in entries if e["kind"] == "input" and e["from"] == "razor"]
        assert len(maintenance) == 1 and maintenance[0]["trigger"] == "idle", maintenance
        assert not any(note in body for body in f.sent()), "the idle rebuild sent a request"
        assert activity() == original_activity, "handover moved activity"
        inputs = [e for e in entries if e["kind"] == "input"]
        phone = f.run("devices", "add", "phone").stdout.splitlines()[1]
        # At `all` every finish is sent, so no finished alert below means the idle close sent none.
        for route, body in (("/devices/self/push", {"token": "aa18", "environment": "development"}),
                            (path + "/follow", {"token": "ff18", "environment": "development"}),
                            ("/devices/self/notifications", {"level": "all"})):
            registered = server.send(f, "PUT", route, body, key=phone)
            assert registered.status == 200, registered.read()
            registered.read()
        with server.send(f, "GET", path + "/events") as stream:
            stream.readline()
            server.until(f, session, lambda _: bool(apns.received))
        requests = lambda: sum(ask in body for body in f.sent())
        before = requests()
        advance(3_600_000)
        entries = server.until(f, session, lambda es: es[-1].get("close_reason") == "idle")
        assert entries[-1]["closed"] and entries[-1]["end"] == "done" and entries[-1]["inputs"] == [], entries[-1]
        assert [e for e in entries if e["kind"] == "input"] == inputs and requests() == before, "close opened a model turn"
        assert activity() == original_activity and json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())["close_reason"] == "idle"
        assert server.listed(f, session), "idle close moved the recently used thread out of the list"
        report = server.until(f, "ember18", lambda es: completed(es) and any(e.get("text") == note for e in es))
        assert not any(e.get("closed") for e in report), "resident home closed"
        time.sleep(1.2)
        assert not any(r["body"].get("razor", {}).get("kind") == "finished"
                       or r["body"]["aps"].get("event") == "end" for r in apns.received), apns.received
        assert any(follow["session"] == session for device in json.loads((f.home / ".razor/devices.json").read_text())
                   for follow in device.get("follows", [])), "idle close ended the Live Activity"
        resumed = f.ask("--resume", session, "Continue our conversation.")
        last = f.sent()[-1]  # One request: the note and the person's input.
        assert resumed["session"] == session and note in last and "Continue our conversation." in last
        assert [e["trigger"] for e in f.record(session) if e["kind"] == "rebuild"] == ["idle"] and "close_reason" not in json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())

# Both native routes preserve their structured overflow reason. The first failed request
# rebuilds with the last note; two rejections pause with the rebuilt byte size, without a loop.
refusal = Reply(json.dumps({"error": {"type": "invalid_request_error",
    "code": "context_length_exceeded", "message": "prompt is too long: 201000 tokens > 200000 maximum"}}).encode(), status=400)
for model in ("opus", "gpt-6.1-sol"):
    for again in (False, True):
        success = reply(text="flow done") if model == "opus" else done()
        with flow([refusal, refusal if again else success], model=model, config=PLAIN) as f:
            result = f.run("--print=json", "Keep the unanswered overflow request.")
            answer = json.loads(result.stdout)
            assert (result.returncode != 0) == again, result
            entries = list(f.record(answer["session"]))
            rebuilds = [e for e in entries if e["kind"] == "rebuild"]
            assert len(rebuilds) == 1 and rebuilds[0]["trigger"] == "overflow", rebuilds
            assert "Keep the unanswered overflow request." in rebuilds[0]["context"], rebuilds
            ends = [e for e in entries if e["kind"] == "turn_end"]
            assert isinstance(ends[0]["end"], dict) and not ends[0]["inputs"], ends
            assert entries[1]["id"] in ends[-1]["inputs"], ends
            if again:
                assert str(len(rebuilds[0]["context"].encode())) + " bytes" in answer["error"], answer
            else:
                assert answer["message"] == "flow done", answer
            assert len(f.sent()) == 2, "overflow retried in a loop"

# Non-image native history can still exceed Razor's own cap and rebuild exactly once.
large_reasoning = "x" * (34 << 20)
replies = [response([{"type": "reasoning", "id": f"large_{n}", "summary": [],
                     "encrypted_content": large_reasoning},
                    call("read", json.dumps({"path": "small.txt"}), f"large_call_{n}")])
           for n in range(2)]
with flow(replies + [done()], model="sol", config=PLAIN) as f:
    (f.cwd / "small.txt").write_text("ok")
    answer = f.ask("Keep this request through the cap.", timeout=180)
    entries = list(f.record(answer["session"]))
    rebuilds = [e for e in entries if e["kind"] == "rebuild"]
    assert len(rebuilds) == 1 and rebuilds[0]["trigger"] == "overflow", rebuilds
    requests = sorted(f.requests.glob("[0-9]*.body"))
    assert len(requests) == 3 and requests[1].stat().st_size > 32 << 20, requests
    assert requests[2].stat().st_size < 1 << 20, requests[2].stat().st_size

# Responses retain the newest image bytes and spill the oldest in stable steps, without a rebuild.
batches = [["large.png"] * 4, ["small.png"], ["small.png"], ["large.png"] * 4]
replies = [response([call("read", json.dumps({"path": path}), f"image_{n}_{i}")
                     for i, path in enumerate(batch)]) for n, batch in enumerate(batches)]
with flow(replies + [done()], model="sol", config=PLAIN) as f:
    small = base64.b64decode(PNG)
    (f.cwd / "small.png").write_bytes(small)
    (f.cwd / "large.png").write_bytes(small.ljust(7_000_000, b"\0"))
    answer = f.ask("Inspect the images and keep this request.", timeout=120)
    entries = list(f.record(answer["session"]))
    assert not [e for e in entries if e["kind"] == "rebuild"], "images caused a rebuild"
    outputs = [json.loads(e["output"]) for e in entries if e["kind"] == "result"]
    paths = [Path(output["images"][0]["path"]) for output in outputs]
    assert len(paths) == 10 and all(p.is_file() for p in paths), paths
    requests = f.sent()
    assert len(requests) == 5, len(requests)
    def images(body):
        content = [c for item in json.loads(body)["input"] for c in item.get("content", [])]
        spills = [c["text"] for c in content if "<persisted-output>" in c.get("text", "")]
        inline = [c["image_url"] for c in content if c.get("type") == "input_image"]
        return spills, inline
    first, between, later, final = [images(body) for body in requests[1:]]
    assert all(sum(len(url.split(",", 1)[1]) for url in inline) <= 32 << 20
               for _, inline in (first, between, later, final)), "image budget exceeded"
    assert len(first[0]) == 1 and str(paths[0]) in first[0][0], first[0]
    assert "read image" in first[0][0] and "pixels" in first[0][0] and "7000000 bytes" in first[0][0]
    assert first[0] == between[0] == later[0], "small images moved the cutoff"
    assert len(final[1]) == 3, "newest images did not remain inline"
    assert len(final[0]) == 7 and all(str(path) in spill for path, spill in zip(paths, final[0])), final[0]
    assert all(len(body.encode()) < 64 << 20 for body in requests), "request cap exceeded"

# A cell's handover uses the same path on GPT and Claude, including its sentence result.
for model in ("opus", "sol"):
    code = f"text(await tools.handover({{note: {json.dumps(first_note)}}}));"
    cell = (reply(("cell", "mcp__razor__exec", {"code": code})) if model == "opus" else
            response([{"type": "custom_tool_call", "id": "ct_cell", "call_id": "cell", "name": "exec", "input": code}]))
    end = reply(text="Rebuilt.") if model == "opus" else done()
    config = '[profiles.claude]\ncode_mode = true\n' if model == "opus" else ""
    with flow([end, cell, end], model=model, config=config) as f:
        session = f.ask(ask)["session"]
        asked = server.post(f, f"/sessions/{session}/handover", {})["entry"]
        results = server.until(f, session, lambda es: any(e["kind"] == "result" and e["call"] == "cell" for e in es))
        assert all(e["outcome"] == "success" for e in results if e["kind"] == "result"), results
        entries = server.until(f, session, lambda es: any(e["kind"] == "rebuild" for e in es)
                               and es[-1]["kind"] == "turn_end" and asked in es[-1]["inputs"])
        rebuilds = [e for e in entries if e["kind"] == "rebuild"]
        assert len(rebuilds) == 1 and rebuilds[0]["note"] == first_note, rebuilds
        results = [e for e in entries if e["kind"] == "result"]
        assert {e["call"] for e in results} == {"cell", "cell:1"} and all(e["outcome"] == "success" for e in results), results
        assert len(f.sent()) == 3, "the last ask was answered twice"
