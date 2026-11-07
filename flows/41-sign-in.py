"""Sign-in tokens stay with each conversation across resumes; a server restart signs in afresh
without losing native history; the sign-in command learns its session. Account rows and rejected
usage windows reach the status pane. A missing native conversation rebuilds once; a fresh CLI
that cannot start ends its turn (docs/design/claude.md)."""
import json
import shutil
import time
from dataclasses import replace

from claude import reply
from driver import Reply, flow
import server

count = 'n=$(cat "$TMPDIR/count" 2>/dev/null || echo 0); echo $((n + 1)) | tee "$TMPDIR/count"'
sign_in = json.dumps(count + ' | sed "s/^/flow-token-$RAZOR_SESSION-/"; echo flow-account')
# No idle conversation stays open, so each resume starts a new CLI. The usage file's path is
# made absolute against the config's folder.
config = (f'[claude]\nsign_in = ["sh", "-c", {sign_in}]\nusage_file = "usage.json"\n'
          '[sessions]\nidle = 0\n')
fable = {"kind": "weekly_scoped", "model": "Fable", "percent": 36.0, "resets_at": 4102444800}
limit = tuple((f"anthropic-ratelimit-unified-{key}", value) for key, value in (("status", "rejected"),
    ("representative-claim", "five_hour"), ("overage-status", "rejected"), ("overage-disabled-reason", "out_of_credits")))
replies = [replace(reply(text=word), headers=limit if word == "four" else ()) for word in ("one", "two", "three", "four", "five")]


with flow(replies, config=config) as f:
    (f.home / ".razor/usage.json").write_text(json.dumps({"observed_at": int(time.time()),
        "accounts": {"work": {"flow-account": {"status": "ok", "limits": [fable]}}}}))
    signed = [session := f.ask("Say 1.")["session"]]
    for args in (["Say 2."], ["--resume", session, "Say 3."], ["--resume", session, "Say 4."]):
        if args[-1] == "Say 4.":  # A restart forgets every kept token.
            server.stop(f)
        signed.append(f.ask(*args)["session"])
    for n, token in ((1, 1), (2, 2), (3, 1), (4, 3)):
        headers = json.loads((f.requests / f"00{n}.json").read_text())["headers"]
        assert headers.get("authorization") == f"Bearer flow-token-{signed[n - 1]}-{token}", headers
        assert "x-api-key" not in headers, "the server's key signed in, not the command's token"
    resumed = f.request(4)["messages"]
    assert "Say 1." in json.dumps(resumed), "the resume lost its conversation"
    kinds = [e["kind"] for e in f.record(session)]
    assert "rebuild" not in kinds, f"the resume rebuilt instead of continuing: {kinds}"
    usage = server.agents(f)[session]["usage"]
    row = {"name": "Fable", "utilization": 36.0, "resets_at": 4102444800000, "rejected": False, "reason": None}
    assert row in usage, f"the usage file's model row is missing: {usage}"
    row = {"name": "5-hour", "utilization": None, "resets_at": None, "rejected": True, "reason": "out of usage credits"}
    assert row in usage, f"the rejected window lost its reason: {usage}"
    # A missing native conversation rebuilds in the same turn from Razor's record.
    shutil.rmtree(f.home / ".razor/claude/projects")
    result = f.run("--print", "--resume", session, "Say 5.")
    assert result.returncode == 0 and result.stdout.strip() == "five", (result.stdout, result.stderr)
    rebuilds = lambda: [e["trigger"] for e in f.record(session) if e["kind"] == "rebuild"]
    assert rebuilds() == ["restart"], rebuilds()
    # A CLI that cannot start rebuilds the resume once; the fresh failure ends the turn.
    f.cwd.rename(f.cwd.with_name("moved"))
    result = replace(f, cwd=f.home).run("--print", "--resume", session, "Say 6.")
    f.cwd.with_name("moved").rename(f.cwd)
    ends = [e["end"] for e in f.record(session) if e["kind"] == "turn_end"]
    assert result.returncode != 0 and rebuilds() == ["restart"] * 2, (result.stderr, rebuilds())
    assert ends[-2] == "done" and "error" in ends[-1], ends[-2:]

# A subscription refusal invalidates this conversation's sign-in, not the server.
refusal = Reply(b'{"type":"error","error":{"type":"permission_error","message":"Your organization has disabled Claude subscription access"}}', status=403)
revoked = Reply(b'{"type":"error","error":{"type":"authentication_error","message":"Revoked"}}', status=401)
invalid = Reply(b'{"type":"error","error":{"type":"invalid_request_error","message":"Bad request"}}', status=400)
# The pinned CLI tries this 400 ten times and the 401 twice before reporting each failure.
with flow([reply(text="before"), *([invalid] * 10), reply(text="unchanged"), refusal, reply(text="still"), revoked, revoked, reply(text="after")], config=config.replace('idle = 0', 'idle = 3600')) as f:
    session = f.ask("Before refusal.")["session"]
    failed = f.run("--print", "--resume", session, "Invalid.")
    assert failed.returncode != 0 and "Bad request" in failed.stderr, failed.stderr
    assert f.run("--print", "--resume", session, "Unchanged.").stdout.strip() == "unchanged"
    failed = f.run("--print", "--resume", session, "Refused.")
    assert failed.returncode != 0 and "disabled Claude subscription access" in failed.stderr, failed.stderr
    assert f.run("--print", "--resume", session, "Still signed in.").stdout.strip() == "still"
    failed = f.run("--print", "--resume", session, "Revoked.")
    assert failed.returncode != 0 and "Revoked" in failed.stderr, failed.stderr
    recovered = f.run("--print", "--resume", session, "After refusal.")
    assert recovered.returncode == 0 and recovered.stdout.strip() == "after", (recovered.stdout, recovered.stderr)
    tokens = [json.loads(path.read_text())["headers"].get("authorization") for path in sorted(f.requests.glob("[0-9]*.json"))]
    assert tokens == [f"Bearer flow-token-{session}-{n}" for n in ([1] * 13 + [2] * 3 + [3])], tokens
