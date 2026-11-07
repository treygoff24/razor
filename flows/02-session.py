"""A reply and a durable turn, through the real Claude SDK and --print; what the startup list
and the session's facts give the model, and the startup.md that shows it."""
import json
import subprocess
import threading
from dataclasses import replace
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import chat
from driver import BINARY, REPLIES, Reply, flow
from terminal import Terminal
import server

prompt = "Hello"

# An immediate chat reply sends its live words before committing them. Print must subscribe
# before admitting input, including on resume; stored entries cannot replay these lines. A relay
# holds the real input acknowledgment until the turn ends, making the scheduling gap repeatable.
fast = chat.reply({"reasoning_content": "Fast thought.", "content": "Fast reply."})
with flow([fast] * 4, model="glm-5.3-flash",
          config='[providers.runinfra]\nbase_url = "{standin}"\n', env={"RUNINFRA_API_KEY": "flow-key"}) as f:
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

    relay = ThreadingHTTPServer(("127.0.0.1", 0), Relay)
    threading.Thread(target=relay.serve_forever, daemon=True).start()
    remote = f.home / "remote"
    remote.mkdir()
    address = f"127.0.0.1:{relay.server_port}"
    (remote / "config.toml").write_text(f'[remote]\naddress = "{address}"\nkey = "{f.home}/.razor/device.key"\n')
    client = replace(f, env={**f.env, "RAZOR_HOME": str(remote)})
    session = None
    for mode in ("stream", "stream", "text", "json"):
        result = client.run(f"--print={mode}", *(["--resume", session] if session else []), "Reply now.")
        assert result.returncode == 0, result.stderr
        if mode == "stream":
            lines = [json.loads(line) for line in result.stdout.splitlines()]
            session = lines[0]["session"]
            for kind, text in (("thought", "Fast thought."), ("delta", "Fast reply.")):
                assert "".join(e["text"] for e in lines if e["kind"] == kind) == text, lines
            assert [e["kind"] for e in lines if e["kind"] in ("input", "turn_end")] == ["input", "turn_end"], lines
        else:
            assert (json.loads(result.stdout)["message"] if mode == "json" else result.stdout.strip()) == "Fast reply.", result
    relay.shutdown()
    relay.server_close()

with flow([Reply((REPLIES / "hello.sse").read_bytes())]) as f:
    # The default list: Razor's text, then the project's AGENTS.md (an include in place) and CLAUDE.md.
    (f.cwd / "AGENTS.md").write_text("flow agents rules\n@shared.md\n@absent.md\n")
    (f.cwd / "shared.md").write_text("flow shared include\n")
    (f.cwd / "CLAUDE.md").write_text("flow claude rules\n")
    answer = f.ask(prompt, "--speed", "fast")
    assert answer["message"] == "flow reply", answer
    session = answer["session"]
    entries = list(f.record(session))
    assert entries[0]["cwd"] == str(f.cwd), entries[0]
    inputs = [e for e in entries if e["kind"] == "input"]
    assert len(inputs) == 1 and inputs[0]["text"] == prompt, inputs
    texts = [e["text"] for e in entries if e["kind"] == "text"]
    assert texts == ["flow reply"], texts
    assert entries[-1]["inputs"] == [inputs[0]["id"]], entries[-1]
    assert entries[-1]["end"] == "done", entries[-1]
    request = f.request(1)
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
# a missing entry named in the facts, never fatal.
with flow([Reply((REPLIES / "hello.sse").read_bytes())],
          config='startup = ["~/notes", "~/missing.md", "project"]') as f:
    (f.home / "notes").mkdir()
    (f.home / "notes/b.md").write_text("flow note b")
    (f.home / "notes/a.md").write_text("flow note a")
    (f.cwd / "AGENTS.md").write_text("flow agents rules")
    result = f.run("--print=json", input=prompt)
    assert result.returncode == 0, result.stderr
    request = f.request(1)
    assert request.get("speed") in (None, "standard"), request
    system = request["system"][-1]["text"]
    assert system.startswith(f"flow note a\n\nflow note b\n\nContents of {f.cwd.resolve()}/AGENTS.md:\n\nflow agents rules\n\n# Your tools"), system
    assert "You are working in Razor" not in system, system
    first = request["messages"][0]["content"][0]["text"]
    assert f"Left out of your startup list: {f.home}/missing.md (" in first, first

with flow([Reply((REPLIES / "hello.sse").read_bytes())]) as f:
    with Terminal(f, "Reply to this first sentence.") as t:
        t.shown("flow reply")

# Six clients start on a stopped home at once: the servers that lose the lock exit, and the winner
# answers every call.
with flow([Reply((REPLIES / "hello.sse").read_bytes())] * 6) as f:
    clients = [subprocess.Popen([BINARY, "--print=json", prompt], env=f.env, cwd=f.cwd, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE) for _ in range(6)]
    results = [(client.communicate(timeout=90), client.returncode) for client in clients]
    assert [code for _, code in results] == [0] * 6, results
