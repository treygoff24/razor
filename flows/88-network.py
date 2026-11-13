"""Pairing, admission, relay policy and connection closure through a local relay; no endpoint
under provider_only. A relay password in the address appears in nothing a person reads, and a
password mismatch and a refusing relay are named as such. Pairing-code expiry is not checked: its
monotonic timer has no flow clock."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import suppress
from claude import reply
from clock import wall_clock
import base64
import json
import os
import select
import signal
import socket
import subprocess
import sys
import time
from driver import BINARY, NETWORK_STOP_TIMEOUT, flow
from server import agents, gone, offer as add, pair, peer, post, send, start, stop, until, wait

if not (RELAY := os.environ.get("RAZOR_FLOW_RELAY")):
    sys.exit("Run network flows with scripts/flows.sh, which starts the relay and builds flow-peer.")
PEER = os.environ["FLOW_PEER_BIN"]
NO_PEER_STOP_TIMEOUT = 2  # An idle endpoint must stop without paying the drain timeout.
ALTERNATE = RELAY.replace("127.0.0.1", "localhost")  # the same relay under another name
PASSWORD = "flow-password"
OURS = f"{RELAY}/?token={PASSWORD}"  # the stand-in relay admits everyone and ignores it
SETTINGS = '[providers.local]\nbase_url = "{standin}"\nmodels = ["m"]\n'
network = lambda *relays: "[network]\nrelays = [" + ", ".join(f'"{r}"' for r in relays) + "]\n"


def ended(shown, sentence):
    out, err = shown.communicate(timeout=15)
    assert shown.returncode == 0 and sentence in out, (shown.returncode, out, err)


def join(f, code, name="a"):
    return f.run("devices", "join", name, input=code + "\n", timeout=30)


def refused(f, code, name="a"):
    done = join(f, code, name)
    assert done.returncode != 0 and "joined" not in done.stdout, done


def udp(pid):
    """The server's UDP sockets, where /proc shows them; None elsewhere. A host without IPv6 has no udp6 table."""
    tables = [path for path in (f"/proc/{pid}/net/udp", f"/proc/{pid}/net/udp6") if os.path.exists(path)]
    inodes = {row.split()[9] for table in tables for row in open(table).readlines()[1:]}
    def target(fd):
        try:
            return os.readlink(f"/proc/{pid}/fd/{fd}")
        except FileNotFoundError:  # closed since listdir
            return ""
    return {fd for fd in os.listdir(f"/proc/{pid}/fd")
            if target(fd).removeprefix("socket:[").removesuffix("]") in inodes} if tables else None


