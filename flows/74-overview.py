"""The overview fits its agents, opens the selected session, and folds at once without motion or below 80 columns; the
keys move between the list and a composer, past helpers. Parallel code-mode results leave the newest open call visible."""
from dataclasses import replace
import base64, json, os, select, signal, socket, socketserver, threading
from clock import wall_clock
from driver import flow
from responses import done, message, response
import server
from terminal import Terminal

LEFT, RIGHT, UP, DOWN, BACKTAB = "\x1b[D", "\x1b[C", "\x1b[A", "\x1b[B", "\x1b[Z"
CYAN = "3fd9f2"
names = ["older74", "middle74", "newest74"]
wait = 'async function wait(run) { let job = await run; while (!job.end) job = await tools.write_stdin({session_id:job.job, yield_time_ms:300000}); }'
source = wait + "await Promise.all([" + ",".join(f'wait(tools.{n}({{path:"{n}"}}))' for n in names) + "]);"
# The keyboard loop through bindings like Trey's pack; the defaults leave these actions unbound.
keys = """[ui.keys.list]\nup = []\ndown = []\nleft = []\nright = []\naccept = ["a"]\nprevious_agent = ["Up", "k"]
next_agent = ["Down", "j"]\noverview = ["Left", "h"]\nbeside = ["Enter", "Right", "l"]\n[ui.keys.line_start]\noutward = []
choose = ["Left"]\n[ui.keys.composer]\nswitch = ["Shift-Tab"]\n"""
config = 'capture = ["sh", "-c", "cat /dev/zero"]\ntranscribe = ["true"]\n[ui]\nmotion = false\n' + keys + "".join(
    f'[tools.{n}]\ncommand = "while [ ! -e {n} ]; do sleep 0.05; done"\n'
    'description = "Wait for the named file."\nread_only = true\ninput = {type = "object"}\n' for n in names)
cell = lambda js, id, match=None: replace(response([{"type": "custom_tool_call", "id": f"ct_{id}", "call_id": f"call_{id}",
                                                    "name": "exec", "input": '// @exec: {"yield_time_ms":120000}\n' + js}]), match=match)
said = lambda words, match=None: replace(response([message(words)]), match=match)
# b74's helper waits on middle74 as work74 does, then reports blocked, which keeps it listed.
replies = [said("answer-a74. transcript-a74"), cell('await tools.agent({action:"start",name:"kid74",brief:"Wait.",placement:"in_place"});', "start"),
           said("answer-b74. transcript-b74"), cell(source, "parallel"), done(), said("noted74", "Blocked74."), said("answer-loop74", "loop74"), said("answer-image74", "typed74"), cell('await tools.message({to:"parent",type:"question",text:"Pick74?",options:["x74","y74"]});', "ask74", "launch74"),
           said("answer-launch74", "launch74"),
           cell(wait + 'await wait(tools.middle74({path:"kid74"})); await tools.agent({action:"finish",status:"blocked",report:"Blocked74."});', "kid", "You are kid74,"),
           said("answer-c74.", "c74")]
