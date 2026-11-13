"""One nightly pass for a project shared across a person's paired servers: it reads their
sessions and never another person's, a server that drops out mid-pass is read by the next, two
homes due in the same minute run one pass, and a resident's home runs it. A question for another
person is a task on their page alone, their pass sees how it was read, and every session starts
with the glossary inside the page's bounds."""
from dataclasses import replace
from pathlib import Path
import json
import os
import tempfile
import time
from claude import reply
from clock import wall_clock
from driver import flow
from responses import PLAIN, message, reply as turn, response
from server import send, start, stop, until, wait
import server

SHARING = '[memory]\nremote = "origin"\n[tasks]\nremote = "origin"\n'


def home(f, name, person, folder=None):
    """A server of `person`'s with its own home, and its checkout of the project whose shared
    memory and tasks are on the bare repository `<folder>.git`."""
    cwd = (folder or f.cwd.parent) / name
    where = f.home_at(name, cwd)
    remote = cwd.parent / "shared.git"
    if not remote.exists():
        where.git("init", "--bare", str(remote))
    where.git("remote", "add", "origin", str(remote))
    where.git("config", "user.name", person)
    where.git("config", "user.email", f"{person.lower()}@example.invalid")
    (cwd / ".razor").mkdir()
    (cwd / ".razor/config.local.toml").write_text(SHARING)
    return where


def pair(one, other):
    """Pairs two started servers by code, each under the other's folder name."""
    server.pair(one, other, other.cwd.name, one.cwd.name)


def unpair(one, other):
    for where, name in ((one, other.cwd.name), (other, one.cwd.name)):
        removed = where.run("devices", "remove", name)
        assert removed.returncode == 0, removed.stderr


def marks(where):
    """The stamp's line for each server: when its night was claimed and what is read of it."""
    stamp = where.home / ".razor/memory/projects" / str(where.cwd).lstrip("/") / ".dream"
    lines = stamp.read_text().splitlines()[2:] if stamp.exists() else []
    return {server: (int(claimed), int(read)) for server, claimed, read in map(str.split, lines)}


def server_id(where):
    return (where.home / ".razor/server.id").read_text().strip()


