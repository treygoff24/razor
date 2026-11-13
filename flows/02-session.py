"""A reply and a durable turn, through the real Claude SDK and --print; what the startup list
and the session's facts give the model, and the startup.md that shows it; an image in a first
message; resume, continue and the three print formats; six clients racing to start a server."""
import base64
import json
import os
import shutil
import signal
import socketserver
import subprocess
import threading
import time
from dataclasses import replace
from pathlib import Path
from http.server import BaseHTTPRequestHandler
import chat
from driver import BINARY, REPLIES, Reply, flow
from png import png
from responses import PLAIN, call, done, response
from terminal import Terminal
import server

prompt = "Hello"
hello = Reply((REPLIES / "hello.sse").read_bytes())

# An immediate chat reply sends its live words before committing them. Print must subscribe
# before admitting input, including on resume; stored entries cannot replay these lines. A relay
# holds the real input acknowledgment until the turn ends, making the scheduling gap repeatable.
fast = chat.reply({"reasoning_content": "Fast thought.", "content": "Fast reply."})
replies = [fast] * 4
with flow(replies, config='[providers.runinfra]\nbase_url = "{standin}"\n',
          env={"RUNINFRA_API_KEY": "flow-key"}) as f:
    def part(name):
        # Each part in a folder of its own: --continue and the project's files see only its sessions.
        (f.cwd / name).mkdir()
        return replace(f, cwd=f.cwd / name)

    server.start(f)
    class Relay(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def forward(self):
            body = self.rfile.read(int(self.headers.get("Content-Length", 0))) or None
            path = self.path.removeprefix("/v1")
            with server.send(f, self.command, path, body, headers={"Content-Type": "application/json"}) as response:
                if path.endswith("/input"):
                    server.until(f, path.split("/")[2], lambda es: es[-1]["kind"] == "turn_end")
                self.send_response(response.status)
                self.end_headers()
                if "/events?" in path:
                    for line in iter(response.readline, b""):
                        self.wfile.write(line)
                        self.wfile.flush()
                        if json.loads(line)["kind"] == "turn_end":
                            break
                else:
                    self.wfile.write(response.read())

        do_GET = do_POST = forward

    # The client's home holds the relay where its server's socket would be.
    remote = f.home / "remote"
    remote.mkdir()
    shutil.copy(f.home / ".razor/device.key", remote)
    relay = socketserver.ThreadingUnixStreamServer(str(remote / "server.sock"), Relay)
    threading.Thread(target=relay.serve_forever, daemon=True).start()
    client = replace(part("relayed"), env={**f.env, "RAZOR_HOME": str(remote)})
    session = None
    for mode in ("stream", "stream", "text", "json"):
        result = client.run(f"--print={mode}", *(["--resume", session] if session else ["--model", "glm-5.3-flash"]), "Reply now.")
        assert result.returncode == 0, result.stderr
        if mode == "stream":
            lines = [json.loads(line) for line in result.stdout.splitlines()]
            session = lines[0]["session"]
            for kind, text in (("thought", "Fast thought."), ("delta", "Fast reply.")):
                assert "".join(e["text"] for e in lines if e["kind"] == kind) == text, lines
            assert [e["kind"] for e in lines if e["kind"] in ("input", "turn_end")] == ["input", "turn_end"], lines
        else:
            assert (json.loads(result.stdout)["message"] if mode == "json" else result.stdout.strip()) == "Fast reply.", result
    assert not (remote / "server.json").exists(), "the client started its own server"
    relay.shutdown()
    relay.server_close()

    # The default list: Razor's text, then the project's AGENTS.md (an include in place) and CLAUDE.md.
    replies.append(hello)
    listed = part("listed")
    (listed.cwd / "AGENTS.md").write_text("flow agents rules\n@shared.md\n@absent.md\n")
    (listed.cwd / "shared.md").write_text("flow shared include\n")
    (listed.cwd / "CLAUDE.md").write_text("flow claude rules\n")
    answer = listed.ask(prompt, "--speed", "fast")
    assert answer["message"] == "flow reply", answer
    session = answer["session"]
    entries = list(f.record(session))
    assert entries[0]["cwd"] == str(listed.cwd), entries[0]
    inputs = [e for e in entries if e["kind"] == "input"]
    assert len(inputs) == 1 and inputs[0]["text"] == prompt, inputs
    texts = [e["text"] for e in entries if e["kind"] == "text"]
    assert texts == ["flow reply"], texts
    assert entries[-1]["inputs"] == [inputs[0]["id"]], entries[-1]
    assert entries[-1]["end"] == "done", entries[-1]
    request = json.loads(f.sent()[-1])
    assert request.get("speed") == "fast", request
    assert prompt in json.dumps(request["messages"]), request
    system = request["system"][-1]["text"]
    marks = ["You are working in Razor", "# The record and your context", "flow agents rules",
             "flow shared include\n@absent.md", "flow claude rules"]
    assert all(m in system for m in marks), system
    assert [system.index(m) for m in marks] == sorted(system.index(m) for m in marks), system
    first = request["messages"][0]["content"][0]["text"]
    shown = (f.home / ".razor/sessions" / session / "startup.md").read_text()
    assert system in shown and first.removesuffix(prompt).strip() in shown, shown
    assert '"builtin": "Bash"' in shown and "## The tools (" in shown, shown

    # A list without `razor`: a folder's files by name, `~/` the home folder, the route part last, and
    # a missing entry named in the facts, never fatal. The list is read as each session starts.
    replies.append(hello)
    config = f.home / ".razor/config.toml"
    default = config.read_text()
    config.write_text('startup = ["~/notes", "~/missing.md", "project"]\n' + default)
    own = part("own-list")
    (f.home / "notes").mkdir()
    (f.home / "notes/b.md").write_text("flow note b")
    (f.home / "notes/a.md").write_text("flow note a")
    (own.cwd / "AGENTS.md").write_text("flow agents rules")
    result = own.run("--print=json", input=prompt)
    assert result.returncode == 0, result.stderr
    config.write_text(default)
    request = json.loads(f.sent()[-1])
    assert request.get("speed") in (None, "standard"), request
    system = request["system"][-1]["text"]
    assert system.startswith(f"flow note a\n\nflow note b\n\nContents of {own.cwd.resolve()}/AGENTS.md:\n\nflow agents rules\n\n# Your tools"), system
    assert "You are working in Razor" not in system, system
    first = request["messages"][0]["content"][0]["text"]
    assert f"Left out of your startup list: {f.home}/missing.md (" in first, first

    replies.append(hello)
    with Terminal(part("terminal"), "Reply to this first sentence.") as t:
        t.shown("flow reply")

    # A session's first message carries an image: it is stored in the session, the record names it,
    # and the model receives its exact bytes before the text (docs/design/claude.md).
    # A real 2-by-2 RGB PNG; the server tells its type from these bytes, not from a label.
    replies.append(hello)
    image = png(2, 2, b"\0\xff\0\0\0\xff\0" * 2)
    data = base64.b64encode(image).decode()
    asked = "What colours are in this picture?"
    session = server.post(f, "/sessions", {"cwd": str(part("image").cwd)})["session"]
    ask = server.post(f, f"/sessions/{session}/input", {"text": asked, "images": [data]})["entry"]
    entries = server.until(f, session, lambda es: es[-1]["kind"] == "turn_end")
    assert entries[-1]["inputs"] == [ask] and entries[-1]["end"] == "done", entries[-1]
    inputs = [e for e in entries if e["kind"] == "input"]
    assert len(inputs) == 1 and inputs[0]["id"] == ask and inputs[0]["text"] == asked, inputs
    images = inputs[0]["images"]
    assert len(images) == 1 and images[0]["media_type"] == "image/png", images
    stored = f.home / ".razor/sessions" / session / images[0]["path"]
    assert not images[0]["path"].startswith("/") and stored.read_bytes() == image, images
    assert [e["text"] for e in entries if e["kind"] == "text"] == ["flow reply"], entries
    request = json.loads(f.sent()[-1])
    blocks = next(m for m in request["messages"] if m["role"] == "user")["content"]
    kinds = [block["type"] for block in blocks]
    assert kinds.count("image") == 1 and kinds.index("image") < kinds.index("text"), blocks
    picture = blocks[kinds.index("image")]["source"]
    assert picture == {"type": "base64", "media_type": "image/png", "data": data}, picture
    assert any(asked in block.get("text", "") for block in blocks), blocks

    # Resume by id, continue by directory, and all three headless formats.
    replies += [Reply((REPLIES / f"{name}.sse").read_bytes()) for name in ("hello", "resumed", "continued", "stream")]
    resumed = part("resumed")
    session = resumed.ask("First ask.")["session"]
    os.kill(server.start(f), signal.SIGTERM)
    server.gone(json.loads((f.home / ".razor/server.json").read_text())["pid"])
    before = list(f.record(session))
    assert resumed.ask("--resume", session, "Second ask.") == {"session": session, "message": "resumed reply"}
    assert list(f.record(session))[:len(before)] == before, "resume rewrote the record"
    continued = resumed.run("--continue", "--print=text", input="Third ask.")
    assert continued.returncode == 0, continued.stderr
    assert continued.stdout == "continued reply\n", continued.stdout
    prior = list(f.record(session))
    streamed = resumed.run("--resume", session, "--print=stream", "Fourth ask.")
    assert streamed.returncode == 0, streamed.stderr
    events = [json.loads(line) for line in streamed.stdout.splitlines()]
    assert events[0]["kind"] == "attached" and events[0]["session"] == session, events
    entries = list(f.record(session))
    # Entries reach the stream without `native`, the model path's replay data.
    assert [e for e in events if "id" in e] == [{k: v for k, v in e.items() if k != "native"} for e in entries[len(prior):]], events
    assert events[-1]["kind"] == "turn_end", events[-1]
    assert [e["text"] for e in entries if e["kind"] == "input"] == [
        "First ask.", "Second ask.", "Third ask.", "Fourth ask."]
    assert len([e for e in entries if e["kind"] == "start"]) == 1, entries
    requests = [json.loads(body) for body in f.sent()[-4:]]
    assert all("First ask." in json.dumps(request["messages"]) for request in requests), requests

    # --continue reopens the person's newest session, never the project's later, unnamed dream.
    replies += [hello, hello, Reply((REPLIES / "continued.sse").read_bytes()), hello]
    dreamt = part("dreamt")
    mine = dreamt.ask("Keep my work.")["session"]
    # A session id leads with the UTC second it started in, so the dream starts in a later one.
    server.wait(lambda: time.strftime("%Y%m%d-%H%M%S", time.gmtime()) > mine[:15], 2)
    made = dreamt.run("dream")
    assert made.returncode == 0, made.stderr
    server.until(f, made.stdout.strip(), lambda es: es[-1]["kind"] == "turn_end")
    continued = dreamt.ask("--continue", "Carry on.")
    assert continued == {"session": mine, "message": "continued reply"}, continued
    lone = part("lone")
    made = lone.run("dream")
    assert made.returncode == 0, made.stderr
    server.until(f, made.stdout.strip(), lambda es: es[-1]["kind"] == "turn_end")
    refused = lone.run("--continue", "--print=text", input="Carry on.")
    assert refused.returncode != 0, refused
    assert "There is no session in this directory to continue." in refused.stderr, refused.stderr

    # In a resident's folder --continue opens the resident's home, as before dreams were skipped:
    # its id is its name, so it sorts before its newer thread; a dream there is the resident's
    # fresh run, left out of the plain list too.
    kept = part("kept")
    config.write_text(default + f'\n[residents.keeper]\ncwd={json.dumps(str(kept.cwd))}\n')
    server.stop(f)
    server.start(f)
    server.until(f, "keeper", bool)
    server.post(f, "/sessions", {"resident": "keeper"})
    replies += [hello, Reply((REPLIES / "continued.sse").read_bytes())]
    made = kept.run("dream")
    assert made.returncode == 0, made.stderr
    server.until(f, made.stdout.strip(), lambda es: es[-1]["kind"] == "turn_end")
    continued = kept.ask("--continue", "Carry on.")
    assert continued == {"session": "keeper", "message": "continued reply"}, continued
    config.write_text(default)

    # Six clients start on a stopped home at once: the servers that lose the lock exit, and the winner
    # answers every call.
    server.stop(f)
    replies += [hello] * 6
    raced = part("raced")
    clients = [subprocess.Popen([BINARY, "--print=json", prompt], env=f.env, cwd=raced.cwd, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE) for _ in range(6)]
    results = [(client.communicate(timeout=90), client.returncode) for client in clients]
    assert [code for _, code in results] == [0] * 6, results

# A session's avatar is a recorded choice, kept across rebuild and process restart.
with flow([done(), response([call("handover", '{"note":"Keep the avatar."}', "avatar_note")]), done()], model="sol", config=PLAIN) as f:
    picture = f.home / ".razor/owl.png"
    picture.write_bytes(Path(__file__).with_name("replies").joinpath("avatar.png").read_bytes())
    person = picture.with_name("config.toml")
    person.write_text(person.read_text() + '\n[brand.avatars]\nowl={file="owl.png"}\n')
    session = f.ask("Choose a face.")["session"]
    route = f"/sessions/{session}/avatar"
    result = server.send(f, "PUT", route, "missing")
    assert result.status == 400, (result.status, result.read())
    result = server.send(f, "PUT", route, "owl")
    assert result.status == 200, (result.status, result.read())
    assert server.agents(f)[session]["avatar"] == "owl"
    f.ask("--resume", session, "Rebuild this conversation.")
    assert any(e["kind"] == "rebuild" for e in f.record(session)), "avatar was never checked over a rebuild"
    assert server.agents(f)[session]["avatar"] == "owl"
    server.stop(f)
    server.start(f)
    assert json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())["avatar"] == "owl"
    result = server.send(f, "PUT", route, "")
    assert result.status == 200, (result.status, result.read())
    assert server.agents(f)[session].get("avatar") is None
    assert [e["name"] for e in f.record(session) if e["kind"] == "avatar"] == ["owl", ""]

