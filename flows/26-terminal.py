"""Attach the real app: transcript, child history, question band, status and MCP cells under a PTY."""
from dataclasses import replace
from datetime import datetime
import json
import os
import re
import shutil
import sys
import time
from pathlib import Path
import claude
from driver import flow
from model import agent, message, reply, tool
import mcp
import server
from terminal import Terminal
from wcwidth import wcswidth
from responses import done


# An explicit octant choice overrides tmux's default, while an omitted choice stays half there.
for setting, want in (('', False), ('[ui]\nblocks = "octant"', True)):
    with flow([], config=setting, env={'TMUX': 'flow'}) as f:
        with Terminal(f, columns=160) as t:
            t.shown('Message the agent')
            t.until(lambda t: any(0x1cd00 <= ord(c) <= 0x1cdef for c in t.text()) == want)


# A new drag can start on the quote chip left by a previous selection. A tap still quotes.
with flow([claude.reply(text='alpha beta gamma delta epsilon zeta')], env={'SSH_CONNECTION': 'flow'}) as f:
    with Terminal(f) as t:
        t.shown('Message the agent')
        t.send('hello\r')
        # The reply and its play mark draw a frame before the idle layout moves them down.
        t.shown('alpha beta gamma delta epsilon zeta', '▶', 'agent idle')
        y, x = t.find('alpha beta')
        t.send(f'\x1b[<0;{x+1};{y+1}M\x1b[<32;{x+5};{y+1}M\x1b[<0;{x+5};{y+1}m')
        t.shown('❝ quote')
        cy, cx = t.find('❝ quote')
        t.send(f'\x1b[<0;{cx+1};{cy+1}M\x1b[<32;{x+10};{y+1}M\x1b[<0;{x+10};{y+1}m')
        t.until(lambda t: t.output.count(b'\x1b]52;c;') >= 2)
        assert '> alpha' not in t.text(), t.text()
        t.click('❝ quote')
        t.shown('> ')


# A failed map mark beside the newest reply's play button keeps its own click.
with flow([claude.reply(text=' '.join('word' for _ in range(600))),
           claude.reply((None, 'Bash', {'command': 'exit 1'})),
           claude.reply(text='latest reply')], env={'SSH_CONNECTION': 'flow'}) as f:
    with Terminal(f, columns=120) as t:
        t.shown('Message the agent')
        t.send('hello\r')
        t.shown('word word')
        t.send('next\r')
        t.shown('latest reply', '▶')
        for rows in range(16, 81):
            t.resize(rows, 120)
            t.pump(.15)
            found = next(((y, wcswidth(line[:line.rindex('✗')])) for y, line in enumerate(t.screen.display)
                          if '▶' in line and '✗' in line and line.index('✗') > line.index('▶')), None)
            if found:
                y, x = found
                t.send(f'\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m')
                t.shown('JUMPED IN TRANSCRIPT', timeout=3)
                break
        else:
            raise AssertionError('failed mark never shared the reply cap row')


def opened(f, session, want):
    """Waits until the session's status row says whether a terminal app has it open."""
    server.wait(lambda: server.agents(f)[session]["terminal"] is want, 10, f"terminal open never became {want}")


# A skill is available on the very first input, before a session has been made.
with flow([claude.reply(text="first skill received")]) as f:
    f.cwd /= "project & skills"
    f.cwd.mkdir()
    local = f.cwd / ".agents/skills/local-first"
    local.mkdir(parents=True)
    (local / "SKILL.md").write_text("---\ndescription: A project skill.\n---\n")
    server.start(f)
    with Terminal(f) as t:
        t.shown("Message the agent")
        t.send("/local-first")
        t.shown("▸ /local-first", timeout=5)
        t.send("\x7f" * len("/local-first"))
        t.send("/new-steward")
        t.shown("▸ /new-steward", timeout=5)
        assert not server.agents(f), "completion created a session"
        t.send("\r")
        t.shown("first skill received")
        session = next((f.home / ".razor/sessions").iterdir()).name
        inputs = [e for e in f.record(session) if e["kind"] == "input"]
        assert any('<skill name="new-steward">' in e["text"] for e in inputs), inputs
    late = f.cwd / ".agents/skills/added-later"
    late.mkdir()
    (late / "SKILL.md").write_text("---\ndescription: Added after the session started.\n---\n")
    with Terminal(f, "--resume", session) as t:
        t.shown("first skill received")
        t.send("/added-later")
        t.shown("▸ /added-later", timeout=5)


# A meeting's lines draw nothing and never take the window or the cache: past more of them than the
# cache holds, an attach reads back to the screenful of reply above them and, following, draws the
# next reply live under it.
with flow([claude.reply(text=" ".join(f"B{i:03}" for i in range(800)) + " before-meeting-26"), claude.reply(text="after-meeting-26")]) as f:
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{session}/input", {"text": "hello"})
    last = server.until(f, session, lambda es: es[-1]["kind"] == "turn_end")[-1]
    server.stop(f)
    lines = [{"event": "started"}, *[{"event": "line", "who": "room", "text": "said", "start_ms": 0, "end_ms": 0}] * 2200,
             {"event": "ended", "reason": "ended"}]
    with (f.home / ".razor/sessions" / session / "record.jsonl").open("a") as record:
        record.writelines(json.dumps({"id": last["id"] + i, "at": last["at"] + i, "kind": "voice_call", "call": "m26",
                                      "source": "meeting", **line}) + "\n" for i, line in enumerate(lines, 1))
    with Terminal(f, "--resume", session) as t:
        t.shown("before-meeting-26")
        server.post(f, f"/sessions/{session}/input", {"text": "again"})
        # The play mark comes to the newest finished reply's cap once its text and turn end fold, never
        # to the streamed words.
        t.until(lambda t: any("▶" in cap and "after-meeting-26" in words for cap, words in zip(t.screen.display, t.screen.display[1:])))
        t.shown("after-meeting-26", "before-meeting-26")


