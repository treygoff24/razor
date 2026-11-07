"""The overview fits its agents, opens the selected session, and folds immediately without
motion or below 80 columns. Parallel code-mode results leave the newest open call visible."""
from dataclasses import replace
import re
from clock import wall_clock
from driver import flow
from responses import done, message, response
import server
from terminal import Terminal

LEFT, RIGHT, UP, DOWN = "\x1b[D", "\x1b[C", "\x1b[A", "\x1b[B"
names = ["older74", "middle74", "newest74"]
source = '// @exec: {"yield_time_ms":120000}\nawait Promise.all([' + ",".join(f'tools.{n}({{path:"{n}"}})' for n in names) + "]);"
config = "[ui]\nmotion = false\n" + "".join(
    f'[tools.{n}]\ncommand = "while [ ! -e {n} ]; do sleep 0.05; done"\n'
    'description = "Wait for the named file."\nread_only = true\ninput = {type = "object"}\n' for n in names)
replies = [response([message("answer-a74. transcript-a74")]), response([message("answer-b74. transcript-b74")]),
           response([{"type": "custom_tool_call", "id": "ct_parallel", "call_id": "call_parallel",
                      "name": "exec", "input": source}]), done()]
with flow(replies, model="sol", config=config) as f:
    def root(name):
        sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        server.send(f, "PUT", f"/sessions/{sid}/name", name).read()
        server.post(f, f"/sessions/{sid}/input", {"text": name})
        return sid

    server.start(f)
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
    with wall_clock() as (_, env, _), Terminal(replace(f, env={**f.env, **env}), "--resume", a, rows=20) as t:
        t.shown("transcript-a74")
        t.send(LEFT)
        t.shown("AGENTS ·", "transcript-a74")
        t.send(LEFT)  # The composer kept the keys; frozen time cannot finish an accidental fold.
        t.shown("PROJECT74", "work74", "a74", "b74")

        def selected(name):
            t.until(lambda _: "AGENTS ·" not in t.text() and any(f"▶ {name} " in line for line in t.screen.display))

        selected("a74")

        def framed(ages=True):
            """Each agent's age inside its tile, and every body row closed by its group's frame."""
            lines = t.screen.display
            assert not ages or all(any(re.search(rf" {n} .*?\d+[smh] +[▌│]", line) for line in lines)
                                   for n in ("work74", "b74", "a74")), t.text()
            assert all(not line.strip() or line.rstrip()[-1] in "│╮╯╴" for line in lines[5:-1]), t.text()

        framed()
        for height in (13, 10, 9):
            t.resize(height, 120)
            t.until(lambda _: all(f" {n} " in t.text() for n in ("work74", "b74", "a74")))
            assert any(" work74 " in line and " b74 " in line and " a74 " in line for line in t.screen.display), t.text()
            framed()
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
        t.shown("AGENTS ·", "transcript-b74")
        t.send("draft74")
        t.shown("draft74")
        t.send("\x01" + RIGHT + "X")  # Select all, collapse to its end, then append.
        t.shown("draft74X", "AGENTS ·")
        t.send(RIGHT)
        t.until(lambda _: "AGENTS" not in t.text() and "draft74X" in t.text())
        t.send("\x15" + LEFT)
        t.shown("AGENTS ·", "transcript-b74")
        t.send(LEFT)
        selected("b74")
        t.send("\x1b")
        t.shown("AGENTS ·", "transcript-b74")
        t.click("‹")
        t.until(lambda _: "AGENTS" not in t.text() and "transcript-b74" in t.text())
        t.send("\x1bOR")
        t.shown("PROJECT74")
        t.send("\x1b")
        t.until(lambda _: "PROJECT74" not in t.text() and "transcript-b74" in t.text())
        # Motion enabled at a frozen time would never land; narrowing must settle the fold.
        path = f.home / ".razor/config.toml"
        path.write_text(path.read_text().replace("motion = false", "motion = true"))
        t.pump(1)
        t.send("\x1bOR")
        t.pump(0.1)
        t.resize(20, 79)
        t.shown("PROJECT74", "b74")
        framed()
        t.resize(20, 40)
        t.pump(2)
        framed()
        t.resize(20, 20)  # Narrower than the smallest tile's group: the tiles narrow to fit.
        t.pump(2)
        framed(ages=False)
        t.send("\x1bOR")
        t.shown("transcript-b74")
        t.resize(20, 120)
        t.pump(2)
    (f.cwd / names[1]).touch()
    server.until(f, work, lambda es: es[-1]["kind"] == "turn_end")
    assert server.agents(f)[work].get("doing") is None, "finished call is still running"
    server.stop(f)
    server.start(f)
    server.wait(lambda: work in server.agents(f))
    assert server.agents(f)[work].get("doing") is None, "replay brought a finished call back"
