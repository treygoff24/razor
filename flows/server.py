"""The running server, through its socket, for flows that act between or across client calls."""
import base64
from contextlib import suppress
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import threading
import time

from driver import BINARY


class Local(http.client.HTTPConnection):
    """HTTP through the server's socket in the state directory."""

    def __init__(self, path):
        super().__init__("localhost", timeout=10)
        self.socket_path = str(path)

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.socket_path)


def start(f):
    """Starts the server through a real client call that sends no input; returns its pid."""
    started = f.run("devices")
    assert started.returncode == 0, started.stderr
    return json.loads((f.home / ".razor/server.json").read_text())["pid"]


def send(where, method, path, body=None, headers=None, key=None):
    """Sends `method` /v1<path> through the socket of the server whose state directory is
    `where` (or a flow's), with its local key unless `key` names another; where is instead the
    function `phone` or `through` returns, to send as that device. A body that is not bytes or
    text goes as JSON. Returns the response, unread; it holds the connection open."""
    if callable(where):
        connection, headers = where(), dict(headers or {})
    else:
        state = where if isinstance(where, Path) else Path(where.env["RAZOR_HOME"])
        connection = Local(state / "server.sock")
        headers = {"Authorization": f"Bearer {key or (state / 'device.key').read_text().strip()}", **(headers or {})}
    if body is not None and not isinstance(body, (bytes, str)):
        body, headers["Content-Type"] = json.dumps(body), "application/json"
    # A route that reads no body (stop) may answer and close before the body, a second write,
    # is sent; its answer is still read below.
    with suppress(BrokenPipeError):
        connection.request(method, "/v1" + path, body, headers)
    return connection.getresponse()


def post(where, path, body):
    """POSTs `body` as JSON; the JSON answer of a 200."""
    answer = send(where, "POST", path, body)
    text = answer.read()
    assert answer.status == 200, (path, answer.status, text)
    return json.loads(text)


def agents(where):
    """The agent rows of the status stream's first line, by session."""
    return {a["session"]: a for a in json.loads(send(where, "GET", "/status").readline())["agents"]}


def listed(where, session):
    """Whether a session appears after the once-a-second layout publication."""
    time.sleep(1.1)
    with send(where, "GET", "/status?views=true") as stream:
        layouts = next(json.loads(line)["layouts"] for line in iter(stream.readline, b"") if json.loads(line)["kind"] == "views")
    return any(row["session"] == session for layout in layouts for section in layout["sections"] for row in section["sessions"])


def until(f, session, done, timeout=30.0):
    """The record, once `done(entries)` holds, waiting for one the server has not yet made;
    fails with the record's tail at the deadline."""
    deadline, entries = time.monotonic() + timeout, []
    while True:
        with suppress(FileNotFoundError):
            if done(entries := list(f.record(session))):
                return entries
        assert time.monotonic() < deadline, f"timed out; record ends {entries[-3:]}"
        time.sleep(0.05)


def wait(condition, timeout=30.0, why="timed out"):
    """`condition()`'s first truthy value, polled every 50 ms; fails with `why` at the deadline."""
    deadline = time.monotonic() + timeout
    while not (found := condition()):
        assert time.monotonic() < deadline, why
        time.sleep(0.05)
    return found


def gone(pid, timeout=30.0):
    """Returns once the server's process group (the server, the bridge, the CLI) is empty."""
    deadline = time.monotonic() + timeout
    with suppress(ProcessLookupError):
        while time.monotonic() < deadline:
            os.killpg(pid, 0)
            time.sleep(0.05)
        raise AssertionError(f"server group {pid} still alive")


def stop(where, sig=signal.SIGTERM):
    """Sends `sig` to the server whose state directory is `where` (or a flow's); returns once it is gone."""
    pid = json.loads(((where if isinstance(where, Path) else where.home / ".razor") / "server.json").read_text())["pid"]
    os.kill(pid, sig)
    gone(pid)


PEER = os.environ.get("FLOW_PEER_BIN")
RELAY = os.environ.get("RAZOR_FLOW_RELAY")
_proxies = []  # held: a proxy stops when its standard input, this pipe, closes


def peer(*argv):
    """Runs flow-peer, the stand-in for a phone or another key; its finished process."""
    return subprocess.run([PEER, *argv], capture_output=True, text=True, timeout=30)


def node(f):
    """The network key of the server whose state is `f`'s."""
    return peer("id", str(Path(f.env["RAZOR_HOME"]) / "node.key")).stdout.strip()


