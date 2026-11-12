"""Memory in Razor's own files: a write is searchable by whole words, a replacement or an expiry
retires the old file, a fact needs a description, a new session's first request holds the state
note and each ruling, and a helper's worktree shares the main checkout's memory. Tasks shared by two
homes through a git remote named or given by URL; a list of one task's steps."""
from dataclasses import replace
import json
import os
import queue
from pathlib import Path
import shutil
import subprocess
import threading
from claude import reply
from driver import flow
from model import tool
from responses import PLAIN, call as response_call, done, response
from server import post, send, stop, until

def task_events(f):
    stream = send(f, "GET", "/status")
    stream.readline()
    found = queue.Queue()
    def collect():
        for line in stream:
            event = json.loads(line)
            if event.get("kind") == "tasks": found.put(event)
    threading.Thread(target=collect, daemon=True).start()
    return found

replies = []
with flow(replies) as f:
    f.base()
    session = None

    def use(tool, where=f, resume=None, words="Keep RULING_FILES_24 in the project.", **arguments):
        """A turn of `where`'s session `resume`, else a new one, calling `tool`: the session and its result."""
        replies.extend([reply((None, f"mcp__razor__{tool}", arguments)), reply(text="saved")])
        sid = where.ask(*(["--resume", resume] if resume else []), words)["session"]
        return sid, [e for e in where.record(sid) if e["kind"] == "result"][-1]

    def memory(**arguments):
        global session
        session, output = use("memory", resume=session, **arguments)
        assert output["outcome"] == "success", output
        return json.loads(output["output"])

    old = memory(action="write", kind="fact", text="Amber lamplight.", description="A light.")["id"]
    assert [h["id"] for h in memory(action="search", query="lamplight")["hits"]] == [old], "write was not searchable"
    assert not memory(action="search", query="lamp")["hits"], "search matched part of a word"
    new = memory(action="write", kind="fact", text="Indigo lantern.", description="A light.", replaces=old)["id"]
    retired = f.home / ".razor/memory/retired"
    assert use("memory", resume=session, action="write", kind="fact", text="Bare.")[1]["outcome"] == "error", "a fact needs a description"
    faded = memory(action="write", kind="fact", text="Faded ember.", description="Old.", expires="2020-01-01")["id"]
    assert not memory(action="search", query="faded")["hits"] and list(retired.rglob(f"{faded}.md")), "an expired memory stayed"
    assert not memory(action="search", query="lamplight")["hits"] and list(retired.rglob(f"{old}.md")), "replacement did not retire the old file"
    assert [h["id"] for h in memory(action="search", query="lantern")["hits"]] == [new], "replacement was not searchable"
    memory(action="write", replaces=new)
    assert not memory(action="search", query="lantern")["hits"] and list(retired.rglob(f"{new}.md")), "retirement did not move the file out of search"
    memory(action="write", kind="state", text="STATE_FILES_24 ready for a new session.", replaces="state")
    ruling = memory(action="write", kind="ruling", text="Keep the marker.")["id"]
    person = next(e for e in reversed(list(f.record(session))) if e["kind"] == "input" and e["from"] == "person")
    original = memory(action="get", id=ruling)["file"]
    source = f"razor:{session}:l-{person['id']}"
    assert f"---\nKeep the marker.\n\n{source}\n> {person['text']}" in original and f"cites: {source}" in original, original
    other_words = "Keep the second marker too.\nExact words on two lines."
    _, second = use("memory", resume=session, words=other_words, action="write", kind="ruling", text="Keep both markers.")
    second = json.loads(second["output"])["id"]
    second_file = memory(action="get", id=second)["file"]
    assert "> " + other_words.replace("\n", "\n> ") in second_file, "ruling lost multiline original words"
    for fields, refusal in [({"ref": 1}, "holds no words"), ({"expires": "2099-01-01"}, "standing decision")]:
        failed = use("memory", resume=session, action="write", kind="ruling", text="Must not land.", **fields)[1]
        assert failed["outcome"] == "error" and refusal in failed["output"], failed
    legacy = next((f.home / ".razor/memory").rglob(f"{ruling}.md")).with_name("ruling-legacy24.md")
    legacy.write_text(f'---\nsource: {source}\n---\nthe human said ({source}): "LEGACY_WORDS_24"\n\nagent\'s reading:\nRead the old marker.')
    # A ruling from before Razor, as the one-time import wrote it, and one with no quotation.
    old_words = "loom:main-r1:l-7\n> PRE_RAZOR_WORDS_24"
    legacy.with_name("ruling-old24.md").write_text(f"---\nsource: {source}\n---\nKeep the old marker.\n\n{old_words}")
    legacy.with_name("ruling-bare24.md").write_text(f"---\nsource: {source}\ncites: {source}\n---\nKeep the bare marker.")
    replies.append(reply(text="opened"))
    f.ask("Read project memory.")
    page = f.sent()[-1]
    assert "STATE_FILES_24" in page and "Keep the marker." in page and ruling in page, "new session lost state or ruling"
    assert "RULING_FILES_24" not in page and "LEGACY_WORDS_24" not in page and "Read the old marker." in page, "page showed quotations or lost the old rule"
    replies.extend([reply((None, "mcp__razor__agent", {"action": "start", "name": "helper",
                         "brief": "Read the project's memory and answer."}), match="START_HELPER_24"),
                    reply(text="started", match="START_HELPER_24"),
                    reply(text="read", match="You are helper,"),
                    reply(text="received", match="START_HELPER_24")])
    root = f.ask("START_HELPER_24")["session"]
    call = next(e for e in f.record(root) if e["kind"] == "result")
    child = json.loads(call["output"])["session"]
    entries = until(f, child, lambda es: any(e["kind"] == "turn_end" for e in es))
    until(f, root, lambda es: sum(e["kind"] == "turn_end" for e in es) >= 2)
    assert entries[0]["cwd"] != str(f.cwd) and (Path(entries[0]["cwd"]) / ".git").is_file(), "helper was not in a git worktree"
    page = next(body for body in f.sent() if "You are helper," in body)
    assert "STATE_FILES_24" in page and ruling in page, "helper lost main checkout's memory"

    # Only dreaming writes pages. Its fresh run closes after the scheduled turn and sees
    # work awaiting merge on a live helper branch, rather than calling it a change on main.
    assert use("memory", resume=session, action="write", kind="page", name="guide",
               description="Guide.", text="Must not land.")[1]["outcome"] == "error", "ordinary session wrote a wiki page"
    helper = replace(f, cwd=Path(entries[0]["cwd"]))
    (helper.cwd / "base").write_text("Changed on the helper branch.")
    helper.git("-c", "user.name=Flow", "-c", "user.email=flow@example.invalid", "commit", "-am", "Change base")
    branch = helper.git("branch", "--show-current").strip()
    cite = f"file:base@{helper.git('rev-parse', 'HEAD').strip()}"
    cited = memory(action="write", kind="fact", text="Helper changed base.", description="Branch work.", cites=[cite])["id"]
    config = f.home / ".razor/config.toml"
    config.write_text(config.read_text() + PLAIN + '[memory]\nnight = false\nmodel = "sol"\n')
    sessions = f.home / ".razor/sessions"
    before, requests = set(sessions.iterdir()), len(f.sent())
    off = f.run("dream")
    assert off.returncode == 0 and "off" in off.stdout.lower(), off
    assert set(sessions.iterdir()) == before and len(f.sent()) == requests, "disabled dreaming started work"
    config.write_text(config.read_text().replace("night = false", 'night = "03:00"'))
    # Large ordinary histories plus wiki instructions used to cut older session ids out
    # after digest assembly. Exercise the actual schedule admission's 64 KiB bound.
    # The 24 sessions take their turns at once.
    replies.extend(reply(text="Source evidence. " * 100) for _ in range(24))
    fillers = [post(f, "/sessions", {"cwd": str(f.cwd)})["session"] for _ in range(24)]
    for filler in fillers:
        post(f, f"/sessions/{filler}/input", {"text": "Source evidence. " * 100})
    for filler in fillers:
        until(f, filler, lambda es: any(e["kind"] == "turn_end" for e in es), timeout=60)
    instructions = f.home / ".razor/memory/all/wiki/_instructions.md"
    instructions.parent.mkdir(parents=True, exist_ok=True)
    instructions.write_text("Keep useful source evidence.\n" * 560)
    before = set(sessions.iterdir())
    replies.extend([response([response_call("bash", '{"command":"touch forbidden-dream-write"}', "dream_shell"),
                             response_call("message", '{"type":"question","text":"Keep both markers?"}', "dream_question")]), done(), done(),
                    response([response_call("memory", json.dumps({"action": "write", "kind": "ruling", "text": "Must not land."}), "dream_ruling")]),
                    response([response_call("memory", json.dumps({"action": "write", "kind": "ruling", "text": "Keep the markers together.", "replaces": [ruling, second]}), "dream_merge"),
                              response_call("memory", json.dumps({"action": "write", "kind": "ruling", "text": "Keep the old marker always.", "replaces": "ruling-old24"}), "dream_old"),
                              response_call("memory", json.dumps({"action": "write", "kind": "ruling", "text": "Keep the bare marker always.", "replaces": "ruling-bare24"}), "dream_bare"),
                              response_call("memory", json.dumps({"action": "write", "kind": "ruling", "text": "Keep it.\n\nloom:main-r1:l-8\n> NEW_WORDS", "replaces": "ruling-legacy24"}), "dream_new")]), done()])
    started = f.run("dream", str(f.cwd))
    assert started.returncode == 0, started
    dream = started.stdout.strip()
    dreamed = until(f, dream, lambda es: any(e["kind"] == "turn_end" for e in es))
    assert set(sessions.iterdir()) - before == {sessions / dream}, "dreaming started more than one session"
    scheduled = next(e for e in dreamed if e["kind"] == "input")
    assert len(scheduled["text"].encode()) <= 64 * 1024, "dream exceeded schedule input bound"
    assert all(f"- {p.name}" in scheduled["text"] for p in before), "dream omitted a source session"
    assert scheduled["from"]["schedule"]["name"] == "dream", scheduled
    assert any(e["kind"] == "turn_end" and e.get("closed") and scheduled["id"] in e["inputs"] for e in dreamed), dreamed[-3:]
    refused = next(e for e in dreamed if e["kind"] == "result" and e["call"] == "dream_shell")
    assert refused["outcome"] == "error" and not (f.cwd / "forbidden-dream-write").exists(), refused
    assert any(all(part in line for part in (cited, cite, "awaiting merge", branch))
               for line in scheduled["text"].splitlines()), "citation lost its unmerged helper branch"
    question = next(e["id"] for e in dreamed if e.get("call") == "dream_question" and e["kind"] == "call")
    answer = post(f, f"/sessions/{dream}/asks/{question}", {"text": "Keep this decision in the project."})["entry"]
    dreamed = until(f, dream, lambda es: any(e["kind"] == "turn_end" and answer in e["inputs"] for e in es))
    assert dreamed[-1].get("closed"), "dream stayed open after the person's answer"
    # A person message gives a valid default citation; dreaming must still refuse a new ruling.
    f.ask("--resume", dream, "Keep this decision in the project.")
    dreamed = list(f.record(dream))
    refused = next(e for e in dreamed if e["kind"] == "result" and e["call"] == "dream_ruling")
    assert refused["outcome"] == "error" and "replacing existing rulings" in refused["output"], refused
    merged = next(e for e in dreamed if e.get("call") == "dream_merge" and e["kind"] == "result")
    assert merged["outcome"] == "success", merged
    combined = memory(action="get", id=json.loads(merged["output"])["id"])["file"]
    for previous in (original, second_file):
        quote = previous.split("---\n", 2)[-1].split("\n\n", 1)[1]
        assert quote in combined and f"cites: {quote.splitlines()[0]}" in combined, "merged ruling lost original words or citation"
    assert all(list(retired.rglob(f"{old}.md")) and not legacy.with_name(f"{old}.md").exists()
               for old in (ruling, second)), "merge did not retire both rulings"
    # A replacement carries a pre-Razor quotation under its own source, unchanged and outside the
    # cites; a ruling with no quotation, or a new quotation written into the rule, is still refused.
    carried, bare, new = (next(e for e in dreamed if e.get("call") == call and e["kind"] == "result")
                          for call in ("dream_old", "dream_bare", "dream_new"))
    assert carried["outcome"] == "success", carried
    carried = memory(action="get", id=json.loads(carried["output"])["id"])["file"]
    assert carried.endswith(f"Keep the old marker always.\n\n{old_words}") and "cites: loom" not in carried, carried
    assert bare["outcome"] == "error" and legacy.with_name("ruling-bare24.md").exists(), bare
    assert new["outcome"] == "error" and "one line" in new["output"] and legacy.exists(), new

    # Two homes share tasks through a bare repository's hidden ref: a claim on one is refused on
    # the other, a write while the remote is unreachable is refused, and closing releases a claim.
    remote = f.home / "tasks.git"
    there = lambda *a: subprocess.run(["git", "--git-dir", remote, *a], capture_output=True, text=True).stdout
    there("init", "--bare")
    f.git("config", "user.name", "Flow")
    f.git("config", "user.email", "flow@example.invalid")
    f.git("remote", "add", "origin", "~/tasks.git")  # git's own home form, which it expands
    (f.cwd / ".razor").mkdir()
    (f.cwd / ".razor/config.local.toml").write_text('[tasks]\nremote = "origin"\n')
    b = replace(f, home=f.home / "b", env={**f.env, "RAZOR_HOME": str(f.home / "b/.razor")})
    shutil.copytree(f.home / ".razor", b.home / ".razor", ignore=lambda _, names: set(names) - {"config.toml", "auth.json"})
    mine, claimed = use("task", action="write", title="Ship it", claim=True)
    f.git("remote", "set-url", "origin", f"{remote}-gone")
    assert use("task", b, action="write", title="Offline")[1]["outcome"] == "error", "an offline write landed"
    f.git("remote", "set-url", "origin", "~/tasks.git")
    # A home whose first read failed has fetched nothing yet, so its page still points to the list.
    theirs, refused = use("task", b, action="write", id=1, claim=True)
    assert json.loads(claimed["output"])["claimed"] and refused["outcome"] == "error", (claimed, refused)
    post(f, f"/sessions/{mine}/quit", {})
    until(f, mine, lambda _: "claim:" not in there("show", "refs/razor/tasks:1.md"))
    assert json.loads(use("task", b, theirs, action="write", id=1, claim=True)[1]["output"])["claimed"]
    events = task_events(f)
    sync_project = str(f.cwd.resolve())
    use("task", b, theirs, action="write", id=1, close="Shipped.")
    os.utime(next((f.home / ".razor/tasks").rglob("FETCH_HEAD")), (0, 0))
    for _ in range(2): send(f, "GET", f"/sessions/{mine}/tasks?filter=open").read()
    try: synced = events.get(timeout=2)
    except queue.Empty: synced = None
    os.utime(next((f.home / ".razor/tasks").rglob("FETCH_HEAD")), (0, 0))
    send(f, "GET", f"/sessions/{mine}/tasks?filter=open").read()
    try: extra = events.get(timeout=1)
    except queue.Empty: extra = None
    assert synced == {"kind": "tasks", "project": sync_project} and extra is None, (synced, extra)
    assert "Shipped." in there("show", "refs/razor/tasks:closed/1.md") and not there("show", "refs/razor/tasks:2.md")
    assert there("for-each-ref", "--format=%(refname)") == "refs/razor/tasks\n", "pushed more than the tasks ref"
    # A remote named by its path, read from the checkout, keeps the tasks there when origin moves; a
    # list keeps one task's steps.
    (f.cwd / ".razor/config.local.toml").write_text(f'[tasks]\nremote = "{os.path.relpath(remote, f.cwd)}"\n')
    f.git("remote", "set-url", "origin", f"{remote}-gone")
    pinned = [use("task", b, theirs, action="write", title=t, parent=n)[1]["outcome"] for t, n in [("Step", 1), ("Other", 0)]]
    assert pinned == ["success"] * 2, f"a relative path as the remote did not take the tasks: {pinned}"
    listed = json.loads(use("task", b, theirs, action="list", parent=1)[1]["output"])["tasks"]
    assert [t["id"] for t in listed] == [2] and "Step" in there("show", "refs/razor/tasks:2.md"), listed
    # A claim made on a view fetched within 30 seconds, which the other home has since changed, is
    # refused once its push is; a push that cannot reach the remote refuses, nothing written.
    os.utime(next((f.home / ".razor/tasks").rglob("FETCH_HEAD")), (0, 0))
    use("task", action="list")
    assert json.loads(use("task", b, theirs, action="write", id=3, claim=True)[1]["output"])["claimed"]
    assert "is claimed by" in (late := use("task", action="write", id=3, claim=True)[1])["output"] and there("show", "refs/razor/tasks:3.md").count("claim:") == 1, late
    (f.cwd / ".razor/config.local.toml").write_text(f'[tasks]\nremote = "{remote}-gone"\n')
    gone = use("task", action="write", title="Unpushed")[1]
    assert "did not answer" in gone["output"] and not there("show", "refs/razor/tasks:4.md"), gone
    stop(b)