ROOT, CHILD = "TERMINAL_RENDER_ROOT", "You are oak,"
replies = [agent(ROOT, "start", name="oak", brief="Read the brief and reply."),
           message(ROOT, "question", text="Which review should ship?", options=["careful", "quick"]),
           reply(ROOT, text="root-transcript-visible"), reply(CHILD, text="child-card-visible"),
           reply(ROOT, text="root-report-visible"), reply("ROOT_P26", "Bash", {"command": "sleep 600"}),
           reply("ROOT_Q26", text="q26-visible"), reply("SENT_Q26", text="sent-q26-visible"),
           *(reply(f"ROOT_{n}", text=f"{n.lower()}-visible\n\n{'x' * 36}{m}[link](https://example.com/w26)") for n, m in (("V26", "ﾞ"), ("W26", "a"))),
           reply("TITLE_ROOT", text="title-turn-visible\n\n1. LIST_26 with:\n   - NESTED_26\n2. after"),
           # One turn of 140 calls: a session longer than the terminal's first page (256 entries).
           claude.reply(*[(None, "mcp__razor__status", {})] * 140, match="LONG_ROOT"),
           reply("LONG_ROOT", text="long-turn-visible"), reply("LONG_AGAIN", text="again-visible"),
           reply("SENT_FROM_APP", text="sent-visible"),
           reply("PROSE_ROOT", text=" ".join(f"W{i:04}" for i in range(3000))),
           reply("PROSE_AGAIN", text=" ".join(f"X{i:04}" for i in range(600))),
           reply("PROSE_THIRD", text="third-visible"),
           # A failing command whose output spills, a small one that succeeds, a finished call,
           # then one still running.
           reply("FAIL_ROOT", "Bash", {"command": "seq 1 9000; echo LAST_REAL_LINE; exit 1"}),
           claude.reply((None, "Bash", {"command": "seq 3 | sed 's/^/SMALL_/'"}),
                        text="Checking a small one.", match="FAIL_ROOT"),
           tool("FAIL_ROOT", "status"), reply("FAIL_ROOT", "Bash", {"command": "sleep 600"})]
