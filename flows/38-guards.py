"""The kill guard, and the deletion guard once turned on, refuse before execution; each deletion
refusal says what works. Razor's own keys (the web providers' and the Claude sign-in's) never
reach shell jobs."""
import json

from driver import flow
from responses import PLAIN, call, done, response

keys = {name: "flow-secret" for name in
        ("EXA_API_KEY", "FIRECRAWL_API_KEY", "PARALLEL_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN",
         "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN")}
commands = [f"printf ran > ran-{n}; ./{name} {arg}" for n, (name, arg) in enumerate(
            [("pkill", "no-process"), ("killall", "no-process"), ("kill", "-1")])]
commands += ["printf ran > delete-ran; rm old", "printf made > own; cat own", "rm own", "env",
             "printf made > ~/new", "rm ~/new", "cat > notes <<EOF\nrm ~/older\nEOF", "rm ~/older", 'rm "~/x"',
             "cat > notes <<EOF\nmade\nEOF\nrm ~/older", "rm ~/*.log", "rm ~nobody/x", "rm $X",
             "HOME=elsewhere; rm ~/kept", "HOME=elsewhere rm ~/kept"]


def refused(entry, older):
    """A deletion refusal is an error; one for an older file names the setting, one for a path not named literally does not."""
    assert entry["outcome"] == "error" and ("guards.delete" in entry["output"]) == older, entry


items = [call("bash", json.dumps({"command": command}), f"call_{n}")
         for n, command in enumerate(commands)]
with flow([response(items), done()], model="gpt-6.1-sol",
          config=PLAIN + "[guards]\ndelete = true\n",
          env={**keys, "FLOW_ORDINARY": "passed-through"}) as f:
    (f.cwd / "old").write_text("before session")
    (f.home / "older").write_text("before session")
    (f.cwd / "~").mkdir()
    (f.cwd / "~/x").write_text("before session")
    (f.cwd / "elsewhere").mkdir()
    (f.cwd / "elsewhere/kept").write_text("before session")
    # Harmless executables keep a broken kill guard safe, including `kill -1`.
    for name in ("pkill", "killall", "kill"):
        (f.cwd / name).write_text("#!/bin/sh\nexit 0\n")
        (f.cwd / name).chmod(0o755)
    entries = list(f.record(f.ask("Check guards and the job environment.")["session"]))
    outputs = [e for e in entries if e["kind"] == "result"]
    assert len(outputs) == len(items), outputs
    for n, entry in enumerate(outputs[:3]):
        assert entry["outcome"] == "error" and "kill" in entry["output"], entry
        assert not (f.cwd / f"ran-{n}").exists(), "a refused command ran"
    assert outputs[3]["outcome"] == "error" and "old" in outputs[3]["output"], outputs[3]
    refused(outputs[3], older=True)
    for old in (f.cwd / "old", f.home / "older", f.cwd / "~/x", f.cwd / "elsewhere/kept"):
        assert old.read_text() == "before session", f"{old} was deleted"
    assert not (f.cwd / "delete-ran").exists(), "a refused deletion command ran"
    assert all(e["outcome"] == "success" for e in outputs[4:10]), outputs[4:10]
    assert json.loads(outputs[4]["output"])["output"] == "made", outputs[4]
    assert not (f.cwd / "own").exists() and not (f.home / "new").exists(), "the session's own file was not deleted"
    # `~/` is the server's home, checked as the literal path; a quoted `~` is a relative path; a
    # heredoc's body is text, and the command after it is checked. A `~` after the command sets
    # HOME is unresolved.
    older, quoted, after = outputs[10:13]
    for entry, path in ((older, str(f.home / "older")), (quoted, "~/x"), (after, str(f.home / "older"))):
        assert path in entry["output"] and "before this session" in entry["output"], entry
        refused(entry, older=True)
    for entry in outputs[13:]:
        refused(entry, older=False)
    jobs = [e for e in entries if e["kind"] == "job" and e["event"] == "started"]
    assert len(jobs) == 6, jobs
    environment = json.loads(outputs[6]["output"])["output"]
    assert "FLOW_ORDINARY=passed-through" in environment, environment
    assert not any(name + "=" in environment for name in keys), environment