# A helper claims for itself, comments, releases and reclaims; settlement releases that claim
# and records its report. Forbidden fields are refused before any part of a write applies.
root, helper = "CHILD_TASKS_24", "You are writer24,"


forbidden = [{"title": "New task"}, *({"id": 1, k: v} for k, v in [
    ("close", "Closed"), ("title", "Changed"), ("text", "Changed"), ("priority", 0),
    ("due", "2026-12-31"), ("parent", 2), ("blocked_by", [2]), ("title", None), ("extra", True)])]
replies = [tool(root, "task", action="write", title="Helper work"),
           tool(root, "task", action="write", title="Parent work", claim=True),
           tool(root, "agent", action="start", name="writer24", task=2, brief="Claim and comment on task 1."),
           tool(root, "agent", action="wait"),
           tool(root, "agent", action="settle", child="writer24", decision="merge"),
           tool(root, "task", action="get", id=1),
           tool(root, "task", action="list", filter="open"), reply(text="done", match=root),
           tool(helper, "task", action="write", id=1, claim=True, comment="Claimed by helper."),
           tool(helper, "task", action="write", id=1, comment="Evidence from helper."),
           tool(helper, "task", action="write", id=1, claim=False),
           tool(helper, "task", action="get", id=1),
           tool(helper, "task", action="write", id=1, claim=True),
           tool(helper, "task", action="write", id=3, claim=True),
           tool(helper, "task", action="write", id=3, claim=False, comment="Must not land."),
           *(tool(helper, "task", action="write", comment="Must not land.", **fields) for fields in forbidden),
           tool(helper, "agent", action="finish", status="finished", report="Helper task evidence.\nKept in full.")]
