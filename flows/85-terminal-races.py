"""Delayed real server replies cannot consume a later draft or select over a new draft.
A socket proxy holds only transport bytes; Razor owns all session and composer state."""
from contextlib import contextmanager
import base64
import os
import select
import signal
import socket
import socketserver
import threading
from driver import flow
from responses import done
import server
from terminal import Terminal


@contextmanager
def delayed_status(f):
    path, upstream = f.home / ".razor/server.sock", f.home / ".razor/upstream.sock"
    path.rename(upstream)
    arrived, release = threading.Event(), threading.Event()

    class Hold(socketserver.BaseRequestHandler):
        def handle(self):
            with socket.socket(socket.AF_UNIX) as remote:
                remote.connect(str(upstream))
                while True:
                    for source in select.select([self.request, remote], [], [], 1)[0]:
                        data = source.recv(65536)
                        if not data:
                            return
                        if source is self.request and b"GET /v1/status?views=true" in data:
                            arrived.set()
                            if not release.wait(30):
                                return
                        (remote if source is self.request else self.request).sendall(data)

    proxy = socketserver.ThreadingUnixStreamServer(str(path), Hold)
    proxy.daemon_threads = True
    thread = threading.Thread(target=proxy.serve_forever, daemon=True)
    thread.start()
    try:
        yield arrived, release
    finally:
        release.set()
        proxy.shutdown()
        proxy.server_close()
        thread.join()
        upstream.replace(path)


with flow([done(), done(), done(), done()], model="sol", config="[ui]\nmotion = false\n") as f:
    pid = server.start(f)
    sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{sid}/input", {"text": "first85"})
    server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end")
    server.send(f, "PUT", f"/sessions/{sid}/name", "root85").read()
    # With no action by the person, startup still opens their latest session.
    with Terminal(f, "--agents") as t:
        t.shown("first85", "flow done")
        server.wait(lambda: server.agents(f)[sid]["terminal"])
    with delayed_status(f) as (arrived, release), Terminal(f, "--agents") as t:
        assert arrived.wait(10), "terminal never requested its first snapshot"
        t.shown("Message the agent")
        t.click("Message the agent")
        t.send("newdraft85")
        t.shown("newdraft85")
        release.set()
        t.shown("newdraft85", "AGENTS")
        assert t.quiet() and "newdraft85" in t.text(), t.text()
        assert not server.agents(f)[sid]["terminal"], "startup selected over the new draft"
    # A FIFO holds a genuine image read open until the attempted departure is checked.
    image = f.cwd / "waiting85.png"
    os.mkfifo(image)
    with Terminal(f) as t:
        t.shown("Message the agent")
        t.send(f"\x1b[200~{image}\x1b[201~")
        t.shown(str(image))
        t.send("\x1bOQ")
        t.shown(" root85 ")
        try:
            t.click(" root85 ")
            assert t.quiet() and not server.agents(f)[sid]["terminal"], "left a draft with its image unread"
        finally:
            with image.open("wb") as out:
                out.write(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aMk0AAAAASUVORK5CYII="))
        t.shown("[Image #1]")
    peer = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{peer}/input", {"text": "peer85"})
    server.until(f, peer, lambda es: es[-1]["kind"] == "turn_end")
    for command in ("/done", "/quit"):
        with Terminal(f, "--resume", sid, "--agents") as t:
            t.shown("flow done", "Message the agent")
            t.click("Message the agent")
            os.kill(pid, signal.SIGSTOP)
            try:
                t.send(command + "\rafterclose85")
                t.shown("afterclose85")
                assert command + "afterclose85" not in t.text(), "submitted command stayed in draft"
            finally:
                os.kill(pid, signal.SIGCONT)
            server.until(f, sid, lambda es: es[-1].get("closed") is True)
            t.until(lambda _: "afterclose85" in t.text() and t.quiet())
            assert "afterclose85" in t.text(), "close reply erased the later draft"
            assert not server.agents(f)[peer]["terminal"], "close selected another session over the draft"
            server.wait(lambda: server.send(f, "GET", f"/sessions/{sid}/draft").read() == b"afterclose85")
        # Clear the unsent draft and reopen for the next close command.
        server.send(f, "PUT", f"/sessions/{sid}/draft", "").read()
        if command == "/done":
            server.post(f, f"/sessions/{sid}/input", {"text": "reopen85"})
            server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end" and not es[-1].get("closed"))

# Hand-back sends only the skill; both the visible draft and its saved copy survive admission.
import claude
for handed85 in (False, True):
    replies85 = ([] if handed85 else [claude.reply((None, "mcp__razor__skill", {"name": "plan"}))])
    replies85 += [claude.reply(text="loaded85"), claude.reply((None, "mcp__razor__handover", {"note": "hand85"})),
                  claude.reply(text="rebuilt85"), claude.reply(text="handed85")]
    with flow(replies85) as f:
        path = f.cwd / ".agents/skills/plan"
        path.mkdir(parents=True)
        (path / "SKILL.md").write_text("---\ndescription: Plans.\n---\nplan85\n")
        server.start(f)
        sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        server.post(f, f"/sessions/{sid}/input", {"text": "Load plan.", **({"skill": "plan"} if handed85 else {})})
        server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end")
        (path / "SKILL.md").write_bytes(b"\xff\xfe")
        asked85 = server.post(f, f"/sessions/{sid}/handover", {})["entry"]
        server.until(f, sid, lambda es: es[-1]["kind"] == "turn_end" and asked85 in es[-1]["inputs"])
        (path / "SKILL.md").write_text("plan85 restored")
        with Terminal(f, "--resume", sid, rows=60) as t:
            t.shown("rebuilt85")
            if not handed85:
                t.send("\x1b1")
            t.shown("hand it back")
            t.send("unfinished85")
            t.shown("unfinished85")
            draft = lambda: server.send(f, "GET", f"/sessions/{sid}/draft").read()
            server.wait(lambda: draft() == b"unfinished85")
            t.click("hand it back")
            if not handed85: t.send("\x1b")  # closes the card; with none open Escape stops the turn
            t.shown("handed85", "unfinished85")
            assert draft() == b"unfinished85", "hand-back discarded the saved draft"
            loaded85 = server.agents(f)[sid]["skills"]
            assert len(loaded85) == 1 and loaded85[0]["how"] == "handed" and loaded85[0]["worn"] is None, loaded85
