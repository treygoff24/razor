"""Tool shapes on both model paths, each part in a folder of its own in one home: shell results; a
person's correction before the next action; a prompt-driven program under a real terminal;
reads that run at once and edits in call order; Razor's own file actions."""
import json
import os
from pathlib import Path
import re
import secrets
import tempfile
import threading
import tomllib
from dataclasses import replace
from claude import reply
from driver import REPLIES, Reply, flow
from responses import PLAIN, call, done, message, response
import server

GPT = "gpt-6.1-sol"
replies = [Reply((REPLIES / f"{name}.sse").read_bytes()) for name in ("shell-short", "shell-large")]
# Over the old 2,000-character preview and under SPILL: inline, as Claude Code's own Bash does.
# It ends with the browser name the shell gives agent-browser: the session's id, so no two agents
# share one browser; then the socket of the home's Cua Driver daemon.
medium = "head -c 2560 /dev/zero | tr '\\0' m; printf %s \"$AGENT_BROWSER_SESSION $CUA_DRIVER_SOCKET\""
replies += [reply(("toolu_medium", "Bash", {"command": medium})), Reply((REPLIES / "done.sse").read_bytes())]
with flow(replies, config=PLAIN) as f:
    def part(name):
        # The flow in its own folder, so each part's files are its own.
        (f.cwd / name).mkdir()
        return replace(f, cwd=f.cwd / name)

    def model(name):
        # The arguments that start a session on `name`; Claude is the home's default.
        return [] if name == "opus" else ["--model", name]

    answer = part("shell").ask("Run the two commands, then finish.")
    assert answer["message"] == "flow done", answer
    entries = list(f.record(answer["session"]))
    calls = [e for e in entries if e["kind"] == "call"]
    outputs = [e for e in entries if e["kind"] == "result"]
    assert [e["call"] for e in calls] == ["toolu_short", "toolu_large", "toolu_medium"], calls
    assert [e["call"] for e in outputs] == [e["call"] for e in calls], outputs
    # The SDK mirrors the trained status-first shape, trimming the trailing newline.
    short, large, medium = outputs
    socket = f.home / ".razor/cua-driver.sock"
    assert medium["output"] == f"Exit code 0\n{'m' * 2560}{answer['session']} {socket}", medium
    assert short["output"] == "Exit code 7\nshell-output" and short["outcome"] == "error", short
    # A spill is Claude Code's <persisted-output>: the job's output file and the end of it.
    assert large["outcome"] == "success" and "<persisted-output>" in large["output"], large
    paths = re.findall(r"/\S+/job-2\b", large["output"])
    assert paths and Path(paths[0]).read_bytes() == b"x" * 40000, large
    assert 0 < len(large["output"]) < 5000 and "x" * 100 in large["output"], large
    ends = [e["end"] for e in entries if e["kind"] == "job" and e["event"] == "ended"]
    assert ends == [{"exit": 7}, {"exit": 0}, {"exit": 0}], ends

    # A person's correction joins before the next action on both model paths.
    server.start(f)
    steer = tomllib.loads((REPLIES.parents[1] / "src/record/text/input.toml").read_text())["steer"]
    for name in ("opus", GPT):
        correction, opening = "copper-heron-06: write steered before the next action.", "Run the sleeping command."
        # Hold the sleeping call until the input is admitted, not for a timing-dependent interval.
        sleeping = "printf ready > ready; while [ ! -e continue ]; do sleep 0.05; done; printf slept"
        commands = [("call_sleep", sleeping), ("call_batch", "printf batch > batch")]
        following = "printf steered > steered"
        if name == "opus":
            replies += [reply(*[(id, "Bash", {"command": command}) for id, command in commands]),
                        reply(("call_next", "Bash", {"command": following}),
                              text="I will write steered before the next action.", match=correction),
                        reply(text="flow done")]
        else:
            replies += [response([call("bash", json.dumps({"command": command}), id) for id, command in commands]),
                        Reply(response([message("I will write steered before the next action."),
                                        call("bash", json.dumps({"command": following}), "call_next")]).body,
                              match=correction), done()]
        here, base = part(f"steering-{name}"), len(f.sent())
        session = server.post(f, "/sessions", {"cwd": str(here.cwd), "model": name})["session"]
        path = f"/sessions/{session}"
        first = server.post(f, path + "/input", {"text": opening})["entry"]
        server.until(f, session, lambda es: (here.cwd / "ready").exists()
                     and len([e for e in es if e["kind"] == "call"]) == 2)
        steering = server.post(f, path + "/input", {"text": correction})["entry"]
        assert not any(e["kind"] == "result" for e in f.record(session)), "sleep ended before steering"
        stream = server.send(f, "GET", f"{path}/events?after={steering}")
        assert stream.status == 200 and json.loads(stream.readline())["kind"] == "attached"
        (here.cwd / "continue").touch()
        server.until(f, session, lambda es: any(
            e.get("call") == "call_next" or e.get("text") == "flow done" for e in es))
        request = json.loads(sent := f.sent()[base + 1])
        # The steering's text is its words and Razor's line after them, the opening's its words alone.
        assert json.dumps(correction + steer) in sent and json.dumps(opening)[1:] in sent and sent.count(steer.strip()) == 1, sent
        if name == "opus":
            blocks = [b for m in request["messages"] if isinstance(m["content"], list) for b in m["content"]]
            results = [b["tool_use_id"] for b in blocks if b["type"] == "tool_result"]
            corrected = [i for i, b in enumerate(blocks) if correction in b.get("text", "")]
            outputs = [i for i, b in enumerate(blocks) if b["type"] == "tool_result"]
        else:
            blocks = request["input"]
            results = [b["call_id"] for b in blocks if b["type"] == "function_call_output"]
            corrected = [i for i, b in enumerate(blocks) if b["type"] == "message"
                         and b["role"] == "user" and correction in json.dumps(b["content"])]
            outputs = [i for i, b in enumerate(blocks) if b["type"] == "function_call_output"]
        assert results == [id for id, _ in commands] and len(corrected) == 1, blocks
        assert max(outputs) < corrected[0], "steering split calls from their outputs"
        live = []
        while not live or live[-1].get("call") != "call_next":
            live.append(json.loads(stream.readline()))
        taken = [i for i, e in enumerate(live) if e["kind"] == "taken" and e["input"] == steering]
        result = next(i for i, e in enumerate(live) if e["kind"] == "result" and e["call"] == "call_sleep")
        assert len(taken) == 1 and taken[0] < result, live
        stream.close()
        entries = server.until(f, session, lambda es: any(
            e["kind"] == "turn_end" and steering in e["inputs"] for e in es))
        ends = [e for e in entries if e["kind"] == "turn_end"]
        calls = [e for e in entries if e["kind"] == "call"]
        result = next(e for e in entries if e["kind"] == "result" and e["call"] == "call_sleep")
        assert len(ends) == 1 and ends[0]["end"] == "done" and ends[0]["inputs"] == [first, steering], ends
        # The input entries keep the person's words as sent; only the model's copy has the line.
        said = {e["id"]: e["text"] for e in entries if e["kind"] == "input"}
        assert said[first].endswith(opening) and said[steering].endswith(correction), said
        assert [e["call"] for e in calls] == ["call_sleep", "call_batch", "call_next"], calls
        assert first < steering < result["id"] < calls[-1]["id"], entries
        assert (here.cwd / "batch").read_text() == "batch" and (here.cwd / "steered").read_text() == "steered"
        requests = f.sent()[base:]
        assert len(requests) == 3 and [r.count(correction) for r in requests] == [0, 1, 1], requests

    # A prompt-driven program under a real terminal, through Claude's terminal and plain GPT's
    # bash/wait/send: text and Enter reach it, screens reach the model, and secret_file records only
    # a path. There is no peek operation or secret flag; secrets never enter a tool argument.
    program = r'''import pathlib
import sys
import termios

sys.stdout.write("\x1b[2J\x1b[HOVERWRITTEN\r\x1b[2KName: ")
sys.stdout.flush()
assert sys.stdin.readline() == "Razor\n"
settings = termios.tcgetattr(sys.stdin)
hidden = settings.copy()
hidden[3] &= ~termios.ECHO
termios.tcsetattr(sys.stdin, termios.TCSANOW, hidden)
print("Secret:", end="", flush=True)
assert sys.stdin.readline().encode() == pathlib.Path("secret.txt").read_bytes()
termios.tcsetattr(sys.stdin, termios.TCSANOW, settings)
print("\nTERMINAL_DONE", flush=True)
'''
    for name in ("opus", GPT):
        here, base = part(f"terminal-{name}"), len(f.sent())
        (here.cwd / "interactive.py").write_text(program)
        secret = secrets.token_hex(24).encode()
        secret_file = here.cwd / "secret.txt"
        secret_file.write_bytes(secret + b"\n")
        if name == "opus":
            calls = [
                ("start", "mcp__razor__terminal", {"command": "python3 interactive.py", "pattern": "Name:"}),
                ("text", "mcp__razor__terminal", {"job": "job-1", "text": "Razor", "timeout_ms": 0}),
                ("key", "mcp__razor__terminal", {"job": "job-1", "key": "Enter", "pattern": "Secret:"}),
                ("secret", "mcp__razor__terminal", {"job": "job-1", "secret_file": str(secret_file)}),
            ]
            replies.extend(reply((id, tool, args)) for id, tool, args in calls)
            replies.append(reply(text="flow done"))
            prompts = {"start": "Name:", "key": "Secret:", "secret": "TERMINAL_DONE"}
        else:
            calls = [
                ("start", "bash", {"command": "python3 interactive.py", "pty": True, "wait_ms": 0}),
                ("prompt", "wait", {"job": 1, "pattern": "Name:", "timeout_ms": 10000}),
                ("text", "send", {"job": 1, "text": "Razor"}),
                ("key", "send", {"job": 1, "key": "Enter"}),
                ("hidden", "wait", {"job": 1, "pattern": "Secret:", "timeout_ms": 10000}),
                ("secret", "send", {"job": 1, "secret_file": str(secret_file), "timeout_ms": 10000}),
            ]
            replies.extend(response([call(tool, json.dumps(args), id)]) for id, tool, args in calls)
            replies.append(done())
            prompts = {"prompt": "Name:", "hidden": "Secret:", "secret": "TERMINAL_DONE"}
        answer = here.ask(*model(name), "Drive the interactive program to its done marker.")
        assert answer["message"] == "flow done", answer
        entries = list(f.record(answer["session"]))
        outputs = [e for e in entries if e["kind"] == "result"]
        assert [e["call"] for e in outputs] == [id for id, _, _ in calls], outputs
        assert all(e["outcome"] == "success" for e in outputs), outputs
        for index, output in enumerate(outputs, 2):
            if output["call"] not in prompts:
                continue
            screen = output["output"] if name == "opus" else json.loads(output["output"])["output"]
            # The stale text was erased by terminal control codes, not merely appended as bytes.
            assert prompts[output["call"]] in screen and "OVERWRITTEN" not in screen and "\x1b" not in screen, screen
            assert "cursor" in output["output"] if name == "opus" else len(json.loads(output["output"])["cursor"]) == 2, output
            request = f.request(base + index)
            if name == "opus":
                block = next(b for m in request["messages"] for b in m["content"]
                             if isinstance(b, dict) and b.get("type") == "tool_result"
                             and b["tool_use_id"] == output["call"])
                shown = block["content"]
                if isinstance(shown, list):
                    shown = "\n".join(b["text"] for b in shown)
            else:
                block = next(i for i in request["input"] if i.get("type") == "function_call_output"
                             and i["call_id"] == output["call"])
                shown = json.loads(block["output"])["output"]
            assert shown == screen, (shown, screen)
        jobs = [e for e in entries if e["kind"] == "job"]
        assert [(e["event"], e.get("end")) for e in jobs] == [("started", None), ("ended", {"exit": 0})], jobs
        directory = f.home / ".razor/sessions" / answer["session"]
        files = [p for p in directory.rglob("*") if p.is_file()]
        assert files and (directory / "record.jsonl") in files, files
        paths = []
        for file in files:
            body = file.read_bytes()
            assert secret not in body, f"secret leaked into {file.relative_to(directory)}"
            if str(secret_file).encode() in body:
                paths.append(file)
        assert directory / "record.jsonl" in paths, "the secret_file path never reached the record"

    # Three reads in one model message run at once; two edits to one file apply in call order.
    # Each read is of a named pipe that the flow writes only after every pipe has a reader, last pipe
    # first, so reads run one at a time deadlock and never return. Claude's SDK returns results as
    # they finish (docs/design/claude.md); Razor's own loop returns them in call order (SPEC 3).
    PIPES = "abc"

    def serve_pipes(folder):
        served = []

        def write():
            for pipe_name in reversed(PIPES):
                with open(folder / f"{pipe_name}.pipe", "w") as pipe:
                    pipe.write(f"pipe-{pipe_name}\n")
                served.append(pipe_name)
        for pipe_name in PIPES:
            os.mkfifo(folder / f"{pipe_name}.pipe")
        threading.Thread(target=write, daemon=True).start()
        return served

    def check(folder, served, result, ids, final):
        assert result.returncode == 0, result.stderr
        answer = json.loads(result.stdout)
        assert answer["message"] == "flow done", answer
        entries = list(f.record(answer["session"]))
        calls = [e for e in entries if e["kind"] == "call"]
        outputs = [e for e in entries if e["kind"] == "result"]
        assert [e["call"] for e in calls] == ids, calls
        assert sorted(e["call"] for e in outputs) == sorted(ids), outputs
        assert served == list(reversed(PIPES)), served
        for pipe_name, id in zip(PIPES, ids):
            output = next(e for e in outputs if e["call"] == id)
            assert output["outcome"] == "success" and f"pipe-{pipe_name}" in output["output"], output
        edits = [e for e in outputs if e["call"] in ids[3:]]
        assert [e["call"] for e in edits] == ids[3:], edits
        assert all(e["outcome"] == "success" for e in edits), edits
        assert (folder / "edits.txt").read_text() == final
        return [e["call"] for e in outputs]

    # Claude: Claude Code's own Read and Edit; the first edit makes the file the second changes.
    with tempfile.TemporaryDirectory() as scratch:
        folder = Path(scratch)
        ids = [f"toolu_read_{pipe_name}" for pipe_name in PIPES] + ["toolu_edit_1", "toolu_edit_2"]
        edits = [{"file_path": str(folder / "edits.txt"), "old_string": "", "new_string": "alpha beta\n"},
                 {"file_path": str(folder / "edits.txt"), "old_string": "beta", "new_string": "gamma"}]
        batch = [(id, "Read", {"file_path": str(folder / f"{pipe_name}.pipe")}) for id, pipe_name in zip(ids, PIPES)]
        batch += [(id, "Edit", edit) for id, edit in zip(ids[3:], edits)]
        replies += [reply(*batch), Reply((REPLIES / "done.sse").read_bytes())]
        base = len(f.sent())
        served = serve_pipes(folder)
        result = f.run("--print=json", "Read the three pipes, then make both edits.", timeout=30)
        order = check(folder, served, result, ids, "alpha gamma\n")
        request = f.request(base + 2)
        seen = [block["tool_use_id"] for block in request["messages"][-1]["content"]
                if block["type"] == "tool_result"]
        assert order == seen, (order, seen)

    # Razor's own loop: its read and edit actions, and every result in call order.
    with tempfile.TemporaryDirectory() as scratch:
        folder = Path(scratch)
        (folder / "edits.txt").write_text("alpha beta\n")
        ids = [f"call_read_{pipe_name}" for pipe_name in PIPES] + ["call_edit_1", "call_edit_2"]
        edits = [{"path": str(folder / "edits.txt"), "old_string": "beta", "new_string": "gamma"},
                 {"path": str(folder / "edits.txt"), "old_string": "gamma", "new_string": "delta"}]
        items = [call("read", json.dumps({"path": str(folder / f"{pipe_name}.pipe")}), id)
                 for id, pipe_name in zip(ids, PIPES)]
        items += [call("edit", json.dumps(edit), id) for id, edit in zip(ids[3:], edits)]
        replies += [response(items), done()]
        base = len(f.sent())
        served = serve_pipes(folder)
        result = f.run("--print=json", *model(GPT), "Read the three pipes, then make both edits.", timeout=30)
        order = check(folder, served, result, ids, "alpha delta\n")
        assert order == ids, order
        second = f.request(base + 2)
        seen = [i["call_id"] for i in second["input"] if i["type"] == "function_call_output"]
        assert seen == ids, seen

    # Razor's file actions: read pages by line; edit needs exactly one match and otherwise changes
    # nothing; write makes the missing parent folders. On the Claude route Read, Edit and Write are
    # Claude Code's own (docs/design/tools.md), so these actions serve Razor's own loop.
    notes = "".join(f"line-{n}\n" for n in range(1, 6))
    twice = "same start\nsame end\n"
    steps = [("read", {"path": "notes.txt", "limit": 2}),
             ("read", {"path": "notes.txt", "offset": 3, "limit": 2}),
             ("edit", {"path": "twice.txt", "old_string": "same", "new_string": "other"}),
             ("edit", {"path": "notes.txt", "old_string": "line-5", "new_string": "line-five"}),
             ("write", {"path": "made/deeper/new.txt", "content": "made\n"})]
    items = [call(action, json.dumps(arguments), f"call_{n}") for n, (action, arguments) in enumerate(steps)]
    replies += [response(items), done()]
    here = part("files")
    (here.cwd / "notes.txt").write_text(notes)
    (here.cwd / "twice.txt").write_text(twice)
    answer = here.ask(*model(GPT), "Page the notes, try both edits, write the new file.")
    assert answer["message"] == "flow done", answer
    entries = list(f.record(answer["session"]))
    outputs = [e for e in entries if e["kind"] == "result"]
    assert [e["call"] for e in outputs] == [i["call_id"] for i in items], outputs
    first, second, refused, edited, written = outputs
    assert all(e["outcome"] == "success" for e in (first, second, edited, written)), outputs
    page = json.loads(first["output"])
    assert page["lines"] == ["line-1", "line-2"] and page["next"] == 3, page
    page = json.loads(second["output"])
    assert page["first"] == 3 and page["lines"] == ["line-3", "line-4"] and page["next"] == 5, page
    assert refused["outcome"] == "error", refused
    assert (here.cwd / "twice.txt").read_text() == twice, "a refused edit changed the file"
    assert (here.cwd / "notes.txt").read_text() == notes.replace("line-5", "line-five")
    assert (here.cwd / "made/deeper/new.txt").read_text() == "made\n"
    assert json.loads(written["output"])["created"] is True, written
