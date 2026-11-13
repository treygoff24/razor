"""Steering reaches its child and gets a visible reply before new work, on both model routes."""
from dataclasses import replace
import json
from claude import reply as claude_reply
from driver import Reply, flow
from model import agent, reply
from responses import PLAIN, call, message, response
import server
from terminal import Terminal

ROOT = "TERMINAL_STEER_ROOT"
replies = [agent(ROOT, "start", name="oak", brief="Read the brief and reply."),
           agent(ROOT, "start", name="elm", brief="Read the brief and reply."),
           reply(ROOT, text="parent-visible"),
           reply("You are oak,", "Bash", {"command": "sleep 120", "run_in_background": True}),
           reply("You are oak,", text="oak-visible"),
           reply("You are elm,", "Bash", {"command": "sleep 120", "run_in_background": True}),
           reply("You are elm,", text="elm-visible"), reply("You are oak,", text="oak-steered"),
           agent(ROOT, "stop", child="oak"), agent(ROOT, "stop", child="elm"), reply(ROOT)]
with flow(replies) as f:
    f.base()
    server.start(f)
    parent = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    server.post(f, f"/sessions/{parent}/input", {"text": ROOT})
    server.until(f, parent, lambda es: es[-1]["kind"] == "turn_end")
    with Terminal(f, "--resume", parent) as t:
        t.shown("parent-visible", "oak-visible", "elm-visible")
        children = {es[0]["child"]["name"]: path.parent.name
                    for path in (f.home / ".razor/sessions").glob("*/record.jsonl")
                    if (es := list(f.record(path.parent.name)))[0].get("child")}
        assert set(children) == {"oak", "elm"}, children
        for sid in children.values():
            jobs = [e for e in f.record(sid) if e["kind"] == "job"]
            assert len(jobs) == 1 and jobs[0]["event"] == "started", jobs
        before = {s: [e for e in f.record(s) if e["kind"] == "input"]
                  for s in [parent, *children.values()]}
        t.click("oak-visible")
        t.shown("talking to oak", "oak-visible", "as you")
        assert "parent-visible" in "\n".join(line[:60] for line in t.screen.display), t.text()
        t.send("Steer only oak.\r")
        t.shown("oak-steered")
        after = {s: [e for e in f.record(s) if e["kind"] == "input"] for s in before}
        assert after[parent] == before[parent] and after[children["elm"]] == before[children["elm"]]
        added = after[children["oak"]][len(before[children["oak"]]):]
        assert len(added) == 1 and added[0]["from"] == "person" and added[0]["text"] == "Steer only oak.", added
        t.send(b"\x1b")
        t.until(lambda t: "talking to oak" not in t.text() and "parent-visible" in t.text())
    f.ask("--resume", parent, "Stop both children's pending jobs.")