with flow(replies) as f:
    f.base()
    other = replace(f, cwd=f.cwd / "other")
    other.cwd.mkdir()
    other.base()
    replies[2:2] = [tool(root, "task", action="write", title="Unassigned work", claim=True)]
    replies[6:6] = [tool(root, "agent", action="start", name="other24", cwd=str(other.cwd), brief="Claim task 1."),
                    tool(root, "agent", action="wait"),
                    tool(root, "agent", action="settle", child="other24", decision="discard")]
    replies += [tool("OTHER_TASK_24", "task", action="write", title="Other work"),
                tool("OTHER_TASK_24", "task", action="write", title="Nested work"), reply(text="saved", match="OTHER_TASK_24"),
                tool("You are other24,", "task", action="write", id=1, claim=True),
                tool("You are other24,", "agent", action="start", name="nested24", brief="Claim task 2 and commit your work."),
                tool("You are other24,", "agent", action="wait"),
                tool("You are nested24,", "task", action="write", id=2, claim=True),
                reply((None, "Bash", {"command": "printf nested > nested-marker && git add nested-marker && git -c user.name=Flow -c user.email=flow@example.invalid commit -m Nested"}), match="You are nested24,"),
                tool("You are nested24,", "agent", action="finish", status="finished", report="Nested project report."),
                tool("You are other24,", "agent", action="finish", status="finished", report="Other project report.")]
    assert other.run("--print=json", "OTHER_TASK_24").returncode == 0
    parent = f.ask(root)["session"]
    results = [e for e in f.record(parent) if e["kind"] == "result"]
    child = json.loads(results[3]["output"])["session"]
    writes = [e for e in f.record(child) if e["kind"] == "result"][:-1]
    assert [e["outcome"] for e in writes[:5]] == ["success"] * 5, writes[:5]
    assert all(json.loads(writes[n]["output"])["claimed"] for n in (0, 4)), writes[:5]
    assert "claim:" not in json.loads(writes[3]["output"])["file"], writes[3]
    assert all(e["outcome"] == "error" and "is claimed by cwd on" in e["output"] for e in writes[5:7]), writes[5:7]
    assert all(e["outcome"] == "error" and "claim" in e["output"] and "comment" in e["output"] for e in writes[7:]), writes[7:]
    task = json.loads(results[-2]["output"])
    assert "claim:" not in task["file"] and "closed:" not in task["file"], task
    assert sorted(c["text"].strip() for c in task["comments"]) == ["Claimed by helper.", "Evidence from helper.", "Helper task evidence.\nKept in full."], task
    tasks = json.loads(results[-1]["output"])["tasks"]
    assert [(t["title"], t["state"], t["held"]) for t in tasks] == [("Unassigned work", "in progress", True), ("Helper work", "ready", False)], tasks
    assigned = next(p for p in (f.home / ".razor/tasks").rglob("closed/2.md"))
    assert "claim:" not in assigned.read_text() and "Helper task evidence." in assigned.read_text(), assigned.read_text()
    # The viewer's task list is bounded after held-first ordering, and detail is structured.
    folder = assigned.parent.parent
    server_id = (f.home / ".razor/server.id").read_text().strip()
    for n in range(10, 216):
        claim = f"claim: {parent} {server_id} viewer on flow\n" if n == 65 else ""
        relation = "parent: 64\nblocked-by: 63\n" if n == 65 else "parent: 65\n" if n == 10 else ""
        (folder / f"{n}.md").write_text(f"---\ntitle: Task {n}\npriority: 4\n{claim}{relation}---\nDescription {n}")
    comments = folder / "comments/65"
    comments.mkdir(parents=True)
    for n in range(22):
        (comments / f"20261108-{n:06}.md").write_text(f"---\nby: {parent} {server_id} viewer on flow\n---\nComment {n}")

    def task_api(path):
        response = send(f, "GET", f"/sessions/{parent}/tasks{path}")
        data = response.read()
        return response.status, json.loads(data)

    _, opened = task_api("?filter=open")
    replies.extend([tool("LIST_TASKS_24", "task", action="list", filter="open"), reply(text="listed", match="LIST_TASKS_24")])
    f.ask("--resume", parent, "LIST_TASKS_24")
    listed = json.loads([e for e in f.record(parent) if e["kind"] == "result"][-1]["output"])
    assert listed == {k: opened[k] for k in ("tasks", "more", "counts")}, listed
    _, search = task_api("?filter=open&q=Task&limit=1000")
    _, done = task_api("?filter=closed&mine=true")
    status, detail = task_api("/65")
    failures = []
    if f"closed-by: {parent} {server_id} " not in assigned.read_text(): failures.append("settlement closer")
    if not any(t["id"] == 65 and t.get("held") for t in opened.get("tasks", [])) or any(a.get("held") is False and b.get("held") for a, b in zip(opened["tasks"], opened["tasks"][1:])): failures.append("held-first past 50")
    if not any(t.get("id") == 65 and t.get("held") is True for t in search.get("tasks", [])) or any(t.get("held") is not False for t in search.get("tasks", []) if t["id"] != 65): failures.append("viewer holds")
    if opened.get("project") != str(f.cwd.resolve()) or not opened.get("counts"): failures.append("project and counts")
    if any(t.get("claimant") != (parent if t["id"] == 65 else None) for t in search.get("tasks", [])): failures.append("claimant")
    if len(search.get("tasks", [])) != 200 or search.get("more") != 6: failures.append("query and limit")
    if [t["id"] for t in done.get("tasks", [])] != [2] or not done["tasks"][0].get("closed_by"): failures.append("mine and closer")
    if detail.get("project") != str(f.cwd.resolve()): failures.append("detail project")
    if status != 200 or detail.get("text") != "Description 65" or len(detail.get("comments", [])) != 20 or detail.get("more") != 2: failures.append("detail and comments")
    if status != 200 or (detail.get("parent") or {}).get("id") != 64 or [t["id"] for t in detail.get("blockers", [])] != [63] or [t["id"] for t in detail.get("parts", [])] != [10]: failures.append("detail relations")
    lines = task_events(f)
    task_api("?filter=open")
    replies.extend([tool("CLOSE_TASK_24", "task", action="write", id=65, close="Complete."), tool("CLOSE_TASK_24", "task", action="write", id=10, close="Also complete."), reply(text="closed", match="CLOSE_TASK_24")])
    f.ask("--resume", parent, "CLOSE_TASK_24")
    task_api("?filter=open")
    events = []
    try:
        while True:
            events.append(lines.get(timeout=2))
    except queue.Empty:
        pass
    if events != [{"kind": "tasks", "project": str(f.cwd.resolve())}] * 2: failures.append("one event per change and none on reads")
    _, done = task_api("?filter=closed&mine=true")
    if [t["id"] for t in done.get("tasks", [])[:2]] != [10, 65]: failures.append("same-day closes newest first")
    if f"closed-by: {parent} {server_id} " not in (folder / "closed/65.md").read_text(): failures.append("writer closer")
    assert not failures, failures
    other_task = next(p for p in (f.home / ".razor/tasks").rglob("1.md") if "title: Other work" in p.read_text())
    assert "claim:" not in other_task.read_text(), other_task.read_text()
    assert [p.read_text().split("---\n")[-1].strip() for p in other_task.parent.glob("comments/1/*.md")] == ["Other project report."]
    nested_task = other_task.with_name("2.md")
    stop(f)
    assert f.run("devices").returncode == 0
    until(f, parent, lambda _: "claim:" not in nested_task.read_text(), timeout=75)
    assert "closed:" not in nested_task.read_text(), nested_task.read_text()
    assert [p.read_text().split("---\n")[-1].strip() for p in nested_task.parent.glob("comments/2/*.md")] == ["Nested project report."]

