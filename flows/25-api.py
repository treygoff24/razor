"""The API: enroll a key, list, read pages, attach and send, revoke (docs/design/api.md).
A root started with no message is live, as if the person had just written to it."""
import http.client
import json
import resource
import tempfile
import time
from pathlib import Path
from driver import REPLIES, Reply, flow
from model import reply
from responses import done
import server


def open_(connect, method, path, key, body=None):
    connection = connect()
    headers = {"Authorization": f"Bearer {key}"} if key else {}
    if isinstance(body, (dict, list)):
        body, headers["Content-Type"] = json.dumps(body), "application/json"
    connection.request(method, "/v1" + path, body=body, headers=headers)
    return connection.getresponse()


def call(connect, method, path, key, body=None):
    answer = open_(connect, method, path, key, body)
    return answer.status, answer.read()


def lines(stream):
    """The stream's next JSON line; None once it has ended."""
    line = stream.readline()
    return json.loads(line) if line else None


# An entry as a client gets it by default: without `native`, the model path's replay data.
bare = lambda entry: {k: v for k, v in entry.items() if k != "native"}


# Each turn reads 3 of its 4 input tokens from the cache; a pack offers the skill `hi`.
hello = (REPLIES / "hello.sse").read_bytes().replace(b'"cache_read_input_tokens": 0',
                                                     b'"cache_read_input_tokens": 3')