# One stdio server, which the /mcp panel lists and disables.
local = f"[mcp.local]\ncommand = {json.dumps(sys.executable)}\nargs = [{json.dumps(str(Path(mcp.__file__).resolve()))}]\n"
with flow(replies, config=local + '[server]\nname="box"\nrealm="work"\n') as f:
    f.base()
    server.start(f)
    with Terminal(f, "--agents") as t:
        t.shown("AGENTS ·", "Message the agent")
        assert not server.agents(f), "opening --agents created a session"
        # It asks for presses and drags, never bare pointer motion, which some terminals send every pixel of.
        assert b"\x1b[?1002h" in t.output and b"\x1b[?1003h" not in t.output, t.output[:400]
        # > widens the list by four columns, and the home keeps the width for the next start.
        edge = lambda t: t.screen.display[0].find("╮") + 1
        t.until(lambda t: edge(t) == 40)  # a third of the screen, once it has slid in
        wide = 44
        t.send("\x1b[D>")
        t.until(lambda t: edge(t) == wide)
        # Its right edge, held and dragged six columns, follows the mouse; letting go keeps it.
        t.send(f"\x1b[<0;{wide};6M\x1b[<32;{wide + 6};6M\x1b[<0;{wide + 6};6m")
        wide += 6
        t.until(lambda t: edge(t) == wide and (f.home / ".razor/list-width").read_text() == str(wide))
        # A click opens a root; prefix (backtick or Ctrl-B) n/p opens its project's neighbour
        # across state headings, j/k the next and previous project's first, 2 its second.
        (f.cwd / "h").mkdir()  # its own folder, so the roots below keep their names
        def root(name, words, folder="h"):
            made = server.post(f, "/sessions", {"cwd": str(f.cwd / folder)})["session"]
            server.send(f, "PUT", f"/sessions/{made}/name", name).read()
            server.post(f, f"/sessions/{made}/input", {"text": words})
            return made
        made = [root("p26", "ROOT_P26"), root("q26", "ROOT_Q26"), root("v26", "ROOT_V26", f.home), root("w26", "ROOT_W26", f.home)]
        # The prefix keys read the list's order, which can trail the cards' ○.
        settled = lambda t: -1 < (x := "\n".join(l[:wide] for l in t.screen.display)).find("p26") < x.find("IDLE") < min(x.find(f" {n}26 ") for n in "qvw")
        t.until(settled)
        conversation = lambda: "\n".join(line[wide:] for line in t.screen.display)
        t.click("○ q26")
        t.until(lambda _: "q26-visible" in conversation())  # opened before p reads which is open
        t.send("`p")
        t.until(lambda _: "sleep 600" in conversation() and "q26-visible" not in conversation())
        t.send("\x02n")
        t.until(lambda _: "q26-visible" in conversation() and "sleep 600" not in conversation())
        t.send("SENT_Q26\r")
        server.until(f, made[1], lambda es: any(e.get("text") == "sent-q26-visible" for e in es))
        t.until(settled)
        t.send("`j")
        t.until(lambda _: re.search("[vw]26-visible", conversation()) and "q26-visible" not in conversation())
        t.send("`k`2")
        t.until(lambda _: "q26-visible" in conversation())
        t.send("``x`")  # the prefix twice types it, and with words in the draft it types at once
        t.until(lambda _: "`x`" in conversation())
        t.send("\x15`\x1b[200~P26\x1b[201~T")  # a paste disarms it: T types, never runs rename
        t.until(lambda _: "P26T" in conversation())
        t.send("\x15")
        # v26 and w26, in a project of their own, are alike but for ﾞ (a column wider to ratatui than to
        # Razor) or a before a link: going from one to the other, both read, rewrites the link's
        # first cell, which is painted again.
        for name in ("w26", "v26", "w26"):
            sent = len(t.output)
            t.click(f"○ {name}")
            t.until(lambda _: f"{name}-visible" in conversation())
        assert (link := b"\x1b]8;;https://example.com/w26\x1b\\") in t.output[sent:], "a rewritten link cell lost OSC 8"
        # A frame that changes nothing leaves the link to the terminal; a resize paints it again.
        sent, frames = len(t.output), t.frames
        t.send("\x1b[I")  # a focus report draws a frame where nothing changed
        t.until(lambda t: t.frames > frames)
        assert link not in t.output[sent:], "an unchanged frame painted the link again"
        t.resize(40, 119)
        t.until(lambda t: link in t.output[sent:])
    server.post(f, f"/sessions/{made[0]}/stop", {})  # its sleep would outlive the flow and ask the model again
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{session}/input", {"text": ROOT})
    # oak ends in prose with nothing pending, so it finishes and its report wakes the root.
    server.until(f, session, lambda es: es[-1]["kind"] == "turn_end"
                 and any(e.get("text") == "root-report-visible" for e in es))
    # A stand-in vim in a folder with a space, which records how a reply reached it.
    vim = f.home / "bin 26/vim"
    vim.parent.mkdir()
    vim.write_text(f"#!{sys.executable}\nimport json, os, sys, termios\np = sys.argv[-1]\n"
                   "seen = {'args': sys.argv[1:], 'mode': os.stat(p).st_mode & 0o777, 'text': open(p).read(),"
                   " 'cooked': bool(termios.tcgetattr(0)[3] & termios.ICANON)}\n"
                   "json.dump(seen, open(os.environ['RAZOR_HOME'] + '/read-26.json', 'w'))\n")
    vim.chmod(0o755)
    with Terminal(replace(f, env={**f.env, "VISUAL": f"'{vim}' --flow"}), "--resume", session, status=True, rows=50) as t:
        t.shown("root-transcript-visible", "child-card-visible", "YOUR CALL",
                "Which review should ship?", "careful", "quick", "YOU", "Message the agent")
        # A terminal that answers the OSC 7501 query keeps a record of the session and of its
        # child, by the child's session id.
        oak = next(p.name for p in (f.home / ".razor/sessions").iterdir() if p.name not in {session, *made})
        t.until(lambda t: t.records().keys() == {"", oak})
        records = t.records()
        assert records[""] == {"state": "blocked", "kind": "question", "app": "razor",
                               "msg": "Which review should ship?"}, records
        assert records[oak]["state"] == "done" and records[oak]["title"] == "oak", records
        # These phrases occupy different UI surfaces, not just historical output bytes.
        lines = t.screen.display
        assert any("oak" in line and "Opus" in line for line in lines), t.text()
        assert any("worker" in line and "press to open" in line for line in lines), t.text()
        assert any("Opus" in line and "high" in line for line in lines[-12:]), t.text()
        # Outside the agents view the context reads on the tape at the left, not in the bar.
        assert "/1M" not in lines[2] and lines[4].startswith(" CONTEXT"), t.text()
        assert "? box-cwd on box · work" in lines[-2], t.text()
        # oak's report is one line, which opens its whole text and closes on Esc.
        t.shown("oak finished · child-card-visible")
        assert "EVENT" not in t.text(), t.text()
        t.click("oak finished")
        t.until(lambda t: "root-transcript-visible" not in t.text() and "oak finished" in t.text())
        t.send("\x1b")
        t.shown("root-transcript-visible", "oak finished · child-card-visible")
        t.send("/status\r")
        t.shown("┌─ status", "state", "context", "last turn", "Esc back")
        t.send("\x1b")
        t.shown("root-transcript-visible", "YOUR CALL", "Message the agent")
        # Prefix ? lists the keys; / keeps the rows holding every typed word; Esc ends the
        # search, then closes the list.
        t.send("`?/close split")
        t.until(lambda t: "prefix.close_split" in t.text() and "list.close" not in t.text())
        t.send("\x1b")
        t.shown("attached_images.focus")
        t.send("\x1b")
        t.until(lambda t: "┌─ keys" not in t.text())
        # With words in the draft, Ctrl-C empties it and Ctrl-Z brings them back. Ctrl-S sets the
        # draft aside and, with the box empty, brings it back; a second is refused, never lost.
        t.send("CLEAR_26\x03")
        t.until(lambda t: "CLEAR_26" not in t.text())
        assert "PRESS CTRL-C" not in t.text(), t.text()
        t.send("\x1a")
        t.shown("CLEAR_26")
        t.send("\x13")
        t.until(lambda t: "CLEAR_26" not in t.text())
        t.send("OTHER_26\x13")
        t.shown("ALREADY STASHED", "OTHER_26")
        t.send("\x03\x13")
        t.until(lambda t: "CLEAR_26" in t.text() and "OTHER_26" not in t.text())
        t.send("\x03")
        t.until(lambda t: "CLEAR_26" not in t.text())
        # Prefix Up selects the reply above; Enter reads it in the person's own editor, read-only,
        # in cooked mode, from a file only they can read and removed after; " quotes it.
        t.send("`\x1b[A")
        t.send("\r")
        read = f.home / ".razor/read-26.json"
        t.until(lambda _: read.exists())
        seen = json.loads(read.read_text())
        assert seen["args"][:2] == ["--flow", "-R"] and seen["mode"] == 0o600 and seen["cooked"], seen
        assert seen["text"] in ("root-transcript-visible", "root-report-visible"), seen
        t.until(lambda _: not Path(seen["args"][-1]).exists())
        t.send('"j')  # the quote leaves browsing, so j is the draft's
        t.until(lambda t: re.search(rf"❯ > {seen['text']} +│ *\n[^\n]*│ +j +│", t.text()))
        t.send("\x03")
        t.until(lambda t: "> root-" not in t.text())
        t.send("/mcp\r")
        t.shown("┌─ mcp", "● local", "connected · 2 tools")
        t.send("\r")
        t.shown("┌─ mcp · local", "d  disable")
        t.send("d")
        t.shown("Disabled local in")
        t.send("\x1b")
        t.shown("○ local", "disabled", "Enter open")
        # /startup pages the session's startup.md in the opened view; Esc backs out of it.
        t.send("\x1b")
        t.until(lambda t: "┌─ mcp" not in t.text())
        startup = f.home / ".razor/sessions" / session / "startup.md"
        with startup.open("a") as saved:
            saved.write("\n" + "startup paging row\n" * 240 + "STARTUP_END_26\n")
        t.send("/startup\r")
        t.shown("# What this conversation was sent ahead of its first input", "## The system prompt (")
        t.send("\x1b[1;5F")
        t.shown("STARTUP_END_26")
        t.send("\x1b[5~" * 100)
        t.shown("# What this conversation was sent ahead of its first input")
        t.send("\x1b[6~" * 100)
        t.shown("STARTUP_END_26")
        t.send("\x1b[5~")
        t.until(lambda t: "STARTUP_END_26" not in t.text())
        t.send("\x1b[1;5F")
        t.shown("STARTUP_END_26")
        t.send("\x1b")
        t.shown("root-transcript-visible", "Message the agent")
        t.send("/fast\r")
        t.shown("model · the model you pick", "Standard", "Fast", "Ultrafast")
        t.send("\x1b[C\x1b[C")
        t.shown("This speed is unavailable")
        assert any(c.data == "U" and c.fg == "7d8993" for row in t.screen.buffer.values() for c in row.values()), t.text()
        t.send("\x1b")
        t.until(lambda t: "model · the model you pick" not in t.text())
    assert not t.records(), "the app left OSC 7501 records behind"
    # The tab title alarms only on trouble: an idle session is plain, a live turn that ended well
    # says done until the next key, and so does the session's OSC 7501 record.
    with Terminal(f, status=True) as t:
        t.until(lambda t: t.screen.title == f.cwd.name)
        t.send("\x1b[D")
        t.shown("AGENTS ·")
        t.send("\x1bOQ")
        t.until(lambda t: "AGENTS ·" not in t.text())
        t.send("TITLE_ROOT\r")
        t.shown("title-turn-visible")
        # A list nested in a tight item starts its own row, indented under its item's words.
        at = t.find("LIST_26")[0]
        assert "  - NESTED_26" in t.screen.display[at + 1], t.text()
        t.until(lambda t: t.screen.title == f"✓ {f.cwd.name} · done")
        t.until(lambda t: t.records().get("", {}).get("state") == "done")
        t.send("\x1b[C")  # a key that leaves the draft empty, so the quit's Ctrl-C needs no clearing
        t.until(lambda t: t.screen.title == f.cwd.name)
        t.until(lambda t: t.records()[""]["state"] == "idle")
    with Terminal(replace(f, cwd=f.home), "--agents") as t:
        t.shown("AGENTS ·")
        t.until(lambda t: edge(t) == wide)  # the kept width, read at start
        t.until(lambda t: any("title-turn-visible" in line[t.screen.display[0].find("╮") + 1:] for line in t.screen.display))
    # Reopened from another folder, a long session still names its own folder.
    (f.cwd / "elsewhere").mkdir()
    long = server.post(f, "/sessions", {"cwd": str(f.cwd / "elsewhere")})["session"]
    server.post(f, f"/sessions/{long}/input", {"text": "LONG_ROOT"})
    server.until(f, long, lambda es: es[-1]["kind"] == "turn_end")
    assert len(list(f.record(long))) > 256
    with Terminal(f, "--resume", long) as t:
        t.shown("long-turn-visible")
        # Not every call of the reopened turn is mounted, so its closing line counts none.
        def closing(t):
            return next((line for line in t.screen.display if "╰─" in line and "done" in line), "")
        t.until(closing)
        assert "call" not in closing(t) and "▪" not in closing(t), closing(t)
        t.until(lambda t: t.screen.title == "elsewhere")
        # One wheel event moves the view one row; output arriving below leaves a held view still.
        # The rows compared reach the turn's last ones, so no step over its alternating call rows
        # passes for one row.
        n = t.find("▁▁▁")[0] - 5
        rows = lambda t, first: [line[17:118] for line in t.screen.display[first:first + n]]
        shown = rows(t, 4)
        t.send("\x1b[<64;60;20M")
        t.until(lambda t: rows(t, 5) == shown)
        held = rows(t, 4)
        server.post(f, f"/sessions/{long}/input", {"text": "LONG_AGAIN"})
        server.until(f, long, lambda es: es[-1]["kind"] == "turn_end"
                     and any(e.get("text") == "again-visible" for e in es))
        t.pump(1)
        assert rows(t, 4) == held and "again-visible" not in t.text(), t.text()
        # Words sent from the app bring the view down to them, even from a page up.
        t.send("\x1b[5~")
        t.send("SENT_FROM_APP\r")
        t.shown("SENT_FROM_APP", "sent-visible")
        # A held view keeps its words near the top across a width change; a composer shrinking
        # under it draws the end without moving or releasing it; scrolling down to the end
        # follows again.
        def top(t, n=1):
            words = (re.findall(r"W\d{4}", line) for line in t.screen.display[2:])
            return sum([w for w in words if w][:n], [])
        def turn(text, reply):
            server.post(f, f"/sessions/{long}/input", {"text": text})
            server.until(f, long, lambda es: es[-1]["kind"] == "turn_end"
                         and any(e.get("text", "").startswith(reply) for e in es))
        turn("PROSE_ROOT", "W0000")
        t.shown("W2999")
        # With an empty draft, Ctrl-U and Ctrl-D scroll half a page.
        t.send("\x15")
        t.until(lambda t: "W2999" not in t.text())
        t.send("\x04")
        t.shown("W2999")
        t.send("\x1b[5~\x1b[5~")
        t.pump(1)
        before = top(t)[0]
        t.resize(40, 100)
        t.pump(1)
        assert before in top(t, 2), (before, t.text())
        t.resize(40, 120)
        t.send("\x1b[1;5F\x1b[200~" + "\n".join(f"draft line {i}" for i in range(9)) + "\x1b[201~")
        t.shown("W2999", "draft line 8")
        t.send("\x1b[<64;60;20M")
        t.pump(1)
        held = top(t)
        t.send("\x01\x7f")
        t.until(lambda t: "draft line 8" not in t.text())
        turn("PROSE_AGAIN", "X0000")
        t.pump(1)
        assert top(t) == held, (held, t.text())
        # Wheel reports split right after their ESC, as a full PTY splits a trackpad's burst,
        # still scroll and never type into the composer.
        for piece in ["\x1b", "[<65;60;20M"] * 200:
            t.send(piece)
            time.sleep(0.01)
        t.shown("X0599", "Message the agent")
        t.pump(1)
        turn("PROSE_THIRD", "third-visible")
        t.shown("third-visible")
    # A failed command's card says how its job ended and its last real line, not the spill's
    # wording, and a live turn's finished tick keeps the rail's green.
    failing = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{failing}/input", {"text": "FAIL_ROOT"})
    server.until(f, failing, lambda es: sum(e["kind"] == "call" for e in es) == 4)
    # A row says a terminal app has its session open: a phone's stream does not count.
    phone = server.send(f, "GET", f"/sessions/{failing}/events")
    assert phone.status == 200
    opened(f, failing, False)
    with Terminal(f, "--resume", failing) as t:
        opened(f, failing, True)
        # The small command's whole output is in its result, so its row counts its lines.
        t.shown("✗ exit 1", "› LAST_REAL_LINE", "3 lines", "Esc stops")
        assert "</persisted-output>" not in t.text() and "whole output is in" not in t.text(), t.text()
        # A reply between calls ends in a blank row before the line resumes, as Loom draws it.
        y = t.find("Checking a small one.")[0]
        assert not t.screen.display[y + 1][17:].strip(), t.text()
        def tick(t):
            y, line = next((y, line) for y, line in enumerate(t.screen.display) if "┼   status" in line)
            return t.screen.buffer[y][line.index("┼")].fg
        # Once its flash is over, the finished tick rests in the live rail's colour.
        t.until(lambda t: tick(t) == "22773c", timeout=5)
        t.send("\x1b")
        # The stopped job's end notice opens no turn of its own.
        server.until(f, failing, lambda es: any(e.get("end") == "stopped" for e in es)
                     and any(e.get("event") == "ended" for e in es))
    assert t.output.count(b"\x1b]7501;") == 1, "a terminal that never answered got OSC 7501 reports"
    opened(f, failing, False)
    phone.close()



