"""A schedule runs on the model and effort it names, and only for its own turn; its result reaches
the person as a note through the notify command, or as an ask the status stream lists; a failed
model turn sends a note without a model tool call; the person's message settles a question, whose
late answer from the ask command is refused; a fresh run's thread is a root whose row names its
resident, listed only among its resident's sessions; a resident's projects (one per request,
renamed in place, never through a link) and its `sky` file, passed through on its row unless bad
(docs/design/residents.md). A fresh run's row is read directly outside the live feed. Schedules
here run through the run-now route, which admits them as the timer does; flow 21 fires them by
the clock."""
import json
import sys
import tempfile
import time
from pathlib import Path
import claude
from driver import REPLIES, Reply, flow
from server import agents, post, send, start as start_server, stop, until, wait


def answered(entry):
    return isinstance(entry["from"], dict) and "answer" in entry["from"]


def sse(name, match):
    return Reply((REPLIES / f"{name}.sse").read_bytes(), match=match)


question = {"type": "question", "to": "parent", "text": "May I ship the review?"}
replies = [sse(name, marker) for name, marker in (
    ("done", "Start the server."), ("resident-note", "Brief me."), ("done", "Brief me."),
    ("resident-ask", "Review."), ("done", "Review."))]
replies += [claude.reply((None, "mcp__razor__message", question), match="Ask me about the review."),
            sse("done", "Ask me about the review."), sse("resident-ask-second", "Yes, ship it."),
            sse("done", "Yes, ship it."), sse("done", "From the client."),
            claude.reply((None, "mcp__razor__message", dict(question, text="A third question?")), match="Ask a third."),
            sse("done", "Ask a third."), sse("done", "Never mind.")]