replies = []
# Ordinary sessions run on the plain GPT route; the Claude route keeps the startup page's request
# and, through `[memory] model`, the manual passes' memory and task calls.
with flow(replies, model="sol", config=PLAIN + '[memory]\nmodel = "opus"\n') as f:
    def use(where, tool, ok=True, **arguments):
        replies.extend([turn(None, tool, **arguments), turn(None, "done")])
        sid = where.ask("Keep this project's work.")["session"]
        out = [e for e in where.record(sid) if e["kind"] == "result"][-1]
        assert (out["outcome"] == "success") == ok, out
        return out["output"]

    def page(where):
        for p in (where.home / ".razor").rglob("FETCH_HEAD"):
            os.utime(p, (0, 0))
        use(where, "task", action="list")
        use(where, "memory", action="search", query="")
        replies.append(reply(text="opened"))
        where.ask("--model", "opus", "Open this project.")
        return json.loads(where.sent()[-1])["messages"][0]["content"][0]["text"]

    def dream(where, *said, gap=0.03):
        """Starts a pass now; its session and its input."""
        replies.extend([*said, reply(text="Tidied.", thinking=["a", "b"], gap=gap)])
        started = where.run("dream", str(where.cwd))
        assert started.returncode == 0, started.stderr
        sid = started.stdout.strip()
        entries = until(where, sid, lambda es: any(e["kind"] == "input" for e in es))
        return sid, next(e["text"] for e in entries if e["kind"] == "input")

    def ended(where, sid):
        return until(where, sid, lambda es: any(e.get("closed") for e in es))

    a, b, carol = home(f, "a", "Pat"), home(f, "b", "Pat"), home(f, "carol", "Carol")
    for where in (a, b, carol):
        start(where)
    pair(a, b), pair(a, carol)
    use(a, "memory", action="write", kind="note", text="Shared.", description="SHARED_90")
    for where in (a, carol):
        use(where, "task", action="write", title=f"{where.cwd.name}'s own")
    for where, said in ((b, "B_SOURCE_90"), (carol, "C_SOURCE_90")):
        use(where, "memory", action="search", query="")
        replies.append(turn(None, "Worked."))
        where.source = where.ask(said)["session"]

    # An old shared stamp cannot certify that any identified home was read.
    memory = a.home / ".razor/memory/projects" / str(a.cwd).lstrip("/")
    (memory / ".dream").write_text(f"{int(time.time()*1000)}\n")
    record = b.home / ".razor/sessions" / b.source / "record.jsonl"
    os.utime(record, (time.time()-3600, time.time()-3600))
    # Supplied digests count as read even if the peer stops before the pass ends.
    first, text = dream(a, gap=4)
    assert f"- {b.source}@b" in text and "B_SOURCE_90" in text, "the pass did not read the paired server"
    assert carol.source not in text and "C_SOURCE_90" not in text, "the pass read another person's sessions"
    stop(b)
    ended(a, first)
    wait(lambda: marks(a).get(server_id(a), (0, 0))[1], why="the pass's own server was not marked read")
    assert marks(a)[server_id(b)][0] and marks(a)[server_id(b)][1], marks(a)
    read_before = marks(a)[server_id(b)][1]
    second, text = dream(a)
    assert b.source not in text, "a server that does not answer was read"
    ended(a, second)
    start(b)

    # A second checkout shares the same memory history and belongs in the server's one part.
    clone = b.cwd.parent / "copy"
    b.git("clone", str(b.cwd), str(clone))
    other = replace(b, cwd=clone)
    (clone / ".razor").mkdir()
    (clone / ".razor/config.local.toml").write_text(SHARING)
    other.git("remote", "set-url", "origin", str(b.cwd.parent / "shared.git"))
    other.git("config", "user.name", "Pat")
    other.git("config", "user.email", "pat@example.invalid")
    use(other, "memory", action="search", query="")
    replies.append(turn(None, "Work in the second checkout."))
    other.source = other.ask("SECOND_CHECKOUT_90")["session"]
    replies.append(turn(None, "Work while the first peer was away."))
    b.source = b.ask("FAILED_DIGEST_90")["session"]
    history = a.git("-C", str(memory), "rev-list", "--max-parents=0", "HEAD").strip()
    part = json.loads(send(b, "GET", "/dream/" + history).read())
    assert {b.source, other.source} <= {id for _, id in part["sessions"]}, part

    # A real network digest failure, with the project route still answering at completion: what
    # stands between a and b carries only the night's route.
    unpair(a, b)
    between = server.Tap(a, b, "b", "a")
    server.say(between.process, "only /v1/dream/")
    # The third pass writes the glossary, and asks Carol what only she can answer.
    terms = "\n".join(f"- term{n}: MEANING_{n}" for n in range(14))
    third, text = dream(a, reply((None, "mcp__razor__memory", {
        "action": "write", "kind": "page", "name": "glossary", "description": "Terms.", "text": terms})),
        reply((None, "mcp__razor__task", {"action": "write", "title": "Carl?", "for": "Carl"})),
        reply((None, "mcp__razor__task", {"action": "write", "title": "QUESTION_90", "for": "Carol", "due": "2000-01-01",
                                          "comment": "READING_90"})))
    assert f"- {b.source}@b" in text and "FAILED_DIGEST_90" not in text, "digest fault was not exercised"
    results = [e for e in ended(a, third) if e["kind"] == "result"]
    assert [r["outcome"] for r in results] == ["success", "error", "success"], results
    assert "Carol" in results[1]["output"] and "Pat" in results[1]["output"], results[1]
    wait(lambda: marks(a)[server_id(a)][1] > read_before)
    assert marks(a)[server_id(b)][1] == read_before, "a successful project probe certified a failed digest"
    unpair(a, b)
    pair(a, b)
    recovered, text = dream(a)
    assert all(source in text for source in ("FAILED_DIGEST_90", "SECOND_CHECKOUT_90")), "a failed digest or second checkout was skipped"
    ended(a, recovered)
    wait(lambda: marks(a)[server_id(b)][1] > read_before, why="successful peer reads were not marked")

    # The question is on Carol's page alone, and her pass reads how her words were read.
    shown = page(carol)
    assert "QUESTION_90" in shown.split("## Memory: ask", 1)[1], shown
    assert all("QUESTION_90" not in page(where) for where in (a, b)), "a question for Carol showed on Pat's page"
    carol_pass, text = dream(carol)
    assert "READING_90" in text and b.source not in text, "Carol's pass lost the reading of her words"
    ended(carol, carol_pass)

    # Every session starts with the glossary, inside the bounds of what is known.
    notes = b.home / ".razor/memory/projects" / str(b.cwd).lstrip("/")
    for n in range(30):
        (notes / f"note-20991231-0000{n:02}-000000{n:02}.md").write_text(f"---\ndescription: NOTE_{n} {'x' * 90}\n---\nNote.")
    known = page(b).split("## Memory: what is known", 1)[1].split("\n## ", 1)[0]
    known = [line for line in known.splitlines() if line]
    assert known[1].startswith("Terms used here, 2 more") and known[2:14] == terms.splitlines()[:12], known[:15]
    kept = [line for line in known[1:] if "more; `memory search`" not in line]
    assert len(kept) <= 24 and len("\n".join(kept)) <= 2400 and any("NOTE_" in line for line in kept), (len(kept), len("\n".join(kept)))
    assert "(wiki/glossary)" not in "\n".join(known), "the glossary was listed twice"

