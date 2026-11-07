"""Fresh doctor reports missing dependencies and fixes, prints no credential, writes no Razor
files, and exits 1; with credentials it exits 0; a configured Claude sign-in command is
what it checks. A silent server cannot hang it (docs/design/done.md). Help and refused commands
touch no state (docs/design/setup.md). The server runs the cua-driver shipped beside it as its
daemon (docs/design/starter.md)."""
import json
import os
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path
from driver import AUTH, BINARY, flow

with flow([]) as f:
    fresh, home = f.home.parent / "fresh", f.home.parent / "fresh/home"
    home.mkdir(parents=True)
    (fresh / "bin").mkdir()
    shutil.copy(shutil.which(BINARY), fresh / "bin/razor")
    env = {"HOME": str(home), "RAZOR_HOME": str(home / ".razor"), "PATH": str(fresh / "bin")}

    def doctor(**more):
        """Its exit, each line's report by what it checks, and everything it printed."""
        run = subprocess.run([str(fresh / "bin/razor"), "doctor"], env={**env, **more},
                             capture_output=True, text=True, timeout=30)
        lines = (line.split(": ", 1) for line in run.stdout.splitlines() if ": " in line)
        return run.returncode, dict(lines), run.stdout + run.stderr

    # No settings, credentials or server are needed for any command's usage.
    top = subprocess.run([BINARY, "--help"], env=env, capture_output=True, text=True, timeout=5)
    commands = list(dict.fromkeys(line.split()[0] for line in top.stdout.split("Commands:\n", 1)[1].splitlines()
                                 if line.startswith("  ") and not line.startswith("   ")))
    for command in commands:
        for flag in ("--help", "-h"):
            helped = subprocess.run([BINARY, command, flag], env=env, capture_output=True, text=True, timeout=5)
            assert helped.returncode == 0 and helped.stdout.startswith(f"Usage: razor {command}"), helped
            syntax = helped.stdout.splitlines()[0].removeprefix("Usage: razor ")
            assert syntax in [line.strip() for line in top.stdout.splitlines()], helped.stdout
        spoken = subprocess.run([BINARY, "help", command], env=env, capture_output=True, text=True, timeout=5)
        assert spoken.stdout == helped.stdout, spoken
    launched = subprocess.run([BINARY, "--model", "opus", "--help"], env=env, capture_output=True, text=True, timeout=5)
    assert launched.returncode == 0 and launched.stdout == top.stdout, launched
    for command in ("nonesuch", "task", "memory", "agent", "message"):
        for argv in ((command, "--help"), (command, "-h"), (command,), (command, "extra"), ("help", command)):
            refused = subprocess.run([BINARY, *argv], env=env, capture_output=True, text=True, timeout=5)
            assert refused.returncode == 2 and all(c in refused.stderr for c in commands), refused
            if command != "nonesuch":
                assert "tool" in refused.stderr and "session" in refused.stderr, refused
    assert not (home / ".razor").exists(), "help or usage wrote state"

    code, found, text = doctor()
    assert code == 1 and "cannot run on its default model, opus" in text, text
    keys = ("EXA_API_KEY", "FIRECRAWL_API_KEY", "PARALLEL_API_KEY", "ELEVENLABS_API_KEY")
    for thing in ("Claude", "ChatGPT sign-in", *keys):
        assert found[thing].startswith("missing. ") and len(found[thing]) > 20, (thing, text)
    assert all("razor keys" in found[thing] for thing in keys), text
    assert "razor login" in found["ChatGPT sign-in"], text
    assert "Node" in found["Claude"] and str(fresh / "bin") in found["Claude"] and not (home / ".razor").exists(), text
    assert "agent-browser install" in found["Browser"] and found["Server"].startswith("stopped;"), text
    assert found["Desktop"].startswith("not running. ") and "razor restart" in found["Desktop"], text
    assert found["Service"].endswith("Run razor install.") and "Memory" not in found, text

    def untouched(*words, code=0):
        """A command that ends before it reads or makes any state: its exit, stdout and stderr."""
        run = subprocess.run([str(fresh / "bin/razor"), *words], env=env, capture_output=True,
                             text=True, timeout=30)
        assert run.returncode == code and not (home / ".razor").exists(), (words, run)
        return run.stdout, run.stderr

    for words in (("mcp", "bogus"), ("mcp", "add", "x"), ("mcp", "remove"),
                  ("mcp", "list", "--scope", "nowhere"), ("mcp", "add", "x", "--config", "{bad"),
                  ("devices", "bogus"), ("devices", "add"), ("devices", "add", "phone", "--address")):
        assert untouched(*words, code=2)[1].endswith("Run razor --help.\n"), words
    untouched("stop")  # Already stopped is success, and stopping makes nothing.

    stub = fresh / "bin/agent-browser"
    stub.write_text("#!/bin/sh\nexit 0\n")
    stub.chmod(0o755)
    shutil.copy2(stub, fresh / "bin/cua-driver")

    planted = ["sk-ant-oat01-flow-planted-claude", "flow-planted-openai-key", "flow-planted-exa-key"]
    (home / ".claude").mkdir(exist_ok=True)
    (home / ".claude/.credentials.json").write_text(json.dumps({"claudeAiOauth": {
        "accessToken": planted[0], "refreshToken": planted[0], "expiresAt": 4102444800000,
        "scopes": ["user:inference"]}}))
    (home / ".razor").mkdir()
    (home / ".razor/auth.json").write_text(json.dumps(AUTH))
    (home / ".razor/key").write_text(planted[1])  # A key file from before keys.toml.
    node = Path(shutil.which("node")).parent
    code, found, text = doctor(PATH=f"{fresh / 'bin'}:{node}", EXA_API_KEY=planted[2], OPENAI_API_KEY=planted[1])
    assert code == 0 and "Razor can run on opus." in text, text
    assert f"Key file: {home}/.razor/key is no longer read; save its key as OPENAI_API_KEY in keys.toml, name the model openai/<id>, then delete the file." in text, text
    for thing in ("Claude", "ChatGPT sign-in", "EXA_API_KEY", "Browser", "Desktop"):
        assert found[thing].startswith(("found.", "found (")), (thing, text)
    assert found["FIRECRAWL_API_KEY"].startswith("missing. ")
    leaked = [value for value in planted + [AUTH["tokens"]["access_token"]] if value in text]
    assert not leaked, f"doctor printed {leaked}"

    (home / ".claude/.credentials.json").unlink()
    (home / ".razor/config.toml").write_text(f'[claude]\nsign_in = ["/bin/sh", "{fresh}/token"]\n')
    for script, said in (("exit 3", "cannot run on its default model, opus"), ("echo flow-planted-sign-in", "Razor can run on opus.")):
        (fresh / "token").write_text(script)
        code, found, text = doctor(PATH=f"{fresh / 'bin'}:{node}")
        assert said in text and "flow-planted-sign-in" not in text, text
        if script == "exit 3":
            assert "/bin/sh failed: exit status: 3" in found["Claude"], text

    # A server that accepts but never answers: doctor asks once, gives up and ends.
    stalled = socket.socket(socket.AF_UNIX)
    stalled.bind(str(home / ".razor/server.sock"))
    stalled.listen()
    (home / ".razor/device.key").write_text("flow-local-key")
    code, found, text = doctor(PATH=f"{fresh / 'bin'}:{node}")
    assert code == 0 and found.get("Mac permissions", "unknown").startswith("unknown"), text

    # With a display, the server runs the cua-driver shipped beside it as its own child on the
    # home's socket, without Cua's telemetry and update check or any key of Razor's, and holds its
    # standard input open; the flow's end fails on a process that outlives the server. Doctor's
    # probe runs it without telemetry or update check too. On macOS Razor's Accessibility is the
    # gate instead of a display.
    if sys.platform == "linux":
        shipped = f.home.parent / "shipped"
        shipped.mkdir()
        shutil.copy(BINARY, shipped / "razor")
        (shipped / "cua-driver").write_text('#!/bin/sh\nenv > "$0.$1.env"\n[ "$1" = serve ] || exit 0\necho $$ "$@" > "$0.args"\nexec cat\n')
        (shipped / "cua-driver").chmod(0o755)
        assert subprocess.run([shipped / "razor", "devices"], env={**f.env, "DISPLAY": ":0", "EXA_API_KEY": planted[2]},
                              capture_output=True, timeout=30).returncode == 0
        args, deadline = shipped / "cua-driver.args", time.monotonic() + 10
        while not args.exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        pid, *args = args.read_text().split()
        state = f.home / ".razor"
        assert args == ["serve", "--embedded", "--parent-liveness-stdio", "--socket",
                        f"{state}/cua-driver.sock", "--pid-file", f"{state}/cua-driver.pid"], args
        made = lambda word: dict(line.split("=", 1) for line in (shipped / f"cua-driver.{word}.env").read_text().splitlines() if "=" in line)
        want = {"CUA_DRIVER_EMBEDDED": "1", "CUA_DRIVER_RS_TELEMETRY_ENABLED": "false", "CUA_DRIVER_RS_UPDATE_CHECK": "false"}
        served = made("serve")
        assert {name: served.get(name) for name in want} == want and not {"EXA_API_KEY", "ANTHROPIC_API_KEY"} & served.keys(), served
        subprocess.run([shipped / "razor", "doctor"], env={**f.env, "PATH": f"{shipped}:/usr/bin:/bin"}, capture_output=True, timeout=30)
        assert {name: made("--socket").get(name) for name in want} == want, made("--socket")
        os.kill(int(pid), 0)  # Still reading after doctor's run: the server holds the pipe open.
