"""Real child reports and task writes drive one reminder; idle handover asks for upkeep."""
from dataclasses import replace
from pathlib import Path
import shutil
import tempfile
import time
from claude import reply
from clock import wall_clock
from driver import flow
from model import tool
import server


root = "ROOT_UPKEEP_77"
replies = []
with flow(replies) as f:
    f.base()
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    path = f"/sessions/{session}/input"

    def reports(entries):
        return [e for e in entries if e["kind"] == "input" and isinstance(e.get("from"), dict)
                and "report" in e["from"]]

    def reminders(entries):
        return [e for e in entries if e["kind"] == "input" and e.get("upkeep")]

    def child(n):
        name = f"upkeep{n}"
        marker = f"You are {name},"
        gate = f"child{n}_go"
        replies.extend([
            tool(root, "agent", action="start", name=name, brief="Finish the work.", placement="in_place"),
            reply(text="Child started.", match=root),
            reply(text="Report received.", match=root),
            reply((None, "Bash", {"command": f"until [ -e {gate} ]; do sleep 0.05; done"}), match=marker),
            tool(marker, "agent", action="finish", status="finished", report=f"Finished {n}."),
        ])
        person = server.post(f, path, {"text": root})["entry"]
        server.until(f, session, lambda es: any(e["kind"] == "turn_end" and person in e["inputs"] for e in es))
        entries = list(f.record(session))
        child_id = next(e["session"] for e in entries if e["kind"] == "child" and
                        isinstance(e.get("event"), dict) and e["event"].get("started", {}).get("name") == name)
        server.until(f, child_id, lambda es: any(e["kind"] == "job" and e["event"] == "started" for e in es))
        (f.cwd / gate).touch()
        entries = server.until(f, session, lambda es: len(reports(es)) == n and any(
            e["kind"] == "turn_end" and reports(es)[-1]["id"] in e["inputs"] for e in es))
        return entries

    assert not reminders(child(1))
    second = child(2)
    assert len(reminders(second)) == 1 and reminders(second)[0]["from"] == "razor", second[-10:]
    assert reminders(second)[0]["text"].strip() in f.sent()[-1]
    assert len(reminders(child(3))) == 1
    replies.extend([tool(root, "task", action="write", title="Kept after reports"),
                    reply(text="Task saved.", match=root)])
    person = server.post(f, path, {"text": root})["entry"]
    entries = server.until(f, session, lambda es: any(e["kind"] == "turn_end" and person in e["inputs"] for e in es))
    assert any(e["kind"] == "result" and e["outcome"] == "success" for e in entries if e.get("call") ==
               next(c["call"] for c in entries if c.get("name") == "mcp__razor__task")), entries[-8:]
    assert len(reminders(child(4))) == 1
    assert len(reminders(child(5))) == 2

with wall_clock() as (_, env, advance):
    replies = [reply(text="Ready."), reply((None, "mcp__razor__handover", {"note": "Continue."}))]
    with flow(replies, config="[sessions]\nhandover_tokens = 0", env=env) as f:
        session = f.ask("Start the idle check.")["session"]
        advance(3_000_000)
        entries = server.until(f, session, lambda es: any(e["kind"] == "rebuild" and e.get("trigger") == "idle" for e in es))
        request = next(e for e in entries if e.get("trigger") == "idle")
        assert all(word in request["text"] for word in ("tasks", "rulings", "facts", "notes", "state note", "nothing")), request
        assert request["text"].index("First") < request["text"].index("handover`"), request

# Discovery uses recorded roots after worktrees disappear, skips missing roots, and gives
# every-project memory to one nightly dream. Fixtures must live outside temp/cache/runtime.
with tempfile.TemporaryDirectory(prefix="razor-dream-flow-", dir=Path.home()) as folder, wall_clock() as (_, env, advance):
    replies = []
    with flow(replies, config='[memory]\nnight = "23:59"', env=env) as f:
        roots = [Path(folder) / name for name in ("a", "b", "0-missing")]
        sources = []
        for root in roots:
            root.mkdir()
            project = replace(f, cwd=root)
            project.base()
            checkout = root.with_name(root.name + "-worktree")
            project.git("worktree", "add", "-b", "source", str(checkout))
            replies.append(reply(text="Source for dreaming."))
            sources.append(replace(f, cwd=checkout).ask("Keep this project's work.")["session"])
            project.git("worktree", "remove", str(checkout))
        # Leave a recorded root that no longer exists without touching any real project.
        shutil.rmtree(roots[-1])
        missing = f.run("dream", str(roots[-1]))
        assert missing.returncode != 0 and "does not exist" in missing.stderr, missing
        all_memory = f.home / ".razor/memory/all"
        (all_memory / "wiki").mkdir(parents=True)
        (all_memory / "wiki/_instructions.md").write_text("ALL_INSTRUCTIONS_79")
        (all_memory / "fact-all79.md").write_text("---\ndescription: ALL_FACT_79\n---\nEvery-project fact.")
        replies.extend([reply(text="Nightly pass complete.") for _ in roots[:2]])

        def dream(root):
            """The session whose first entry is in `root`, independently of stamp layout."""
            return next((p.parent.name for p in (f.home / ".razor/sessions").glob("*/record.jsonl")
                         if next(f.record(p.parent.name), {}).get("cwd") == str(root)), None)
        # The night comes on the wall clock; the residents loop sees it at its next wake, up to a
        # minute later on the server's monotonic clock, which the flow then moves a minute on.
        advance(86_400_000)
        time.sleep(1.2)
        assert not any(map(dream, roots)), "a dream started before the residents loop woke"
        advance(60_000, monotonic=True)
        dreams = []
        for root, source in zip(roots[:2], sources):
            sid = server.wait(lambda: dream(root), timeout=10, why=f"no dream for {root.name}")
            entries = server.until(f, sid, lambda es: any(e.get("closed") for e in es))
            assert not dream(roots[-1]), "missing root started a dream"
            text = next(e["text"] for e in entries if e["kind"] == "input")
            assert f"- {source}" in text, "dream omitted a deleted worktree's session"
            dreams.append(text)
        assert all(marker in dreams[0] and marker not in dreams[1] for marker in ("ALL_FACT_79", "ALL_INSTRUCTIONS_79")), "every-project memory did not belong only to the first dream"
