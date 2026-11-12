"""Mail between two paired servers (docs/design/mail.md): they pair by code over iroh; `list` shows the other server's agents and `<name>@<server>` is admitted there with
`@<server>` in its sender; a send to that server stopped, or to one that is stopped and so never completes a
connection (an asleep machine's), fails saying nothing was sent, nothing is recorded as sent, and `list` names
both servers as silent. Mail from the paired server whose sender id is the recipient's parent's
is heard without the parent's header and is not among the parent's inputs at a rebuild."""
import json
import os
import shutil
import signal
import time
from contextlib import suppress
from dataclasses import replace
from http.client import IncompleteRead
from driver import flow
from model import agent, message, reply, tool
import server
from server import send, start, stop, until, wait
from terminal import Terminal

a, b, kid = "FLOW_A44", "FLOW_B44", "You are kid44,"
replies = [tool(b, "status"), agent(b, "start", name="kid44", brief="Wait.", placement="in_place"),
           reply(b, text="Named."), message(kid, "question", text="KIDQ_44"), reply(kid, text="Waiting."),
           reply("(your child) asks", text="Noted."), reply("FORGED_44", text="Forged."),
           tool("Your context is nearly full", "handover", note="Wait."),
           reply("The context was rebuilt", text="Rebuilt.")]