def offer(f, name):
    """Starts `devices add name` on `f`; the process, once it has printed its code, and the code."""
    shown = subprocess.Popen([BINARY, "devices", "add", name], env=f.env, cwd=f.cwd, text=True,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    for line in shown.stdout:
        if line.startswith("razor1-"):
            return shown, line.strip()
    raise AssertionError(shown.stderr.read())


def joined(shown, name):
    """Waits for the offer to end with the other device joined as `name`."""
    out, err = shown.communicate(timeout=15)
    assert shown.returncode == 0 and f"{name} joined" in out, (shown.returncode, out, err)


def pair(a, b, a_calls_b, b_calls_a):
    """Pairs servers `a` and `b` by code: `a` names `b` a_calls_b, and `b` names `a` b_calls_a."""
    shown, code = offer(a, a_calls_b)
    done = b.run("devices", "join", b_calls_a, input=code + "\n", timeout=30)
    assert done.returncode == 0, done
    joined(shown, a_calls_b)


def device(f, name, server_id=None, keyfile=None):
    """Enrolls a key in `f` as `name`, a phone or with `server_id` a server; the file of the key,
    made new unless `keyfile` names one."""
    shown, code = offer(f, name)
    secret = base64.urlsafe_b64encode(base64.urlsafe_b64decode(code[7:] + "===")[33:49]).decode().rstrip("=")
    keyfile = keyfile or f.home / f"{name}.key"
    done = peer("pair", str(keyfile), node(f), RELAY, secret,
                *([server_id] if server_id else []))
    assert done.returncode == 0, done
    joined(shown, name)
    return keyfile


def through(f, keyfile):
    """A function opening an HTTP connection to a local proxy that sends each request to `f` as
    the key in `keyfile`; a key `f` has not enrolled gets a 502."""
    proxy = subprocess.Popen([PEER, "proxy", str(keyfile), node(f), RELAY],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    _proxies.append(proxy)
    port = int(proxy.stdout.readline())
    return lambda: http.client.HTTPConnection("127.0.0.1", port, timeout=10)


def phone(f, name):
    """Enrolls a phone as `name`; a function opening an HTTP connection to `f` as it, already
    connected, so a first request is not slow."""
    connect = through(f, device(f, name))
    assert send(connect, "GET", "/device").status == 200
    return connect


def stand(f, name, *lines):
    """Enrolls a key in `f` as the paired server `name` and serves it: its status stream says each
    line given, a text, and any more written to the returned process's standard input."""
    keyfile = device(f, name, "0" * 32)
    proxy = subprocess.Popen([PEER, "serve", str(keyfile), RELAY], stdin=subprocess.PIPE, text=True)
    _proxies.append(proxy)
    say(proxy, *lines)
    return proxy


def say(proxy, *lines):
    """Writes lines to a stand's status stream."""
    proxy.stdin.write("".join(line + "\n" for line in lines))
    proxy.stdin.flush()


class Tap:
    """A flow-peer standing between servers `a` and `b`, which have each enrolled it, `a` as
    `a_calls_b` and `b` as `b_calls_a`, so each takes it for the other. `report` and `detail`
    list what `a` has asked of `b` through it: a child's report and a session's facts. `to_a` and
    `to_b` open connections that send as the other server would."""

    def __init__(self, a, b, a_calls_b, b_calls_a):
        key = a.home / f"{a_calls_b}.key"
        for f, name, other in ((a, a_calls_b, b), (b, b_calls_a, a)):
            device(f, name, (Path(other.env["RAZOR_HOME"]) / "server.id").read_text().strip(), key)
        self.process = subprocess.Popen([PEER, "tap", str(key), RELAY, node(a), node(b)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True)
        _proxies.append(self.process)
        self.to_a, self.to_b = (self.connect(int(port)) for port in self.process.stdout.readline().split())
        self.report, self.detail = [], []
        threading.Thread(target=self.read, daemon=True).start()
        # Each server asks the other through it, once the stand-in is reachable at its relay.
        for f, name in ((a, a_calls_b), (b, b_calls_a)):
            wait(lambda: send(f, "GET", f"/sessions?server={name}&limit=1").status == 200, 30, "the stand-in never answered")

    @staticmethod
    def connect(port):
        return lambda: http.client.HTTPConnection("127.0.0.1", port, timeout=10)

    def read(self):
        for line in self.process.stdout:
            word, _, session = line.strip().partition(" ")
            (self.report if word == "report" else self.detail).append(session)

    def silence(self):
        """Makes it refuse new connections and swallow what open ones carry."""
        say(self.process, "silent")
