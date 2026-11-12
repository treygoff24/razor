"""A home that ran 0.5.0 updates and starts: its settings with `[server] address` and a `[remote]`
holding an address and key, and its devices.json with a server paired by address and a phone's
key, token and notification settings, load unchanged; doctor asks for the address's removal and
session commands refuse the remote. Pairing by code under the old names upgrades each entry where
it is, keeping its settings and leaving its old key and address behind, and an entry with network
keys never answers to a shared key; a name that holds network keys is still refused to another
key. A terminal keeps the churn fixture live while fresh
snapshots race its row updates."""
import base64
import json
from threading import Thread
from driver import flow
from server import RELAY, device, node, offer, pair, peer, post, send, start, stop, wait

OLD = '[server]\nname = "alpha"\naddress = "desk.tailnet.example:7433"\n'
REMOTE = '[remote]\naddress = "laptop.tailnet.example:7433"\nkey = "~/.razor/laptop.key"\n'


def paired(name, n, address=None, **more):
    """An entry as 0.5.0 wrote it: a shared key, and a server's address."""
    return {"name": name, "added": n, "key": f"{n:064x}", "address": address, **more}


with flow([], config=OLD + REMOTE) as f, flow([], config='[server]\nname = ""\n') as beta:
    first, second = f.home / ".razor", beta.home / ".razor"
    phone = paired("phone", 3, push={"token": "cc33", "environment": "development"},
                   notifications={"level": "off", "muted": []})
    written = {first: [paired("beta", 1, "127.0.0.1:7434"), phone], second: [paired("alpha", 2, "127.0.0.1:7433")]}
    for home, entries in written.items():
        home.mkdir(parents=True, exist_ok=True)
        (home / "devices.json").write_text(json.dumps(entries))
    start(f)
    assert beta.run("devices").returncode == 0
    assert f.run("devices").stdout == "beta\nphone\n"
    assert [json.loads((h / "devices.json").read_text()) for h in written] == list(written.values()), "loading rewrote devices.json"
    assert send(first, "GET", "/devices", key=phone["key"]).status == 200, "a keyed entry lost its key on the socket"
    assert "server.address is no longer used; remove address under [server]" in f.run("doctor").stdout
    refused = f.run("--print", "Hello.")
    assert refused.returncode != 0 and "[remote] names a paired server" in refused.stderr, refused
    assert "server.address" not in (first / "server.log").read_text()

    # Pairing by code under each old name upgrades its entry whole but for the old key and
    # address; the phone keeps its token and settings. Another key cannot take a paired name.
    pair(f, beta, "beta", "alpha")
    device(f, "phone")
    entries = lambda: [json.loads((h / "devices.json").read_text()) for h in written]
    for old, new in zip(written.values(), entries()):
        for before, after in zip(old, new):
            assert after.pop("keys") and after.pop("relays"), after
            assert (after.pop("server_id", None) is None) == ("push" in before), after
            kept = {**{k: v for k, v in before.items() if k != "address"}, "key": None}
            assert after == {**kept, **({"push": {"service": "apns", **before["push"]}} if "push" in before else {})}, (before, after)
    upgraded = entries()
    shown, code = offer(f, "beta")
    secret = base64.urlsafe_b64encode(base64.urlsafe_b64decode(code[7:] + "===")[33:49]).decode().rstrip("=")
    other = peer("pair", str(f.home / "other.key"), node(f), RELAY, secret)
    assert other.returncode != 0, other
    shown.kill()
    assert entries() == upgraded, "a second key took a paired name"
    # An entry an earlier build upgraded kept its shared key; it no longer opens the socket.
    stop(f)
    held = upgraded[0]
    next(d for d in held if d["name"] == "beta")["key"] = written[first][0]["key"]
    (first / "devices.json").write_text(json.dumps(held))
    start(f)
    assert send(first, "GET", "/devices", key=written[first][0]["key"]).status == 401, "a paired server's old key opened the socket"

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
