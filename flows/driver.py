"""Run the Razor build RAZOR_BIN names in a scratch home against the model stand-in, and say PASS
or FAIL."""

from collections.abc import Iterator
from contextlib import contextmanager, suppress
from dataclasses import dataclass
from pathlib import Path
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import traceback

from standin import Reply, serve

REPLIES = Path(__file__).with_name("replies")
# Never `razor` from PATH: that may be an installed copy, which runs the person's real
# server and sign-in, not this stand-in.
if not os.environ.get("RAZOR_BIN"):
    sys.exit("Set RAZOR_BIN to the razor binary under test; flows never take one from PATH.")
BINARY = os.path.abspath(os.environ["RAZOR_BIN"])
# A GPT sign-in whose access token, {"exp": 4102444800}, never needs a refresh.
AUTH = {"tokens": {"access_token": "flow.eyJleHAiOjQxMDI0NDQ4MDB9.flow", "account_id": "flow-account"}}


@dataclass
class Flow:
    """One flow's scratch home and working directory, the environment Razor runs in, and the
    folder where the stand-in spools every model request."""

    home: Path
    cwd: Path
    requests: Path
    env: dict[str, str]

    def run(self, *argv: str, input: str | None = None,
            timeout: float = 60) -> subprocess.CompletedProcess[str]:
        """Runs the binary in `cwd` with the flow's environment; its exit status and text."""
        return subprocess.run([BINARY, *argv], input=input, timeout=timeout, capture_output=True,
                              text=True, cwd=self.cwd, env=self.env)

    def ask(self, *argv: str, timeout: float = 60) -> dict:
        """Runs `razor --print=json` with `argv`, failing on a nonzero exit; its JSON answer."""
        done = self.run("--print=json", *argv, timeout=timeout)
        assert done.returncode == 0, done.stderr
        return json.loads(done.stdout)

    def git(self, *args: str) -> str:
        """Runs git in `cwd`, failing on a nonzero exit; returns its standard output."""
        return subprocess.run(["git", *args], cwd=self.cwd, check=True,
                              capture_output=True, text=True).stdout

    def base(self, *files: str):
        """Initializes main with a base file and any named files in one commit."""
        self.git("init", "-b", "main")
        (self.cwd / "base").write_text("base")
        self.git("add", "--", "base", *files)
        self.git("-c", "user.name=Flow", "-c", "user.email=flow@example.invalid", "commit", "-m", "Base")

    def sent(self) -> list[str]:
        """The body of each model request (each POST) the stand-in has had, oldest first."""
        return [path.read_text() for path in sorted(self.requests.glob("[0-9]*.body"))]

    def request(self, n: int) -> dict:
        """Model request `n`, counted from 1, as JSON."""
        return json.loads((self.requests / f"{n:03}.body").read_bytes())

    def record(self, session: str) -> Iterator[dict]:
        """The session's record entries in the flow's RAZOR_HOME as written so far; a line still
        being written is left out."""
        path = Path(self.env["RAZOR_HOME"]) / "sessions" / session / "record.jsonl"
        return map(json.loads, path.read_bytes().split(b"\n")[:-1])


def _processes(groups):
    """The recorded groups' current members, by pid and command, on either platform."""
    found = {}
    for line in subprocess.check_output(["ps", "-axo", "pid=,pgid=,args="], text=True).splitlines():
        pid, group, command = line.split(None, 2)
        if int(group) in groups:
            found[int(pid)] = command[:120]
    return found


