"""A failed project scan cannot discharge a closed session's claim-release obligation."""
from dataclasses import replace
import json
from claude import reply
from clock import wall_clock
from driver import flow
import server

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
        marker.write_text(marker.read_text().replace(
            next(line for line in marker.read_text().splitlines() if line.startswith("due:")),
            f"due: {due}"))
        replies.append(reply(text="Reminder received."))
        advance(minute * 60_000)
        server.post(f, "/residents/watcher/start", {})
        server.until(f, observer, lambda _: f"notified: {due}" in marker.read_text())

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