pack = Path(tempfile.mkdtemp()) / "pack"
(pack / "skills/hi").mkdir(parents=True)
(pack / "skills/hi/SKILL.md").write_text("---\ndescription: Says hi.\n---\nSay hi first.\n")
(pack / "config.toml").write_text("")
with flow([reply("Reply with the flow marker.", "Bash", {"command": "true"}),
           reply("Reply with the flow marker.", "Bash", {"command": "exit 1"}),
           *[Reply(hello) for _ in range(4)]],
          config=f'pack = {json.dumps(str(pack))}\n[server]\naddress = "127.0.0.1:0"') as f:
    # 1. --print starts the server through the socket and makes a session. It starts under a
    # soft limit of 64 open files and raises it, since twenty agents' pipes pass macOS's 256.
    hard = resource.getrlimit(resource.RLIMIT_NOFILE)[1]
    resource.setrlimit(resource.RLIMIT_NOFILE, (64, hard))
    result = f.run("--print=json", "Reply with the flow marker.")
    resource.setrlimit(resource.RLIMIT_NOFILE, (min(hard, 65_536), hard))
    assert result.returncode == 0, result.stderr
    session = json.loads(result.stdout)["session"]
    state = f.home / ".razor"
    key = (state / "device.key").read_text().strip()
    running = json.loads((state / "server.json").read_text())
    host, port = running["address"].rsplit(":", 1)
    soft = next(line.split()[3] for line in open(f"/proc/{running['pid']}/limits") if line.startswith("Max open files"))
    assert int(soft) == min(hard, 65_536), soft

    def local():
        return server.Local(state / "server.sock")

    def remote():
        return http.client.HTTPConnection(host, int(port), timeout=10)

    # 2. razor devices enrolls a phone and shows its key once; the list never shows a key.
    added = f.run("devices", "add", "phone")
    assert added.returncode == 0, added.stderr
    phone = added.stdout.splitlines()[1]
    assert len(phone) == 64 and phone != key, added.stdout
    assert f.run("devices").stdout == "phone\n"
    status, body = call(local, "GET", "/devices", key)
    assert [d["key"] for d in json.loads(body)] == [None], body

    # 3. Over TCP the phone's key lists the session; no key, or a wrong one, gets 401.
    status, body = call(remote, "GET", "/sessions", phone)
    assert status == 200 and [s["session"] for s in json.loads(body)] == [session], body
    for wrong in (None, "0" * 64):
        status, body = call(remote, "GET", "/sessions", wrong)
        assert status == 401 and json.loads(body)["error"], (status, body)

    # 4. A page with `native=true` is the record's own lines, byte for byte (step 10 reads one
    # without); entry 1 alone is Start.
    stored = (state / "sessions" / session / "record.jsonl").read_bytes().splitlines(True)
    status, body = call(remote, "GET", f"/sessions/{session}/entries?limit=2&native=true", phone)
    assert status == 200 and body == b"".join(stored[-2:]), (status, body)
    status, body = call(remote, "GET", f"/sessions/{session}/entries?before=2&limit=1",
                        phone)
    assert status == 200 and json.loads(body)["kind"] == "start", (status, body)

    # 5. A draft reads back; attach after the last entry; an input handed the skill `hi` holds
    # its resolved text separately from its words, renders it, and clears the sent draft.
    status, body = call(remote, "GET", f"/sessions/{session}/skills", phone)
    assert json.loads(body) == [{"name": "hi", "description": "Says hi."}], (status, body)
    draft = f"/sessions/{session}/draft"
    status, body = call(remote, "PUT", draft, phone, "half a thought")
    assert status == 200, (status, body)
    assert call(remote, "GET", draft, phone) == (200, b"half a thought")
    events = open_(remote, "GET", f"/sessions/{session}/events?after={len(stored)}",
                   phone)
    attached = lines(events)
    assert attached == {"kind": "attached", "session": session, "last": len(stored), "pending": []}, attached
    status, body = call(remote, "POST", f"/sessions/{session}/input", phone,
                        {"text": "Again.", "skill": "hi"})
    assert status == 200, (status, body)
    entry = json.loads(body)["entry"]
    streamed = []
    while not streamed or streamed[-1]["kind"] != "turn_end":
        line = lines(events)
        assert line is not None, streamed
        if line["kind"] not in ("delta", "taken", "tail", "attached"):
            streamed.append(line)
    assert streamed[0]["id"] == entry and streamed[0]["kind"] == "input", streamed
    skill = '<skill name="hi">\n---\ndescription: Says hi.\n---\nSay hi first.\n\n</skill>\n\n'
    assert streamed[0]["text"] == "Again.", streamed[0]
    assert streamed[0]["skill"] == {"name": "hi", "text": (pack / "skills/hi/SKILL.md").read_text()}, streamed[0]
    assert any(part.get("text") == skill + "Again." for message in f.request(4)["messages"]
               for part in message["content"] if isinstance(part, dict)), f.request(4)
    assert streamed[-1]["inputs"] == [entry], streamed[-1]
    assert streamed == [bare(e) for e in f.record(session)][len(stored):], streamed
    assert call(remote, "GET", draft, phone) == (200, b"")
    # Handing the skill alone sends none of the saved draft, so admission leaves it intact.
    assert call(remote, "PUT", draft, phone, "still composing")[0] == 200
    assert call(remote, "POST", f"/sessions/{session}/input", phone, {"text": "", "skill": "hi"})[0] == 200
    while lines(events)["kind"] != "turn_end":
        pass
    assert call(remote, "GET", draft, phone) == (200, b"still composing")

    # 6. No file name leaves the session's directory; a file that is not there is 404.
    for name, refused in (("../../device.key", 400), ("..%2F..%2Fdevice.key", 400),
                          ("nothing", 404)):
        status, body = call(remote, "GET", f"/sessions/{session}/files/{name}", phone)
        assert status == refused and json.loads(body)["error"], (name, status, body)

    # 7. The stream carries live rows with their newest cache share; an idle row's membership
    # coalesces within one second. Removing the phone ends its stream and key, no other device's.
    watch = remote()
    watched = open_(lambda: watch, "GET", "/status", phone)
    rows = lines(watched)
    assert rows["kind"] == "agents", rows
    deadline = time.monotonic() + 2
    row = next((a for a in rows["agents"] if a["session"] == session), None)
    while row is None:
        assert time.monotonic() < deadline, "live idle row never joined the stream"
        watch.sock.settimeout(max(0.001, deadline - time.monotonic()))
        line = lines(watched)
        candidates = line.get("agents", [line] if line["kind"] == "agent" else [])
        row = next((a for a in candidates if a["session"] == session), None)
    usage = [e for e in f.record(session) if e["kind"] == "turn_end"][-1]["usage"]
    share = usage["cache_read"] / (usage["input"] + usage["cache_read"] + usage["cache_write"])
    assert share > 0 and row["cache"] == share, (usage, row)
    assert row.get("calls_made") == 2 and row["failed"] == 1 and row["calls"] == [], row
    mine = open_(local, "GET", "/status", key)
    assert lines(mine)["kind"] == "agents"
    removed = f.run("devices", "remove", "phone")
    assert removed.returncode == 0, removed.stderr
    assert lines(watched) is None, "the phone's status stream outlived its key"
    assert call(remote, "GET", "/sessions", phone)[0] == 401
    f.ask("--resume", session, "Once more.")
    line = {}
    while session not in [row.get("session") for row in line.get("agents", [line])]:
        line = lines(mine)
        assert line is not None, "the local key's status stream ended with the phone's"

    # 8. The project route reads this folder now, not the branch recorded for a child.
    f.base()
    project = f"/sessions/{session}/project"
    status, body = call(local, "GET", project, key)
    assert status == 200 and json.loads(body) == {"commits": 1, "merges": 0, "branch": "main", "changed": 0}, (status, body)
    # Uncommitted files count from git (Trey, 2026-10-30), so a fresh session in a dirty tree shows them.
    f.git("checkout", "--detach")
    (f.cwd / "loose").write_text("x")
    status, body = call(local, "GET", project, key)
    assert status == 200 and json.loads(body) == {"commits": 1, "merges": 0, "changed": 1}, (status, body)
    (f.cwd / "loose").unlink()
    outside = f.home / "outside"
    outside.mkdir()
    status, body = call(local, "POST", "/sessions", key, {"cwd": str(outside)})
    assert status == 200, (status, body)
    plain = json.loads(body)["session"]
    assert call(local, "GET", f"/sessions/{plain}/project", key) == (200, b"{}")
    assert json.loads(server.send(f, "GET", f"/sessions/{plain}/agent").read()).get("calls_made") == 0
    assert plain in server.agents(f) and server.listed(f, plain), "a root started with no message is not live"
    # 9. A reader's report outlives its row until a restart.
    last, before = list(f.record(session))[-1]["id"], int(time.time() * 1000)
    assert call(local, "PUT", f"/sessions/{session}/seen?entry={last}", key) == (200, b"{}")
    row = server.agents(f)[session]
    assert row.get("seen") == last and row.get("looked_at", 0) >= before, row
    server.stop(f)
    server.start(f)
    rows = server.agents(f)
    assert rows[session].get("calls_made") == 2 and rows[session]["failed"] == 1, rows[session]
    assert "seen" not in rows[session] and "looked_at" not in rows[session], rows[session]
    assert json.loads(server.send(f, "GET", f"/sessions/{plain}/agent").read()).get("calls_made") == 0