# Two homes due in the same minute run one pass of the project they share; where one hosts a
# project's resident, the other leaves that night to it, though it gets there first: b has a
# project of its own to dream before. Night discovery skips temporary folders, so the projects
# are under the home. The resident's own night, ten minutes gone, stays out of this one.
night, own = (time.strftime("%H:%M:%S", time.localtime(time.time() + seconds)) for seconds in (120, -600))
with tempfile.TemporaryDirectory(prefix="razor-night-flow-", dir=Path.home()) as folder, wall_clock() as (_, env, advance):
    replies = []
    with flow(replies, model="sol", config=PLAIN + f'[memory]\nnight = "{night}"', env=env) as f:
        first, alone, kept = (Path(folder) / name for name in ("first", "ibis", "kept"))
        first.mkdir(), alone.mkdir(), kept.mkdir()
        a, b = home(f, "a", "Pat", first), home(f, "b", "Pat", first)
        with (b.home / ".razor/config.toml").open("a") as config:
            config.write(f'[residents.ember]\ncwd = {json.dumps(str(kept / "b"))}\nnight = "{own}"\n')
        homes = [a, b, home(f, "a", "Pat", kept), home(f, "b", "Pat", kept), home(f, "b", "Pat", alone)]
        start(a), start(b)
        pair(a, b)
        for where in homes:  # a's write starts each project's shared memory; b's read joins it
            call = {"action": "write", "kind": "note", "text": "Shared.", "description": "Shared."}
            replies.extend([turn(None, "memory", **call if where.home == a.home else {"action": "search", "query": ""}),
                            turn(None, "done")])
            where.source = where.ask("Keep this project's work.")["session"]

        def dreams(where):
            """The passes in `where`'s home, by the folder each runs in."""
            found = {}
            for record in (where.home / ".razor/sessions").glob("*/record.jsonl"):
                entries = list(where.record(record.parent.name))
                if any(e["kind"] == "input" and "nightly dreaming session" in e.get("text", "") for e in entries):
                    found[entries[0]["cwd"]] = entries
            return found

        for record in (b.home / ".razor/sessions").glob("*/record.jsonl"):
            entries = list(b.record(record.parent.name))
            if entries[0].get("project_root") == str(kept / "b"):
                os.utime(record, (time.time()-8*86400, time.time()-8*86400))
        # Each pass's answer streams in two pieces, the gap apart.
        replies.extend(replace(response([message("Tidied", ".")], gap=gap), match=match) for gap, match in ((1, None), (1, None), (3, str(alone / "b"))))
        advance(240_000)
        # The timers go a minute on in steps the two servers' connection lives through, as it does
        # through real time: one jump times it out at the moment each asks the other for its night.
        for ahead in range(5_000, 65_000, 5_000):
            advance(ahead, monotonic=True)
            for where, other in ((a, b), (b, a)):
                assert send(where, "GET", f"/sessions?server={other.cwd.name}&limit=1").status == 200
        wait(lambda: sum(any(e.get("closed") for e in es) for where in (a, b) for es in dreams(where).values()) == 3,
             why="the night did not run each project's pass")
        ran = [cwd for where in (a, b) for cwd in dreams(where)]
        assert len(ran) == 3 and (str(first / "a") in ran) != (str(first / "b") in ran), f"two homes did not run one pass: {ran}"
        resident = dreams(b).get(str(kept / "b"))
        assert resident and resident[0]["thread"]["resident"] == "ember", f"the resident's home did not run its project's pass: {ran}"
        assert f"- {homes[2].source}@a" in next(e["text"] for e in resident if e["kind"] == "input"), "the resident's pass did not read the other home"
        for claimed in (marks(homes[0]) or marks(homes[1]), marks(homes[3])):
            assert len(claimed) == 2 and len({at for at, _ in claimed.values()}) == 1, f"one claim did not cover both homes: {claimed}"

