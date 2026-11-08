"""Configured scripts reach both model paths; code-mode apply_patch edits the named file.
Claude's plain Edit is native to the SDK, not Razor's patch action (docs/design/tools.md).
"""
import json

from claude import reply
from driver import flow
from responses import PLAIN, call, done, response
import server

patch = "*** Begin Patch\n*** Update File: note\n@@\n-before\n+after\n*** End Patch"
source = ('text(await tools.flow_script({text:"seed"}));'
          f'text(await tools.apply_patch({json.dumps(patch)}));')
for before, after, destination in [("after", "same", "note"), ("same", "alias", "../cwd/note"),
                                    ("alias", "moved", "moved")]:
    move = f"*** Begin Patch\n*** Update File: note\n*** Move to: {destination}\n@@\n-{before}\n+{after}\n*** End Patch"
    source += f'text(await tools.apply_patch({json.dumps(move)}));'
script = "python3 -c 'import json,sys; print(json.load(sys.stdin)[\"text\"].upper())'"
config = (f"[tools.flow_script]\ncommand = {json.dumps(script)}\n"
          'description = "Uppercase the input text."\n'
          'input = {type = "object", properties = {text = {type = "string"}}, required = ["text"]}\n')
for model, extra, replies in [
    ("gpt-6.1-sol", "", [response([{"type": "custom_tool_call", "id": "ct_script",
        "call_id": "call_script", "name": "exec", "input": source}]), done()]),
    ("opus", "[profiles.claude]\ncode_mode = true\n",
        [reply((None, "mcp__razor__exec", {"code": source})), reply(text="flow done")]),
]:
    with flow(replies, model=model, config=extra + config) as f:
        (f.cwd / "note").write_text("before\n")
        entries = list(f.record(f.ask("Call the script, then apply the patch.")["session"]))
        outputs = [e for e in entries if e["kind"] == "result"]
        assert outputs and all(e["outcome"] == "success" for e in outputs), outputs
        if model == "opus":
            # The CLI cuts an MCP tool's description at 4,096 characters unless told otherwise.
            tools = f.request(1)["tools"]
            exec_ = next(t for t in tools if t["name"] == "mcp__razor__exec")
            assert "flow_script(" in exec_["description"], "exec's description was cut short"
        second = f.request(2)
        assert "SEED" in json.dumps(second), "the model did not receive the script's output"
        assert not (f.cwd / "note").exists() and (f.cwd / "moved").read_text() == "moved\n", "patch lost the moved file"

# A command that ran reports its exit to JavaScript even when the record marks it failed.
source = ('const first = await tools.exec_command({cmd:"exit 1"});'
          'text(first.end.exit);'
          'text(await tools.exec_command({cmd:"printf continued > continued"}));'
          'const job = await tools.exec_command({cmd:"read line; exit 7", tty:true, yield_time_ms:1});'
          'text((await tools.write_stdin({session_id:job.job, chars:"go\\n"})).end.exit);')
with flow([response([{"type":"custom_tool_call", "id":"ct_exit", "call_id":"call_exit",
                     "name":"exec", "input":source}]), done()], model="gpt-6.1-sol") as f:
    entries = list(f.record(f.ask("Read the exits and continue.")["session"]))
    outputs = {e["call"]:e for e in entries if e["kind"] == "result"}
    assert (f.cwd / "continued").exists(), outputs
    assert outputs["call_exit"]["outcome"] == "success", outputs
    assert outputs["call_exit:1"]["outcome"] == outputs["call_exit:4"]["outcome"] == "error", outputs
    assert "\n1\n" in outputs["call_exit"]["output"] and "\n7\n" in outputs["call_exit"]["output"], outputs

# A long cell keeps its opening read and final output inside its character cap.
source = '// @exec: {"max_output_tokens":100}\ntext("FIRST 🪶");text("🪶".repeat(3000));text("LAST 🪶");'
with flow([response([{"type":"custom_tool_call", "id":"ct_cap", "call_id":"call_cap",
                     "name":"exec", "input":source}]), done()], model="gpt-6.1-sol") as f:
    output = next(e["output"] for e in f.record(f.ask("Read the long output.")["session"])
                  if e["kind"] == "result" and e["call"] == "call_cap")
    assert "FIRST 🪶" in output and "LAST 🪶" in output, output
    assert "bytes omitted" in output and "Full output:" in output and "�" not in output, output
    assert len(output.split("\nFull output:")[0].split("\n",1)[1]) <= 400, output

# A failed final status write is an error, even after another exec evicts the ended cell.
execute = lambda id: response([{"type":"custom_tool_call", "id":"ct_" + id, "call_id":id,
                                "name":"exec", "input":'text("finished");'}])
with flow([execute("status"), execute("prune"), response([call("wait", '{"cell_id":"status"}', "status_wait")]),
           done()], model="sol") as f:
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    (f.home / ".razor/sessions" / session / "cells/status.end").mkdir(parents=True)
    outputs = {e["call"]: e for e in f.record(f.ask("--resume", session, "Read both statuses.")["session"])
               if e["kind"] == "result"}
    assert outputs["prune"]["outcome"] == "success", outputs
    assert all(outputs[c]["outcome"] == "error" and "reading the cell status" in outputs[c]["output"]
               and "Script running" not in outputs[c]["output"] for c in ("status", "status_wait")), outputs

# A configured tool whose name ends in "message" asks the person nothing, though its input looks
# like a question: only Razor's own `message` does.
config = (PLAIN + '[tools.archive_message]\ncommand = "cat > /dev/null"\ndescription = "Archive a message."\n'
          'input = {type = "object", properties = {type = {type = "string"}, text = {type = "string"}}}\n')
question = json.dumps({"type": "question", "text": "Archived question?"})
with flow([response([call("archive_message", question)]), done()], model="gpt-6.1-sol", config=config) as f:
    session = f.ask("Archive it.")["session"]
    assert [e["outcome"] for e in f.record(session) if e["kind"] == "result"] == ["success"]
    marks = json.loads(server.send(f, "GET", f"/sessions/{session}/marks").read())
    assert json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())["asks"] == [] and "question" not in json.dumps(marks), marks