# Ctrl-S opens the new-agent chooser here, so words can stand in the draft behind it.
keys = '[ui.keys.global]\nnew = ["Ctrl-S"]\n[ui.keys.composer]\nstash = []\n'
with flow(replies, config=keys) as f:
    other = f.home / ".razor-b"
    other.mkdir()
    for name in ("config.toml", "auth.json"):
        shutil.copy(f.home / ".razor" / name, other / name)
    config = (other / "config.toml").read_text() + '[server]\nlabel = "far44"\n'
    (other / "config.toml").write_text(config + '[providers.opv44]\nbase_url = "http://127.0.0.1:1"\nmodels = ["opv44"]\n')
    alpha = replace(f, cwd=f.cwd / "alpha44")
    beta = replace(f, cwd=f.cwd / "beta44", env={**f.env, "RAZOR_HOME": str(other)})
    alpha.cwd.mkdir(), beta.cwd.mkdir()
    start(f)
    server.pair(alpha, beta, "b", "a")

    def results(record):
        return [e for e in record if e["kind"] == "result"]

    def request(path, body=None, via=None, *, home=other, method="POST", status=200, headers=None):
        answer = send(via or home, method, path, body, headers)
        raw = answer.read()
        assert answer.status == status, (path, answer.status, raw)
        return json.loads(raw) if raw else None

    there = beta.ask(b)["session"]
    until(beta, there, lambda es: any(e.get("text") == "Noted." for e in es))  # the child's question

    # A forwarded terminal stays attached past the peer request's 30-second header deadline: it is
    # opened here, the steps below run while it stays attached, and then every frame it drew is read.
    with Terminal(alpha, "--resume", f"{there}@b") as far:
        far.shown("Noted.")
        attached = time.monotonic()

        there_name = json.loads(results(beta.record(there))[0]["output"])["agents"][0]["name"]
        replies += [message(a, "list"), message(a, "note", to=f"{there_name}@b", text="HELLO_44"),
                    reply(a, text="Sent."), reply("HELLO_44", text="Heard.")]
        here = alpha.ask(a)["session"]
        record = list(f.record(here))
        listed = json.loads(results(record)[0]["output"])
        assert f"{there}@b" in [r["to"] for r in listed["agents"]] and listed["silent"] == [], listed
        assert json.loads(results(record)[1]["output"])["to"] == f"{there}@b", results(record)[1]
        hello = [e for e in beta.record(there) if e["kind"] == "input" and e["text"] == "HELLO_44"]
        assert len(hello) == 1 and hello[0]["from"]["mail"]["from"].endswith("@a in alpha44"), hello
        assert hello[0]["from"]["mail"]["id"][0] == f"{here}@a", hello
        heard = lambda entries: any(e["kind"] == "turn_end" and hello[0]["id"] in e["inputs"] for e in entries)
        until(beta, there, heard)

        kid_id = next(e["session"] for e in beta.record(there) if e["kind"] == "child")
        # A paired server that is not alpha, enrolled on both: its calls stand in for a peer's.
        forger = server.device(beta, "forger", "f" * 32)
        server.device(alpha, "forger", "f" * 32)
        to_beta, to_alpha = server.through(beta, forger), server.through(alpha, forger)
        request("/mail", {"to": kid_id, "id": [there, 1], "kind": "note", "from": "forger", "project": "alpha44",
                      "text": "FORGED_44"}, via=to_beta)
        until(beta, kid_id, lambda es: any(e.get("text") == "Forged." for e in es))
        forged = [body for body in f.sent() if "FORGED_44" in body]
        assert forged and not any("(your parent): FORGED_44" in body for body in forged), "paired mail heard as the parent's"
        request(f"/sessions/{kid_id}/handover", {})
        rebuilt = until(beta, kid_id, lambda es: any(e.get("text") == "Rebuilt." for e in es))
        context = next(e["context"] for e in rebuilt if e["kind"] == "rebuild")
        assert "FORGED_44" in context, context
        assert "FORGED_44" not in context.split("## Calls since")[0], "a rebuild took paired mail for the parent's input"

        # Forwarded session input is still the person, with the authenticated peer as its origin.
        remote = f"/sessions/{there}@b"
        local = f.home / ".razor"
        replies.append(reply("FORWARD_44", text="Forwarded."))
        body = {"text": "FORWARD_44", "request": "forward-44"}
        entry = request(remote + "/input", body, home=local)
        assert request(remote + "/input", body, home=local) == entry
        forwarded = until(beta, there, lambda es: any(e.get("text") == "Forwarded." for e in es))
        inputs = [e for e in forwarded if e["kind"] == "input" and e["text"] == "FORWARD_44"]
        assert len(inputs) == 1 and (inputs[0]["from"], inputs[0]["device"]) == ("person", "a"), inputs
        assert inputs[0]["id"] == entry["entry"], (inputs, entry)

        # The terminal lists b once A's reader of it has started.
        wait(lambda: "b" in [s["name"] for s in json.loads(send(local, "GET", "/status").readline())["servers"]], why="b was never listed")
        # Ctrl-S lists every server's projects, taking only its own clicks, and keeps the chosen one while
        # b is slow to answer; Escape goes back, past a second choice; a click aims at b's project, where
        # the first words start the person's agent; /model on a session here lists this server's models.
        before, b_pid = set(os.listdir(other / "sessions")), json.loads((other / "server.json").read_text())["pid"]
        replies.append(reply("FIRST_44", text="Started."))
        with Terminal(alpha, "--resume", here) as t:
            t.shown("Sent."), t.send("OLD_44\x13"), t.shown("new agent ·"), t.click("Send ⏎"), t.send("\x1b")
            t.until(lambda t: "new agent ·" not in t.text()), t.send("\x15"), os.kill(b_pid, signal.SIGSTOP)
            try:
                t.send("\x13"), t.shown("› alpha44")
            finally:
                os.kill(b_pid, signal.SIGCONT)
            t.shown("far44"), t.send("\r"), t.shown("New agent in alpha44 on"), t.send("\x13"), t.shown("far44")
            t.send("\r"), t.until(lambda t: "new agent ·" not in t.text())
            t.send("\x1b"), t.until(lambda t: "New agent in" not in t.text() and "Sent." in t.text())
            t.send("\x13"), t.shown("far44"), t.click("far44"), t.shown("New agent in beta44 on far44")
            t.send("FIRST_44\r"), t.shown("Started."), t.click("alpha44"), t.send("/model\r"), t.shown("model · ")
            assert "opv44" not in t.text(), t.text()
            t.send("\x1b"), t.until(lambda t: "model · " not in t.text())
        assert not any(e.get("text") == "OLD_44" for e in f.record(here)), "a click behind the chooser sent"
        (new,) = set(os.listdir(other / "sessions")) - before
        begun = next(beta.record(new))
        assert begun["cwd"] == str(beta.cwd) and begun.get("child") is None, begun

        # A git folder can hold projects; a hidden manager is not one, and the same origin reached
        # over SSH and HTTPS is one chooser row with both servers.
        ventures = f.home / "Ventures"
        one, two, hidden = ventures / "picker-shared", ventures / "group" / "picker-shared", f.home / ".pyenv"
        for folder in (ventures, one, two, hidden):
            folder.mkdir(parents=True, exist_ok=True)
            replace(f, cwd=folder).git("init", "-q")
        for folder, origin in ((one, "git@github.com:Example/picker-shared.git"),
                               (two, "https://github.com/example/picker-shared/")):
            replace(f, cwd=folder).git("remote", "add", "origin", origin)
        places = request("/repos?under=" + str(f.home), home=local, method="GET")["repos"]
        by_path = {r["path"]: r for r in places}
        assert str(one) in by_path and str(two) in by_path and str(hidden) not in by_path, places
        assert by_path[str(one)]["identity"] == by_path[str(two)]["identity"], places
        label = json.loads(send(local, "GET", "/status").readline())["server"]["label"]
        with Terminal(replace(f, cwd=ventures), "--resume", here) as t:
            t.shown("Sent."), t.send("\x13"), t.shown("new agent ·")
            t.until(lambda t: any("picker-shared" in line and "far44" in line and label in line
                                  for line in t.text().splitlines()))
            assert sum("picker-shared" in line for line in t.text().splitlines()) == 1, t.text()
            t.send("\x1b"), t.until(lambda t: "new agent ·" not in t.text())

        f.cwd.chmod(0o111)  # an unreadable folder holding the past folders is skipped, not refused
        listed = request("/repos", home=local, method="GET")["repos"]
        f.cwd.chmod(0o755)
        assert str(alpha.cwd) in [r["path"] for r in listed], listed
        made = request("/sessions?server=b", {"cwd": str(beta.cwd), "request": "start-44"}, home=local)
        assert request("/sessions?server=b", {"cwd": str(beta.cwd), "request": "start-44"}, home=local) == made
        assert set(os.listdir(other / "sessions")) - before == {new, made["session"]}, made

        # None of these harmless requests may cross the session forwarding boundary.
        request(remote + "/devices", home=local, method="GET", status=404)
        for path in ("missing/./file", "missing/../file", "missing/%2e/file", "missing/%2E%2e/file",
                     "missing/a%2fb", "missing/a%2Fb", "missing/a%5cb", "missing/a%5Cb", "missing/a\\b"):
            request(remote + "/files/" + path, home=local, method="GET", status=400)
        request(remote + "/draft", via=to_alpha, method="GET", status=400)
        assert alpha.run("devices", "remove", "forger").returncode == 0  # a peer that never answers would be silent
        request(remote + "/draft", home=local, method="GET", status=400, headers={"x-razor-forwarded": "1"})

        # Search finds a word's start, accents folded, each conversation's newest hit, newest first,
        # here and on b; `recall` searches and reads b's entry whole, its `native` too.
        said = next(e for e in beta.record(there) if e.get("text") == "Heard.")
        replies += [tool("RECALL_44", "recall", query="hel"),
                    reply("RECALL_44", "mcp__razor__recall", {"session": f"{there}@b", "id": said["id"]}),
                    reply("RECALL_44", text="Recalled.")]
        alpha.ask("--resume", here, "RECALL_44 hello Próspera")
        found, entry = [json.loads(r["output"]) for r in results(f.record(here))[-2:]]
        assert [h["session"] for h in found["hits"]] == [here, f"{there}@b"] and entry == said and said["native"], (found, entry)
        search = lambda q: request("/search?q=" + q, home=local, method="GET")
        hits = [(h["session"], h["entry"]) for h in search("44")["hits"]]
        assert [s for s, _ in hits] == [here, f"{new}@b", f"{there}@b"] and hits[2][1] == inputs[0]["id"], hits
        assert [h["session"] for h in search("prosp")["hits"]] == [here] and not search("ello")["hits"]
        # The rest of the 32 s, and at least a second to read the frames drawn meanwhile.
        assert not far.pump(max(attached + 32 - time.monotonic(), 1), lambda t: "reconnecting" in t.text().lower()), far.text()
        far.shown("Forwarded.")

    # Revoking either the caller or the peer ends an already attached forwarded stream.
    def revoked(where, name, server="b"):
        answer = send(where, "GET", f"/sessions/{there}@{server}/events?after={list(beta.record(there))[-1]['id']}")
        assert answer.status == 200 and json.loads(answer.readline())["kind"] == "attached"
        request("/devices/" + name, home=local, method="DELETE")
        with suppress(IncompleteRead):
            assert answer.read() == b"", "the revoked device's forwarded stream stayed open"

    revoked(server.phone(alpha, "phone"), "phone")
    revoked(alpha, "b")
    server.pair(alpha, beta, "b", "a")

    # A third server is stopped, as an asleep machine is: its connections never complete.
    third = replace(f, env={**f.env, "RAZOR_HOME": str(f.home / ".razor-c")})
    (f.home / ".razor-c").mkdir()
    shutil.copy(other / "config.toml", f.home / ".razor-c" / "config.toml")
    server.pair(alpha, third, "asleep", "a")
    stop(other)
    sleeper = json.loads((f.home / ".razor-c/server.json").read_text())["pid"]
    os.kill(sleeper, signal.SIGSTOP)
    try:
        began, found = time.monotonic(), search("hel")
        assert found["partial"] and [h["session"] for h in found["hits"]] == [here] and time.monotonic() - began < 4, found
        replies += [message(a, "note", to=f"{there_name}@b", text="LOST_44"),
                    message(a, "note", to="anyone@asleep", text="ASLEEP_44"), message(a, "list"),
                    reply(a, text="Down.")]
        alpha.ask("--resume", here, "Again.")
        record = list(f.record(here))
        lost, asleep, silent = results(record)[-3:]
        for name, result in (("b", lost), ("asleep", asleep)):
            assert result["outcome"] == "error" and f"{name} did not answer; nothing was sent." in result["output"], result
        assert json.loads(silent["output"])["silent"] == ["b", "asleep"], silent
        assert len([e for e in record if e["kind"] == "mail_sent"]) == 1, "a failed send was recorded as sent"
    finally:
        os.kill(sleeper, signal.SIGCONT)
