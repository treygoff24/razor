"""Child prose reports, pending jobs, mail grace/nudge, a person's split input and errors."""
from dataclasses import replace
import json
import os
from pathlib import Path
import signal
import time
from driver import flow
from model import reply
from responses import PLAIN as CODEX, call, done, message, response, reply as gpt
import server
from standin import Reply
from terminal import Terminal

PLAIN, JOB, MAIL, PERSON, ERROR, LONG, CRASH, HELD = [f"ROOT_40_{name}" for name in ("PLAIN", "JOB", "MAIL", "PERSON", "ERROR", "LONG", "CRASH", "HELD")]
plain, job, mail, sibling, person, error, long, crash, held = [f"You are {name}40," for name in ("plain", "job", "mail", "sibling", "person", "error", "long", "crash", "held")]
closing = ["First closing paragraph40.", "Closing report40. " + "🪒" * 17000]
long_text = "\n\n".join(closing)
replies = [
    gpt(PLAIN, "agent", action="start", name="plain40", brief="End in prose, without finish.", model="opus", placement="in_place"),
    gpt(PLAIN, "agent", action="wait"), gpt(PLAIN, "plain-report-received40"),
    reply(plain, text=closing),
    gpt(JOB, "agent", action="start", name="job40", brief="Leave a job running, then report.", placement="in_place"),
    gpt(JOB, "job-parent-ready40"), gpt(JOB, "job-report-received40"),
    gpt(job, "bash", command="while [ ! -f release40 ]; do sleep 0.05; done", wait_ms=0),
    gpt(job, "job-waiting40"), gpt(job, "job-closing40"),
    gpt(MAIL, "agent", action="start", name="sibling40", brief="Do not reply to mentions.", placement="in_place"),
    gpt(MAIL, "agent", action="start", name="mail40", brief="Mention your sibling and wait.", placement="in_place"),
    gpt(MAIL, "mail-parent-ready40"), gpt(MAIL, "mail-report-received40"),
    gpt(sibling, "bash", command="sleep 120", wait_ms=0),
    gpt(sibling, "sibling-ready40"), gpt(sibling, "sibling-no-reply40"),
    gpt(mail, "message", type="note", to="channel", text="@sibling40 Please look at this."),
    gpt(mail, "mail-waiting40"), gpt(mail, "mail-nudged-report40"),
    gpt(PERSON, "agent", action="start", name="person40", brief="Send your parent a note, then wait.", placement="in_place"),
    gpt(PERSON, "person-parent-ready40"), gpt(PERSON, "person-parent-ready40"),
    gpt(person, "message", type="note", text="Ready for steering40."),
    gpt(person, "person-visible40"), gpt(person, "person-answer40"),
]