for model in ("opus", "sol"):
    parent_marker, child_marker = "MAIL_SOURCE36", "You are courier36,"
    def step(*calls, text=None, match=child_marker):
        if model == "opus":
            return claude_reply(*[(id, "Bash" if name == "bash" else "mcp__razor__" + name, args)
                                  for id, name, args in calls], text=text, match=match)
        items = [message(text)] if text is not None else []
        return replace(response(items + [call(name, json.dumps(args), id) for id, name, args in calls]), match=match)

    def shell(id, command):
        return id, "bash", {"command": command}

    def held(n):
        return f"printf ready > ready{n}; while [ ! -e continue{n} ]; do sleep 0.05; done; printf slept"

    denied = [shell("blocked_a", "printf 'a\\n' >> effects"), shell("blocked_b", "printf 'b\\n' >> effects")]
    mail = "Continue the mail check36."
    replies = [step(("child_start", "agent", {"action": "start", "name": "courier36", "brief": "Wait for my note.",
                                            "placement": "in_place"}), match=parent_marker),
               step(text="child started36", match=parent_marker), step(shell("mail_sleep", held(1))),
               step(("send_note", "message", {"type": "note", "to": "courier36", "text": mail}), match=parent_marker),
               step(text="note sent36", match=parent_marker), step(shell("mail_effect", "printf mail > mail; " + held(2))),
               step(*denied), step(shell("one_refusal", "printf once > once")),
               step(*[(id + "_retry", name, args) for id, name, args in denied], text="I heard your correction36."),
               step(shell("handover_sleep", held(3))),
               step(shell("before_handover", "printf wrong > handover_effect"),
                    ("handover_call", "handover", {"note": "Continue the person's correction."})),
               step(("finish", "agent", {"action": "finish", "status": "finished", "report": "Steering checked."}), text="rebuilt36"),
               step(text="report received36", match=parent_marker)]
    with flow(replies, model=model, config=PLAIN) as f:
        f.base()
        server.start(f)
        parent = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        def input(sid, text):
            return server.post(f, f"/sessions/{sid}/input", {"text": text})["entry"]
        input(parent, parent_marker)
        server.until(f, parent, lambda es: es[-1]["kind"] == "turn_end")
        child = next(p.parent.name for p in (f.home / ".razor/sessions").glob("*/record.jsonl")
                     if list(f.record(p.parent.name))[0].get("child", {}).get("name") == "courier36")
        server.until(f, child, lambda es: (f.cwd / "ready1").exists())
        input(parent, "Send your note now.")
        server.until(f, child, lambda es: any(e["kind"] == "input" and mail in e["text"] for e in es))
        (f.cwd / "continue1").touch()
        server.until(f, child, lambda es: (f.cwd / "ready2").exists())
        correction = input(child, "Please answer before you continue this work.")
        (f.cwd / "continue2").touch()
        entries = server.until(f, child, lambda es: (f.cwd / "ready3").exists())
        refused = [e for e in entries if e["kind"] == "not_run"]
        assert [e["call"] for e in refused] == [id for id, _, _ in denied], refused
        ordinary = [e for e in entries if e["kind"] in ("call", "result")]
        assert not any(e["call"] in {id for id, _, _ in denied} for e in ordinary), "withheld calls reached call cards"
        assert all((e["name"], e["input"]) == (name if model == "sol" else "Bash", args)
                   for e, (_, name, args) in zip(refused, denied)), refused
        if model == "opus":
            assert all(e["native"]["claude"]["uuid"] for e in refused), refused
        results = {e["call"]: e for e in entries if e["kind"] == "result"}
        assert all(e["outcome"] == "success" for e in results.values()), results
        assert (f.cwd / "effects").read_text().splitlines() == ["a", "b"] and (f.cwd / "once").read_text() == "once"
        assert (f.cwd / "mail").read_text() == "mail" and correction < results["mail_effect"]["id"]
        request = f.request(len(f.sent()))
        if model == "opus":
            outputs = [(b["tool_use_id"], b["content"] if isinstance(b["content"], str) else
                        "\n".join(p["text"] for p in b["content"] if "text" in p)) for m in request["messages"]
                       if isinstance(m["content"], list) for b in m["content"] if b["type"] == "tool_result"]
        else:
            outputs = [(b["call_id"], b["output"]) for b in request["input"] if b["type"] == "function_call_output"]
        for e in refused:
            found = [output for id, output in outputs if id == e["call"]]
            assert len(found) == 1 and found[0].strip(), (e, outputs)
        denial = next(output for id, output in outputs if id == refused[0]["call"]).strip()
        with Terminal(f, "--resume", child) as t:
            t.shown("I heard your correction36.")
            assert denial not in t.text()
        digest = json.loads(server.send(f, "GET", f"/sessions/{child}/digest").read())
        assert "not run" in digest and all(f"#{e['id']}" in digest for e in refused), digest
        assert denial not in digest, digest
        input(child, "Please answer this correction after rebuilding.")
        (f.cwd / "continue3").touch()
        entries = server.until(f, child, lambda es: es[-1]["kind"] == "turn_end" and any(e["kind"] == "rebuild" for e in es))
        assert next(e for e in entries if e["kind"] == "result" and e["call"] == "handover_call")["outcome"] == "success"
        withheld = next(e for e in entries if e["kind"] == "not_run" and e["call"] == "before_handover")
        assert not (f.cwd / "handover_effect").exists()
        rebuilt = next(s for s in f.sent() if "not run" in s and f"#{withheld['id']}" in s)
        assert denial not in rebuilt, rebuilt
        server.until(f, parent, lambda es: es[-1]["kind"] == "turn_end" and any(e.get("text") == "report received36" for e in es))


# A yielded cell keeps running even while the model's next wait is withheld.
for model in ("sol", "opus"):
    code = ('// @exec: {"yield_time_ms":30000}\n'
            'text(await tools.agent({action:"wait", timeout_ms:30000})); yield_control();'
            'await tools.exec_command({cmd:"while [ ! -e continue_cell ]; do sleep 0.05; done"});'
            'await tools.exec_command({cmd:"printf finished > cell_effect"});')
    read = {"cell_id": "cell", "yield_time_ms": 30000}
    if model == "opus":
        replies = [claude_reply(("cell", "mcp__razor__exec", {"code": code})),
                   claude_reply(("read", "mcp__razor__wait", read))]
    else:
        replies = [response([{"type": "custom_tool_call", "id": "ct_cell", "call_id": "cell",
                              "name": "exec", "input": code}]), response([call("wait", json.dumps(read), "read")])]
    with flow([*replies, Reply(b"", hold=True)], model=model,
              config='[profiles.claude]\ncode_mode = true\n') as f:
        server.start(f)
        session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        server.post(f, f"/sessions/{session}/input", {"text": "Start a cell and wait for my correction."})
        server.until(f, session, lambda es: any(e.get("call") == "cell:1" for e in es))
        server.post(f, f"/sessions/{session}/input", {"text": "Please answer before continuing."})
        server.until(f, session, lambda es: any(e["kind"] == "not_run" and e["call"] == "read" for e in es))
        (f.cwd / "continue_cell").touch()
        entries = server.until(f, session, lambda es: any(e["kind"] in ("not_run", "result")
                                                         and e["call"] == "cell:3" for e in es))
        assert [e["call"] for e in entries if e["kind"] == "not_run"] == ["read"], entries[-5:]
        assert (f.cwd / "cell_effect").read_text() == "finished"