with wall_clock(still=False) as (_, env, advance), flow([], model="local/m", config=SETTINGS + network(OURS), env=env) as a, \
        flow([], model="local/m", config=SETTINGS + network(OURS), env=env) as b, \
        flow([], model="local/m", config="provider_only = true\n" + SETTINGS + network(OURS, ALTERNATE), env=env) as c:
    state = {name: f.home / ".razor" for name, f in (("a", a), ("b", b), ("c", c))}
    for f in (a, b, c):
        start(f, relay=f is not c)

    # provider_only opens no endpoint; turning it off opens one.
    pid = json.loads((state["c"] / "server.json").read_text())["pid"]
    assert not (state["c"] / "node.key").exists() and not udp(pid), "provider_only opened an endpoint"
    config = state["c"] / "config.toml"
    config.write_text(config.read_text().replace("provider_only = true", "provider_only = false"))
    agents(c)  # a read of the settings, which reopens the endpoint
    wait(lambda: (state["c"] / "node.key").exists() and udp(pid) != set(), 10, "endpoint never opened")
    relays = wait(lambda: (lines := json.loads(send(c, "GET", "/network").read()))
                  and any("(connected)" in line for line in lines) and lines, 10)
    assert len(relays) == 2, relays  # the network key and the one home relay iroh selected
    config.write_text(config.read_text().replace(f', "{ALTERNATE}"', ""))

    # A wrong code is refused; the right one still pairs after it, and a second use is refused.
    offer, code = add(a, "b")
    secret = bytearray(base64.urlsafe_b64decode(code[7:] + "==="))
    secret[40] ^= 1
    refused(b, "razor1-" + base64.urlsafe_b64encode(secret).decode().rstrip("="))
    done = join(b, code)
    assert done.returncode == 0, done
    refused(b, code, "again")  # b already names a server a, so another name reaches the code
    ended(offer, "b joined")

    # A replaced code is refused, as is a relay outside either server's list, and one whose password
    # differs is named as that; none spends the new code, which pairs b again, replacing its
    # enrollment by key.
    first, old = add(a, "b")
    offer, code = add(a, "b")
    ended(first, "replaced")
    refused(b, old)
    bconfig = state["b"] / "config.toml"
    original, shown = bconfig.read_text(), []
    for relays, said in ((ALTERNATE, f"Relay {RELAY}/ is outside"), (f'{OURS}", "{ALTERNATE}', f"Relay {ALTERNATE}/ is outside"),
                         (f"{RELAY}/?token=other", f"Relay {RELAY}/ has another password")):
        bconfig.write_text(original.replace(f'"{OURS}"', f'"{relays}"'))
        done = join(b, code)
        assert done.returncode != 0 and said in done.stderr and "token=" not in done.stderr, done
        shown.append(done.stderr)
    bconfig.write_text(original)
    assert join(b, code).returncode == 0
    ended(offer, "b joined")
    assert a.run("devices").stdout.split() == ["b"] and b.run("devices").stdout.split() == ["a"]
    # Doctor's network lines: this server's key, for a relay's list, its relay and its pair's path.
    me = peer("id", str(state["a"] / "node.key")).stdout.strip()
    doctor = wait(lambda: (d := json.loads(send(a, "GET", "/network").read()))
                  and len(d) > 2 and "round trip" in d[2] and d, 10)
    assert doctor[:2] == [f"Network key: {me}.", f"Home relay: {RELAY}/ (connected)."], doctor
    shown += [doctor, a.run("devices").stdout, *(send(f, "GET", "/devices").read().decode() for f in (a, b))]

    # The key pairing enrolled reaches the API; a key nobody enrolled gets no answer.
    stop(b)
    unreached = send(a, "GET", "/sessions?server=b&limit=1")
    assert unreached.status == 502 and b"b did not answer; nothing was sent." in unreached.read(), "an unreached server was reported as possibly having run the request"
    key = str(state["b"] / "node.key")
    reached = peer("get", key, me, RELAY, "/v1/device")
    assert reached.returncode == 0 and '"name":"b"' in reached.stdout, reached
    shown += [reached.stdout, *((state[name] / "server.log").read_text() for name in ("a", "b"))]
    assert not [s for s in map(str, shown) if PASSWORD in s], shown
    stranger = peer("get", str(a.home / "stranger.key"), me, RELAY, "/v1/device")
    assert stranger.returncode != 0 and not stranger.stdout, stranger

    # A server that stops tells the device holding a stream why, so no device waits on a dead
    # connection; removing the device ends its stream too, and its key is refused after.
    for end, why in ((lambda: (stop(a), start(a)), b"Server stopped"),
                     (lambda: a.run("devices", "remove", "b"), b"Device removed")):
        holding = subprocess.Popen([PEER, "hold", key, me, RELAY], stdout=subprocess.PIPE)
        heard = b""
        while b"READY" not in heard:
            assert select.select([holding.stdout], [], [], 15)[0], f"stream never opened: {heard}"
            heard += os.read(holding.stdout.fileno(), 65536)
        end()
        try:
            rest = holding.communicate(timeout=10)[0]
        except subprocess.TimeoutExpired:
            holding.kill()
            raise AssertionError(f"the stream stayed open, not told {why}")
        assert rest.splitlines()[-1].startswith(b"ENDED") and why in rest.splitlines()[-1], (why, rest)
    assert peer("get", key, me, RELAY, "/v1/device").returncode != 0

    # A relay that refuses this server's password is named as refusing it in doctor.
    with socket.socket() as taken:
        taken.bind(("127.0.0.1", 0))
        port = taken.getsockname()[1]
    guarded = c.home / "relay.toml"
    guarded.write_text(f'http_bind_addr = "127.0.0.1:{port}"\nenable_metrics = false\n[access]\nshared_token = ["right"]\n')
    guarding = subprocess.Popen([os.environ["RAZOR_RELAY_BIN"], "--dev", "--config-path", guarded],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        before = config.read_text()
        config.write_text(before.replace(OURS, f"http://127.0.0.1:{port}/?token=wrong"))
        refusal = f"Home relay: http://127.0.0.1:{port}/ (refused this server; check this server's access to it, including the password in network.relays in config.toml)."
        wait(lambda: refusal in json.loads(send(c, "GET", "/network").read()), 30, "doctor never named the refusal")
        showing, code = add(c, "not-connected")
        showing.send_signal(signal.SIGINT)
        out, err = showing.communicate(timeout=15)
        assert showing.returncode == 0 and refusal in err and "device elsewhere" in err, (out, err)
        config.write_text(before)
    finally:
        guarding.kill()

    # A relay that accepts TCP but never answers cannot hold the local API or pairing forever.
    with socket.socket() as silent:
        silent.bind(("127.0.0.1", 0))
        silent.listen()
        url = f"http://127.0.0.1:{silent.getsockname()[1]}/"
        config.write_text(before.replace(OURS, url))
        pending = json.loads(send(c, "GET", "/network").read())[1]
        assert pending == f"Home relays: {url} (not yet answered since start).", pending
        showing = subprocess.Popen([BINARY, "devices", "add", "silent"], env=c.env, cwd=c.cwd,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        assert not select.select([showing.stdout, showing.stderr], [], [], 0.2)[0], "code before the relay's first answer"
        advance(5_001, monotonic=True)
        assert next(line for line in showing.stdout if line.startswith("razor1-"))
        showing.send_signal(signal.SIGINT)
        out, err = showing.communicate(timeout=15)
        assert showing.returncode == 0 and pending in err and "device elsewhere" in err, (out, err)
        config.write_text(before)

    # Nor do the settings: `config get network` and a session's settings; for an address that does
    # not parse, a string where the list belongs, a time or a model that is wrong, doctor, `config
    # get`, a session's settings, the status stream's notice and the log, each notice naming the
    # key; nor a file that is not TOML, at `config set`.
    session = post(c, "/sessions", {"cwd": str(c.cwd)})["session"]
    settings = lambda: send(c, "GET", f"/sessions/{session}/settings").read().decode()
    got = c.run("config", "get", "network")
    assert got.returncode == 0 and f"{RELAY}/" in got.stdout, got
    shown, good, notices = [got.stdout, settings()], config.read_text(), []
    assert good.count(f'["{OURS}"]') == 1, good
    assert good.count('model = "local/m"') == 1 and good.count("provider_only = false") == 1, good
    resident = f'\n[residents.r88]\ncwd = "{c.cwd}"\nnight = "{PASSWORD}"\n'
    for key, broken in (("network.relays[0]", good.replace(RELAY, RELAY.rsplit(":", 1)[0] + ":bad")),
                        ("network.relays", good.replace(f'["{OURS}"]', f'"{OURS}"')),
                        ("residents.r88.night", good + resident),
                        ("model must", good.replace('model = "local/m"', f'model = "{PASSWORD}"')
                         .replace("provider_only = false", "provider_only = true"))):
        config.write_text(broken)
        notices.append(wait(lambda: (n := json.loads(send(c, "GET", "/status").readline())["config"])
                            not in notices and n, 10, "no new notice"))
        doctor, got = c.run("doctor"), c.run("config", "get", "network")
        shown += [notices[-1], doctor.stdout, doctor.stderr, got.stderr, settings(),
                  (state["c"] / "server.log").read_text()]
        assert key in notices[-1], (key, notices[-1])
    config.write_text(good.replace(f'{PASSWORD}"]', f'{PASSWORD}" x]'))  # no longer TOML
    edited = c.run("config", "set", "model", "local/m")
    assert edited.returncode != 0 and "does not parse" in edited.stderr, edited
    shown.append(edited.stderr)
    assert not [s for s in shown if PASSWORD in s], shown

# A delayed acknowledgement can inflate an unused QUIC path's drain timeout. Queue it before
# resuming the server, then leave the peer unable to answer its close. The Claude turn is live:
# the server, bridge and SDK must all leave, not just release the API socket.
with ThreadPoolExecutor(max_workers=1) as requests, wall_clock() as (_, env, advance), flow([reply((None, "mcp__razor__agent", {"action": "wait", "timeout_ms": 600_000}))], env=env) as a:
    b = a.home_at("b")
    pids = [start(f, relay=True) for f in (a, b)]
    began = time.monotonic()
    stop(a)
    assert time.monotonic() - began < NO_PEER_STOP_TIMEOUT, "a server with no peers waited to drain"
    pids[0] = start(a, relay=True)
    pair(a, b, "b", "a")
    sid = post(a, "/sessions", {"cwd": str(a.cwd)})["session"]
    post(a, f"/sessions/{sid}/input", {"text": "Keep working."})
    until(a, sid, lambda es: any(e["kind"] == "call" for e in es))
    try:
        os.kill(pids[1], signal.SIGSTOP)
        request = requests.submit(lambda: send(a, "GET", "/sessions?server=b&limit=1").read())
        time.sleep(0.1)  # Let the request reach the paused peer before moving the clock.
        os.kill(pids[0], signal.SIGSTOP)
        advance(10_000, monotonic=True)
        os.kill(pids[1], signal.SIGCONT)
        time.sleep(0.1)  # The acknowledgement is queued when the server resumes.
        os.kill(pids[0], signal.SIGCONT)
        request.result(timeout=20)  # Propagate a failed read as well as a timeout.
        send(a, "GET", "/network").read()
        os.kill(pids[1], signal.SIGSTOP)
        os.kill(pids[0], signal.SIGTERM)
        gone(pids[0], timeout=NETWORK_STOP_TIMEOUT)
    finally:
        for pid in pids:
            with suppress(ProcessLookupError):
                os.kill(pid, signal.SIGCONT)