def windows(first, second, reset):
    return tuple((f"x-codex-{key}-{field}", str(value))
                 for key, percent, minutes, at in (("primary", first, 300, reset),
                                                   ("secondary", second, 10080, reset + 86400))
                 for field, value in (("used-percent", percent), ("window-minutes", minutes),
                                      ("reset-at", at)))


def bars(t, readings):
    for name, percent, color in readings:
        y, line = next((y, line) for y, line in enumerate(t.screen.display) if name in line)
        assert f"{percent}%" in line, t.text()
        assert t.screen.buffer[y][line.index("━")].fg == color, (name, t.text())


# Two conversations' response headers stay with their own rows, even after changing focus.
reset = int(time.time()) + 3600
replies = [replace(done(), match="USAGE_PEER", headers=windows(91, 100, reset)),
           replace(done(), match="USAGE_ROOT", headers=windows(14, 81, reset))]
with flow(replies, model="gpt-6.1-sol", config='[server]\nname="box"\nrealm="work"\n') as f:
    f.base()
    (f.cwd / ".agents/skills/plan").mkdir(parents=True)
    (f.cwd / ".agents/skills/plan/SKILL.md").write_text("---\ndescription: Plans.\n---\n")
    (f.cwd / "peer").mkdir()
    f.git("-C", "peer", "init", "-b", "peer-branch")
    f.git("-C", "peer", "-c", "user.name=Flow", "-c", "user.email=flow@example.invalid",
          "commit", "--allow-empty", "-m", "Peer")
    server.start(f)
    peer = server.post(f, "/sessions", {"cwd": str(f.cwd / "peer")})["session"]
    server.post(f, f"/sessions/{peer}/input", {"text": "USAGE_PEER"})
    server.until(f, peer, lambda es: es[-1]["kind"] == "turn_end")
    with Terminal(f, columns=160, rows=60) as t:
        t.shown("0 of 272k", "0%", "7-day", "not measured yet")
        assert "5-hour" not in t.text(), t.text()
        t.send("USAGE_ROOT\r")
        t.shown("flow done", "cwd · main", "idle", "14%", "81%", "60% reused")
        bars(t, [("5-hour", 14, "3ee56d"), ("7-day", 81, "ffb319")])
        # The agents list's servers line names, by its word in the servers table, the server
        # whose window is fullest: the peer's, on this one.
        t.send("\x1bOQ")
        t.shown("servers · all 1 answer · box 7-day 100%")
        t.send("\x1bOQ")
        t.until(lambda t: "‹ F2" not in t.text() and "60% reused" in t.text())
        for at in (reset, reset + 86400):
            reset_at = datetime.fromtimestamp(at)
            form = "%-I:%M%p" if reset_at.date() == datetime.now().date() else "%a %-I:%M%p"
            expected = reset_at.strftime(form)
            expected = expected[:-2] + expected[-2:].lower()
            assert expected in t.text(), t.text()
        assert "Account limit:" not in t.text() and "ALERTS" not in t.text(), t.text()
        # The draft lights a known skill's name; the mouse selects in it as the cockpit's did.
        t.send("use /plan now")
        t.shown("use /plan now")
        y, x = t.find("use /plan now")
        cell = lambda dx: t.screen.buffer[y][x + dx]
        t.until(lambda t: all(cell(dx).fg == "ecd48c" and cell(dx).bold for dx in range(4, 9)))
        assert cell(3).fg != "ecd48c" and cell(9).fg != "ecd48c", t.text()
        # A drag reverses what it passes over and copies it when the button comes up.
        t.send(f"\x1b[<0;{x + 1};{y + 1}M\x1b[<32;{x + 4};{y + 1}M\x1b[<0;{x + 4};{y + 1}m")
        t.until(lambda t: all(cell(dx).reverse for dx in range(3)) and not cell(3).reverse)
        t.shown("COPIED 3 CHARACTERS")
        t.send("x")
        t.shown("x /plan now")
        # A double click takes the word under it.
        click = f"\x1b[<0;{x + 9};{y + 1}M\x1b[<0;{x + 9};{y + 1}m"
        t.send(click + click + "later")
        t.shown("x /plan later")
        t.send("\x7f" * len("x /plan later"))
        t.until(lambda t: "x /plan" not in t.text())
        t.send(f"/attach {peer}\r")
        t.shown("peer · peer-branch")
        t.send("/status\r")
        t.shown("91%", "100%")
        bars(t, [("5-hour", 91, "ffb319"), ("7-day", 100, "ff3d32")])
        f.git("-C", "peer", "switch", "-c", "after-turn")
        soon = int(time.time()) + 8
        replies.append(replace(done(), match="USAGE_AGAIN", headers=windows(20, 30, soon)))
        server.post(f, f"/sessions/{peer}/input", {"text": "USAGE_AGAIN"})
        t.shown("peer · after-turn", "20%", "30%")
        t.shown("awaiting a reading", "reset time passed", timeout=15)
        t.send("\x1b")
        t.until(lambda t: "┌─ status" not in t.text())
        t.send("/unattach\r")
        t.shown("cwd · main", "14%", "81%", "idle")
        bars(t, [("5-hour", 14, "3ee56d"), ("7-day", 81, "ffb319")])
        # `/done` closes the addressed session, not the root, and the app goes back to the root; the
        # closed session leaves the list at once, so its "closed" card may never draw.
        replies.append(replace(done(), match="The person asked for `/done`"))
        t.send(f"/attach {peer}\r")
        t.shown("peer · after-turn")
        assert "/unattach" not in t.text(), "a command was kept as a draft"
        t.send("/done\r")
        t.shown("cwd · main")
        closing = lambda es: [e for e in es if e["kind"] == "input" and e["from"] == "razor"]
        server.until(f, peer, lambda es: closing(es) and es[-1]["kind"] == "turn_end")
        root = next(p.name for p in (f.home / ".razor/sessions").iterdir() if p.name != peer)
        assert not closing(f.record(root)), "the root was closed"
        assert os.waitpid(t.pid, os.WNOHANG) == (0, 0), "the app ended"