with flow(replies, model="sol", config=config) as f:
    def root(name):
        sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        server.send(f, "PUT", f"/sessions/{sid}/name", name).read()
        server.post(f, f"/sessions/{sid}/input", {"text": name})
        return sid

    pid = server.start(f)
    a = root("a74")
    server.until(f, a, lambda es: es[-1]["kind"] == "turn_end")
    b = root("b74")
    server.until(f, b, lambda es: es[-1]["kind"] == "turn_end")
    work = root("work74")
    entries = server.until(f, work, lambda es: sum(e["kind"] == "call" for e in es) == 4)
    calls = [e for e in entries if e["kind"] == "call"][1:]

    def doing(call):
        expected = [call["name"], call["input"]["path"], call["at"]]
        server.wait(lambda: server.agents(f)[work].get("doing") == expected,
                    why=f"newest open call is not {expected}")

    doing(calls[2])
    for released, current in ((0, 2), (2, 1)):
        (f.cwd / names[released]).touch()
        server.until(f, work, lambda es: any(e["kind"] == "result" and e["call"] == calls[released]["call"] for e in es))
        doing(calls[current])
    view = {"name": "Work", "groups": [{"id": "g74", "name": "Project74", "colour": "plum",
            "matches": [{"session": sid} for sid in (b, a, work)], "sort": "mine", "order": [b, a, work]}]}
    answer = server.send(f, "PUT", "/views", {"version": 0, "views": [view]})
    assert answer.status == 200, answer.read()
    answer.read()
    # Closing the list's last row, c, addresses the open top-level row above it, b, and not b's
    # working helper drawn between them, which is another tree's, though c's row leaves first.
    c = root("c74")
    server.until(f, c, lambda es: es[-1]["kind"] == "turn_end")

    def order(*sessions):
        version = json.loads(server.send(f, "GET", "/views").read())["version"]
        group = {**view["groups"][0], "matches": [{"session": s} for s in sessions], "order": list(sessions)}
        server.send(f, "PUT", "/views", {"version": version, "views": [{**view, "groups": [group]}]}).read()

    order(a, work, b, c)
    with Terminal(f, "--resume", c, rows=20) as t:
        t.send("\x1bOQ")
        # The list in the order just saved: layouts publish at most once a second, and /quit picks its neighbour from the rows shown.
        at = lambda n: next((i for i, line in enumerate(t.screen.display) if n in line[:40]), 99)
        t.until(lambda _: at("a74") < at("work74") < at("b74") < at("kid74") < at("c74") < 99)
        t.send("/quit\r")
        t.until(lambda _: "b74" in t.screen.display[2][41:])
        # Quitting beside the list names the session on screen, though another was opened first.
        t.send(BACKTAB + UP + "\r")
        t.until(lambda _: "work74" in t.screen.display[2][41:])
        t.quit()
        assert b"Left work74; it stays open." in t.output, t.output[-300:]
    order(b, a, work)
    with wall_clock() as (clock, env, advance), Terminal(replace(f, env={**f.env, **env}), "--resume", a, rows=20) as t:
        # A real clock passes the minute boundary; a frozen one must start clear of it.
        advance(-(int((clock / "now").read_text()) % 60000))
        t.shown("transcript-a74")
        t.send(LEFT)  # the next layout, published at most once a second, leaves c out
        t.until(lambda _: "transcript-a74" in t.text() and "AGENTS · WORK" in t.text() and at("b74") < at("a74") < at("c74") == 99)
        t.send(LEFT)  # The list has the keys and opens the overview; frozen time cannot finish a fold.
        t.shown("PROJECT74", "work74", "a74", "b74")

        def selected(name):
            t.until(lambda _: "AGENTS · WORK" not in t.text() and any(f"▶ {name} " in line for line in t.screen.display))

        selected("a74")

        # With motion off and the wall clock still, the overview falls quiet once rows settle.
        assert t.quiet(), t.text()
        # Too short for its group at 40 columns, the overview scrolls to keep the selection in sight.
        t.resize(8, 40)
        t.until(lambda _: " a74 " in t.text() and " work74 " not in t.text())
        t.send(UP * 2)
        t.until(lambda _: " work74 " in t.text() and " a74 " not in t.text())
        t.resize(20, 120)
        t.shown("work74", "b74", "a74")
        t.send(UP + DOWN)  # Up at the first agent skips the open group heading.
        selected("b74")
        for key in (RIGHT, " ", "\r"):
            t.send(" ")
            t.until(lambda _: "❯─ 1  PROJECT74" in t.text() and " work74 " not in t.text())
            t.send(key)
            selected("work74")
            t.send(DOWN)
            selected("b74")
        t.send(RIGHT)  # Open b, although a was addressed before the overview.
        t.shown("AGENTS · WORK", "transcript-b74")
        t.send("\x0e\r")  # Beside the list, Ctrl-N selects b's helper's card and Enter opens it.
        t.until(lambda _: "kid74" in t.screen.display[2][41:])
        # Left goes back to b. Ctrl-T gives the pane Ctrl-N opened the keys, marking its tab; Ctrl-T
        # again gives them back and leaves the pane open; Esc from the conversation closes it.
        t.send(LEFT)
        t.until(lambda _: "b74" in t.screen.display[2][41:] and "STATUS" in t.text())
        tab = lambda: "STATUS" in t.text() and (lambda y, x: t.screen.buffer[y][x].bg)(*t.find("STATUS"))
        t.send("\x14")
        t.until(lambda _: tab() == CYAN)
        t.send("\x14")
        t.until(lambda _: tab() not in (CYAN, False))
        t.send("\x1b")
        t.until(lambda _: "STATUS" not in t.text())
        t.send("draft74")
        t.until(lambda _: "b74" in t.screen.display[2][41:] and "draft74" in t.text())
        t.send("\x01" + RIGHT + "X")  # Select all, collapse to its end, then append.
        t.shown("draft74X", "AGENTS · WORK")
        t.send(RIGHT)
        t.until(lambda _: "AGENTS" not in t.text() and "draft74X" in t.text())
        # Shift-Tab gives the list the keys over the draft, which closing the list kept, though typed
        # within the half second before its save; Down passes b's helper to a and Enter opens it.
        # Right closes the list before a's saved draft is read, with the server paused: the draft
        # stays a's. Typing and Enter send; Shift-Tab, Up past the helper and Enter bring b's back.
        server.wait(lambda: server.send(f, "GET", f"/sessions/{b}/draft").read() == b"draft74X")
        server.send(f, "PUT", f"/sessions/{a}/draft", "saved74 ").read()
        os.kill(pid, signal.SIGSTOP)
        t.send(BACKTAB + DOWN + "\r" + RIGHT)
        t.until(lambda _: "AGENTS" not in t.text() and "draft74X" not in t.text())
        os.kill(pid, signal.SIGCONT)
        t.shown("transcript-a74", "saved74")
        assert not t.pump(1, lambda _: "saved74" not in t.text()) and server.send(f, "GET", f"/sessions/{a}/draft").read() == b"saved74 ", t.text()
        t.send("loop74\r")
        t.shown("answer-loop74")
        # With the server paused, leaving b before its saved draft is read keeps the draft.
        header = lambda name: t.until(lambda _: name in t.screen.display[2][41:])
        os.kill(pid, signal.SIGSTOP)
        t.send(BACKTAB + UP + "\r")
        header("b74")
        t.send(BACKTAB + DOWN + "\r")
        header("a74")
        os.kill(pid, signal.SIGCONT)
        assert not t.pump(1) and server.send(f, "GET", f"/sessions/{b}/draft").read() == b"draft74X", t.text()
        # Words and an image typed while b's draft cannot be read keep b addressed until a refused
        # switch reads the draft again, bringing it in ahead of them, image and all.
        lock, png = f.home / ".razor/sessions" / b / "draft", f.cwd / "dot74.png"
        png.write_bytes(base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aMk0AAAAASUVORK5CYII="))
        lock.chmod(0)
        t.send(BACKTAB + UP + "\r")
        t.shown("PERMISSION DENIED")
        t.send(f"typed74 \x1b[200~{png}\x1b[201~")
        t.until(lambda _: "[Image #1]" in t.text() and "preparing" not in t.text())
        t.send(BACKTAB + DOWN + "\r")
        assert not t.pump(1) and "b74" in t.screen.display[2][41:], t.text()
        lock.chmod(0o600)
        t.send(BACKTAB + DOWN + "\r")
        t.shown("draft74X", "typed74 [Image #1]")
        assert "b74" in t.screen.display[2][41:], t.text()
        server.wait(lambda: server.send(f, "GET", f"/sessions/{b}/draft").read() == b"draft74X\ntyped74 [Image #1]", why=t.text())
        t.send("\r")
        t.shown("answer-image74")
        assert any(e["kind"] == "input" and e["text"].startswith("draft74X\n") and e["images"] for e in f.record(b))
        # A saved draft joins the words typed before it is read past 1 MiB too; the server refuses
        # to save them, keeping its draft, until they are shortened.
        big = "S" * (1 << 20)
        server.send(f, "PUT", f"/sessions/{a}/draft", big).read()
        os.kill(pid, signal.SIGSTOP)
        t.send(BACKTAB + DOWN + "\rover74")
        header("a74")
        os.kill(pid, signal.SIGCONT)
        t.shown("THE REQUEST WAS REFUSED", "over74")
        assert server.send(f, "GET", f"/sessions/{a}/draft").read() == big.encode()
        t.send("\x7f" * 7)
        t.until(lambda _: "over74" not in t.text())
        assert not t.pump(1) and server.send(f, "GET", f"/sessions/{a}/draft").read() == big.encode()
        t.send("\x15")
        server.wait(lambda: server.send(f, "GET", f"/sessions/{a}/draft").read() == b"")
        server.send(f, "PUT", f"/sessions/{b}/draft", "pre74").read()
        t.send(BACKTAB + UP + "\r")
        t.shown("pre74")
        # Drafts keep words only. Refused words lose their images to a stashed draft brought
        # back meanwhile, and leaving a session drops the editor's: each says so.
        folder = f.home / ".razor/sessions" / b

        def image(words=""):
            t.send(f"\x1b[200~{png}\x1b[201~")
            t.until(lambda _: f"❯ {words}[Image #" in t.text() and "preparing" not in t.text())

        image("pre74")
        t.send("\x13")
        t.until(lambda _: "❯ pre74" not in t.text())
        image()
        os.kill(pid, signal.SIGSTOP)
        folder.chmod(0o500)
        t.send("\r")
        t.pump(0.5)
        t.send("\x13")
        t.shown("pre74[Image #1]")
        os.kill(pid, signal.SIGCONT)
        t.shown("THE RETURNED WORDS' IMAGES GAVE WAY TO NEWER ONES. API 500")
        folder.chmod(0o700)
        t.send("\x01\x7f")
        t.shown("❯ Message the agent")
        image()
        t.send(BACKTAB + DOWN + "\r")
        header("a74")
        t.shown("DRAFTS KEEP WORDS ONLY; THE IMAGES IN THE WORDS LEFT WERE DROPPED")
        t.send(BACKTAB + UP + "\r")
        header("b74")
        t.shown("❯ [Image #")  # The saved draft keeps the chip's words alone.
        t.send("\x01\x7f")
        server.wait(lambda: server.send(f, "GET", f"/sessions/{b}/draft").read() == b"", why=t.text())
        # Words typed while an admission's draft save fails go after the returned ones, past 1 MiB
        # too: deleting them leaves the returned words, which save.
        old = "OLD74" + "o" * ((1 << 20) - 10)
        os.kill(pid, signal.SIGSTOP)
        folder.chmod(0o500)
        t.send(f"\x01\x1b[200~{old}\x1b[201~\rnew74")
        t.shown("new74")  # All typed before the save fails: a key after it would pass 1 MiB.
        os.kill(pid, signal.SIGCONT)
        t.shown("PERMISSION DENIED", "new74")
        folder.chmod(0o700)
        t.send("\x7f" * 6)
        server.wait(lambda: server.send(f, "GET", f"/sessions/{b}/draft").read() == old.encode(), why=t.text())
        # Leaving waits for the server to hold the words: a failed save keeps b addressed with
        # them, and the next switch saves them.
        folder.chmod(0o500)
        t.send("\x15fail74" + BACKTAB + DOWN + "\r")
        t.shown("PERMISSION DENIED", "fail74")
        # Behind a save already on its way, a switch still waits for its own and then goes; while
        # it waits, keys other than editing ones, such as recording, are refused.
        os.kill(pid, signal.SIGSTOP)
        folder.chmod(0o700)
        t.send("!")
        t.pump(1)
        t.send(BACKTAB + DOWN + "\r\x12")
        t.shown("SAVING BEFORE LEAVING; WAIT OR CTRL-C")
        assert not t.pump(1) and "b74" in t.screen.display[2][41:] and "Recording" not in t.text(), t.text()
        os.kill(pid, signal.SIGCONT)
        header("a74")
        assert not t.pump(1) and server.send(f, "GET", f"/sessions/{b}/draft").read() == b"fail74!"
        # Behind eight writes and a paused server, a switch cannot queue its save and is refused.
        os.kill(pid, signal.SIGSTOP)
        t.send("/name a74\r" * 9)
        t.pump(0.2)
        t.send("queue74" + BACKTAB + UP + "\r")
        t.shown("EIGHT DRAFT OR INPUT WRITES ARE PENDING", "queue74")
        assert "a74" in t.screen.display[2][41:], t.text()
        os.kill(pid, signal.SIGCONT)
        server.wait(lambda: server.send(f, "GET", f"/sessions/{a}/draft").read() == b"queue74")
        t.send(BACKTAB + UP + "\r")
        header("b74")
        t.send("\x15" + LEFT)  # Left at an empty line gives the list the keys, then the overview.
        t.until(lambda _: t.screen.cursor.hidden and "AGENTS · WORK" in t.text())
        t.send(LEFT)
        selected("b74")
        t.send("\x1b")
        t.shown("AGENTS · WORK", "transcript-b74")
        t.click("‹")
        t.until(lambda _: "AGENTS" not in t.text() and "transcript-b74" in t.text())
        t.send("\x1bOR")
        t.shown("PROJECT74")
        t.send("\x1b")
        t.until(lambda _: "PROJECT74" not in t.text() and "transcript-b74" in t.text())
        # Frozen time cannot finish motion: narrowing below 80 columns must land the fold.
        settings = f.home / ".razor/config.toml"
        settings.write_text(settings.read_text().replace("motion = false", "motion = true"))
        t.pump(1)
        t.send("\x1bOR")
        t.pump(.1)
        t.resize(20, 79)
        t.shown("PROJECT74", "b74")
        t.send("\x1bOR")
        t.shown("transcript-b74")
    # A first input given with --resume is sent over the saved draft, which stays.
    server.send(f, "PUT", f"/sessions/{a}/draft", "kept74").read()
    with Terminal(f, "--resume", a, "Say launch74.") as t:
        t.shown("answer-launch74", "Pick74?", "kept74")
        assert server.send(f, "GET", f"/sessions/{a}/draft").read() == b"kept74"
        # While leaving waits for its save, an answer waits as an input does, and words typed
        # after it go with the saved ones.
        t.send("\x15\rans74")
        t.shown("ans74")
        server.send(f, "PUT", f"/sessions/{b}/draft", "bdraft74").read()
        os.kill(pid, signal.SIGSTOP)
        t.send(BACKTAB)
        t.shown("○ b74 ")
        t.click("○ b74 ")
        t.send("\rlate74")
        t.shown("ans74late74")
        assert not t.pump(1) and "a74" in t.screen.display[2][41:], t.text()
        os.kill(pid, signal.SIGCONT)
        header("b74")
        server.wait(lambda: server.send(f, "GET", f"/sessions/{a}/draft").read() == b"ans74late74")
        assert not any(e["kind"] == "input" and "ans74" in e["text"] for e in f.record(a))
        # Accept closes the list once the deferred switch has opened the agent; until then the
        # list key is refused. b's draft is read first, or the switch would be refused for it.
        t.shown("bdraft74")
        os.kill(pid, signal.SIGSTOP)
        t.send("acc74" + BACKTAB)
        t.until(lambda _: t.screen.cursor.hidden)
        t.send(DOWN)
        t.until(lambda _: any("❯" in line and "work74" in line for line in t.screen.display))
        t.send("a\x1bOQ")
        t.shown("SAVING BEFORE LEAVING")
        assert not t.pump(1) and "b74" in t.screen.display[2][41:], t.text()
        os.kill(pid, signal.SIGCONT)
        t.until(lambda _: "work74 · cwd" in t.screen.display[2] and "AGENTS" not in t.text())
    # A save held past 17 seconds on the terminal's clock is still waited for: a's old words land before
    # the newer ones saved behind them as it is left, which stay its draft.
    upstream, path = f.home / ".razor/upstream.sock", f.home / ".razor/server.sock"
    path.rename(upstream)
    held, release = threading.Event(), threading.Event()

    class Hold(socketserver.BaseRequestHandler):
        def handle(self):
            with socket.socket(socket.AF_UNIX) as up:
                up.connect(str(upstream))
                while True:
                    for source in select.select([self.request, up], [], [], 1)[0]:
                        if not (data := source.recv(65536)):
                            return
                        if source is self.request and data.endswith(b"old408"):
                            held.set()
                            if not release.wait(30):
                                return
                            up.sendall(data)
                            source, data = up, up.recv(65536)
                        (up if source is self.request else self.request).sendall(data)

    threading.Thread(target=socketserver.ThreadingUnixStreamServer(str(path), Hold).serve_forever, daemon=True).start()
    with wall_clock(still=False) as (_, env, advance), Terminal(replace(f, env={**f.env, **env}), "--resume", a) as t:
        t.shown("ans74late74")
        t.send("\x15old408")
        assert held.wait(10)
        t.send(" new408" + BACKTAB)
        t.shown("○ b74 ")
        t.click("○ b74 ")
        assert "a74" in t.screen.display[2][41:], t.text()
        advance(17_000, monotonic=True)
        t.resize(40, 120)  # Wake Tokio so a regressed draft deadline must fire before release.
        assert not t.pump(1, lambda _: "b74" in t.screen.display[2][41:]), "the held save was given up on"
        release.set()
        header("b74")
        server.wait(lambda: server.send(f, "GET", f"/sessions/{a}/draft").read() == b"old408 new408")
        # The header precedes its draft read; quit must see the loaded draft before clearing it.
        t.shown("bdraft74acc74")
    upstream.replace(path)
    # A full write queue refuses take-back before copying words into the editor. Otherwise the
    # original could still run while its copied words are sent again.
    with Terminal(f, "--resume", work) as t:
        t.shown("Message the agent")
        t.send("queue_refused74\r")
        t.shown("queue_refused74", "↑ take back", "Message the agent")
        queued = next(e["id"] for e in f.record(work) if e.get("text") == "queue_refused74")
        os.kill(pid, signal.SIGSTOP)
        try:
            t.send("/name work74\r" * 12)
            t.shown("EIGHT DRAFT OR INPUT WRITES ARE PENDING")
            t.send("\x15" + UP)
            t.pump(.2)
            assert "Message the agent" in t.text(), "refused withdrawal copied queued words"
        finally:
            os.kill(pid, signal.SIGCONT)
        server.send(f, "DELETE", f"/sessions/{work}/input/{queued}").read()
    (f.cwd / names[1]).touch()
    server.until(f, work, lambda es: es[-1]["kind"] == "turn_end")
    server.until(f, b, lambda es: es[-1]["kind"] == "turn_end" and any(e["kind"] == "input" and "Blocked74." in str(e) for e in es))
    assert server.agents(f)[work].get("doing") is None, "finished call is still running"
    server.stop(f)
    server.start(f)
    server.wait(lambda: work in server.agents(f))
    assert server.agents(f)[work].get("doing") is None, "replay brought a finished call back"