# The server records one lottery result; changing the pool never redraws existing sessions.
with flow([]) as f:
    state = Path(f.env["RAZOR_HOME"])
    state.joinpath("owl.png").write_bytes(REPLIES.joinpath("avatar.png").read_bytes())
    person = state / "config.toml"
    original = person.read_text()
    pool = [f"face{n:02}" for n in range(16)]
    config = original + "\n[brand.avatars]\n" + "".join(f'{name}={{file="owl.png",pool=true}}\n' for name in pool)
    person.write_text(config)
    crew = sum(line.startswith("%actor/") and line.split()[0].endswith("/0")
               for line in Path(__file__).resolve().parents[1].joinpath("src/terminal/data/stage.txt").read_text().splitlines())
    server.start(f)
    def draw_for(session):
        hashed = 2166136261
        for byte in session.encode():
            hashed = ((hashed ^ byte) * 16777619) & 0xffffffff
        return hashed % (crew + len(pool))
    chosen = {}
    kinds = set()
    for _ in range(64):
        session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        draw = draw_for(session)
        expected = pool[draw-crew] if draw >= crew else None
        row = json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())
        assert row.get("avatar") == expected, (session, draw, row)
        entries = [e for e in f.record(session) if e["kind"] == "avatar"]
        assert [e["name"] for e in entries] == ([] if expected is None else [expected]), entries
        kinds.add(expected is not None)
        chosen[session] = expected
        if len(kinds) == 2:
            break
    assert len(kinds) == 2, "no example of both pool and crew selection in 64 draws"
    person.write_text(config.replace("pool=true", "pool=false"))
    server.wait(lambda: not json.loads(server.send(f, "GET", f"/sessions/{next(iter(chosen))}/settings").read())["config"]["brand"]["avatars"]["face00"]["pool"], 5)
    server.stop(f)
    server.start(f)
    for session, expected in chosen.items():
        assert json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read()).get("avatar") == expected
    # Resident homes and their threads inherit; an explicit session choice takes priority.
    person.write_text(config + f'\n[residents.scout]\ncwd={json.dumps(str(f.cwd))}\navatar="face00"\n')
    server.stop(f)
    server.start(f)
    server.until(f, "scout", lambda es: bool(es))
    for _ in range(64):
        thread = server.post(f, "/sessions", {"resident": "scout"})["session"]
        if draw_for(thread) >= crew:
            break
    assert draw_for(thread) >= crew, "no resident thread whose hash would draw from the pool"
    with server.send(f, "GET", "/status") as stream:
        json.loads(stream.readline())
        response = server.send(f, "PUT", "/sessions/scout/name", "Watching the resident")
        assert response.status == 200, response.read()
        while True:
            line = json.loads(stream.readline())
            if line["kind"] == "agent" and line["session"] == "scout":
                assert line.get("avatar") is None, line
                break
    for session in ("scout", thread):
        row = json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())
        assert row.get("avatar") is None, row
        assert not any(e["kind"] == "avatar" for e in f.record(session)), "resident drew from the pool"
    response = server.send(f, "PUT", f"/sessions/{thread}/avatar", "face01")
    assert response.status == 200, response.read()
    assert json.loads(server.send(f, "GET", f"/sessions/{thread}/agent").read())["avatar"] == "face01"
    with server.send(f, "GET", "/status") as stream:
        assert json.loads(stream.readline())["brand"]["resident_avatars"] == {"scout": "face00"}
        person.write_text(person.read_text().replace('avatar="face00"', 'avatar="face02"'))
        while True:
            line = json.loads(stream.readline())
            if line["kind"] == "agents" and line["brand"]["resident_avatars"].get("scout") == "face02":
                assert next(r for r in line["agents"] if r["session"] == "scout").get("avatar") is None
                break
    assert json.loads(server.send(f, "GET", f"/sessions/{thread}/agent").read())["avatar"] == "face01", "config overrode the session's recorded choice"
    response = server.send(f, "PUT", f"/sessions/{thread}/avatar", "")
    assert response.status == 200, response.read()
    assert json.loads(server.send(f, "GET", f"/sessions/{thread}/agent").read()).get("avatar") is None