# A removed peer's background preview never keeps renewing its failure toast.
with flow([done()], model="gpt-6.1-sol") as f:
    root = f.ask("USAGE_GHOST")["session"]
    ghost = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    folder = f.home / ".razor/sessions" / ghost
    assert (folder / "record.jsonl").is_file(), folder
    shutil.rmtree(folder)
    with Terminal(f, "--resume", root, columns=160, rows=60) as t:
        seen = [False, 0]

        def failures(t):
            failed = any(words in t.text() for words in ("API 404:", "The API answer was interrupted"))
            seen[1] += failed and not seen[0]
            seen[0] = failed
            return False

        t.pump(16, failures)
        assert "flow done" in t.text(), t.text()
        print("Removed peer failure appearances over 16 seconds:", seen[1], flush=True)
        assert seen[1] <= 1, t.text()

# A working child that rebuilt says when on its card.
FIR = "You are fir,"
replies = [agent("FIR_ROOT", "start", name="fir", brief="Hand over, then wait."),
           reply("FIR_ROOT", text="fir-root-visible"),
           claude.reply(("toolu_fir_note", "mcp__razor__handover", {"note": "Then wait."}), match=FIR),
           reply(FIR, "Bash", {"command": "sleep 600"})]
with flow(replies) as f:
    f.base()
    server.start(f)
    root = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{root}/input", {"text": "FIR_ROOT"})
    server.until(f, root, lambda es: es[-1]["kind"] == "turn_end")
    fir = next(p.name for p in (f.home / ".razor/sessions").iterdir() if p.name != root)
    server.until(f, fir, lambda es: any(e["kind"] == "job" for e in es))
    with Terminal(f, "--resume", root) as t:
        t.shown("fir-root-visible", "context", "rebuilt")
        assert re.search(r"rebuilt (\w{3} )?\d{1,2}:\d{2}[ap]m", t.text()), t.text()
        server.post(f, f"/sessions/{fir}/stop", {})
        # Its job ends with the stop, and its end notice opens no turn.
        server.until(f, fir, lambda es: any(e.get("event") == "ended" for e in es))

