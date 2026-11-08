"""Discover real Razor servers through a Tailscale CLI stand-in, confirm pairing on both
machines without carrying a key, refuse forged requests, and recover a late network bind.
A terminal keeps the churn fixture live while fresh snapshots race its row updates."""
import http.client
import json
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from driver import flow
from server import post, send, start, stop, wait


def remote(address, method, path, body=None, source=None):
    connection = http.client.HTTPConnection(address, timeout=5, source_address=source)
    connection.request(method, "/v1" + path, json.dumps(body) if body is not None else None,
                       {"Content-Type": "application/json"})
    response = connection.getresponse()
    data = response.read()
    connection.close()
    return response.status, data


with flow([], config='[server]\nname = "alpha"\naddress = "127.0.0.1:0"') as f, \
        flow([], config='[server]\nname = ""\naddress = "127.0.0.1:0"') as beta:
    first = f.home / ".razor"
    second = beta.home / ".razor"
    start(f)
    assert beta.run("devices").returncode == 0
    a, b = (json.loads((p / "server.json").read_text())["address"] for p in (first, second))
    assert remote(b, "GET", "/pair") == (200, b'{"name":""}'), "public identity leaked data or needs auth"
    assert remote(b, "POST", "/pair", {"address": "127.0.0.2:7433", "key": "0" * 64})[0] == 400, "pair request accepted a different sender IP"
    assert remote(b, "GET", "/devices")[0] == 401, "discovery opened protected routes"
    assert remote(b, "POST", "/pair", {"address": a, "key": "a" * 1100})[0] == 413
    forged = "0" * 64
    assert remote(b, "POST", "/pair", {"address": a, "key": forged})[0] == 200
    assert remote(a, "GET", "/devices")[0] == 401
    assert send(first, "GET", "/devices", key=forged).status == 401, "unenrolled pushed key accepted"
    assert json.loads(send(second, "GET", "/devices").read()) == [], "push enrolled a device"
    # A held key the address does not accept must not become an enrollment key.
    assert beta.run("devices", "add", "forged", "--address", a).returncode == 0
    assert json.loads((second / "devices.json").read_text())[0]["key"] != forged, "unverified key enrolled"
    assert beta.run("devices", "remove", "forged").returncode == 0

    # A valid key must identify this server's own enrollment, not another client or server.
    for name, address in (("phone", None), ("elsewhere", "127.0.0.1:1")):
        enrollment = post(first, "/devices", {"name": name, "address": address})
        assert remote(b, "POST", "/pair", {"address": a, "key": enrollment["key"]})[0] == 200
        assert beta.run("devices", "add", "wrong", "--address", a).returncode == 0
        kept = json.loads((second / "devices.json").read_text())
        assert kept[0]["key"] != enrollment["key"], "key for a different enrollment became this server's key"
        assert beta.run("devices", "remove", "wrong").returncode == 0
        assert f.run("devices", "remove", name).returncode == 0

    # Static discovery routing must not shadow an ordinary device named discover.
    post(first, "/devices", {"name": "discover"})
    assert f.run("devices", "remove", "discover").returncode == 0
    assert remote(a, "GET", "/device")[0] == 401
    assert json.loads(send(first, "GET", "/devices").read()) == []

    # Doctor must not send a credential to a configured address, even with stale state.
    requests = []
    class Identity(BaseHTTPRequestHandler):
        def do_GET(self):
            requests.append((self.path, self.headers.get("Authorization"), self.headers.get("User-Agent")))
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"name":"another-server"}')

        def log_message(self, *_):
            pass

    settings = first / "config.toml"
    original = settings.read_text()
    with HTTPServer(("127.0.0.1", 0), Identity) as trap:
        worker = Thread(target=trap.serve_forever, daemon=True)
        worker.start()
        settings.write_text(original.replace("127.0.0.1:0", f"127.0.0.1:{trap.server_port}"))
        try:
            assert "not listening" in f.run("doctor").stdout, "doctor accepted a different identity"
        finally:
            settings.write_text(original)
            trap.shutdown()
            worker.join()
    # A program that probes every listening port here (Go-http-client's GET /) names itself;
    # doctor's request names none. No request may carry a credential.
    doctors = [(path, key) for path, key, agent in requests if agent is None]
    assert doctors == [("/v1/pair", None)] and not any(key for _, key, _ in requests), \
        f"doctor sent credentials, probed a protected route or none: {requests}"

    # Tailscale is other people's software; the discovered HTTP server is the real binary.
    peer_ip, peer_port = b.rsplit(":", 1)
    settings = first / "config.toml"
    settings.write_text(settings.read_text().replace('127.0.0.1:0', f'127.0.0.1:{peer_port}'))
    stop(f)
    # Take that port until after alpha starts: its Unix socket must still answer.
    tailscale = f.home / "tailscale"
    status = {"BackendState": "Running", "Peer": {"beta": {"Online": True, "TailscaleIPs": [peer_ip], "DNSName": "laptop.tailnet.example."}}}
    tailscale.write_text("#!/bin/sh\ncat <<'STATUS'\n" + json.dumps(status) + "\nSTATUS\n")
    tailscale.chmod(0o755)
    f.env["PATH"] = f"{f.home}:{f.env['PATH']}"
    start(f)
    assert json.loads((first / "server.json").read_text())["address"] is None, "occupied bind pretended to succeed"
    assert "not listening" in f.run("doctor").stdout
    # Listing never consults Tailscale; only find invokes it.
    tailscale.write_text(tailscale.read_text().replace("cat <<", f"touch {f.home}/scanned\ncat <<"))
    assert f.run("devices").returncode == 0
    assert not (f.home / "scanned").exists(), "plain device listing scanned the tailnet"
    discovered = f.run("devices", "find")
    assert discovered.returncode == 0 and f"razor devices add laptop --address {b}" in discovered.stdout, discovered
    stop(beta)
    wait(lambda: json.loads((first / "server.json").read_text())["address"], timeout=8, why="network bind never recovered")
    log = (first / "server.log").read_text()
    assert log.count("not listening on server.address") == 1 and "is now listening on server.address" in log, log
    stop(f)
    # Restore separate ports and start both; both people explicitly enroll, without --key.
    settings.write_text(settings.read_text().replace(f'127.0.0.1:{peer_port}', '127.0.0.1:0'))
    start(f)
    assert beta.run("devices").returncode == 0
    a, b = (json.loads((p / "server.json").read_text())["address"] for p in (first, second))
    # Clear beta's stale untrusted requests by its restart above.
    first_add = f.run("devices", "add", "beta", "--address", b)
    assert first_add.returncode == 0 and "--key" not in first_add.stdout, first_add
    assert json.loads(send(second, "GET", "/devices").read()) == [], "first add trusted both sides"
    # One other host must not evict alpha by varying its advertised port.
    for port in range(7400, 7416):
        assert remote(b, "POST", "/pair", {"address": f"127.0.0.2:{port}", "key": "0" * 64}, source=("127.0.0.2", 0))[0] == 200
    second_add = beta.run("devices", "add", "alpha", "--address", a)
    assert second_add.returncode == 0, second_add
    aa, bb = (json.loads((p / "devices.json").read_text()) for p in (first, second))
    assert aa[0]["key"] == bb[0]["key"], "second add did not verify and reuse the pushed key"
    caller = json.loads(send(first, "GET", "/device", key=bb[0]["key"]).read())
    assert caller["address"] == b and caller.get("key") is None and "push" not in caller, caller
    assert send(first, "GET", "/devices", key=bb[0]["key"]).status == 200
    assert send(second, "GET", "/devices", key=aa[0]["key"]).status == 200
    # Every status stream opens with the agents snapshot, even while a paired row keeps changing.
    remote_session = post(second, "/sessions", {"cwd": str(beta.cwd)})["session"]
    watched = send(second, "GET", f"/sessions/{remote_session}/events?terminal=true")
    assert json.loads(watched.readline())["kind"] == "attached"
    wait(lambda: remote_session in send(first, "GET", "/status").readline().decode(), why="peer row")
    churn = Thread(target=lambda: [send(second, "PUT", f"/sessions/{remote_session}/seen?entry={n % 2 * 99}").read() for n in range(3000)])
    churn.start()
    while churn.is_alive():
        assert json.loads(send(first, "GET", "/status").readline())["kind"] == "agents"
    watched.close()

    stop(f)
    settings.write_text(settings.read_text().replace("127.0.0.1:0", "invalid-address"))
    invalid = f.run("serve", timeout=5)
    assert invalid.returncode != 0 and "server.address must be a host:port" in invalid.stderr, invalid