# Closing releases the row; looks and seen survive until restart, and records remain readable.
with flow([]) as f:
    server.start(f)
    sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    row = lambda: json.load(server.send(f, "GET", f"/sessions/{sid}/agent"))
    events = server.send(f, "GET", f"/sessions/{sid}/events?terminal=true")
    assert lines(events)["kind"] == "attached"
    server.send(f, "PUT", f"/sessions/{sid}/seen?entry=1").read()
    before = row()
    assert before["seen"] == 1 and before["looked_at"] and before["terminal"], before
    followed = server.send(f, "GET", f"/sessions/{sid}/events?follow=true")
    assert lines(followed)["kind"] == "attached"
    events.close()
    server.post(f, f"/sessions/{sid}/quit", {})
    assert not server.listed(f, sid) and row().get("looked_at"), "a loaded quit root lost its readings"
    before = row()
    followed.close()
    for _ in range(2):
        assert not server.listed(f, sid)
    after = row()
    assert (after.get("seen"), after.get("looked_at")) == (before["seen"], before["looked_at"]) and not after["terminal"], after
    assert after["name"] == before["name"] and after["headline"] == before["headline"] and after["close_reason"] == "person", after
    server.send(f, "PUT", f"/sessions/{sid}/seen?entry=2").read()
    assert row()["seen"] == 2, "an evicted row ignored its reader's last seen entry"
    assert json.loads(server.send(f, "GET", f"/sessions/{sid}/entries?before=2").read())["kind"] == "start"
    with server.send(f, "GET", f"/sessions/{sid}/events?terminal=true") as reopened:
        assert lines(reopened)["kind"] == "attached"
        assert row()["looked_at"] and row()["terminal"]
        assert sid not in server.agents(f), "a quit root rejoined the live feed"

# 10. A GPT response entry, about 100 KB of replay data on the sign-in, reaches a stream and a
# page without `native` by default and whole with `native=true`.
with flow([done()], model="gpt-6.1-sol") as f:
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    streams = [server.send(f, "GET", f"/sessions/{session}/events?after=0{flag}") for flag in ("", "&native=true")]
    server.post(f, f"/sessions/{session}/input", {"text": "Say done."})
    stored = next(e for e in server.until(f, session, lambda es: es[-1]["kind"] == "turn_end") if e["kind"] == "response")
    got = [next(line for line in iter(lambda s=s: lines(s), None) if line["kind"] == "response") for s in streams]
    assert stored["native"]["responses"]["items"] and got == [bare(stored), stored], got
    line = (f.home / ".razor/sessions" / session / "record.jsonl").read_bytes().splitlines(True)[stored["id"] - 1]
    page = f"/sessions/{session}/entries?before={stored['id'] + 1}&limit=1"
    assert server.send(f, "GET", page + "&native=true").read() == line
    assert json.loads(server.send(f, "GET", page).read()) == bare(stored)

