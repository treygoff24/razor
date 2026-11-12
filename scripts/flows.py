#!/usr/bin/env python3
"""Run selected numbered flows, eight at a time, printing each file's output when it exits; a flow
still running after LIMIT seconds is stopped and fails."""
from contextlib import suppress
from pathlib import Path
import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--shard", help="one-based INDEX/TOTAL, balanced by recorded durations")
parser.add_argument("--repeat", type=int, default=1, help="complete repetitions, including after failures")
parser.add_argument("selectors", nargs="*")
args = parser.parse_args()
if args.repeat < 1:
    parser.error("--repeat must be positive")
LIMIT = 120
files, running, late, stopped, failed = set(), {}, set(), 0, 0
for arg in args.selectors or ["all"]:
    selector = Path(arg).name.removesuffix(".py")
    files |= {p for p in Path("flows").glob("[0-9][0-9]-*.py") if selector in
              ("all", p.stem, p.stem[3:], p.name[:2], str(int(p.name[:2])))} or sys.exit(
        f"No flow matches '{selector}'; use a flow number or name.")
if args.shard:
    try:
        index, total = map(int, args.shard.split("/"))
        if not 1 <= index <= total <= len(files):
            raise ValueError
    except ValueError:
        parser.error("--shard must be INDEX/TOTAL with 1 <= INDEX <= TOTAL <= flow count")
    durations = json.loads(Path("scripts/flow-times.json").read_text())
    shards, loads = [[] for _ in range(total)], [0] * total
    for file in sorted(files, key=lambda p: (-durations.get(p.name, max(durations.values())), p.name)):
        shard = min(range(total), key=lambda i: loads[i])
        shards[shard].append(file)
        loads[shard] += durations.get(file.name, max(durations.values()))
    files = list(reversed(shards[index - 1]))
else:
    files = sorted(files)
# Each repetition finishes before the next starts, so no file races its own fixtures.
files = [(file, repeat) for repeat in reversed(range(1, args.repeat + 1)) for file in files]


def start_relay(folder):
    """Starts the iroh relay the network flows connect through, in development mode on a free
    port so that two runs at once do not share one; RAZOR_FLOW_RELAY names its address."""
    with socket.socket() as taken:
        taken.bind(("127.0.0.1", 0))
        port = taken.getsockname()[1]
    config = Path(folder) / "relay.toml"
    config.write_text(f'http_bind_addr = "127.0.0.1:{port}"\nenable_metrics = false\n')
    log = open(Path(folder) / "relay.log", "w")
    relay = subprocess.Popen([os.environ["RAZOR_RELAY_BIN"], "--dev", "--config-path", config],
                             stdin=subprocess.DEVNULL, stdout=log, stderr=log)
    for _ in range(100):
        with suppress(OSError), socket.create_connection(("127.0.0.1", port), timeout=1):
            os.environ["RAZOR_FLOW_RELAY"] = f"http://127.0.0.1:{port}"
            return relay
        time.sleep(0.05)
    relay.kill()
    sys.exit(f"The relay did not start: {(Path(folder) / 'relay.log').read_text()}")


def stop(signum, _):
    global stopped
    stopped = stopped or 128 + signum
    for pid in running:
        with suppress(ProcessLookupError):
            os.killpg(pid, signal.SIGTERM)


def overdue(*_):
    """Stops each flow past LIMIT as `stop` stops them all."""
    for pid, (*_, started) in running.items():
        if pid not in late and time.monotonic() - started >= LIMIT:
            late.add(pid)
            with suppress(ProcessLookupError):
                os.killpg(pid, signal.SIGTERM)


for signum in (signal.SIGINT, signal.SIGTERM):
    signal.signal(signum, stop)
signal.signal(signal.SIGALRM, overdue)
relay_folder = tempfile.TemporaryDirectory(prefix="razor-relay-")
relay = start_relay(relay_folder.name) if "RAZOR_RELAY_BIN" in os.environ else None
while running or files and not stopped:
    if (files and len(running) < 8 and not stopped
            and (not running or files[-1][1] == next(iter(running.values()))[1])):
        (file, repeat), output = files.pop(), tempfile.TemporaryFile()
        process = subprocess.Popen([sys.executable, "-u", file], stdin=subprocess.DEVNULL,
                                   stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        running[process.pid] = file, repeat, output, process, time.monotonic()
        if stopped:  # A signal during Popen must also stop the newly registered child.
            stop(stopped - 128, None)
        continue
    due = [started + LIMIT for pid, (*_, started) in running.items() if pid not in late]
    signal.setitimer(signal.ITIMER_REAL, max(min(due) - time.monotonic(), .001) if due else 0)
    pid, status = os.wait()
    file, repeat, output, process, started = running.pop(pid)
    # A flow stopped for running late fails, however it handled the signal.
    process.returncode = code = os.waitstatus_to_exitcode(status) or int(pid in late)
    output.seek(0)
    sys.stdout.buffer.write(output.read() + (f"{file} ran past {LIMIT} s and was stopped\n".encode() if pid in late else b"")
                            + (f"{file} exited {code}\n".encode() if code else b""))
    sys.stdout.flush()
    late.discard(pid)
    print(f"FLOW {file.name} repeat={repeat} seconds={time.monotonic() - started:.3f} exit={code}", flush=True)
    failed = failed or code != 0
if relay:
    relay.terminate()
sys.exit(stopped or failed)
