"""Idle conversation close preserves helpers; an explicit close preserves their edits on the
branch."""
from clock import wall_clock
from driver import flow
import server
from model import agent, reply

with wall_clock() as (_, env, advance):
    root, child = "CLOSE_ROOT_12", "You are close12,"
    replies = [agent(root, "start", name="close12", brief="Edit base and leave an untracked file."), reply(root),
               reply(child, "Bash", {"command": "printf edited > base; printf untracked > kept12"}), reply(child), reply(root), agent(root, "continue", child="close12", brief="Check the edits still exist."),
               reply(root), reply(child, "Bash", {"command": "cat base kept12"}), reply(child), reply(root)]
    with flow(replies, config="[sessions]\nclose_minutes = 0.5", env=env) as f:
        f.base()
        f.git("config", "user.name", "Flow")
        f.git("config", "user.email", "flow@example.invalid")
        parent = f.ask(root)["session"]
        sid = next(e["session"] for e in f.record(parent) if e["kind"] == "child")
        server.until(f, sid, lambda es: any(e["kind"] == "turn_end" for e in es))
        checkout = f.home / ".razor/sessions" / sid / "worktree"
        def reported(entries, n):
            reports = [e["id"] for e in entries if e["kind"] == "input" and isinstance(e.get("from"), dict) and "report" in e["from"]]
            return len(reports) == n and any(e["kind"] == "turn_end" and reports[-1] in e["inputs"] for e in entries)

        server.until(f, parent, lambda es: reported(es, 1))
        advance(31_000)
        server.until(f, parent, lambda es: any(e.get("close_reason") == "idle" for e in es))
        f.ask("--resume", parent, root)
        continuation = [e for e in f.record(parent) if e["kind"] == "result"][-1]
        assert continuation["outcome"] == "success", "idle close settled a helper"
        server.until(f, parent, lambda es: reported(es, 2))
        server.post(f, f"/sessions/{parent}/quit", {})
        server.wait(lambda: not checkout.exists(), why="parent quit kept the helper worktree")
        assert f.git("show", f"child/{sid}:base") == "edited" and f.git("show", f"child/{sid}:kept12") == "untracked", "parent quit lost uncommitted helper work"