# Paired enrollment remains authoritative if its file becomes unreadable at runtime;
# search still reports the unavailable peer, and revocation immediately removes it.
with flow([], config='[server]\naddress="127.0.0.1:0"') as f:
    server.start(f)
    assert f.run("devices", "add", "down", "--address", "127.0.0.1:1").returncode == 0
    path = f.home / ".razor/devices.json"
    original = path.read_bytes()
    for failure in ("parse", "read"):
        if failure == "parse":
            path.write_text("{broken")
        else:
            path.unlink()
            path.mkdir()
        answer = json.loads(server.send(f, "GET", "/search?q=enrollment").read())
        assert answer["partial"], answer
        if failure == "read":
            path.rmdir()
        path.write_bytes(original)
    assert f.run("devices", "remove", "down").returncode == 0
    assert not json.loads(server.send(f, "GET", "/search?q=enrollment").read())["partial"]

# Invalid enrollment must refuse startup rather than silently forget paired servers.
for failure in ("parse", "read"):
    with flow([]) as f:
        path = f.home / ".razor/devices.json"
        if failure == "parse":
            path.write_text("{broken")
        else:
            path.mkdir()
        result = f.run("serve", timeout=5)
        assert result.returncode != 0 and "devices.json" in result.stderr, result

# A live row with an open terminal joins an existing stream; closed before any once-a-second
# publication retained it, it still leaves that stream at the next publication.
with flow([]) as f:
    server.start(f)
    watch = server.Local(f.home / ".razor/server.sock")
    stream = open_(lambda: watch, "GET", "/status?views=true", (f.home / ".razor/device.key").read_text().strip())
    listed = lambda line: {r["session"] for l in line["layouts"] for s in l["sections"] for r in s["sessions"]}
    stay, leave = (server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"] for _ in range(2))
    terminals = [server.send(f, "GET", f"/sessions/{stay}/events?terminal=true")]
    # Its join's publication has just run, so the next one is a second away.
    while not ((line := lines(stream))["kind"] == "views" and stay in listed(line)):
        pass
    terminals.append(server.send(f, "GET", f"/sessions/{leave}/events?terminal=true"))
    while not ((line := lines(stream))["kind"] == "agent" and line["session"] == leave):
        pass
    server.post(f, f"/sessions/{leave}/quit", {})
    deadline = time.monotonic() + 2
    watch.sock.settimeout(2)
    try:
        while not ((line := lines(stream))["kind"] == "agents" and leave not in {a["session"] for a in line["agents"]}):
            assert line["kind"] != "views" or leave not in listed(line), "published before it closed"
            watch.sock.settimeout(max(0.001, deadline - time.monotonic()))
    except TimeoutError:
        raise AssertionError("the closed row stayed on its existing stream") from None

# The feed owns skill order, latest-load links and worn state, including after recovery.
from claude import reply as skill_reply
with flow([skill_reply(*[(None, "mcp__razor__skill", {"name": f"feed25-{i}"}) for i in range(33)]),
           skill_reply(text="loaded25"), skill_reply((None, "mcp__razor__handover", {"note": "feed25 note"})),
           skill_reply(text="rebuilt25"), skill_reply((None, "mcp__razor__skill", {"name": "feed25-1"})),
           skill_reply(text="reloaded25")]) as f:
    root = f.cwd / ".agents/skills"
    for i in range(33):
        (root / f"feed25-{i}").mkdir(parents=True)
        (root / f"feed25-{i}/SKILL.md").write_text(f"feed25 instructions {i}")
    sid = f.ask("Load feed25 skills.")["session"]
    skills25 = lambda: server.agents(f)[sid].get("skills", [])
    initial25 = skills25()
    checks25 = {"newest 32 in first-load order": [s["name"] for s in initial25] == [f"feed25-{i}" for i in range(1, 33)]}
    checks25["load source and time"] = bool(initial25) and all(s["how"] == "call" and not s["worn"] and s["entry"] > 0 and s["at"] > 0 for s in initial25)
    (root / "feed25-1").rename(root / "feed25-away")
    asked25 = server.post(f, f"/sessions/{sid}/handover", {})["entry"]
    entries25 = server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end" and asked25 in es[-1]["inputs"])
    worn25 = skills25()
    checks25["missing worn with original source"] = bool(worn25) and worn25[0]["worn"] == next(e["at"] for e in entries25 if e["kind"] == "rebuild") and worn25[0]["entry"] == initial25[0]["entry"] and not any(s["worn"] for s in worn25[1:])
    (root / "feed25-away").rename(root / "feed25-1")
    f.ask("--resume", sid, "Reload feed25-1.")
    latest25 = skills25()
    checks25["repeat load keeps slot and clears worn"] = bool(latest25) and [s["name"] for s in latest25] == [s["name"] for s in initial25] and not latest25[0]["worn"] and latest25[0]["entry"] > initial25[0]["entry"]
    server.stop(f)
    server.start(f)
    checks25["state recovered from record"] = bool(latest25) and skills25() == latest25
    assert all(checks25.values()), [name for name, passed in checks25.items() if not passed]