# A start claims the child's project, and reports both the claim and a competing refusal.
    replies += [tool("START_24", "agent", action="start", name="claim24", cwd=str(other.cwd), task=1, brief="Hold."),
                tool("START_24", "agent", action="wait"), reply(text="Started.", match="START_24"), reply(text="Held.", match="You are claim24,")]
    parent = f.ask("START_24")["session"]
    launched = json.loads(next(e["output"] for e in f.record(parent) if e["kind"] == "result"))
    replies += [tool("COMPETE_24", "agent", action="start", name="compete24", cwd=str(other.cwd), task=1, brief="Hold."),
                tool("COMPETE_24", "agent", action="wait"), reply(text="Started.", match="COMPETE_24"), reply(text="Held.", match="You are compete24,")]
    third = replace(f, cwd=f.cwd / "third")
    third.cwd.mkdir()
    third.base()
    second = third.ask("COMPETE_24")["session"]
    refused = json.loads(next(e["output"] for e in f.record(second) if e["kind"] == "result"))
    files = list((f.home / ".razor/tasks").rglob("1.md"))
    claimed = next(p.read_text() for p in files if "title: Other work" in p.read_text())
    untouched = next(p.read_text() for p in files if "title: Helper work" in p.read_text())
    assert launched.get("task") == {"id": 1, "title": "Other work", "claimed": True} and launched["session"] in claimed and "claim:" not in untouched, (launched, claimed, untouched, refused)
    assert isinstance(refused.get("task"), str) and "is claimed by" in refused["task"], refused