def _stop(home):
    """Stops the servers alone, as their service does (a signalled CLI may start one more request),
    then live jobs' groups, whose end could wake a turn; kills and names any alive 10 s later.
    Every server whose state is under `home` counts, a flow's second server included."""
    server = {json.loads(info.read_text())["pid"] for info in home.rglob("server.json")}
    jobs = set()
    for record in home.rglob("sessions/*/record.jsonl"):
        live = {}
        for entry in map(json.loads, record.read_bytes().split(b"\n")[:-1]):
            if entry["kind"] == "job":
                if entry["event"] == "started":
                    live[entry["job"]] = entry["pid"]
                else:
                    live.pop(entry["job"], None)
        jobs.update(live.values())
    deadline = time.monotonic() + 10
    for groups, kill in ((server, os.kill), (jobs, os.killpg)):
        for pid in groups:
            with suppress(ProcessLookupError):
                kill(pid, signal.SIGTERM)
        while _processes(groups) and time.monotonic() < deadline:
            time.sleep(0.05)
    if left := _processes(server | jobs):
        for pid in server | jobs:
            with suppress(ProcessLookupError):
                os.killpg(pid, signal.SIGKILL)
    return left


@contextmanager
def flow(replies: list[Reply], *, model: str = "opus", config: str = "",
         env: dict[str, str] | None = None) -> Iterator[Flow]:
    """Yields a Flow whose model requests the stand-in answers from `replies`, with `model` and
    `config` in its config file, `{standin}` in it standing for the stand-in's URL. Then stops
    the server: a process of the flow's alive 10 s later fails it. Prints PASS and the script's
    name, or FAIL, where the first failure was raised (each flows file and line from the script
    inward), its message as written and the scratch folder, which is kept, and exits 1."""
    scratch = Path(tempfile.mkdtemp(prefix="razor-flow-"))
    home, cwd = scratch / "home", scratch / "cwd"
    (home / ".razor").mkdir(parents=True)
    (home / "tmp").mkdir()
    quiet = scratch / "quiet"
    quiet.mkdir()
    for name in ("osascript", "notify-send"):
        program = quiet / name
        program.write_text("#!/bin/sh\nexit 0\n")
        program.chmod(0o755)
    cwd.mkdir()
    name = Path(sys.argv[0]).stem
    previous = signal.signal(signal.SIGTERM, lambda *_: sys.exit(1))
    # The server outlives the CLI that starts it, so a stopper in its own session holds the one
    # end of a pipe and stops the server unless this flow says it already has: a flow killed
    # outright, as a stopped job's group is a second after TERM, leaves nothing running.
    stopper = subprocess.Popen([sys.executable, __file__, home], stdin=subprocess.PIPE, start_new_session=True)
    try:
        with serve(replies, scratch / "requests") as url:
            (home / ".razor/config.toml").write_text(
                f"skill_updates = false\nmodel = {json.dumps(model)}\n{config.replace('{standin}', url)}\n[responses]\nbase_url = {json.dumps(url)}\n")
            (home / ".razor/auth.json").write_text(json.dumps(AUTH))
            try:
                yield Flow(home, cwd, scratch / "requests", {
                    "HOME": str(home), "TMPDIR": str(home / "tmp"),
                    "LANG": os.environ.get("LANG", "C.UTF-8"), "RAZOR_HOME": str(home / ".razor"),
                    "ANTHROPIC_BASE_URL": url, "ANTHROPIC_API_KEY": "flow-dummy-key", **(env or {}),
                    "PATH": f"{quiet}:{(env or {}).get('PATH', os.environ['PATH'])}"})
            finally:
                left = _stop(home)
                stopper.communicate(b"stopped")
            assert not left, f"processes outlived the server: {sorted(left.items())}"
    except Exception as error:
        # The first frame is this generator's; a failure of its own has no other.
        frames = [f for f in traceback.extract_tb(error.__traceback__)
                  if Path(f.filename).resolve().parent == REPLIES.resolve().parent]
        where = " › ".join(f"{Path(f.filename).name}:{f.lineno}" for f in frames[1:] or frames)
        message = str(error) or type(error).__name__
        print(f"FAIL {name} at {where}: {message} (scratch {scratch})", flush=True)
        raise SystemExit(1) from None
    finally:
        signal.signal(signal.SIGTERM, previous)
    shutil.rmtree(scratch)
    print(f"PASS {name}", flush=True)


if __name__ == "__main__" and not sys.stdin.buffer.read():
    _stop(Path(sys.argv[1]))
