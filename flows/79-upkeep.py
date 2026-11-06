"""Real child reports and task writes drive one reminder; idle handover asks for upkeep."""
import json
from claude import reply
from clock import wall_clock
from driver import flow
import server


def tool(marker, tool_name, **args):
    return reply((None, f"mcp__razor__{tool_name}", args), match=marker)


root = "ROOT_UPKEEP_77"
replies = []
with flow(replies) as f:
    f.base()
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    path = f"/sessions/{session}/input"

    def reports(entries):
        return [e for e in entries if e["kind"] == "input" and isinstance(e.get("from"), dict)
                and "report" in e["from"]]

    def reminders(entries):
        return [e for e in entries if e["kind"] == "input" and e.get("upkeep")]

    def child(n):
        name = f"upkeep{n}"
        marker = f"You are {name},"
        gate = f"child{n}_go"
        replies.extend([
            tool(root, "agent", action="start", name=name, brief="Finish the work.", placement="in_place"),
            reply(text="Child started.", match=root),
            reply(text="Report received.", match=root),
            reply((None, "Bash", {"command": f"until [ -e {gate} ]; do sleep 0.05; done"}), match=marker),
            tool(marker, "agent", action="finish", status="finished", report=f"Finished {n}."),
        ])
        person = server.post(f, path, {"text": root})["entry"]
        server.until(f, session, lambda es: any(e["kind"] == "turn_end" and person in e["inputs"] for e in es))
        entries = list(f.record(session))
        child_id = next(e["session"] for e in entries if e["kind"] == "child" and
                        isinstance(e.get("event"), dict) and e["event"].get("started", {}).get("name") == name)
        server.until(f, child_id, lambda es: any(e["kind"] == "job" and e["event"] == "started" for e in es))
        (f.cwd / gate).touch()
        entries = server.until(f, session, lambda es: len(reports(es)) == n and any(
            e["kind"] == "turn_end" and reports(es)[-1]["id"] in e["inputs"] for e in es))
        return entries

    assert not reminders(child(1))
    second = child(2)
    assert len(reminders(second)) == 1 and reminders(second)[0]["from"] == "razor", second[-10:]
    assert reminders(second)[0]["text"].strip() in sorted(f.requests.glob("[0-9]*.body"))[-1].read_text()
    assert len(reminders(child(3))) == 1
    replies.extend([tool(root, "task", action="write", title="Kept after reports"),
                    reply(text="Task saved.", match=root)])
    person = server.post(f, path, {"text": root})["entry"]
    entries = server.until(f, session, lambda es: any(e["kind"] == "turn_end" and person in e["inputs"] for e in es))
    assert any(e["kind"] == "result" and e["outcome"] == "success" for e in entries if e.get("call") ==
               next(c["call"] for c in entries if c.get("name") == "mcp__razor__task")), entries[-8:]
    assert len(reminders(child(4))) == 1
    assert len(reminders(child(5))) == 2

with wall_clock() as (_, env, advance):
    replies = [reply(text="Ready."), reply((None, "mcp__razor__handover", {"note": "Continue."}))]
    with flow(replies, config="[sessions]\nhandover_tokens = 0", env=env) as f:
        first = f.run("--print=json", "Start the idle check.")
        assert first.returncode == 0, first.stderr
        session = json.loads(first.stdout)["session"]
        advance(3_000_000)
        entries = server.until(f, session, lambda es: any(e["kind"] == "rebuild" and e.get("trigger") == "idle" for e in es))
        request = next(e for e in entries if e.get("trigger") == "idle")
        assert all(word in request["text"] for word in ("tasks", "rulings", "facts", "notes", "state note", "nothing")), request
        assert request["text"].index("First") < request["text"].index("handover`"), request