# The agents list: settled and stopped children leave; finished children with branch work past
# the newest three fold into a line under their own parent, each child takes one line, ← and Space fold
# a parent, the bar follows the addressed agent, Ctrl-T opens the pane beside it, Esc stops the
# addressed agent at any width, which stays addressed, and another session's ask takes its digit.
kids, grand = [f"kid{n}" for n in (1, 3, 4, 5, 6)], [f"g{n}" for n in range(4)]
start = lambda match, names: claude.reply(*[(None, "mcp__razor__agent", {"action": "start", "name": n, "brief": "Go.", "placement": "worktree", "effort": "high"}) for n in names], match=match)
poll = 'until [ $(grep -l \'"turn_end"\' "$RAZOR_HOME"/sessions/*/record.jsonl | wc -l) -ge 10 ]; do sleep .1; done'
replies = [start("KIDS", ["kid2"]), reply("KIDS", "Bash", {"command": "sleep 1.1"}), claude.reply((None, "mcp__razor__agent", {"action": "headline", "text": "kids-headline"}), match="KIDS"),
           start("KIDS", kids), reply("KIDS", "Bash", {"command": poll}), agent("KIDS", "stop", child="kid1"), agent("KIDS", "settle", child="kid1", decision="discard"),
           reply("KIDS", text="kids-visible"), reply("KIDS"), reply("OTHER"), reply("NAP", "Bash", {"command": "sleep 60"}), reply("NAP"),
           start("You are kid1,", ["leaf"]), reply("You are kid1,", "Bash", {"command": "until grep -q 'sleep 6[1]' \"$RAZOR_HOME\"/sessions/*/record.jsonl; do sleep .1; done"}),
           reply("You are leaf,", "Bash", {"command": "sleep 61"}), start("You are kid2,", grand), agent("You are g1,", "headline", text="pinecone audit"),
           reply("You are kid2,", "Bash", {"command": "until grep -q 'sleep 6[0]' \"$RAZOR_HOME\"/sessions/*/record.jsonl; do sleep .1; done"}), message("ASK26", "question", text="Choose 26", options=["one", "two"]), reply("ASK26"), reply("ASK26", text="answered26"),
           *[r for k in ["kid2", *kids, *grand] for r in (
               reply(f"You are {k},", "Bash", {"command": "echo work > work; git add work && git -c user.name=Flow -c user.email=flow@example.invalid commit -m Work"}), agent(f"You are {k},", "finish", status="finished", report="ok"))]]