with tempfile.TemporaryDirectory() as scratch:
    gate, asked, tardy, log, record = (Path(scratch) / name for name in ("gate", "asked", "tardy", "sent.jsonl", "record"))
    # Both commands log their arguments and the four variables Razor sets, None when unset.
    record.write_text(f"#!{sys.executable}\nimport json, os, sys\nwith open({str(log)!r}, 'a') as log:\n    print(json.dumps("
                      "[sys.argv[1:], *map(os.environ.get, ('RAZOR_SESSION', 'RAZOR_NAME', 'RAZOR_PROJECT', 'RAZOR_ASK'))]), file=log)\n")
    record.chmod(0o755)
    log.touch()
    sender = lambda text: wait(lambda: next((e for e in map(json.loads, reversed(log.read_text().splitlines()))
                                             if text in e[0][-1]), None), 10, text)
    # The ask command answers the review question after a second, and the second and third
    # questions once the flow lets it, marking that it did; the scheduled review's ask it leaves open.
    command = Path(scratch) / "ask"
    command.write_text(f'#!/bin/sh\n"{record}" "$@"\ncase "$2" in\n*"May I ship"*) sleep 1;;\n*second*) until [ -e "{gate}" ]; '
                       f'do sleep .05; done; echo "Yes, ship it."; exec touch "{asked}";;\n*third*) until [ -e "{gate}" ]; '
                       f'do sleep .05; done; echo "Too late."; exec touch "{tardy}";;\n*) exit 1;;\nesac\n'
                       'echo "Yes, ship it."\n')
    command.chmod(0o755)
    failure = lambda match: Reply(b'{"error":{"code":"invalid_request_error","message":"scheduled model failure"}}',
                                  status=400, match=match)
    # The server's own stale values of the variables reach neither command. Notify is the
    # documented shell form, which gets the text as $1; `$RAZOR_NAME` in ask's arguments stays literal.
    stale = dict.fromkeys(("RAZOR_SESSION", "RAZOR_NAME", "RAZOR_PROJECT", "RAZOR_ASK"), "stale")
    notify = json.dumps(f'exec "{record}" "$1"')
    with flow(replies + [failure("Fail."), failure("Fresh run.")], env=stale,
              config=f'notify = ["sh", "-c", {notify}, "sh"]\n'
                     f'ask = [{json.dumps(str(command))}, "$RAZOR_NAME"]\n') as f:
        (f.cwd / "persona.md").write_text("You are Ember.")
        with (f.home / ".razor/config.toml").open("a") as config:
            config.write(f"\n[residents.ember]\ncwd = {json.dumps(str(f.cwd))}\n"
                         f"startup = [{json.dumps(str(f.cwd / 'persona.md'))}, \"razor\"]\n"
                         'sky = "sky.json"\n'
                         '[residents.ember.schedules.brief]\nat = []\ntext = "Brief me."\n'
                         'model = "sonnet"\neffort = "low"\ndeliver = "notify"\n'
                         '[residents.ember.schedules.review]\nat = []\ntext = "Review."\n'
                         'deliver = "ask"\n'
                         '[residents.ember.schedules.broken]\nat = []\ntext = "Fail."\n'
                         'model = "gpt-6.1-sol"\n'
                         '[residents.ember.schedules.fresh]\nat = []\ntext = "Fresh run."\nfresh = true\n'
                         'model = "gpt-6.1-sol"\n')
        f.ask("Start the server.")
        until(f, "ember", lambda es: es and es[0]["kind"] == "start")
        for n, schedule in enumerate(("brief", "review"), 1):
            post(f, f"/residents/ember/run/{schedule}", {})
            entries = until(f, "ember", lambda es: sum(e["kind"] == "turn_end" for e in es) >= n)
        requests = [json.loads(body) for body in f.sent()]
        brief = next(r for r in requests if "Brief me." in json.dumps(r) and "Review." not in json.dumps(r))
        review = next(r for r in requests if "Review." in json.dumps(r))
        assert "sonnet" in brief["model"] and "opus" in review["model"], (brief["model"], review["model"])
        # Claude Code sends the effort level in the request body; the next turn's is the session's.
        assert brief["output_config"]["effort"] == "low" != review["output_config"]["effort"], "effort"
        inputs = [e for e in entries if e["kind"] == "input"]
        assert [e["from"]["schedule"]["name"] for e in inputs] == ["brief", "review"], inputs
        assert inputs[0]["model"] == "sonnet" and inputs[0]["effort"] == "low", inputs[0]
        assert "type `note`, `to` `parent`" in inputs[0]["text"], inputs[0]
        assert sender("Brief: all quiet.") == [["Brief: all quiet."], "ember", "ember", f.cwd.name, None], log.read_text()
        ask = [e for e in entries if e["kind"] == "call"][-1]
        assert ["ember", ask["id"]] in [question["id"] for question in agents(f)["ember"].get("asks", [])], agents(f)
        post(f, "/residents/ember/run/broken", {})
        entries = until(f, "ember", lambda es: any(e["kind"] == "turn_end" and
                        isinstance(e["end"], dict) and "error" in e["end"] for e in es))
        failed = [e for e in entries if e["kind"] == "turn_end"][-1]
        assert "scheduled model failure" in failed["end"]["error"], failed
        expected = f"Resident ember's schedule broken failed: {failed['end']['error'].rstrip('.')}."
        assert sender(expected) == [[expected], "ember", "ember", f.cwd.name, None], log.read_text()
        # A failed fresh run's notice names the resident and carries the run's own identity.
        run = post(f, "/residents/ember/run/fresh", {})["entry"]
        thread = next(e for e in f.record("ember") if e["id"] == run)["session"]
        until(f, thread, lambda es: any(e["kind"] == "turn_end" for e in es))
        row = json.load(send(f, "GET", f"/sessions/{thread}/agent"))
        assert row["name"].startswith("fresh · "), row
        assert sender("Resident ember's schedule fresh failed")[1:] == [thread, row["name"], f.cwd.name, None], log.read_text()
        assert row.get("resident") == "ember" and row["parent"] is None, row
        ask = lambda method, path, body=None: ((a := send(f, method, path, body)).status, json.loads(a.read()))
        assert thread not in [s["session"] for s in ask("GET", "/sessions")[1]], "a fresh run is listed"
        assert [s["job"] for s in ask("GET", "/sessions?resident=ember")[1] if s["session"] == thread] == ["fresh"]
        # A project made twice by one request is one project; a rename keeps its folder; a link is none.
        made = ask("POST", "/residents/ember/projects", {"name": "Alpha", "request": "make-22"})
        assert made[0] == 201 and ask("POST", "/residents/ember/projects", {"name": "Alpha", "request": "make-22"}) == made
        assert ask("PATCH", "/residents/ember/projects/Alpha", {"name": "Beta"})[1]["id"] == "Alpha"
        (f.cwd / "projects/Link").symlink_to(f.home / "tmp", target_is_directory=True)
        assert ask("PATCH", "/residents/ember/projects/Link", {"name": "Gone"})[0] == 404 and not (f.home / "tmp/.razor").exists()
        assert [(p["id"], p["name"]) for p in ask("GET", "/residents/ember/projects")[1]] == [("Alpha", "Beta")]
        # The sky file reaches the row as its days and counts alone; one malformed or over 64 KiB
        # leaves the field out and the row in.
        days = [{"day": "2026-08-27", "n": 2}, {"day": "2026-09-01", "n": 18}]
        for text, shown in [(json.dumps([{**day, "note": "private"} for day in days]), days),
                            ('[{"day": "2026-08-27"', None), (json.dumps(days * 2000), None)]:
            (f.cwd / "sky.json").write_text(text)
            row = json.load(send(f, "GET", "/residents"))[0]
            assert row["session"] == "ember" and row.get("sky") == shown, (text[:60], row.get("sky"))
        # A resident's question to the person returns once recorded as an open ask; the ask
        # command's answer reaches the resident's next request; an ask answered first through the
        # API is not answered again by its command.
        mark, base = len(list(f.record("ember"))), len(f.sent())
        f.ask("--resume", "ember", "Ask me about the review.")
        ends = lambda n: lambda es: sum(e["kind"] == "turn_end" for e in es[mark:]) >= n
        entries = until(f, "ember", ends(2))[mark:]
        first = [e for e in entries if e["kind"] == "call"][0]
        answers = [e for e in entries if e["kind"] == "input" and answered(e)]
        assert answers[0]["from"]["answer"]["ask"] == first["id"], answers
        assert answers[0]["text"] == "Yes, ship it.", answers[0]
        assert sender("May I ship")[0][0] == "$RAZOR_NAME", log.read_text()
        assert sender("May I ship")[1:] == ["ember", "ember", f.cwd.name, str(first["id"])], log.read_text()
        turn = [e for e in entries if e["kind"] == "turn_end"][0]
        assert turn["id"] < answers[0]["id"], "the question waited for its answer"
        assert "Yes, ship it." in f.sent()[base + 2]
        second = [e for e in entries if e["kind"] == "call"][-1]
        # A script answers through the session and entry its ask command was given.
        _, session, _, _, entry = sender("second")
        answer = lambda: send(f, "POST", f"/sessions/{session}/asks/{entry}", {"text": "From the client."}).status
        assert answer() == 200
        assert answer() == 400, "a second answer to one ask was admitted"
        # The person's next message settles a third question; the command's late answer to it is
        # not recorded and starts no turn.
        until(f, "ember", ends(3))
        f.ask("--resume", "ember", "Ask a third.")
        f.ask("--resume", "ember", "Never mind.")
        settled = len(list(f.record("ember")))
        gate.touch()  # The command's own answers to the second and third asks come now.
        wait(lambda: asked.exists() and tardy.exists(), 10, "the ask command never answered the later questions")
        time.sleep(1)
        assert not [e for e in list(f.record("ember"))[settled:] if e["kind"] == "input"], "a settled ask took a late answer"
        answers = [e for e in until(f, "ember", ends(3))[mark:] if e["kind"] == "input" and answered(e)]
        late = [e for e in answers if e["from"]["answer"]["ask"] == second["id"]]
        assert [e["text"] for e in late] == ["From the client."], late
        # Replay closed runs older than a day: a refreshed old name and a new weekly name
        # survive churn even when name order and first insertion disagree with run age.
        stop(f)
        config = f.home / ".razor/config.toml"
        config.write_text(config.read_text().split("\n[residents.ember]")[0] +
                          f'\n[residents.ember]\ncwd = {json.dumps(str(f.cwd))}\n' +
                          ''.join(f'[residents.ember.schedules.{name}]\nat = []\ntext = "Run."\n'
                                  for name in ("old65", "old64", "old01", "old00", "weekly")))
        now, day = int(time.time() * 1000), 86_400_000
        entries = [dict(next(f.record("ember")), at=now - 141 * day)]
        names = [f"old{i:02}" for i in reversed(range(66))] + ["old65", "weekly", "old64"]
        for i, name in enumerate(names):
            at_ms, entry = now - (140 - i * 2) * day, len(entries) + 1
            entries.append({"id": entry, "at": at_ms, "kind": "input", "text": name,
                            "from": {"schedule": {"name": name, "due": at_ms}}})
            entries.append({"id": entry + 1, "at": at_ms + 1, "kind": "withdrawn", "inputs": [entry]})
        (f.home / ".razor/sessions/ember/record.jsonl").write_text(''.join(json.dumps(e) + '\n' for e in entries))
        start_server(f)
        row = next(r for r in json.load(send(f, "GET", "/residents")) if r["session"] == "ember")
        for name, i in (("old65", 66), ("weekly", 67), ("old01", 64), ("old00", 65), ("old64", 68)):
            latest = row["schedules"][name]["latest"]
            assert latest and latest["at"] == now - (140 - i * 2) * day, (name, latest)
        stop(f)
        entries = [entries[0],
                   {"id": 2, "at": now - 2 * day, "kind": "input", "text": "Delayed run.",
                    "from": {"schedule": {"name": "old65", "due": now - 2 * day}}},
                   {"id": 3, "at": now, "kind": "input", "text": "New run.",
                    "from": {"schedule": {"name": "weekly", "due": now}}},
                   {"id": 4, "at": now, "kind": "withdrawn", "inputs": [3]},
                   {"id": 5, "at": now, "kind": "text", "text": "Delayed run completed."},
                   {"id": 6, "at": now, "kind": "turn_end", "inputs": [2], "end": "done",
                    "usage": {"input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "requests": 0, "waited": 0}}]
        (f.home / ".razor/sessions/ember/record.jsonl").write_text(''.join(json.dumps(e) + '\n' for e in entries))
        start_server(f)
        row = next(r for r in json.load(send(f, "GET", "/residents")) if r["session"] == "ember")
        latest = row["schedules"]["old65"]["latest"]
        assert latest["start"] == 5 and latest["line"] == "Delayed run completed.", latest
