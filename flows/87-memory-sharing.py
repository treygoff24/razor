"""Shared memory authors, ruling ownership and settlements, secret refusal, joining, and a
resident's private layer, task visibility and dream input, and a project session's state and
`/done` closeout in a resident's folder, through the real server and git refs."""
import json
import os
from datetime import date, timedelta
from claude import reply
from driver import flow
from server import post, start, stop, until, wait

replies = []
with flow(replies) as f:
    f.base()
    remote = f.home / "shared.git"
    f.git("init", "--bare", str(remote))

    def checkout(name, shared=True):
        other = f.home_at(name)
        cwd = other.cwd
        other.git("remote", "add", "origin", str(remote))
        (cwd / ".razor").mkdir()
        if shared:
            (cwd / ".razor/config.local.toml").write_text('[memory]\nremote = "origin"\n')
        return other

    def use(where, **arguments):
        replies.extend([reply((None, "mcp__razor__memory", arguments)), reply(text="saved")])
        sid = where.ask("Remember: " + (arguments.get("text") or "Keep the person's decision in this project."))["session"]
        return [e for e in where.record(sid) if e["kind"] == "result"][-1]

    def write(where, **arguments):
        out = use(where, action="write", **arguments)
        assert out["outcome"] == "success", out
        return json.loads(out["output"])["id"]

    def page(where, refresh=False):
        if refresh:
            for p in (where.home / ".razor/memory").rglob("FETCH_HEAD"):
                os.utime(p, (0, 0))
            assert use(where, action="search", query="")["outcome"] == "success"
        replies.append(reply(text="opened"))
        where.ask("Open this project.")
        return json.loads(where.sent()[-1])["messages"][0]["content"][0]["text"]

    def git(*args):
        return f.git("--git-dir", str(remote), *args).strip()

    alice, bob, unknown = [checkout(name) for name in ("alice", "bob", "unknown")]
    for where, name in ((alice, "Alice"), (bob, "Bob")):
        where.git("config", "user.name", name)
        where.git("config", "user.email", f"{name.lower()}@example.invalid")
    first = write(alice, kind="ruling", text="Ship on Fridays.")
    assert git("log", "-1", "--format=%an <%ae>", "refs/razor/memory") == "Alice <alice@example.invalid>", "shared author"
    assert "Alice" in next(line for line in page(bob, True).splitlines() if first in line), "ruling owner missing"
    before = git("rev-parse", "refs/razor/memory")
    for change in ({}, {"kind": "ruling", "text": "Ship on Mondays."}):
        refused = use(bob, action="write", replaces=first, **change)
        assert refused["outcome"] == "error" and "Alice" in refused["output"], "another person retired Alice's ruling"
        assert git("rev-parse", "refs/razor/memory") == before, "refused retirement changed memory"

    # Missing either half of the checkout's identity refuses before local or remote writes.
    for name, email in (("", ""), ("Unknown", ""), ("", "unknown@example.invalid")):
        unknown.git("config", "user.name", name)
        unknown.git("config", "user.email", email)
        before = git("rev-parse", "refs/razor/memory")
        refused = use(unknown, action="write", kind="note", text="Unowned note.", description="Unowned.")
        assert refused["outcome"] == "error" and git("rev-parse", "refs/razor/memory") == before, "write without identity"
        assert not list((unknown.home / ".razor/memory").rglob("*.md")), "identity refusal changed local memory"

    settled = write(bob, kind="ruling", text="Ship on Thursdays.", settles=[first])
    assert git("log", "-1", "--format=%an <%ae>", "refs/razor/memory") == "Bob <bob@example.invalid>", "shared author"
    shown = page(alice, True)
    line = next(line for line in shown.splitlines() if first in line)
    assert "set aside" in line and settled in line, "settled ruling still stands"
    assert first in shown.split("## Memory: ask", 1)[1], "owner was not asked to confirm or restore"
    confirmed = use(alice, action="write", replaces=first)
    assert confirmed["outcome"] == "success", confirmed
    shown = page(alice)
    assert first not in shown and "## Memory: ask" not in shown, "confirm left the ask open"
    second = write(alice, kind="ruling", text="Ship on Fridays.")
    write(bob, kind="ruling", text="Ship on Thursdays.", settles=[second])
    assert second in page(alice, True).split("## Memory: ask", 1)[1], "second settlement was not offered"
    restored = write(alice, kind="ruling", text="Ship on Fridays.", replaces=second)
    shown = page(alice)
    line = next(line for line in shown.splitlines() if restored in line)
    assert "set aside" not in line and "## Memory: ask" not in shown, "restored ruling did not stand again"

    # Synthetic keys never leave the scratch home. Refusals must not echo their input.
    for text in ("ghp_" + "A1b2" * 10,
                 "-----BEGIN OPENSSH PRIVATE KEY-----\nYnl0ZXM=\n-----END OPENSSH PRIVATE KEY-----",
                 "12345678-1234-1234-1234-123456789abc:" + "a1" * 16):
        before = git("rev-parse", "refs/razor/memory")
        refused = use(alice, action="write", kind="note", text=text, description="Key location.")
        assert refused["outcome"] == "error" and all(part not in refused["output"] for part in text.splitlines()), "secret saved or echoed"
        assert git("rev-parse", "refs/razor/memory") == before, "secret refusal changed memory"
    for text in ("sk-learn-style", "razor:20261109-181821-5a03:l-5", "abcde12345" * 4,
                 "12345678-1234-1234-1234-123456789abc"):
        saved = write(alice, kind="note", text=text, description="Ordinary reference.")
        found = use(alice, action="get", id=saved)
        assert found["outcome"] == "success" and text in json.loads(found["output"])["file"], "ordinary text lost"
    # A home with memory of its own stays writable locally when it cannot join this history.
    shared_folder = next((alice.home / ".razor/memory").rglob(saved + ".md")).parent
    (shared_folder / "state.md").write_text("Their shared state.\n")
    write(alice, kind="note", text="Shared before joining.", description="Shared.")
    for name, collision in (("existing", True), ("nonconflicting", False)):
        existing = checkout(name, shared=False)
        kept = write(existing, kind="note", text="Local before joining.", description="Local.")
        folder = next((existing.home / ".razor/memory").rglob(kept + ".md")).parent
        if collision:
            (folder / "state.md").write_text("My local state.\n")
            existing.git("-C", str(folder), "init", "-q")
            existing.git("-C", str(folder), "add", "-A")
            existing.git("-C", str(folder), "-c", "user.name=Existing", "-c", "user.email=existing@example.invalid", "commit", "-qm", "Private memory")
        before = {p.name: p.read_bytes() for p in folder.glob("*.md")}
        tip = git("rev-parse", "refs/razor/memory")
        (existing.cwd / ".razor/config.local.toml").write_text('[memory]\nremote = "origin"\n')
        new = write(existing, kind="note", text="Local after joining.", description="Local.")
        after = {p.name: p.read_bytes() for p in folder.glob("*.md")}
        assert all(after.get(p) == text for p, text in before.items()) and set(after) == set(before) | {new + ".md"}, "joining lost or duplicated local memory"
        assert git("rev-parse", "refs/razor/memory") == tip and "staying local" in page(existing), "unrelated memory was shared without a warning"
        stop(existing)
    for other in (alice, bob, unknown):
        stop(other)

    # Probe both hidden refs separately; wait on later startup pages, never on a fixed delay.
    for reference in ("memory", "tasks"):
        if reference == "tasks":
            git("update-ref", "refs/razor/tasks", git("rev-parse", "refs/razor/memory"))
            git("update-ref", "-d", "refs/razor/memory")
        joining = checkout(f"join-{reference}", shared=False)
        wait(lambda: "## Memory: ask" in page(joining), timeout=15, why=f"no join offer for {reference}")
        (joining.cwd / ".razor/config.local.toml").write_text('[memory]\njoin = false\n')
        assert "## Memory: ask" not in page(joining), "declined join still offered"
        (joining.cwd / ".razor/config.local.toml").unlink()
        (joining.cwd / ".razor/config.toml").write_text('[memory]\njoin = false\n')
        joining.git("add", ".razor/config.toml")
        joining.git("-c", "user.name=Flow", "-c", "user.email=flow@example.invalid", "commit", "-m", "Project settings")
        refused = use(joining, action="write", kind="note", text="Private choice.", description="Choice.")
        assert refused["outcome"] == "error" and "join" in refused["output"], "committed join setting accepted"
        stop(joining)