with flow(replies, model="sol", config=CODEX + '[children]\ngrace = "1h"\n') as f:
    server.start(f)

    def start(text):
        sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        server.post(f, f"/sessions/{sid}/input", {"text": text})
        return sid

    def child(parent, name):
        entries = server.until(f, parent, lambda es: any(e["kind"] == "child" and e["event"].get("started", {}).get("name") == name for e in es))
        return next(e["session"] for e in entries if e["kind"] == "child" and e["event"].get("started", {}).get("name") == name)

    def reports(parent, sid):
        return [e for e in f.record(parent) if e["kind"] == "input"
                and isinstance(e["from"], dict) and e["from"].get("report", {}).get("child") == sid]

    def ended(sid, text):
        return server.until(f, sid, lambda es:
                            max((e["id"] for e in es if e["kind"] == "turn_end"), default=0) >
                            max((e["id"] for e in es if e["kind"] == "text" and e["text"] == text), default=float("inf")))

    # Nothing pending: the recorded prose, bounded at a UTF-8 boundary, is the report.
    parent = start(PLAIN)
    sid = child(parent, "plain40")
    server.until(f, parent, lambda _: bool(reports(parent, sid)))
    report = reports(parent, sid)
    assert len(report) == 1 and report[0]["from"]["report"]["status"] == "finished", report
    texts = [e for e in f.record(sid) if e["kind"] == "text"]
    assert [e["text"] for e in texts] == closing and len(long_text.encode()) > 65536, texts
    suffix = f"\nFull closing text: session {sid}, record entries {texts[0]['id']}–{texts[-1]['id']}.\n"
    expected = long_text.encode()[:65536 - len(suffix.encode())].decode("utf-8", "ignore") + suffix
    actual = report[0]["text"]
    assert actual == expected and len(actual.encode()) <= 65536, (len(actual.encode()), actual[:100], actual[-128:])
    assert len([e for e in f.record(sid) if e["kind"] == "turn_end"]) == 1
    ended(parent, "plain-report-received40")
    replies += [gpt(PLAIN, "agent", action="continue", child="plain40", brief="End as one long block40."),
                gpt(PLAIN, "agent", action="wait"), gpt(PLAIN, "single-report-received40"),
                reply(plain, text=closing[1])]
    server.post(f, f"/sessions/{parent}/input", {"text": "Continue plain40 with one block."})
    server.until(f, parent, lambda _: len(reports(parent, sid)) == 2)
    texts = [e for e in f.record(sid) if e["kind"] == "text"]
    assert len(texts) == 3 and texts[-1]["text"] == closing[1], texts
    suffix = f"\nFull closing text: session {sid}, record entry {texts[-1]['id']}.\n"
    expected = closing[1].encode()[:65536 - len(suffix.encode())].decode("utf-8", "ignore") + suffix
    assert reports(parent, sid)[-1]["text"] == expected
    ended(parent, "single-report-received40")  # The next restart must not cut this reply.

    # A live job holds completion; only its notice turn's closing text reaches the parent.
    parent = start(JOB)
    sid = child(parent, "job40")
    entries = ended(sid, "job-waiting40")
    assert any(e["kind"] == "job" and e["event"] == "started" for e in entries) and not reports(parent, sid), entries
    assert not any(e["kind"] == "job" and e["event"] == "ended" for e in entries), entries
    ended(parent, "job-parent-ready40")  # Only the child's job is unfinished at shutdown.
    # It ends while the server is down: its notice still opens the turn that reports.
    server.stop(f)
    (f.cwd / "release40").touch()
    server.gone(next(e["pid"] for e in entries if e["kind"] == "job" and e["event"] == "started"))
    server.start(f)
    server.until(f, parent, lambda _: bool(reports(parent, sid)))
    assert [e["text"] for e in reports(parent, sid)] == ["job-closing40"]
    entries = list(f.record(sid))
    # Its notice is recorded before its end, so no stop between the two leaves the child idle.
    assert [(e["kind"], e.get("event", e.get("from"))) for e in entries if e["kind"] in ("input", "job")][-2:] == [("input", "razor"), ("job", "ended")], entries
    assert len([e for e in entries if e["kind"] == "turn_end"]) == 2, entries
    ended(parent, "job-report-received40")  # The later kill must not replay this parent turn.

    # No sibling reply: changing the real live setting shortens the existing grace.
    parent = start(MAIL)
    sid = child(parent, "mail40")
    entries = ended(sid, "mail-waiting40")
    assert any(e["kind"] == "mail_sent" for e in entries) and not reports(parent, sid), entries
    # A turn the kill cuts runs again on restart, so the sibling's answer to the mention ends first.
    ended(child(parent, "sibling40"), "sibling-no-reply40")
    ended(parent, "mail-parent-ready40")  # Restart only the child's mail grace.
    server.stop(f, signal.SIGKILL)
    server.start(f)
    config = f.home / ".razor/config.toml"
    config.write_text(config.read_text().replace('grace = "1h"', 'grace = "1s"'))
    server.until(f, parent, lambda _: bool(reports(parent, sid)))
    entries = list(f.record(sid))
    nudges = [e for e in entries if e["kind"] == "input" and e["from"] == "razor"]
    assert len(nudges) == 1, entries
    assert [e["text"] for e in reports(parent, sid)] == ["mail-nudged-report40"]

    # Continue clears the nudge as well: sent mail must receive a fresh grace, not finish now.
    config.write_text(config.read_text().replace('grace = "1s"', 'grace = "1h"'))
    replies += [gpt(MAIL, "agent", action="continue", child="mail40", brief="A new task40: mention the sibling again."),
                gpt(MAIL, "continued-parent-ready40"), gpt(MAIL, "continued-report-received40"),
                gpt(mail, "message", type="note", to="channel", text="@sibling40 New task40."),
                gpt(mail, "continued-waiting40"), gpt(mail, "continued-nudged-report40"),
                gpt(sibling, "sibling-still-no-reply40")]
    server.post(f, f"/sessions/{parent}/input", {"text": "Continue mail40 on a new task."})
    ended(sid, "continued-waiting40")
    assert len(reports(parent, sid)) == 1, reports(parent, sid)
    config.write_text(config.read_text().replace('grace = "1h"', 'grace = "1s"'))
    server.until(f, parent, lambda _: len(reports(parent, sid)) == 2)
    assert reports(parent, sid)[-1]["text"] == "continued-nudged-report40"
    assert len([e for e in f.record(sid) if e["kind"] == "input" and e["from"] == "razor"]) == 2

    ended(parent, "continued-report-received40")
    replies += [gpt(MAIL, "agent", action="stop", child="sibling40"), gpt(MAIL, "mail-cleaned40")]
    server.post(f, f"/sessions/{parent}/input", {"text": "Stop the sibling's pending job."})
    ended(parent, "mail-cleaned40")

    # No job, question or queued input: a person's split turn alone forbids a report.
    config.write_text(config.read_text().replace('grace = "1s"', 'grace = "1h"'))
    parent = start(PERSON)
    sid = child(parent, "person40")
    ended(sid, "person-visible40")
    with Terminal(f, "--resume", parent) as t:
        t.shown("person-visible40", "person-parent-ready40")
        t.click("person-visible40")
        t.shown("talking to person40", "as you")
        t.send("Person asks for progress40.\r")
        t.shown("person-answer40")
        entries = ended(sid, "person-answer40")
        asks = [e for e in entries if e["kind"] == "input" and e["from"] == "person"]
        assert len(asks) == 1 and asks[0]["id"] in entries[-1]["inputs"], entries
        assert not any(e["kind"] in ("job", "question") or (e["kind"] == "mail_sent" and e["mail_kind"] == "question") for e in entries), entries
        answered = {i for e in entries if e["kind"] == "turn_end" for i in e["inputs"]}
        assert all(e["id"] in answered for e in entries if e["kind"] == "input"), entries
        time.sleep(1.2)
        assert not reports(parent, sid), reports(parent, sid)

    # Continue reaches the idle child that did not report, whatever its last turn's end.
    replies += [gpt(PERSON, "agent", action="continue", child="person40", brief="Again40."), gpt(PERSON, "person-continued40"),
                gpt(PERSON, "person-report40"), gpt("Again40.", "flow done"), Reply(b'{"type": "error", "error": {"type": "permission_error", "message": "Failed40."}}', status=403, match=person)]
    server.post(f, f"/sessions/{parent}/input", {"text": "Continue person40."})
    ended(parent, "person-report40")
    # A failed turn that took the person's input does not report: its end carries no report.
    server.post(f, f"/sessions/{sid}/input", {"text": "Fail40."})
    end = server.until(f, sid, lambda es: "error" in es[-1].get("end", ""))[-1]
    assert "report" not in end and len(reports(parent, sid)) == 1, end

    # A turn ending in an error reports blocked at once with its sentence; continue runs it again.
    rejected = Reply(b'{"error": {"type": "invalid_request_error", "message": "Rejected40"}}', status=400, match=error)
    replies += [gpt(ERROR, "agent", action="start", name="error40", brief="Fail once.", model="gpt-6.1-sol", placement="in_place"),
                gpt(ERROR, "agent", action="wait"), gpt(ERROR, "error-report-received40"), rejected]
    parent = start(ERROR)
    sid = child(parent, "error40")
    entries = ended(parent, "error-report-received40")
    blocked = "The model endpoint returned HTTP 400: Rejected40.\n\nContinue error40 to try again."
    wait = next(e["call"] for e in entries if e["kind"] == "call" and e["input"].get("action") == "wait")
    waited = json.loads(next(e["output"] for e in entries if e["kind"] == "result" and e["call"] == wait))
    assert waited["from"]["report"]["status"] == "blocked" and waited["text"] == blocked, waited
    replies += [gpt(ERROR, "agent", action="continue", child="error40", brief="Try again40."), gpt(ERROR, "agent", action="wait"),
                gpt(ERROR, "retry-report-received40"), replace(done(), match=error)]
    server.post(f, f"/sessions/{parent}/input", {"text": "Continue error40."})
    ended(parent, "retry-report-received40")
    assert [(r["from"]["report"]["status"], r["text"]) for r in reports(parent, sid)] == [
        ("blocked", blocked), ("finished", "flow done")], reports(parent, sid)
    ends = [e["end"] for e in f.record(sid) if e["kind"] == "turn_end"]
    assert ends == [{"error": blocked.split("\n")[0]}, "done"], ends

    # An error sentence past 64 KiB is cut so the report fits, the retry line kept.
    big = json.dumps({"type": "response.incomplete", "response": {"incomplete_details": {"reason": "🪒" * 20000}}}, ensure_ascii=False)
    replies += [gpt(LONG, "agent", action="start", name="long40", brief="Fail long.", model="gpt-6.1-sol", placement="in_place"),
                gpt(LONG, "agent", action="wait"), gpt(LONG, "long-report-received40"), Reply(f"data: {big}\n\n".encode(), match=long)]
    parent = start(LONG)
    sid = lone = child(parent, "long40")
    ended(parent, "long-report-received40")
    line = "\n\nContinue long40 to try again."
    cut = f"The model's reply stopped early ({'🪒' * 20000}).".encode()[:65536 - len(line)].decode("utf-8", "ignore")
    assert [(r["from"]["report"]["status"], r["text"]) for r in reports(parent, sid)] == [("blocked", cut + line)]

    # A conversation that dies with a job's end notice queued runs that notice's turn, whose end reports.
    # The notice stays queued while a model reply streams (`agent wait` would take it); the CLI is
    # stopped before it reads that reply's end and killed once the stand-in has sent it.
    (f.cwd / "crash40").mkdir()
    replies += [gpt(CRASH, "agent", action="start", name="crash40", brief="Wait on a job.", cwd="crash40", model="opus", placement="in_place"),
                gpt(CRASH, "agent", action="wait"), gpt(CRASH, "crash-report-received40"),
                reply(crash, "Bash", {"command": "while [ ! -f go40 ]; do sleep 0.05; done", "run_in_background": True}),
                replace(reply(crash, text="unread40"), cuts=tuple(range(10, 850, 10)))]  # about 2.5 s
    parent = start(CRASH)
    sid = child(parent, "crash40")
    server.until(f, sid, lambda es: any(e["kind"] == "result" for e in es))
    # The job must end during the second request's stream, before the CLI is stopped.
    server.wait(lambda: sum(crash in body for body in f.sent()) >= 2)
    replies.append(reply(f"{sid}/job-1</output-file>", text="crash-recovered40"))
    (f.cwd / "crash40/go40").touch()
    server.until(f, sid, lambda es: any(e["kind"] == "job" and e["event"] == "ended" for e in es))
    # The child's Claude Code process works in crash40.
    pids = [int(proc.name) for proc in Path("/proc").glob("[0-9]*") if os.path.realpath(proc / "cwd") == str((f.cwd / "crash40").resolve())]
    for sig in signal.SIGSTOP, signal.SIGKILL:
        for pid in pids:
            os.kill(pid, sig)
        server.until(f, sid, lambda _: all('"sent"' in p.read_text() for p in f.requests.glob("[0-9]*.json")))
    ended(parent, "crash-report-received40")
    assert [(r["from"]["report"]["status"], r["text"]) for r in reports(parent, sid)] == [("finished", "crash-recovered40")]
    assert len([e for e in f.record(sid) if e["kind"] == "turn_end" and "error" in e["end"]]) == 1

    # A report survives a stop the moment its child's turn end is written. Words the person sent
    # as the child streamed its finish wait for its next task, which the person's first message
    # after the restart opens with both.
    replies += [gpt(HELD, "agent", action="start", name="held40", brief="Finish at once.", placement="in_place"),
                gpt(HELD, "held-parent-ready40"), gpt(HELD, "held-report-heard40"),
                gpt(held, "agent", action="wait"),
                replace(response([message("I read your message; I will send the finished report."),
                                  call("agent", json.dumps({"action": "finish", "status": "finished", "report": "held-report40"}), "held_finish40")]),
                        match=held, cuts=tuple(range(1, 60))),
                gpt("HELD_LATER40", "held-answer40")]
    parent = start(HELD)
    sid = child(parent, "held40")
    # Let the parent's opening turn end before releasing the child's finish: the kill tests
    # report recovery, without also interrupting and replaying the parent's opening reply.
    ended(parent, "held-parent-ready40")
    server.until(f, sid, lambda es: any(e["kind"] == "call" and e["input"].get("action") == "wait" for e in es))
    server.post(f, f"/sessions/{sid}/input", {"text": "HELD_FINISH40"})
    server.wait(lambda: any("HELD_FINISH40" in body for body in f.sent()))
    early = server.post(f, f"/sessions/{sid}/input", {"text": "HELD_EARLY40"})["entry"]
    while b'"kind":"turn_end"' not in (f.home / ".razor/sessions" / sid / "record.jsonl").read_bytes():
        pass
    os.kill(json.loads((f.home / ".razor/server.json").read_text())["pid"], signal.SIGSTOP)
    server.stop(f, signal.SIGKILL)
    server.start(f)
    # The restart makes the blocked helper without a worktree its row from its own record.
    row = server.wait(lambda: (r := server.agents(f).get(lone)) and r["report"] and r, 10, "no reported row")
    assert (row["branch"], row["model"], row["report"]["status"]) == (None, "gpt-6.1-sol", "blocked") and row["last_activity"], row
    ended(parent, "held-report-heard40")
    assert [r["text"] for r in reports(parent, sid)] == ["held-report40"], reports(parent, sid)
    later = server.post(f, f"/sessions/{sid}/input", {"text": "HELD_LATER40"})["entry"]
    assert {early, later} <= set(ended(sid, "held-answer40")[-1]["inputs"])
