"""Helpers under one server, each tree in its own folder of a folder outside git: a shared brief
and a merge, a reviewer's discard and settled names, a question and its answer, the channel and
status board, continuing a finished child, and last the cap on working children."""
from dataclasses import replace
import json
from pathlib import Path
import re
import claude
from driver import flow
from model import agent, message, reply, tool
import server

replies = []
config = ('startup = ["razor", "~/parent.md"]\n[server]\nname = "desk"\n'
          '[children]\nreviewer = "reviewer-12.md"\nstartup = ["child.md", "project"]\n')
with flow(replies, config=config) as top:
    (top.home / "parent.md").write_text("PARENT_STARTUP_16")
    (top.home / ".razor/child.md").write_text("CHILD_STARTUP_16")
    (top.home / ".razor/reviewer-12.md").write_text("REVIEWER_TEXT_12: review what the brief names.\n")

    def folder(name):
        """The flow as seen from a new folder of its own."""
        (top.cwd / name).mkdir()
        return replace(top, cwd=top.cwd / name)

    def children_of(parent):
        """The session ids of `parent`'s children."""
        return [p.parent.name for p in (top.home / ".razor/sessions").glob("*/record.jsonl")
                if (next(top.record(p.parent.name)).get("child") or {}).get("parent") == parent]

    # A shared brief reaches a fresh child; its committed branch merges and disappears with its
    # worktree, whose uncommitted files move to the child's session folder, and a channel mention it
    # held when it finished ends with the settle, so nothing keeps the child live.
    root, child = "FLOW_ROOT_11", "You are oak11,"
    replies += [agent(root, "start", name="oak11", brief="Commit the child marker.", shared_brief="shared.md"),
                message(root, "note", to="channel", text="@oak11 HELD_NOTE_11"),
                agent(root, "wait", children=["oak11"]),
                agent(root, "settle", child="oak11", decision="merge"), reply(root),
                # The child works until the note is in its record; the bracket keeps the pattern from
                # matching this command's own text there.
                reply(child, "Bash", {"command": "until grep -qs 'HELD_NOTE_1[1]' \"$RAZOR_HOME/sessions/$RAZOR_SESSION/record.jsonl\"; do sleep 0.05; done; printf child > child-marker; git add -- child-marker && git -c user.name=Flow -c user.email=flow@example.invalid commit -m 'Child marker'; mkdir __pycache__ && printf pyc > __pycache__/m.pyc && printf ' edited' >> shared.md"}),
                agent(child, "finish", status="finished", report="Committed child-marker.")]
    f = folder("merge11")
    (f.cwd / "shared.md").write_text("SHARED_BRIEF_11: keep this exact sentence.")
    f.base("shared.md")
    (f.cwd / "base").write_text("uncommitted parent edit")
    parent = f.ask(root)["session"]
    children = children_of(parent)
    assert len(children) == 1, children
    sid = children[0]
    entries = list(f.record(sid))
    start = entries[0]
    assert start["child"]["parent"] == parent and start["child"]["name"] == "oak11", start
    first = next(e for e in entries if e["kind"] == "input")
    assert first["text"].index("SHARED_BRIEF_11") < first["text"].index("Commit the child marker."), first
    assert (f.cwd / "child-marker").read_text() == "child", "merge did not land the child commit"
    assert (f.cwd / "base").read_text() == "uncommitted parent edit", "merge overwrote the person's edit"
    assert f.git("show", "HEAD:base") == "base", "the child inherited the parent's dirty edit"
    assert not (f.home / ".razor/sessions" / sid / "worktree").exists(), "settle kept the worktree"
    kept = f.home / ".razor/sessions" / sid / "uncommitted"
    assert (kept / "__pycache__/m.pyc").read_text() == "pyc", "settle lost an untracked file"
    assert (kept / "shared.md").read_text().endswith(" edited"), "settle lost an uncommitted edit"
    settle = next(e for e in f.record(parent) if e["kind"] == "call" and e["input"].get("action") == "settle")
    said = json.loads(next(e["output"] for e in f.record(parent) if e["kind"] == "result" and e["call"] == settle["call"]))["text"]
    assert f"Moved into {kept}, at their paths: `shared.md`, `__pycache__/`." in said, said
    assert f"child/{sid}" not in f.git("branch", "--list", f"child/{sid}"), "merged branch survived"
    held = [e["id"] for e in entries if e["kind"] == "input" and "HELD_NOTE_11" in e["text"]]
    ended = [i for e in entries if e["kind"] == "turn_end" for i in e["inputs"]]
    withdrawn = [i for e in entries if e["kind"] == "withdrawn" for i in e["inputs"]]
    assert len(held) == 1 and held[0] in withdrawn and held[0] not in ended, (
        "the held note was not withdrawn at the settle", held, withdrawn, ended)
    assert not (f.home / ".razor/live" / sid).exists(), "a settled child stayed live"
    reports = [e for e in f.record(parent) if e["kind"] == "input" and isinstance(e["from"], dict) and "report" in e["from"]]
    assert len(reports) == 1 and reports[0]["text"] == "Committed child-marker.", reports
    requests = f.sent()
    shown = [request for request in requests if root in request and "Committed child-marker." in request]
    assert shown, "the waiting parent never saw its child's report"
    assert all(request.count("Committed child-marker.") == 1 for request in shown), "wait and the input notice delivered the report twice"

    # A reviewer gets its own worktree and branch and the person's reviewer text after its contract;
    # discard closes it, commits its uncommitted files to its branch and removes its worktree,
    # not its committed branch. Settled row facts are read directly after leaving the live feed.
    root, child = "FLOW_ROOT_12", "You are oak12,"
    replies += [agent(root, "start", name="oak12", kind="reviewer", brief="Commit the discard marker."),
                agent(root, "wait", children=["oak12"]),
                agent(root, "settle", child="oak12", decision="discard"), reply(root),
                reply(child, "Bash", {"command": "printf discarded > discard-marker; git add -- discard-marker && git -c user.name=Flow -c user.email=flow@example.invalid commit -m 'Discard marker'; mkdir __pycache__ && printf pyc > __pycache__/m.pyc && printf ' edited' >> base"}),
                agent(child, "finish", status="partial", report="Partial work committed.")]
    f = folder("discard12")
    f.base()
    f.git("config", "user.name", "Flow")
    f.git("config", "user.email", "flow@example.invalid")
    before = f.git("rev-parse", "HEAD")
    parent = f.ask(root)["session"]
    sid = children_of(parent)[0]
    first = next(e["text"] for e in f.record(sid) if e["kind"] == "input")
    assert first.index(child) < first.index("REVIEWER_TEXT_12") < first.index("Commit the discard marker."), first
    assert f.git("rev-parse", "HEAD") == before, "discard changed the parent's commit"
    assert f.git("show", f"child/{sid}:discard-marker") == "discarded", "discard deleted the committed branch"
    assert not (f.cwd / "discard-marker").exists(), "discard merged the child's file"
    assert not (f.home / ".razor/sessions" / sid / "worktree").exists(), "discard kept the worktree"
    assert f.git("show", f"child/{sid}:__pycache__/m.pyc") == "pyc", "discard lost an untracked file"
    assert f.git("show", f"child/{sid}:base") == "base edited", "discard lost an uncommitted edit"
    entries = list(f.record(parent))
    reports = [e for e in entries if e["kind"] == "input" and isinstance(e["from"], dict) and "report" in e["from"]]
    assert len(reports) == 1 and reports[0]["from"]["report"]["status"] == "partial", reports
    row = json.load(server.send(f, "GET", f"/sessions/{sid}/agent"))
    assert row["settled"] == "discard", row

    # A name used only under a settled helper is free again; a settled child's own name is not.
    elm, ash = "You are elm12,", "You are ash12,"
    replies += [agent(root, "start", name="elm12", placement="in_place", brief="Start one."), agent(root, "wait"),
                agent(root, "settle", child="elm12", decision="discard"),
                *(agent(root, "start", name=n, placement="in_place", brief="Finish.") for n in ("ash12", "elm12")),
                tool(root, "status"), agent(root, "wait"),
                agent(root, "settle", child="ash12", decision="discard"), reply(root),
                agent(elm, "start", name="ash12", placement="in_place", brief="Finish."), agent(elm, "wait"),
                agent(elm, "finish", status="finished", report="Elm done."),
                *(agent(ash, "finish", status="finished", report="Ash done.") for _ in "12")]
    assert f.run("--resume", parent, "--print=json", "Grow the tree.").returncode == 0
    entries = list(f.record(parent))
    outputs = {e["call"]: e for e in entries if e["kind"] == "result"}
    calls = [e for e in entries if e["kind"] == "call"]
    starts = [outputs[e["call"]] for e in calls if e["input"].get("action") == "start"]
    assert [s["outcome"] for s in starts[1:]] == ["success", "success", "error"], starts
    assert "`elm12` is taken in this tree" in starts[3]["output"], starts[3]
    board = json.loads(next(outputs[e["call"]]["output"] for e in calls if "status" in e["name"]))["agents"]
    assert sorted(a["session"] for a in board) == sorted([parent, json.loads(starts[2]["output"])["session"]]), board

    # A child's question interrupts a wait; the parent's answer reaches the child, under a header
    # naming the parent and the question. A researcher outside git works in its parent's folder and
    # is told Razor's researcher text.
    root, child = "FLOW_ROOT_13", "You are asker13,"
    replies += [agent(root, "start", name="asker13", kind="researcher", brief="Ask which marker to use."),
                agent(root, "wait", children=["asker13"], timeout_ms=600000), reply(root, text="Question received."),
                message(child, "question", text="Which exact marker?"), reply(child, text="Waiting for the parent.")]
    f = folder("question13")
    parent = f.ask(root, timeout=30)["session"]
    sid = children_of(parent)[0]
    start = next(f.record(sid))
    assert start["cwd"] == str(f.cwd.resolve()), "a researcher outside git left its parent's folder"
    researcher = (Path(__file__).parents[1] / "src/children/text/researcher.md").read_text().strip()
    first = next(e["text"] for e in f.record(sid) if e["kind"] == "input")
    assert first.index(child) < first.index(researcher) < first.index("Ask which marker to use."), first
    questions = [e for e in f.record(sid) if e["kind"] == "call" and e["input"].get("type") == "question"]
    assert len(questions) == 1, questions
    question = [sid, questions[0]["id"]]
    admitted = [e for e in f.record(parent) if e["kind"] == "input" and e["text"] == "Which exact marker?"]
    assert len(admitted) == 1 and admitted[0]["from"]["mail"]["id"] == question, admitted
    waits = [e for e in f.record(parent) if e["kind"] == "call" and e["input"].get("action") == "wait"]
    ended = next(e for e in f.record(parent) if e["kind"] == "result" and e["call"] == waits[0]["call"])
    assert ended["at"] - waits[0]["at"] < 30000, "a question did not interrupt the ten-minute wait"
    replies += [message(root, "answer", to="asker13", text="Use copper-heron-13.", question=question),
                agent(root, "wait", children=["asker13"]),
                agent(root, "settle", child="asker13", decision="discard"), reply(root),
                agent(child, "finish", status="finished", report="Used copper-heron-13.")]
    f.ask("--resume", parent, "Answer the child.")
    answers = [e for e in f.record(sid) if e["kind"] == "input" and e["text"] == "Use copper-heron-13."]
    assert len(answers) == 1, answers
    assert answers[0]["from"]["mail"]["kind"] == {"answer": {"question": question}}, answers
    requests = f.sent()
    assert any(child in request and "Use copper-heron-13." in request for request in requests), "the admitted answer never reached the model"
    # The root is named by the server's name and its folder's.
    header = rf"desk-{f.cwd.name} \(your parent\) answers #{question[1]}: Use copper-heron-13\."
    assert any(re.search(header, request) for request in requests), "the answer reached the model without its header"

    # Only a channel mention wakes the root; compact status shows headlines and files; the root
    # answers to its server's and project's name, not main.
    root, child = "FLOW_ROOT_15", "You are oak15,"
    replies += [tool(root, "status"), reply(root, text="Named.")]
    f = folder("channel15")
    f.base()
    parent = f.ask(root)["session"]

    def boards():
        entries = list(f.record(parent))
        calls = [e["call"] for e in entries if e["kind"] == "call" and "status" in e["name"]]
        return [json.loads(e["output"])["agents"] for e in entries if e["kind"] == "result" and e["call"] in calls]

    alone = boards()[0]
    name = alone[0]["name"]
    assert len(alone) == 1 and name == f"desk-{f.cwd.name}", alone
    replies += [agent(root, "start", name="oak15", brief="Commit a marker and post to the tree."),
                agent(root, "wait", children=["oak15"]), tool(root, "status"),
                agent(root, "wait", children=["oak15"]),
                agent(root, "settle", child="oak15", decision="discard"), reply(root),
                agent(child, "headline", text="Working on channel flow."),
                reply(child, "Bash", {"command": "printf owned > owned15; git add -- owned15 && git -c user.name=Flow -c user.email=flow@example.invalid commit -m 'Owned marker'"}),
                message(child, "note", to="channel", text="QUIET_POST_15"),
                message(child, "note", to="channel", text=f"@{name} @{name} MENTION_POST_15"),
                agent(child, "finish", status="finished", report="Channel work finished.")]
    f.ask("--resume", parent, "Start the lane.")
    entries = list(f.record(parent))
    inputs = [e for e in entries if e["kind"] == "input"]
    assert not any(e["text"] == "QUIET_POST_15" for e in inputs), inputs
    mentions = [e for e in inputs if e["text"] == f"@{name} @{name} MENTION_POST_15"]
    assert len(mentions) == 1, mentions
    assert mentions[0]["from"]["mail"]["kind"] == "mention" and mentions[0]["from"]["mail"]["from"] == "oak15", mentions
    sid = mentions[0]["from"]["mail"]["id"][0]
    channels = [p for p in (f.home / ".razor/channels").glob("*.jsonl") if "POST_15" in p.read_text()]
    assert len(channels) == 1 and channels[0].stem == parent, channels
    posts = [json.loads(line) for line in channels[0].read_text().splitlines()]
    assert [p["text"] for p in posts] == ["QUIET_POST_15", f"@{name} @{name} MENTION_POST_15"], posts
    assert posts[1]["id"] == mentions[0]["from"]["mail"]["id"], (posts, mentions)
    rows = boards()[-1]
    assert len(rows) == 2, rows
    main = next(row for row in rows if row["session"] == parent)
    lane = next(row for row in rows if row["session"] == sid)
    assert main["name"] == name and main["branch"] is None and main["changed_files"] == [], main
    assert lane["headline"] == "Working on channel flow." and lane["changed_files"] == ["owned15"], lane

    # Mail does not reopen a finished child; continue reuses its session and branch. A child given
    # no model or effort runs on its parent's, not the config's defaults. A note sent to a child
    # after its finishing batch's last request stays unanswered, and its continuation reads it. Each
    # request of a twice-continued child starts with the one before, so its prompt cache is never
    # written again from the middle. Finished and fresh untouched rows are read directly outside
    # the live feed.
    root, child = "FLOW_ROOT_16A", "You are oak16,"
    replies += [agent(root, "start", name="oak16", brief="Commit the first marker."),
                agent(root, "wait", children=["oak16"]), reply(root, text="First report received."),
                # The child says what it is doing but never sets a headline.
                claude.reply((None, "Bash", {"command": "printf first > first16; git add -- first16 && git -c user.name=Flow -c user.email=flow@example.invalid commit -m 'First marker'"}),
                             text="\nCommitting the first marker.\nThen reporting.", match=child),
                agent(child, "finish", status="finished", report="First report.")]
    f = folder("continue16")
    (f.cwd / "AGENTS.md").write_text("PROJECT_STARTUP_16")
    f.base("AGENTS.md")
    f.git("remote", "add", "origin", "git@example.invalid:acme/widget16.git")
    parent = f.ask("--model", "opus", "--speed", "fast", "--effort", "low", root)["session"]
    children = children_of(parent)
    assert len(children) == 1, children
    sid = children[0]
    shown = (f.home / ".razor/sessions" / sid / "startup.md").read_text()
    assert "CHILD_STARTUP_16\n\nContents of " in shown and "/AGENTS.md:\n\nPROJECT_STARTUP_16" in shown and "PARENT_STARTUP_16" not in shown, \
        "the child did not use its independent startup list"
    named = [(e["model"], e["effort"]) for e in (next(f.record(parent)), next(f.record(sid)))]
    assert named == [("opus", "low")] * 2, f"the child did not inherit its parent's model and effort: {named}"
    assert [next(f.record(s))["speed"] for s in (parent, sid)] == ["fast", "standard"]
    assert server.listed(f, sid), "a helper with branch work left the agents list"
    before = list(f.record(sid))
    branch = f"child/{sid}"
    tip = f.git("rev-parse", branch).strip()
    replies += [message(root, "note", to="oak16", text="Record this without reopening."),
                tool(root, "status"),
                agent(root, "continue", child="oak16", speed="fast", brief="CONTINUE_BRIEF_16: commit the second marker."),
                agent(root, "wait", children=["oak16"]),
                agent(root, "settle", child="oak16", decision="discard"), reply(root),
                reply(child, "Bash", {"command": "printf second > second16; git add -- second16 && git -c user.name=Flow -c user.email=flow@example.invalid commit -m 'Second marker'"}),
                agent(child, "finish", status="partial", report="Second report.")]
    f.ask("--resume", parent, "Continue the child.")
    after = list(f.record(sid))
    assert json.load(server.send(f, "GET", f"/sessions/{sid}/agent"))["speed"] == "fast"
    assert after[:len(before)] == before, "continue rewrote history"
    assert len([e for e in after if e["kind"] == "start"]) == 1, after
    assert f.git("rev-parse", branch).strip() != tip, "continue made no new commit"
    assert f.git("show", f"{branch}:first16") == "first", "continue lost the old branch"
    assert f.git("show", f"{branch}:second16") == "second", "continue did not reuse the branch"
    assert f.git("merge-base", "--is-ancestor", tip, branch) == "", "continue started from another commit"
    entries = list(f.record(parent))
    status = next(e for e in entries if e["kind"] == "call" and "status" in e["name"])
    output = json.loads(next(e["output"] for e in entries if e["kind"] == "result" and e["call"] == status["call"]))
    row = next(row for row in output["agents"] if row["session"] == sid)
    assert row["state"] == "done", "mail reopened a finished child without continue"
    # A repository is named by its main checkout's folder, not its origin's last segment; a linked
    # worktree and a worktree child carry the same name.
    linked = f.cwd.parent / "linked16"
    f.git("worktree", "add", "-q", "-b", "linked16", str(linked))
    made = server.post(f, "/sessions", {"cwd": str(linked)})["session"]
    rows = {s: json.load(server.send(f, "GET", f"/sessions/{s}/agent")) for s in (parent, sid, made)}
    assert rows[parent]["repo"] == rows[sid]["repo"] == rows[made]["repo"] == "continue16", rows
    assert row["headline"] == "Committing the first marker.", row
    briefs = [e for e in after if e["kind"] == "input" and "CONTINUE_BRIEF_16" in e["text"]]
    assert len(briefs) == 1, briefs
    requests = f.sent()
    assert any(child in request and "CONTINUE_BRIEF_16" in request for request in requests), "the continuation input never reached the child"
    assert any(f"- Name: oak16 (session id {sid})" in request for request in requests), "the child's facts did not name it"
    reports = [e for e in entries if e["kind"] == "input" and isinstance(e["from"], dict) and "report" in e["from"]]
    assert [e["text"] for e in reports] == ["First report.", "Second report."], reports
    # A person's input to a settled child is refused with a sentence; nothing is recorded.
    answer = server.send(f, "POST", f"/sessions/{sid}/input", {"text": "PERSON_SETTLED_16"})
    assert answer.status == 400 and b"settled child" in answer.read(), answer.status
    assert "PERSON_SETTLED_16" not in json.dumps(list(f.record(sid)))

    # The note joins at the Bash result, after the request that called finish.
    root, child = "FLOW_ROOT_16B", "You are elm16,"
    f = folder("unread16")
    f.base()
    go = f.cwd / "go16"
    replies += [agent(root, "start", name="elm16", speed="fast", brief="Wait, then finish."),
                message(root, "note", to="elm16", text="UNREAD_NOTE_16"),
                reply(root, "Bash", {"command": f"sleep 1; touch {go}"}),
                agent(root, "wait", children=["elm16"]), reply(root, text="Done."),
                claude.reply((None, "Bash", {"command": f"until [ -e {go} ]; do sleep 0.1; done"}),
                             (None, "mcp__razor__agent", {"action": "finish", "status": "finished", "report": "Finished."}),
                             match=child)]
    parent = f.ask(root)["session"]
    child_sid = children_of(parent)[0]
    assert not server.listed(f, child_sid), "a finished read-only helper stayed in the agents list"
    entries = list(f.record(child_sid))
    assert entries[0]["speed"] == "fast"
    note = next(e["id"] for e in entries if e["kind"] == "input" and "UNREAD_NOTE_16" in e["text"])
    answered = [i for e in entries if e["kind"] == "turn_end" for i in e["inputs"]]
    assert note not in answered, "the finish answered a note the child never read"
    replies += [agent(root, "continue", child="elm16", brief="CONTINUE_16B"),
                agent(root, "wait", children=["elm16"]), reply(root, text="Again."),
                reply(child, "Bash", {"command": "true"}), agent(child, "finish", status="finished", report="Read it.")]
    f.ask("--resume", parent, "Continue elm16.")
    requests = f.sent()
    assert any(child in r and "CONTINUE_16B" in r and "UNREAD_NOTE_16" in r for r in requests), "the continuation lost the unread note"
    answered = [i for e in f.record(child_sid) if e["kind"] == "turn_end" for i in e["inputs"]]
    assert note in answered, "the continuation left the note unanswered"
    # A person's input to the finished, unsettled child continues it, as the parent's continue does.
    replies += [agent(child, "finish", status="finished", report="Heard the person."), reply(root, text="Heard.")]
    server.post(f, f"/sessions/{child_sid}/input", {"text": "PERSON_16B"})
    server.until(f, child_sid, lambda es: sum(e["kind"] == "turn_end" for e in es) == 3)
    heard = server.until(f, parent, lambda es: any(e["kind"] == "text" and e["text"] == "Heard." for e in es))
    continued = [e for e in heard if e["kind"] == "child" and e.get("event") == "continued"]
    assert len(continued) == 2, continued
    # A cache mark moves to the newest blocks; the newest message may gain blocks at its end.
    bare = lambda v: {k: bare(x) for k, x in v.items() if k != "cache_control"} if isinstance(v, dict) \
        else [bare(x) for x in v] if isinstance(v, list) else v
    sent = [bare(json.loads(body)["messages"]) for body in f.sent() if child in body]
    assert len(sent) == 4 and all(b[:len(a) - 1] == a[:-1] for a, b in zip(sent, sent[1:])), \
        "a continuation changed the child's history before its newest message"

    server.post(f, f"/sessions/{parent}/quit", {})
    closed = server.until(f, parent, lambda es: any(e["kind"] == "child" and isinstance(e["event"], dict) and "settled" in e["event"] for e in es))
    assert any(e["kind"] == "child" and e["session"] == child_sid and e.get("event") == {"settled": "discard"} for e in closed), "close did not discard the read-only helper"

    # The twenty-first unfinished child is refused; a live cap increase permits it.
    root = "FLOW_ROOT_14"
    names = [f"cap{n:02}" for n in range(1, 22)]
    replies += [agent(root, "start", name=name, kind="researcher", brief="Read, then wait without finishing.")
                for name in names[:20]]
    replies += [agent(root, "start", name=names[20], kind="researcher", brief="The over-cap launch."),
                reply(root, "Bash", {"command": "python3 -c 'import os; from pathlib import Path; p=Path(os.environ[\"RAZOR_HOME\"])/\"config.toml\"; p.write_text(p.read_text().replace(\"[children]\\n\", \"[children]\\nmax = 21\\n\"))'"}),
                agent(root, "start", name=names[20], kind="researcher", brief="The now-permitted launch.")]
    replies += [reply(root, "Bash", {"command": "python3 - <<'PY'\nimport json, os, time\nfrom pathlib import Path\nend = time.monotonic() + 120\nwhile time.monotonic() < end:\n    rows = [[json.loads(line) for line in p.read_text().splitlines(keepends=True) if line.endswith('\\n')] for p in Path(os.environ['RAZOR_HOME']).glob('sessions/*/record.jsonl')]\n    children = [r for r in rows if r and (r[0].get('child') or {}).get('parent') == os.environ['RAZOR_SESSION']]\n    if len(children) == 21 and all(any(e['kind'] == 'turn_end' for e in r) for r in children):\n        break\n    time.sleep(.05)\nelse:\n    raise SystemExit('children did not all complete their first turns')\nPY"})]
    replies += [agent(root, "stop", child=name) for name in names]
    replies += [agent(root, "settle", child=name, decision="discard") for name in names]
    replies += [reply(root)]
    replies += [part for name in names for part in (
        reply(f"You are {name},", "Bash", {"command": "sleep 180", "run_in_background": True}),
        reply(f"You are {name},", text="Read complete; waiting for further work."))]
    f = folder("cap14")
    parent = f.ask(root, timeout=180)["session"]
    entries = list(f.record(parent))
    starts = [e for e in entries if e["kind"] == "call" and e["input"].get("action") == "start"]
    assert len(starts) == 22, starts
    failed = [e for e in entries if e["kind"] == "result" and e["call"] == starts[20]["call"]]
    assert len(failed) == 1 and failed[0]["outcome"] == "error", failed
    assert "children.max" in failed[0]["output"] and "20" in failed[0]["output"], failed
    accepted = [e for e in entries if e["kind"] == "result" and e["call"] == starts[21]["call"]]
    assert len(accepted) == 1 and accepted[0]["outcome"] == "success", accepted
    children = [next(f.record(s)) for s in children_of(parent)]
    assert len(children) == 21, children
    assert sorted(e["child"]["name"] for e in children) == names, children
    assert "max = 21" in (f.home / ".razor/config.toml").read_text(), "the raised cap was not live config"
