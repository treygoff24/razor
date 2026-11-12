"""Mail in a tree and between roots (docs/design/mail.md), each part in a folder of its own in one
home: a child's note, question and long mention reach the parent under a header naming the
sender; a parent's note joins its working child at the next tool boundary; `list` shows the other
root by name and project and not the caller; mail by name arrives under `<name> in <project>`, a
reply by project comes back, a name that is also two roots' project reaches the root of that name,
a project two roots share that is no session's name is refused with both named, and after a
restart a new root takes no name a root of this week holds. A closed row is read directly outside
the live feed."""
import json
from dataclasses import replace
from driver import flow
from model import agent, message, reply, tool
import server
from server import until


def texts(value):
    """Every string in a request body: the words the model was sent."""
    if isinstance(value, dict):
        return [t for v in value.values() for t in texts(v)]
    if isinstance(value, list):
        return [t for v in value for t in texts(v)]
    return [value] if isinstance(value, str) else []


def answered(text):
    def done(entries):
        ids = [e["id"] for e in entries if e["kind"] == "input" and e["text"] == text]
        return ids and any(e["kind"] == "turn_end" and ids[0] in e["inputs"] for e in entries)
    return done


root, child = "FLOW_ROOT_42", "You are wren42,"
replies = [tool(root, "status"), reply(root, text="Named.")]
with flow(replies) as f:
    def part(name):
        (f.cwd / name).mkdir()
        return replace(f, cwd=f.cwd / name)

    def run(where, *words):
        return where.ask(*words)["session"]

    def output(session, n=-1):
        entries = list(f.record(session))
        return json.loads([e for e in entries if e["kind"] == "result"][n]["output"])

    # Mail in a tree reaches the model under a header naming its sender: a child's note and
    # question, and a long mention cut to 2,000 characters; the record keeps the sender's words
    # alone, and the channel file holds the whole post.
    tree = part("tree42")
    parent = run(tree, root)
    status = next(e for e in f.record(parent) if e["kind"] == "result")
    name = json.loads(status["output"])["agents"][0]["name"]
    assert name == tree.cwd.name, name
    long = f"@{name} " + "y" * 10_000 + " TAIL_42"
    # So each mail opens a turn of its own, the child sends the next only once the flow has seen
    # the parent's turn on the last one end.
    gates = [{"command": f"while [ ! -f {tree.cwd / gate} ]; do sleep 0.05; done"} for gate in ("note", "question", "mention")]
    replies += [agent(root, "start", name="wren42", kind="researcher", brief="Write to your parent."),
                reply(root, text="Started."),
                reply(child, "Bash", gates[0]), message(child, "note", text="NOTE_42"),
                reply(child, "Bash", gates[1]), message(child, "question", text="Which marker?"),
                reply(child, "Bash", gates[2]), message(child, "note", to="channel", text=long),
                reply(child, text="Posted."),
                reply("NOTE_42", text="Noted."), reply("Which marker?", text="Asked."),
                reply("(the rest: message read)", text="Mentioned.")]
    tree.ask("--resume", parent, "Start the child.")
    for gate, mail in (("note", "NOTE_42"), ("question", "Which marker?"), ("mention", long)):
        (tree.cwd / gate).touch()
        entries = until(f, parent, answered(mail))
    inputs = {e["text"]: e for e in entries if e["kind"] == "input"}
    note, question, mention = (inputs[text]["from"]["mail"] for text in ("NOTE_42", "Which marker?", long))
    sid = note["id"][0]
    assert (note["kind"], note["from"]) == ("note", "wren42"), note
    assert (question["kind"], question["from"]) == ("question", "wren42"), question
    assert (mention["kind"], mention["from"]) == ("mention", "wren42"), mention
    seen = ["\n".join(texts(json.loads(body))) for body in f.sent() if root in body]
    shown = ["wren42 (your child): NOTE_42",
             f'wren42 (your child) asks ["{sid}",{question["id"][1]}]: Which marker?',
             "wren42 in the channel: " + long[:2000] + " ... (the rest: message read)"]
    for words in shown:
        assert any(words in text for text in seen), f"no request held {words[:60]!r}"
    assert not any("TAIL_42" in text for text in seen), "the model read past the mention's 2,000 characters"
    channel = (f.home / ".razor/channels" / f"{parent}.jsonl").read_text()
    assert json.loads(channel.splitlines()[-1])["text"] == long, "the channel file lost the post's end"

    # A parent's mail reaches its working child at the child's next tool boundary, as the person's
    # words do (Trey, 2026-10-29): the child's one turn answers its brief and the note together.
    # The child's command runs until the flow has sent the note.
    fern = part("steer42")
    root, child = "FLOW_ROOT_B42", "You are fern42,"
    replies += [agent(root, "start", name="fern42", brief="Work, then reply."), reply(root, text="Started."),
                reply(child, "Bash", {"command": f"while [ ! -f {fern.cwd / 'sent'} ]; do sleep 0.05; done"}),
                reply("(your parent): STEER_42", text="Steered."),
                message("SEND_42", "note", to="fern42", text="STEER_42"), reply("SEND_42", text="Sent."),
                reply(root, text="Heard.")]
    parent = run(fern, root)
    sid = next(p.parent.name for p in (f.home / ".razor/sessions").glob("*/record.jsonl")
               if (es := list(f.record(p.parent.name)))[0].get("child", {}).get("name") == "fern42")
    until(f, sid, lambda es: any(e["kind"] == "job" for e in es))
    fern.ask("--resume", parent, "SEND_42")
    (fern.cwd / "sent").touch()
    entries = until(f, sid, answered("STEER_42"))
    ends = [e for e in entries if e["kind"] == "turn_end"]
    assert len(ends) == 1 and len(ends[0]["inputs"]) == 2, ends
    until(f, parent, lambda es: es[-1]["kind"] == "turn_end" and any(e.get("text") == "Steered." for e in es))

    # Mail between roots. `list` is read for this part's roots: the parts before left roots too.
    a, b, c, g1, g2 = "FLOW_A43", "FLOW_B43", "FLOW_C43", "FLOW_G1_43", "FLOW_G2_43"
    replies += [tool(a, "status"), reply(a, text="Named."),
                message(b, "list"), reply(b, text="Listed.")]
    alpha, beta = part("alpha43"), part("beta43")
    before = {p.parent.name for p in (f.home / ".razor/sessions").glob("*/record.jsonl")}
    first = run(alpha, a)
    name = output(first)["agents"][0]["name"]
    second = run(beta, b)
    listed = output(second)
    rows = [r for r in listed["agents"] if r["to"] not in before]
    assert listed["silent"] == [] and [r["to"] for r in rows] == [first], listed
    assert (rows[0]["name"], rows[0]["project"]) == (name, "alpha43"), listed
    replies += [message(b, "note", to=name, text="GPU_43"), reply(b, text="Sent."),
                message("GPU_43", "note", to="beta43", text="ACK_43"), reply(a, text="Acked."),
                reply("ACK_43", text="Thanks.")]
    run(beta, "--resume", second, "Send it.")
    assert output(second) == {"id": [second, output(second)["id"][1]], "to": first}, output(second)
    sent = [e for e in f.record(second) if e["kind"] == "mail_sent"]
    assert [(e["to"], e["mail_kind"]) for e in sent] == [(first, "note")], sent
    gpu = next(e for e in until(f, first, answered("GPU_43")) if e.get("text") == "GPU_43")
    ack = next(e for e in until(f, second, answered("ACK_43")) if e.get("text") == "ACK_43")
    other = gpu["from"]["mail"]["from"].removesuffix(" in beta43")
    assert other == "beta43" and other != name and gpu["from"]["mail"]["kind"] == "note", gpu
    assert ack["from"]["mail"]["from"] == f"{name} in alpha43", ack
    requests = f.sent()
    for words in (f"{other} in beta43: GPU_43", f"{name} in alpha43: ACK_43"):
        assert any(words in r for r in requests), f"no request held {words!r}"
    replies += [reply(c, text="Here.")]
    third = run(alpha, c)
    # alpha43 is the first root's name and both alpha roots' project: the name wins, where the
    # send was refused as ambiguous before names were stored.
    replies += [message(b, "note", to="alpha43", text="EXACT_43"), reply(b, text="Sent."),
                reply("EXACT_43", text="Got it.")]
    run(beta, "--resume", second, "Send to the name.")
    until(f, first, answered("EXACT_43"))
    assert not any(e.get("text") == "EXACT_43" for e in f.record(third)), "a project match beat a name"
    gamma = part("gamma.43")  # its roots are gamma-43 and gamma-43-2
    replies += [reply(g1, text="Here."), reply(g2, text="Here.")]
    fourth, fifth = run(gamma, g1), run(gamma, g2)
    replies += [message(b, "note", to="gamma.43", text="AMBIGUOUS_43"), reply(b, text="Refused.")]
    run(beta, "--resume", second, "Send to the project.")
    refused = [e for e in f.record(second) if e["kind"] == "result"][-1]
    assert refused["outcome"] == "error" and "names 2 agents" in refused["output"], refused
    assert fourth in refused["output"] and fifth in refused["output"], refused
    assert not any(e.get("text") == "AMBIGUOUS_43" for s in (fourth, fifth) for e in f.record(s)), "a refused send was admitted"
    # A root the person closed reopens only for the person: mail to it is refused (Trey, 2026-11-01).
    server.post(f, f"/sessions/{third}/quit", {})
    closed = until(f, third, lambda es: es[-1].get("closed"))
    assert closed[-1]["close_reason"] == "person" and json.load(server.send(f, "GET", f"/sessions/{third}/agent"))["close_reason"] == "person", closed[-1]
    replies += [message(b, "note", to=third, text="CLOSED_43"), reply(b, text="Refused.")]
    run(beta, "--resume", second, "Send to the closed root.")
    refused = [e for e in f.record(second) if e["kind"] == "result"][-1]
    assert refused["outcome"] == "error" and ("ended" in refused["output"] or "closed" in refused["output"]), refused
    assert not any(e.get("text") == "CLOSED_43" for e in f.record(third)), "mail reopened a closed root"
    server.stop(f)
    replies += [reply("RESTART_43", text="Here.")]
    start = next(f.record(run(alpha, "RESTART_43")))
    assert start["name"] == "alpha43-3", start

    # Open messaging (Trey, 2026-11-01): a child sees past the 100 newest agents with a query, its
    # own tree marked; it reaches a child of another tree by id; and a name its sibling shares with
    # that child is refused, naming both.
    r1, r2, mine_twin, other_twin, scout = "FLOW_R1_43", "FLOW_R2_43", "SIB_CHILD_43", "OTHER_CHILD_43", "You are scout43,"
    hold, wait = ({"command": f"while [ ! -f {go} ]; do sleep 0.05; done"} for go in ("go43", "done43"))
    replies += [agent(r1, "start", name="twin43", brief=mine_twin, placement="in_place"),
                agent(r1, "start", name="scout43", brief="Look around.", placement="in_place"),
                reply(r1, text="Started."), agent(r2, "start", name="twin43", brief=other_twin, placement="in_place"),
                reply(r2, text="Started."), reply(mine_twin, "Bash", wait), reply(other_twin, "Bash", wait),
                reply(scout, "Bash", hold)]
    open43 = part("open43")
    first, second = run(open43, r1), run(open43, r2)
    starts = {p.parent.name: next(f.record(p.parent.name)) for p in (f.home / ".razor/sessions").glob("*/record.jsonl")}
    kids = {(s["child"]["name"], s["child"]["parent"]): sid for sid, s in starts.items() if "child" in s}
    mine, theirs, me = kids["twin43", first], kids["twin43", second], kids["scout43", first]
    for sid in (mine, theirs):  # each busy until done43, so neither reports
        until(f, sid, lambda es: any(e["kind"] == "job" for e in es))
    for _ in range(101):  # newer than every child, so the newest 100 hold no child
        server.post(f, "/sessions", {"cwd": str(open43.cwd)})
    replies += [message(scout, "list"), message(scout, "list", query="TWIN43"),
                message(scout, "note", to=mine, text="SIB_43"), message(scout, "note", to=theirs, text="TWIN_43"),
                message(scout, "note", to="twin43", text="BOTH_43"), reply(scout, text="Sent."),
                reply(mine_twin, text="Here."), reply(other_twin, text="Here."), reply("SIB_43", text="Got it."),
                reply("TWIN_43", text="Got it."), reply(r1, text="Heard."), reply(r2, text="Heard.")]
    (open43.cwd / "go43").touch()
    until(f, me, lambda es: es[-1]["kind"] == "turn_end" and sum(e["kind"] == "result" for e in es) == 6)
    everyone, found, *sent = (output(me, n) for n in range(1, 5))
    refused = [e for e in f.record(me) if e["kind"] == "result"][5]["output"]
    assert len(everyone["agents"]) == 100 and everyone["more"] > 0 and theirs not in str(everyone), everyone
    assert {(r["to"], r["tree"]) for r in found["agents"]} == {(mine, True), (theirs, False)}, found
    assert [r["to"] for r in sent] == [mine, theirs], sent
    assert "names 2 agents" in refused and mine in refused and theirs in refused, refused
    (open43.cwd / "done43").touch()
    for sid, text in ((mine, "SIB_43"), (theirs, "TWIN_43")):
        entries = until(f, sid, answered(text))
        end = next(e for e in entries if e["kind"] == "turn_end")
        assert next(e for e in entries if e.get("text") == text)["id"] not in end["inputs"], "mail joined a sibling's turn"
    assert any("scout43 in open43: SIB_43" in body for body in f.sent()), "no request held the note"
    assert not any(e.get("text") == "BOTH_43" for s in (mine, theirs) for e in f.record(s)), "a refused send was admitted"
    for sid in (first, second):
        until(f, sid, lambda es: es[-1]["kind"] == "turn_end" and any("report" in e.get("from", "") for e in es))

    # An idle root outside the live feed remains reachable by its exact id after eviction.
    replies += [reply("WAKE_43", text="awake")]
    sid = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    sender = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.stop(f)
    path = f.home / ".razor/sessions" / sid / "record.jsonl"
    start = next(f.record(sid))
    start["at"] -= 13 * 3600_000
    path.write_text(json.dumps(start) + "\n")
    server.start(f)
    for _ in range(2):
        assert not server.listed(f, sid)
    answer = server.send(f, "POST", "/mail", {"to": sid, "id": [sender, 1], "kind": "note",
        "from": "flow43", "project": "mail", "text": "WAKE_43"})
    assert answer.status == 200, answer.read()
    until(f, sid, lambda es: any(e.get("text") == "awake" for e in es))
