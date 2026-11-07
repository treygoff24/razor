"""Setup and keys in a real terminal: hidden keys saved at 0600 and kept on Enter, a rerun changing nothing,
serve loading them, and --console reaching Claude Code's own sign-in. Provider auth files, the Claude Code
CLI and the browser installer stand in for other people's software (docs/design/setup.md)."""
import json
import os
import select
import re
import socket
import shutil
import subprocess
import time
import tomllib
from driver import AUTH, BINARY, Reply, flow
from model import reply
from standin import serve


def terminal(f, argv, answers, binary=BINARY):
    master, slave = os.openpty()
    process = subprocess.Popen([binary, *argv], stdin=slave, stdout=slave, stderr=slave,
                               env=f.env, cwd=f.cwd, start_new_session=True)
    os.close(slave)
    output, cursor = b"", 0
    try:
        for prompt, answer in answers:
            deadline = time.monotonic() + 30
            while prompt.encode() not in output[cursor:]:
                assert time.monotonic() < deadline and process.poll() is None, f"{argv} did not reach {prompt}: {output!r}"
                if select.select([master], [], [], .1)[0]:
                    try:
                        output += os.read(master, 65536)
                    except OSError:  # The program has ended; the check above says what it printed.
                        time.sleep(.05)
            cursor = len(output)
            os.write(master, (answer + "\r").encode())
        deadline = time.monotonic() + 30
        while process.poll() is None:
            assert time.monotonic() < deadline, f"{argv} did not finish"
            if select.select([master], [], [], .1)[0]:
                try:
                    output += os.read(master, 65536)
                except OSError:
                    break
        assert process.wait(timeout=5) == 0, output
        return output
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)


with flow([reply(None, "mcp__razor__search", {"query": "setup saved key"}), reply(None)]) as f:
    browser = f.home / "agent-browser"
    browser.write_text("#!/bin/sh\nexit 0\n")
    browser.chmod(0o755)
    f.env["PATH"] = f"{f.home}:{f.env['PATH']}"
    secret = "flow-setup-exa-hidden"
    keys = [("EXA_API_KEY:", secret), ("FIRECRAWL_API_KEY:", ""),
            ("PARALLEL_API_KEY:", ""), ("ELEVENLABS_API_KEY:", "")]
    ending = [("background when you log in", ""), ("Open Razor for your first message", "")]
    saved, person, you = (f.home / ".razor" / name for name in ("keys.toml", "config.toml", "about.md"))
    before = tomllib.loads(person.read_text())
    tailscale = f.home / "tailscale"
    tailscale.write_text("#!/bin/sh\necho '{\"BackendState\":\"Running\",\"Self\":{\"TailscaleIPs\":[\"127.0.0.1\"]},\"Peer\":{}}'\n")
    tailscale.chmod(0o755)
    terminal(f, ["setup"], [("Use Claude, ChatGPT, both, or an API key", "chatgpt"), ("other machines", "y"), *ending])
    settings = tomllib.loads(person.read_text())
    server = settings.pop("server")
    assert not saved.exists() and not you.exists() and settings == before, "setup changed unrelated settings"
    expected = re.sub("-+", "-", re.sub("[^a-z0-9]", "-", socket.gethostname().split(".")[0].lower()))[:32].strip("-")
    assert server["name"] == expected and "label" not in server, server
    host, port = server["address"].rsplit(":", 1)
    assert host == "127.0.0.1" and 7433 <= int(port) <= 7442, server
    person.write_text(person.read_text().replace(f'name = "{server["name"]}"', 'name = "chosen59"'))
    before = person.read_bytes()
    # Keys are typed hidden, saved together at 0600, and Enter keeps a saved one.
    screen = terminal(f, ["keys"], keys)
    assert secret.encode() not in screen, "keys echoed the typed key"
    assert tomllib.loads(saved.read_text()).get("EXA_API_KEY") == secret, "keys did not save the key"
    assert saved.stat().st_mode & 0o777 == 0o600, "keys.toml is not owner-only"
    kept = saved.read_bytes()
    terminal(f, ["keys"], [(p, "") for p, _ in keys])
    assert saved.read_bytes() == kept and saved.stat().st_mode & 0o777 == 0o600, "Enter did not keep the saved key"
    terminal(f, ["setup"], [("Use Claude, ChatGPT, both, or an API key", ""), ("other machines", ""), *ending])
    assert [p.read_bytes() for p in (saved, person)] == [kept, before], "rerunning setup overwrote saved files"
    # A newly spawned serve must read keys.toml itself. No key is in the client's environment.
    with serve([Reply(b'{"results":[{"title":"Found","url":"https://example.invalid","text":"saved key works"}]}')],
               f.home / "web-requests") as url:
        with person.open("a") as config:
            config.write(f'\n[web.exa]\nbase_url = "{url}"\n')
        result = f.run("--model", "opus", "--print=json", "Search with the saved key.")
        assert result.returncode == 0, result.stderr
        sent = json.loads((f.home / "web-requests/001.json").read_text())
        assert sent["headers"].get("x-api-key") == secret, "serve did not load the saved key"
    # Claude Code's CLI is signed in. Plain setup leaves it; --console runs its console sign-in.
    bundle = f.home / "bundle"
    cli = bundle / "bridge/node_modules/@anthropic-ai/claude-agent-sdk-flow/claude"
    cli.parent.mkdir(parents=True)
    (bundle / "bridge/main.ts").touch()
    cli.write_text('#!/bin/sh\necho "$*" >> "$HOME/claude-calls"\n'
                   '[ "$1 $2" = "auth status" ] && echo \'{"loggedIn":true,"authMethod":"flow"}\'\nexit 0\n')
    cli.chmod(0o755)
    shutil.copy2(BINARY, bundle / "razor")
    calls = f.home / "claude-calls"
    for option in ([], ["--console"]):
        terminal(f, ["setup", *option], [("Use Claude, ChatGPT, both, or an API key", ""), ("other machines", ""), *ending], bundle / "razor")
        logins = [l for l in (calls.read_text() if calls.exists() else "").splitlines() if l.startswith("auth login")]
        assert logins == (["auth login --console"] if option else []), (option, logins)
    saved.write_text('EXA_API_KEY = "flow-nul-prefix\\u0000flow-nul-suffix"\n')
    refused = f.run("doctor")
    assert refused.returncode != 0 and "flow-nul-" not in refused.stdout + refused.stderr, "NUL key was accepted or printed"

