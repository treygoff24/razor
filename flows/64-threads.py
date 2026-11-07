"""A resident's threads in the agents list (docs/design/agents-view.md): after a restart, its open
threads and no fresh run, its closed ones in Earlier; one starts from the
"+ New thread" row and from `n`, as the resident's (flow 56 starts one with `/thread`)."""
import json
import os
import signal
import time

from driver import REPLIES, Reply, flow
import server
from server import gone, post, send, until
from terminal import Terminal
from model import agent, reply

turns = [f"THREAD_64_{n}" for n in range(1, 8)]
replies = [Reply((REPLIES / "done.sse").read_bytes(), match=m) for m in turns]
replies += [agent("Fresh run.", "start", name="fresh-child", brief="Wait for release."),
            agent("Fresh run.", "wait", timeout_ms=30000),
            reply("You are fresh-child,", "Bash", {"command": "until [ -e release ]; do sleep .05; done"}),
            agent("You are fresh-child,", "finish", status="finished", report="FRESH_DONE"),
            reply("FRESH_DONE")]
with flow(replies,
          config='[server]\naddress = "127.0.0.1:0"') as f:
    (f.cwd / "persona.md").write_text("You are Ember.")
    with (f.home / ".razor/config.toml").open("a") as config:
        config.write(f"\n[residents.ember]\ncwd = {json.dumps(str(f.cwd))}\n"
                     f"startup = [{json.dumps(str(f.cwd / 'persona.md'))}, \"razor\"]\n"
                     '[residents.ember.schedules.fresh]\nat = []\ntext = "Fresh run."\nfresh = true\n')
    pid = server.start(f)
    ask = lambda path: json.loads(send(f, "GET", path).read())

    def talk(n, name):
        """A thread of Ember's, named `name`, that has answered turn `n`."""
        made = post(f, "/sessions", {"resident": "ember"})["session"]
        post(f, f"/sessions/{made}/input", {"text": f"THREAD_64_{n}"})
        until(f, made, lambda es: es[-1]["kind"] == "turn_end")
        send(f, "PUT", f"/sessions/{made}/name", name).read()
        return made

    # Two roots with no input: the older one's record will be days old at the restart.
    old = post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    time.sleep(1.1)
    # Thirteen threads nobody spoke in, then five a second apart, so their ids order them: the first
    # four closed; then a fresh run.
    spare = [post(f, "/sessions", {"resident": "ember"})["session"] for _ in range(13)]
    # Each is stored named for its resident and a word, no two alike.
    names = {next(f.record(s)).get("name", "") for s in spare}
    assert len(names) == 13 and all(n.startswith("ember-") for n in names), names
    time.sleep(1.1)
    threads = []
    for n, name in enumerate(named := ["tc-one", "tc-two", "tc-three", "tc-four", "tc-five"], 1):
        threads.append(talk(n, name))
        time.sleep(1.1)
    for thread in threads[:4]:
        post(f, f"/sessions/{thread}/quit", {})
        until(f, thread, lambda es: es[-1].get("closed") and es[-1]["at"] < time.time_ns() // 1_000_000)
    run = post(f, "/residents/ember/run/fresh", {})["entry"]
    fresh = next(e for e in f.record("ember") if e["id"] == run)["session"]
    started = until(f, fresh, lambda es: any(e["kind"] == "child" and "started" in e["event"] for e in es))
    kid = next(e["session"] for e in started if e["kind"] == "child")
    until(f, kid, lambda es: any(e["kind"] == "job" for e in es))
    assert not any(server.listed(f, s) for s in (fresh, kid, spare[0])), "an unspoken thread or its helper was listed"
    (f.cwd / "release").touch()
    until(f, fresh, lambda es: any(e["kind"] == "turn_end" for e in es))
    # The list leaves a fresh run out when asked to, and its limit counts only what it returns.
    newest = lambda query: [s["session"] for s in ask(f"/sessions?resident=ember&limit=1{query}")]
    assert newest("") == [fresh] and newest("&jobs=false") == [threads[-1]]
    # A fresh run's thread is renamed for its job at once.
    assert ask("/sessions?resident=ember&limit=1")[0]["name"].startswith("fresh · ")
    recent = post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    # A restart loads no idle session, yet the status holds the roots changed in the last twelve
    # hours, and the twelve most recently changed threads whatever their age, among them the spare
    # one made first, which changed after the others, but not the last.
    oldest, *rest = sorted(spare)
    os.kill(pid, signal.SIGKILL)
    gone(pid)
    for age, session in enumerate([old, *threads, oldest, *rest]):
        os.utime(f.home / ".razor/sessions" / session / "record.jsonl", (time.time() - 259200 - age,) * 2)
    server.start(f)
    newest("")  # the fresh run's row joins the status, to be left out of the list
    assert server.listed(f, threads[-1])

    with Terminal(f, "--agents", columns=160, rows=40) as t:
        t.shown("RESIDENTS")
        t.click("RESIDENTS")
        t.shown("+ New thread", "tc-five")
        # A closed thread leaves its resident for the folded Earlier, as a closed root does.
        assert not any(n in t.text() for n in (*named[:4], "fresh ·")), t.text()
        # Closed sessions' rows arrive in the background after a restart; click once all are in.
        # With them is the root nobody wrote to, which --agents therefore left unopened.
        t.until(lambda t: any("EARLIER" in line and "5 AGENTS" in line for line in t.screen.display))
        t.click("EARLIER")
        t.shown(*named[:4])
        assert {recent, oldest} <= set(agents := server.agents(f)) and not {old, rest[-1]} & set(agents)

        def start(how, n, name):
            """Aims the composer at a new thread of Ember's, sends turn `n`, and names the thread."""
            how()
            t.shown("New thread with ember", "New thread · ember")
            t.send(f"THREAD_64_{n}\r")
            t.until(lambda t: len(ask("/sessions?resident=ember&jobs=false")) == n + len(spare))
            t.until(lambda t: "flow done" in "\n".join(line[54:] for line in t.screen.display))
            t.send(f"/name {name}\r")
            t.until(lambda t: any(f"○ {name} " in line[:54] for line in t.screen.display)
                    and name in t.screen.display[2][54:])

        # From the row, and from `n` after moving down.
        start(lambda: t.click("+ New thread"), 6, "tc-six")
        t.send("/name\r")
        t.shown("USE /NAME <NEW NAME> TO RENAME THIS SESSION.")
        t.send("\x15/name " + "x" * 65 + "\r")
        t.shown("A NAME IS ONE LINE OF 1 TO 64 CHARACTERS.")
        assert ask("/sessions?resident=ember&jobs=false&limit=1")[0]["name"] == "tc-six"
        t.send("`h")  # the prefix and h give the list the keys
        start(lambda: t.send("jn"), 7, "tc-seven")
        # The resident's block folds and opens again.
        t.click("▾ ○ ember")
        t.until(lambda t: "+ New thread" not in t.text() and "tc-six" not in t.text())
        t.click("▸ ○ ember")
        t.shown("+ New thread", "tc-six", "tc-seven")
        # Every arrow press lands on a drawn mark, up past the resident: its fold is no stop of its own.
        t.click("○ tc-seven ")
        t.send("`h")
        for _ in range(13):
            t.send("k")
            t.pump(0.3)
            # A selected parent's fold arrow turns cyan; any other selected row shows ❯.
            cells = [t.screen.buffer[y][x] for y in range(t.rows) for x in range(54)]
            assert any(c.data == "❯" or c.data in "▾▸" and c.fg == "3fd9f2" for c in cells), t.text()

# A close moves on only a person still on that session: one who went on to type a new thread keeps
# it, and the words come back after a visit elsewhere (Trey, 2026-11-06: "I lost everything I was typing").
closing = "The person asked for `/done`"
rs = [f"r{n}" for n in range(1, 8)]
replies = [reply("CLOSE_64_A"), reply("CLOSE_64_B"),
           reply(closing, "Bash", {"command": "until [ -e closed ]; do sleep .05; done"}), reply(closing)]
with flow(replies, config='[server]\naddress = "127.0.0.1:0"') as f:
    with (f.home / ".razor/config.toml").open("a") as config:
        config.write("".join(f"\n[residents.{r}]\ncwd = {json.dumps(str(f.cwd))}\n" for r in ("ember", "fir", *rs)))
    server.start(f)
    a, b = (post(f, "/sessions", {"resident": "ember"})["session"] for _ in "ab")
    for s, name in ((a, "close-a"), (b, "close-b")):
        post(f, f"/sessions/{s}/input", {"text": name.replace("close-", "CLOSE_64_").upper()})
        until(f, s, lambda es: es[-1]["kind"] == "turn_end")
        send(f, "PUT", f"/sessions/{s}/name", name).read()
    with Terminal(f, "--agents", columns=160, rows=40) as t:
        t.shown("RESIDENTS")
        t.click("RESIDENTS")
        t.shown("close-a", "close-b")
        t.click("○ close-a ")
        t.until(lambda t: "close-a" in t.screen.display[2][54:])
        t.send("/done\r")
        t.shown("closing after this turn")
        t.click("+ New thread")
        t.send("UNSENT WORDS")
        t.shown("New thread · ember", "UNSENT WORDS")
        (f.cwd / "closed").touch()
        until(f, a, lambda es: es[-1].get("closed"))
        t.pump(2)
        t.shown("New thread · ember", "UNSENT WORDS")
        t.click("○ close-b ")
        t.until(lambda t: "UNSENT WORDS" not in t.text())
        # Go to, opening the closed one, opens the folded Earlier to select its row.
        t.send("/goto\r/close-a")
        t.shown("1 of")
        t.send("\r")
        t.until(lambda t: "┌─ Go to" not in t.text() and any("❯" in row and "close-a" in row[:54] for row in t.screen.display))
        # Going to a thread in the open Residents keeps them open, its row selected.
        t.send("/goto\r/close-b")
        t.shown("1 of")
        t.send("\r")
        t.until(lambda t: "close-b" in t.screen.display[2][54:] and any("❯" in row and "close-b" in row[:54] for row in t.screen.display))
        t.click("+ New thread")
        t.shown("New thread · ember", "UNSENT WORDS")
        # They reach only that destination: another resident's new thread starts empty, from a
        # session or straight from this one, and its own words wait the same way.
        t.click("○ close-b ")
        t.send("/thread fir\r")
        t.shown("New thread · fir")
        assert "UNSENT WORDS" not in t.text(), t.text()
        t.send("FIR WORDS")
        t.shown("FIR WORDS")
        t.click("+ New thread")
        t.shown("New thread · ember", "UNSENT WORDS")
        assert "FIR WORDS" not in t.text(), t.text()
        # Eight new agents' words wait at most: with eight waiting, leaving a ninth's is refused,
        # but going back to one that waits is not; quitting names where they wait.
        for r in rs:
            t.click("○ close-b ")
            t.until(lambda t: "close-b" in t.screen.display[2][54:])
            t.send(f"/thread {r}\r")
            t.shown(f"New thread · {r}")
            t.send(f"WORDS-{r}")
            t.shown(f"WORDS-{r}")
        t.click("○ close-b ")
        t.shown("WORDS FOR 8 NEW AGENTS ALREADY WAIT", "New thread · r7", "WORDS-r7")
        t.click("+ New thread")
        t.shown("New thread · ember", "UNSENT WORDS")
        # At 80 columns the armed quit shows every waiting destination and the way back whole,
        # under its instruction; Ctrl-C first empties ember's draft, so it is no longer listed.
        t.resize(40, 80)
        t.send("\x03")
        t.until(lambda t: "UNSENT WORDS" not in t.text())
        t.send("\x03")
        lost = "Quitting loses the unsent words for " + ", ".join(f"a thread with {r}" for r in ["fir", *rs])

        def band(t):
            """The screen from the armed quit notice's column, right of the list, as one line
            without spaces, since a row may end after a slash."""
            x = next((row.index("PRESS CTRL-C") - 1 for row in t.screen.display if "PRESS CTRL-C" in row), 0)
            return "".join("".join(row[x:] for row in t.screen.display).split())

        t.until(lambda t: "PRESS CTRL-C AGAIN TO QUIT" in t.text() and
                "".join(f"{lost}; /new or /thread there keeps them.".split()) in band(t))
        # Its wrapped rows start under its first word.
        y, x = next((y, row.index("Quitting loses")) for y, row in enumerate(t.screen.display) if "Quitting loses" in row)
        assert t.screen.display[y + 1][x - 1] == " " != t.screen.display[y + 1][x], t.text()
        t.quit_said = lost
        t.send("\x03")
        t.leaves("the second Ctrl-C did not quit")