replies = []
with flow(replies) as f:
    (f.cwd / "persona.md").write_text("You are Ember.")
    with (f.home / ".razor/config.toml").open("a") as config:
        config.write(f'\n[residents.ember]\ncwd = {json.dumps(str(f.cwd))}\nstartup = [{json.dumps(str(f.cwd / "persona.md"))}, "razor"]\n')
    start(f)

    def turn(session, name=None, **arguments):
        if name:
            replies.append(reply((None, f"mcp__razor__{name}", arguments)))
        replies.append(reply(text="done"))
        return f.ask("--resume", session, "Open this project.")["session"]

    def result(session):
        return [e for e in f.record(session) if e["kind"] == "result"][-1]

    turn("ember", "memory", action="write", kind="state", text="HOME_LAYER_STATE")
    assert result("ember")["outcome"] == "success", result("ember")
    notes = f.home / ".razor/memory/projects" / str(f.cwd).lstrip("/")
    state = notes / "residents/ember/state/ember.md"
    assert state.exists() and "HOME_LAYER_STATE" in state.read_text(), "home state not in resident layer"
    turn("ember", "memory", action="write", kind="note", text="Ember keeps its own work.",
         description="EMBER_LAYER_NOTE", resident=True)
    assert result("ember")["outcome"] == "success", result("ember")
    # Due this week puts tasks on the page without an overdue reminder starting another turn
    # in the home session and consuming a reply meant for this scene's next request.
    due = (date.today() + timedelta(days=1)).isoformat()
    turn("ember", "task", action="write", title="EMBER_TASK", resident="ember", due=due)
    assert result("ember")["outcome"] == "success", result("ember")
    turn("ember", "task", action="write", title="PROJECT_TASK", due=due)
    # A relative project that leaves the resident's folder is that folder's project, not the caller's.
    other = f.cwd.parent / "other87"
    other.mkdir()
    turn("ember", "task", action="write", title="OTHER_TASK", project="../other87")
    kept = f.home / ".razor/tasks" / str(other).lstrip("/") / "%/1.md"
    assert result("ember")["outcome"] == "success" and "OTHER_TASK" in kept.read_text(), "a relative project used the caller's own"
    real = f.cwd / "real-resident"
    real.mkdir()
    alias = f.cwd.parent / "linked-resident"
    alias.symlink_to(real, target_is_directory=True)
    with (f.home / ".razor/config.toml").open("a") as config:
        config.write(f'[residents.linked]\ncwd = {json.dumps(str(alias))}\n')
    turn("ember", "task", action="write", title="SYMLINK_TASK", project=str(alias))
    kept = f.home / ".razor/tasks" / str(alias).lstrip("/") / "%/1.md"
    assert result("ember")["outcome"] == "success" and "SYMLINK_TASK" in kept.read_text(), "a symlink targeted the containing repository"
    thread = post(f, "/sessions", {"resident": "ember"})["session"]
    turn(thread)
    shown = json.loads(f.sent()[-1])["messages"][0]["content"][0]["text"]
    assert all(text in shown for text in ("HOME_LAYER_STATE", "EMBER_LAYER_NOTE", "EMBER_TASK", "PROJECT_TASK")), "resident page missing its state or tasks"
    before = state.read_text()
    turn(thread, "memory", action="write", kind="state", text="THREAD_STATE")
    assert result(thread)["outcome"] == "error" and state.read_text() == before, "thread wrote state"
    plain = post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    turn(plain)
    shown = json.loads(f.sent()[-1])["messages"][0]["content"][0]["text"]
    assert "PROJECT_TASK" in shown and "HOME_LAYER_STATE" not in shown and "EMBER_TASK" not in shown and "EMBER_LAYER_NOTE" not in shown, "resident layer or task leaked to another session"
    turn(plain, "memory", action="write", kind="state", text="PROJECT_STATE")
    assert result(plain)["outcome"] == "success" and state.read_text() == before, "project session overwrote resident state"
    # A project session in the resident's folder, opened by cwd, writes project state and closes
    # as a project session.
    assert "PROJECT_STATE" in (notes / f"state/{plain}.md").read_text(), "project state not in the project layer"
    replies.append(reply(text="closed"))
    post(f, f"/sessions/{plain}/done", {})
    entries = until(f, plain, lambda es: any(e.get("closed") for e in es))
    closeout = next(e["text"] for e in entries if e["kind"] == "input" and "The person asked for `/done`" in e.get("text", ""))
    assert "write the project's state note" in closeout, closeout
    replies.append(reply(text="Tidied the project's memory."))
    dreamed = f.run("dream", str(f.cwd))
    assert dreamed.returncode == 0, dreamed.stderr
    entries = until(f, dreamed.stdout.strip(), lambda es: any(e.get("closed") for e in es))
    prompt = next(e["text"] for e in entries if e["kind"] == "input")
    assert "ember's own layer" in prompt and "EMBER_LAYER_NOTE" in prompt.split("ember's own layer", 1)[1], "dream input omitted resident layer"