# A gateway-only home can finish setup and doctor without Claude or ChatGPT credentials.
chat = b'data: {"choices":[{"index":0,"delta":{"content":"OK"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'
with flow([Reply(chat), Reply(b'{"error":{"message":"bad key"}}', status=401)]) as f:
    (f.home / ".razor/auth.json").unlink()
    f.env.pop("ANTHROPIC_API_KEY")
    browser = f.home / "agent-browser"
    browser.write_text('#!/bin/sh\n[ "$1" != install ] || touch "$HOME/browser-installed"\n')
    browser.chmod(0o755)
    f.env["PATH"] = f"{f.home}:{f.env['PATH']}"
    config = f.home / ".razor/config.toml"
    url = tomllib.loads(config.read_text())["responses"]["base_url"]
    secret = "flow-gateway-hidden"
    screen = terminal(f, ["setup"], [("Use Claude, ChatGPT, both, or an API key", "key"),
        ("Whose key", "other"), ("Its API", "chat"), ("Its address", url), ("The model id", "gateway59"),
        ("Its context window", ""), ("LOCAL_API_KEY", secret), ("Contact nothing but", "y"), *ending])
    settings = tomllib.loads(config.read_text())
    assert settings["provider_only"] and settings["model"] == "local/gateway59", settings
    keys = f.home / ".razor/keys.toml"
    assert tomllib.loads(keys.read_text())["LOCAL_API_KEY"] == secret and keys.stat().st_mode & 0o777 == 0o600
    assert secret.encode() not in screen and b"Connect to your Razor servers" not in screen, screen
    assert not (f.home / "browser-installed").exists(), "provider_only installed a browser"
    sent = json.loads((f.requests / "001.body").read_bytes())
    meta = json.loads((f.requests / "001.json").read_text())
    assert sent["model"] == "gateway59" and meta["headers"]["authorization"] == "Bearer " + secret, (sent, meta)
    result = f.run("doctor")
    assert result.returncode == 1 and secret not in result.stdout + result.stderr, result
    (f.home / ".razor/auth.json").write_text(json.dumps(AUTH))
    f.env["ANTHROPIC_API_KEY"] = "flow-native-key"
    for model in ("sol", "opus"):
        refused = f.run("--model", model, "--print", "Use this sign-in.")
        assert refused.returncode != 0 and "provider_only" in refused.stderr, refused
    assert len(list(f.requests.glob("[0-9]*.body"))) == 2, "a refused native route contacted its host"
