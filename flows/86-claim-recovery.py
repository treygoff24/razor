"""A failed project scan cannot discharge a closed session's claim-release obligation."""
from dataclasses import replace
import json
from claude import reply
from clock import wall_clock
from driver import flow
import server
from model import agent

replies = []
with wall_clock() as (_, env, advance), flow(replies, env=env) as f:
    f.base()
    other = replace(f, cwd=f.cwd / "other")
    other.cwd.mkdir()
    other.base()
    watcher = f.home / "watcher"
    watcher.mkdir()
    with (f.home / ".razor/config.toml").open("a") as config:
        config.write(f'\n[residents.watcher]\ncwd={json.dumps(str(watcher))}\n')

    def task(where, **arguments):
        replies.extend([reply((None, "mcp__razor__task", {"action": "write", **arguments})),
                        reply(text="saved")])
        sid = where.ask("Keep this task.")["session"]
        result = next(e for e in where.record(sid) if e["kind"] == "result")
        assert result["outcome"] == "success", result
        return sid

    holder = task(f, title="Release after close", claim=True)
    claimed = next((f.home / ".razor/tasks").rglob("1.md"))
    observer = task(other, title="Minute marker", due="2000-01-01")
    marker = next(p for p in (f.home / ".razor/tasks").rglob("1.md") if p != claimed)

    def tick(minute, due):
        previous = next(line for line in marker.read_text().splitlines() if line.startswith("due:"))
        marker.write_text(marker.read_text().replace(previous, f"due: {due}"))
        replies.append(reply(text="Reminder received."))
        advance(minute * 60_000)
        server.post(f, "/residents/watcher/start", {})
        # The marker precedes admission; finish this reminder before the next tick or shutdown.
        # The observer has its initial turn, then one reminder turn per tick.
        server.until(f, observer, lambda es: f"notified: {due}" in marker.read_text() and
                     sum(e["kind"] == "turn_end" for e in es) == minute + 1)

    tick(1, "2000-01-01")  # A completed visit has discharged the server's startup recovery.
    folder = claimed.parent
    folder.chmod(0)
    try:
        failed = server.send(f, "GET", f"/sessions/{holder}/tasks")
        assert failed.status != 200, (failed.status, failed.read())
        server.post(f, f"/sessions/{holder}/quit", {})
        tick(2, "2000-01-02")  # The failed scan occurs while recovery is owed.
    finally:
        folder.chmod(0o755)
    tick(3, "2000-01-03")
    server.until(f, holder, lambda _: "claim:" not in claimed.read_text())

    # A working child with a foreign parent keeps its claim across startup.
    beta = f.home_at("parent86")
    beta.run("devices")
    tap = server.Tap(beta, f, "owner", "parent")
    replies += [agent("PARENT_86", "start", server="owner", name="recovery86", cwd=str(f.cwd), task=1, placement="in_place", brief="Hold."),
                agent("PARENT_86", "wait"), reply(text="Started.", match="PARENT_86"),
                reply((None, "mcp__razor__message", {"type": "question", "text": "Hold this claim?"}), match="You are recovery86,"),
                reply(text="Held.", match="You are recovery86,")]
    parent = beta.ask("PARENT_86")["session"]
    launched = json.loads(next(e["output"] for e in beta.record(parent) if e["kind"] == "result"))
    child = launched["session"].split("@")[0]
    server.until(f, child, lambda es: any(e["kind"] == "turn_end" for e in es))
    owner = (f.home / ".razor/server.id").read_text().strip()
    assert f"claim: {child} {owner} " in claimed.read_text(), claimed.read_text()
    server.stop(f)
    server.start(f)
    tick(4, "2000-01-04")
    assert f"claim: {child} " in claimed.read_text(), claimed.read_text()
    assert not any("report" in e for e in f.record(child)), "the remote child finished during startup"