# A command and a patch's Delete File remove files older than the session unless the guard is on.
patch = json.dumps("*** Begin Patch\n*** Delete File: older\n*** End Patch")
source = f'text(await tools.exec_command({{cmd: "rm old"}}).catch(String)); text(await tools.apply_patch({patch}).catch(String));'
for on in (False, True):
    with flow([response([{"type": "custom_tool_call", "id": "ct_delete", "call_id": "delete", "name": "exec",
                          "input": source}]), done()], model="gpt-6.1-sol", config="[guards]\ndelete = true\n" * on) as f:
        for name in ("old", "older"):
            (f.cwd / name).write_text("before session")
        entries = f.record(f.ask("Delete the old files.")["session"])
        outputs = [e for e in entries if e["kind"] == "result" and e["call"].startswith("delete:")]
        assert [e["outcome"] for e in outputs] == ["error" if on else "success"] * 2, outputs
        assert all((f.cwd / name).exists() == on for name in ("old", "older")), "the guard setting was not followed"
        for entry in outputs if on else ():
            refused(entry, older=True)

# Full-command patterns select every matching owned job, including a child process; an identical
# process outside the session survives and cannot be stopped after the owned jobs are gone. A bare
# stop runs the rest of its command, a wrapped one none of it; each job's end shows once, as a notice.
import os
import subprocess
commands = [("one", "sleep 600 & printf one-ready > one; wait"),
            ("two", "sleep 601 & printf two-ready > two; wait")]
for stop, bare in (("pkill -9 -f '^sleep 60[01]$'", True), ("env pkill -TERM -x sleep", False)):
    replies = [response([call("bash", json.dumps({"command": command, "wait_ms": 0}), id)
                         for id, command in commands]),
               response([call("bash", json.dumps({"command": "pkill -USR1 -f sleep"}), "signal_only"),
                         call("bash", json.dumps({"command": stop + "; printf restarted > after"}), "stop_owned"),
                         call("bash", json.dumps({"command": "pkill -f '^sleep 600$'"}), "stop_absent")]), done()]
    with flow(replies, model="gpt-6.1-sol", config=PLAIN) as f:
        outside = subprocess.Popen(["sleep", "600"])
        try:
            entries = list(f.record(f.ask("Stop my matching jobs.")["session"]))
            signal_only = next(e for e in entries if e["kind"] == "result" and e["call"] == "signal_only")
            assert signal_only["outcome"] == "error", signal_only
            stopped = next(e for e in entries if e["kind"] == "result" and e["call"] == "stop_owned")
            assert (stopped["outcome"] == "success") == bare == (f.cwd / "after").exists(), stopped
            assert '"job":3' in stopped["output"] if bare else "without `pkill`" in stopped["output"], stopped
            notices = [e["text"].split()[1] for e in entries if e["kind"] == "input" and e["from"] == "razor"]
            assert sorted(notices) == ["job-1", "job-2"], notices
            absent = next(e for e in entries if e["kind"] == "result" and e["call"] == "stop_absent")
            assert absent["outcome"] == "error" and "kill a job by its handle" in absent["output"], absent
            assert outside.poll() is None, "a pattern stop killed a process outside this session"
            starts = [e for e in entries if e["kind"] == "job" and e["event"] == "started"]
            assert len(starts) == 2 + bare, starts
            for job in starts[:2]:
                try:
                    os.kill(job["pid"], 0)
                except ProcessLookupError:
                    pass
                else:
                    raise AssertionError(f"pattern stop left job {job['job']} alive")
        finally:
            outside.terminate()
            outside.wait()