with flow(replies, config='[ui.keys.goto]\nsearch = ["s"]\n[ui.keys.panel]\nsearch = []\n[ui.keys.prefix]\n"attached_images.focus" = ["i"]\n') as f:
    f.base()
    root = f.ask("KIDS", timeout=120)["session"]
    server.until(f, root, lambda es: sum(e["kind"] == "turn_end" for e in es) == 2)  # and the reports'
    server.post(f, f"/sessions/{root}/input", {"text": "NAP"})
    (f.cwd / "other").mkdir()
    other = replace(f, cwd=f.cwd / "other").ask("OTHER")["session"]
    with Terminal(f, "--resume", other, columns=160) as t:
        # A panel's last line leaves out the hint of a key the settings unbound.
        t.shown("Message the agent")
        t.send("/keys\r")
        t.shown("┌─ keys", "Esc back")
        foot = next(line for line in t.screen.display if "Esc back" in line)
        assert "scroll" in foot and "unbound" not in foot and "search" not in foot, foot
        t.send("\x1b")
        t.until(lambda t: "┌─ keys" not in t.text())
        t.send("\x1bOQ")
        kid = lambda t: [line[:52] for line in t.screen.display if re.search(r" kid\d ", line[:52])]
        kids = lambda t: len(kid(t)) == 3 and all("waiting for merge" in line for line in kid(t)) and "kid1" not in t.text()
        # Slid fully open; kid1's stopped child leaves the list too. Earlier remains folded
        # without a total while its historical rows have not been requested.
        t.until(lambda t: "✓ 2 done" in t.text() and t.screen.display[0][52] == "╮" and kids(t) and "EARLIER" in t.text() and "1 AGENT" in t.text())
        earlier = next(line for line in t.screen.display if "EARLIER" in line)
        assert "AGENT" not in earlier and "LOADED" not in earlier, earlier
        assert t.screen.display[0].startswith("╭─ RAZOR") and " move " in t.screen.display[-1][:52], t.text()
        # Go to a helper's helper, kid2's g1, with kid2 folded into the done line opens the line
        # and selects its row.
        assert not any(" kid2 " in line for line in kid(t)), t.text()
        t.send("`gsg1")
        t.shown("1 of")
        t.send("\r")
        t.until(lambda t: len(kid(t)) == 5 and any("❯" in row and " g1 " in row[:52] for row in t.screen.display))
        t.click("✓ 2 done")
        t.until(lambda t: len(kid(t)) == 3)
        t.click("✓ 2 done")
        t.until(lambda t: len(kid(t)) == 5)
        y = next(y for y, line in enumerate(t.screen.display) if " cwd " in line[:52])
        t.send(f"\x1b[<0;6;{y + 1}M\x1b[<0;6;{y + 1}m")
        t.shown("Enter queues it · Esc stops it", "kids-headline", "cwd · main")
        assert t.screen.buffer[y][2].data == "▌" and t.screen.buffer[y][30].bg == "3fd9f2", "the addressed row is unmarked"
        t.send("`h ")  # the prefix and h give the list the keys; Left now opens the overview
        t.until(lambda t: not kid(t))
        t.send(" ")  # Space unfolds it again
        t.until(lambda t: len(kid(t)) == 5)
        t.send("\r")  # Enter opens the selected agent, the addressed one here, closing the list
        t.until(lambda t: not t.screen.display[0].startswith("╭─ RAZOR"))
        t.send("\x1bOQ")
        t.until(lambda t: len(kid(t)) == 5 and t.screen.display[0][52] == "╮")
        t.send("\x14")
        t.shown(" STATUS ")
        t.send(f"\x1b[<0;6;{y + 1}M\x1b[<0;6;{y + 1}m\x14")  # the click hands the keys back
        t.resize(40, 60)
        t.send("\x1b")
        server.until(f, root, lambda es: es[-1].get("end") == "done" and any(e.get("end") == "stopped" for e in es))  # kid2's report wakes it
        t.resize(40, 160)
        t.until(lambda t: any(line[2] == "▌" and " cwd " in line[:52] for line in t.screen.display))
        server.post(f, f"/sessions/{other}/input", {"text": "ASK26"})
        t.shown("Choose 26")
        t.pump(0.7)  # past the 600 ms a new question refuses digits for
        t.send("2")
        server.until(f, other, lambda es: any(e.get("text") == "answered26" for e in es))

        # Go to finds a grandchild by its headline and opens the real session stream.
        t.send("/goto\r/pinecone")
        t.pump(.2)
        assert "/ pinecone" not in t.text(), t.text()  # the old, unbound slash cannot search
        t.send("spinecone")
        t.until(lambda t: "/ pinecone" in t.text() and "1 of" in t.text() and any("└" in line and " g1 " in line and "pinecone audit" in line for line in t.screen.display))
        t.send("\r")
        g1 = next(id for id, a in server.agents(f).items() if a["name"] == "g1")
        opened(f, g1, True)
        t.until(lambda t: "┌─ Go to" not in t.text() and "pinecone audit" in "".join(t.screen.display[:4]))
        t.send("`i")  # an action named under the prefix keeps its own context's conditions
        t.shown("PREFIX ACTION IS UNBOUND OR UNAVAILABLE")
        t.send("\x1b[D")  # From a helper, Left addresses its parent before changing views.
        opened(f, g1, False)
        opened(f, server.agents(f)[g1]["parent"], True)
    server.stop(f)
    server.start(f)
    row = next(c for c in json.loads(server.send(f, "GET", f"/sessions/{root}/agent").read())["children"] if c["name"] == "kid1")
    header = next(f.record(row["session"]))
    assert row["settled"] == "discard" and (row["model"], row["effort"]) == (header["model"], header["effort"]) and row["effort"] == "high", row