# Internet Git remains reachable while neither paired home can reach the other.
night = time.strftime("%H:%M:%S", time.localtime(time.time() + 120))
with tempfile.TemporaryDirectory(prefix="razor-partition-", dir=Path.home()) as folder, wall_clock() as (_, env, advance):
    replies = []
    with flow(replies, model="sol", config=PLAIN + f'[memory]\nnight = "{night}"', env=env) as f:
        a, b = home(f, "a", "Pat", Path(folder)), home(f, "b", "Pat", Path(folder))
        start(a), start(b)
        pair(a, b)
        for where in (a, b):
            call = {"action": "write", "kind": "note", "description": "Shared.", "text": "Shared."} if where is a else {"action": "search", "query": ""}
            replies.extend([turn(None, "memory", **call), turn(None, "done")])
            where.source = where.ask("Work on this home.")["session"]
        for where in (a, b):
            stop(where)
            file = where.home / ".razor/devices.json"
            devices = json.loads(file.read_text())
            for device in devices:
                if device.get("relays"): device["relays"] = ["http://127.0.0.1:1"]
            file.write_text(json.dumps(devices))
            start(where)
        replies.extend([turn(None, "Tidied."), turn(None, "Tidied.")])
        advance(240_000)
        advance(60_000, monotonic=True)
        for where, peer in ((a, b), (b, a)):
            def passes():
                records = [list(where.record(p.parent.name)) for p in (where.home / ".razor/sessions").glob("*/record.jsonl")]
                return [es for es in records if any(e["kind"] == "input" and "nightly dreaming session" in e.get("text", "") for e in es)]
            wait(lambda: any(any(e.get("closed") for e in es) for es in passes()))
            ran = passes()
            assert len(ran) == 1, "a home ran its night twice"
            text = next(e["text"] for e in ran[0] if e["kind"] == "input")
            assert where.source in text and peer.source not in text, "partitioned passes shared or lost a source"
        def pushed():
            text = a.git("--git-dir", str(Path(folder) / "shared.git"), "show", "refs/razor/memory:.dream")
            read = {server: int(read) for server, _, read in map(str.split, text.splitlines()[2:])}
            return all(read.get(server_id(where), 0) for where in (a, b))
        wait(pushed, why="both partitioned passes did not push their read progress")
