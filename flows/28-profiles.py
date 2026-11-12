"""GPT code mode preserves completed native items, renders the pinned declarations, and runs a
cell's web.run as Codex's search request on the sign-in, view_image as `read` showing the image,
and image_gen.imagegen as Codex's image request (docs/design/tools.md, responses.md)."""
import base64
import json
import os
import signal
import server
import re
from dataclasses import replace
from pathlib import Path

from claude import reply as claude
from driver import Reply, flow
from mcp import PNG
from responses import done, response
from terminal import Terminal

# Families are stable choices; requests resolve them, while provider-reported IDs stay facts.
families = ["fable", "opus", "sonnet", "haiku", "sol", "terra", "luna", "astra", "gemini", "deepseek", "glm"]
source_models = Path(__file__).parents[1] / "src/profiles/models.rs"
targets = dict(re.findall(r'\("(' + "|".join(families) + r')", "[^"]+", "([^"]+)"\)', source_models.read_text()))
sdk = Path(__file__).parents[1] / "bridge/node_modules/@anthropic-ai"
catalog = next(sdk.glob("claude-agent-sdk-*/claude")).read_bytes()
for family in families[:4]:
    assert f'{family.upper()}_ID:"{targets[family]}"'.encode() in catalog, (family, targets[family])
del catalog
served = "claude-fable-5-2-served"
reply = claude(text="FAMILY_28")
reply = replace(reply, body=reply.body.replace(b'claude-opus-4-8', served.encode()))
with flow([reply] * 4 + [done()], model="Claude Fable", config='[models.fable]\nwindow = 123456\n[models.claude-fable-5-2]\nwindow = 234567\n') as f:
    for index, spelling in enumerate(("Claude Fable", "fable", "Fable"), 1):
        session = f.ask(*(["--model", spelling] if index > 1 else []), "Use the family.")["session"]
        entries = list(f.record(session))
        assert entries[0]["model"] == entries[-1]["model"] == "fable", entries
        assert f.request(index)["model"] == "claude-fable-5-2", f.request(index)
        assert entries[-1]["served_model"] == served and entries[-1]["window"] == 123456, entries[-1]
    offered = json.loads(server.send(f, "GET", "/models").read())
    assert offered["models"][:11] == families and not any(n.startswith(("claude-", "gpt-", "gemini-", "deepseek-", "glm-")) for n in offered["models"]), offered
    assert offered["labels"] == dict(zip(families, ["Fable", "Opus", "Sonnet", "Haiku", "Sol", "Terra", "Luna", "Astra", "Gemini", "DeepSeek", "GLM"])), offered
    config = f.home / ".razor/config.toml"
    config.write_text(config.read_text().replace('[models.fable]\nwindow = 123456\n', ''))
    # The exact-ID row applies when the family has no row, including after a native resume.
    pid = server.start(f)
    os.kill(pid, signal.SIGTERM)
    server.gone(pid)
    server.start(f)
    row = json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())
    assert row["model"] == "fable" and row["served_model"] == served, row
    f.ask("--resume", session, "Keep the conversation.")
    entries = list(f.record(session))
    assert entries[-1]["model"] == "fable" and entries[-1]["window"] == 234567, entries[-1]
    row = json.loads(server.send(f, "GET", f"/sessions/{session}/agent").read())
    assert row["model"] == "fable" and row["served_model"] == served, row
    assert not any(e["kind"] == "rebuild" for e in entries), entries
    with Terminal(f, "--resume", session) as t:
        t.shown("FAMILY_28")
        t.send("/status\r")
        t.shown("served", served)
        t.send("\x1b")
        t.shown("FAMILY_28")
        t.send("/model\r")
        t.shown("▸ Fable", "Opus")
        assert served not in t.text(), t.text()
        t.send("\x1b")
        t.until(lambda t: "▸ Fable" not in t.text())
        t.send("/model GPT 6.1 Sol\rTYPED_FAMILY_28\r")
        t.shown("flow done")
    entries = list(f.record(session))
    assert f.request(5)["model"] == "gpt-6.1-sol" and entries[-1]["model"] == "sol", entries[-1]
    assert next(e for e in entries if e.get("text") == "TYPED_FAMILY_28")["model"] == "sol", entries

reply = done()
reply = replace(reply, body=reply.body.replace(b'"status": "completed"', b'"status": "completed", "model": "gpt-6.1-sol-served"'))
with flow([reply] * 3, model="GPT 6.1 Sol") as f:
    for index, spelling in enumerate(("GPT 6.1 Sol", "sol"), 1):
        session = f.ask(*(["--model", spelling] if index > 1 else []), "Use Sol.")["session"]
        entries = list(f.record(session))
        assert entries[0]["model"] == entries[-1]["model"] == "sol", entries
        assert f.request(index)["model"] == "gpt-6.1-sol", f.request(index)
        assert entries[-1]["served_model"] == "gpt-6.1-sol-served", entries[-1]
    pid = server.start(f)
    os.kill(pid, signal.SIGTERM)
    server.gone(pid)
    f.ask("--resume", session, "Keep Sol.")
    assert f.request(3)["model"] == "gpt-6.1-sol" and not any(e["kind"] == "rebuild" for e in f.record(session))
    # Unknown names keep their spelling in the refusal; provider IDs keep theirs on the wire.
    refused = f.run("--print", "--model", "Unknown Family", "Check a name.")
    assert refused.returncode != 0 and "Unknown Family" in refused.stderr, refused

with flow([done()], model="gateway/Claude Fable", config='[providers.gateway]\nformat = "responses"\nbase_url = "{standin}"\n[models."gateway/Claude Fable"]\nwindow = 200000\n') as f:
    session = f.ask("Use a provider.")["session"]
    assert f.request(1)["model"] == "Claude Fable" and next(f.record(session))["model"] == "gateway/Claude Fable"

source = 'text("flow exec 🪶"); text(await tools.web__run({search_query: [{q: "flow kingfisher"}]}))'
items = [
    {"type": "reasoning", "id": "rs_flow", "summary": [{"type": "summary_text", "text": "SUMMARY_A"}], "encrypted_content": "DONE_REPLAY_EXACTLY"},
    {"type": "custom_tool_call", "id": "ct_flow", "call_id": "call_exec", "name": "exec", "input": source},
]
search = Reply(json.dumps({"output": "flow search found", "encrypted_output": "SEARCH_NOT_SENT"}).encode())
look = ('image(await tools.view_image({path: "shot.png"}));'
        'text(await tools.view_image({path: "big.png"}).catch(String));'
        'text(await tools.image_gen__imagegen({prompt: "a fox", referenced_image_paths: "shot.png"}).catch(String));'
        'generatedImage(await tools.image_gen__imagegen({prompt: "a fox"}));')
edit = 'generatedImage(await tools.image_gen__imagegen({prompt: "add a hat", num_last_images_to_include: 1}))'
cell = lambda id, source: response([{"type": "custom_tool_call", "id": "ct_" + id, "call_id": id, "name": "exec", "input": source}])
SHOT = base64.b64decode(PNG)
MADE = SHOT + b"made"
made = lambda data: Reply(json.dumps({"created": 1, "data": [{"b64_json": base64.b64encode(data).decode()}]}).encode(),
                          match='"gpt-image-2"')
url = lambda data: "data:image/png;base64," + base64.b64encode(data).decode()
summary_events = [{"type": "response.reasoning_summary_text." + kind, "item_id": "rs_flow", "output_index": 0,
                   "summary_index": 0, field: "SUMMARY_A"} for kind, field in (("delta", "delta"), ("done", "text"))]
streamed = b"".join(f"data: {json.dumps(e)}\n\n".encode() for e in summary_events)
first_reply = response(items, tier="default")
first_reply = Reply(first_reply.body.replace(b'data: {"type": "response.output_item.done"', streamed + b'data: {"type": "response.output_item.done"', 1))
replies = [first_reply, search, cell("call_look", look), made(MADE), cell("call_edit", edit), made(SHOT), done(), done(), done()]
with flow(replies, model="gpt-6.1-sol", config='startup = ["razor", "project"]\n[profiles.codex]\nimage_gen = true') as f:
    (f.cwd / "shot.png").write_bytes(SHOT)
    (f.cwd / "big.png").write_bytes(SHOT[:8] + bytes(7_500_000))
    answer = f.ask("--speed", "fast", "Run the cell and continue.")
    entries = list(f.record(answer["session"]))
    assert [e["text"] for e in entries if e["kind"] == "thinking"] == ["SUMMARY_A"], "summary missing or duplicated"
    assert [e["text"] for e in entries if e["kind"] == "notice"] == ["Fast unavailable; provider used Standard for this request."]
    responses = [e for e in entries if e["kind"] == "response"]
    assert responses[0]["native"]["responses"]["items"] == items, responses[0]
    with Terminal(f, "--resume", answer["session"], rows=80) as t:
        t.shown("SUMMARY_A")
    calls = [e for e in entries if e["kind"] == "call" and e["call"].startswith("call_exec")]
    assert [c["call"] for c in calls] == ["call_exec", "call_exec:1"] and calls[0]["input"] == source, calls
    outputs = [e for e in entries if e["kind"] == "result" and e["call"].startswith("call_exec")]
    assert [o["call"] for o in outputs] == ["call_exec:1", "call_exec"], outputs
    assert "Script completed" in outputs[1]["output"] and "flow exec 🪶" in outputs[1]["output"], outputs
    assert "flow search found" in outputs[1]["output"], outputs
    meta = json.loads((f.requests / "002.json").read_text())
    asked = f.request(2)
    assert meta["path"] == "/alpha/search" and meta["headers"]["chatgpt-account-id"] == "flow-account", meta
    assert asked["commands"] == {"search_query": [{"q": "flow kingfisher"}]} and asked["model"] == "gpt-6.1-sol", asked
    assert asked["settings"] == {"allowed_callers": ["direct"], "external_web_access": False}, asked
    assert asked["id"] == answer["session"] and "Run the cell and continue." in json.dumps(asked["input"]), asked
    assert responses[0]["usage"]["input"] == 40 and responses[0]["usage"]["cache_read"] == 60, responses[0]
    first = f.request(1)
    second = f.request(3)
    assert first["service_tier"] == "priority", first
    assert first["store"] is False and first["parallel_tool_calls"] is False, first
    assert "reasoning.encrypted_content" in first["include"], first
    assert first["reasoning"]["context"] == "all_turns", first
    assert first["reasoning"].get("summary") == "auto", first["reasoning"]
    prefix = first["input"][:-1]
    # Four characters per token errs high against the last real-backend measurement.
    prefix_text = json.dumps(prefix, ensure_ascii=False) + first.get("instructions", "")
    estimate = (len(prefix_text) + 3) // 4
    # 12,037 with the notify wording (2026-11-03): it took right reach-the-person behaviour from
    # 30/42 (opus low) and 20/42 (terra medium) to 41/42 for each, so the extra tokens stay.
    # 12,159 with razor.md's "Browser and desktop" section (Trey, 2026-11-06: "update the Razor
    # system prompt to let agents know about agent browser and agent desktop").
    # 12,226 with the agent tool's start_top (Trey, 2026-11-06: top-level agents "spin up other
    # root level agents for me").
    # 12,248 with "if that call would be a wait, end the turn instead" (2026-11-10): an answer
    # written before a wait was lost in thinking 8/18 (Fable) and 10/18 (Opus), 0/18 for each after.
    assert estimate <= 12_248, estimate
    offered = next(i for i in prefix if i["type"] == "additional_tools")["tools"]
    tools = next(t for t in offered if t["name"] == "functions")["tools"]
    assert {t["name"] for t in tools} == {"exec", "wait"}, tools
    assert second["input"][:len(first["input"])] == first["input"], second
    assert second["input"][len(first["input"]):len(first["input"]) + 2] == items, second
    assert "ADDED_MUST_NOT_REPLAY" not in json.dumps(second), second
    assert any(i["type"] == "custom_tool_call_output" and i["call_id"] == "call_exec" for i in second["input"]), second

    # The image file reaches the model after the cell's output, each image after its path; one
    # over 7.5 MB is refused, and so is an argument of the wrong type, before any request.
    results = {e["call"]: e for e in entries if e["kind"] == "result"}
    assert results["call_look:2"]["outcome"] == "error" and "over 7.5 MB" in results["call_look:2"]["output"], results
    assert results["call_look:3"]["outcome"] == "error" and "does not fit" in results["call_look:3"]["output"], results
    after = f.request(5)["input"]
    output = next(i for i in after if i["type"] == "custom_tool_call_output" and i["call_id"] == "call_look")
    shown = after[-1]["content"]
    assert after.index(output) < len(after) - 1 and [c["type"] for c in shown] == ["input_text", "input_image"] * 2, shown
    assert [c["image_url"] for c in shown[1::2]] == [url(SHOT), url(MADE)], "the model did not see both images"
    # The generation is Codex's request on the sign-in; the PNG is saved under the call's entry,
    # and the record keeps its path, which generatedImage() writes for the model.
    entry = next(e["id"] for e in entries if e["kind"] == "call" and e["call"] == "call_look:4")
    meta, body = json.loads((f.requests / "004.json").read_text()), f.request(4)
    assert meta["path"] == "/images/generations", meta
    assert body == {"prompt": "a fox", "background": "opaque", "model": "gpt-image-2", "quality": "auto", "size": "auto"}, body
    headers = meta["headers"]
    assert headers.get("chatgpt-account-id") == "flow-account" and headers.get("originator") == "codex_cli_rs", headers
    assert headers.get("x-codex-image-turn-id") == f"{answer['session']}:{entry}", headers
    saved = f.home / ".razor/sessions" / answer["session"] / f"images/gen-{entry}.png"
    assert json.loads(results["call_look:4"]["output"])["path"] == str(saved) and saved.read_bytes() == MADE, results
    assert str(saved) in json.dumps(output), output
    kept = json.dumps([e for e in entries if e["kind"] in ("call", "result")])
    assert base64.b64encode(MADE).decode() not in kept, "the record holds the generated image's bytes"
    # An edit of the latest image sends the image the model last saw.
    meta, body = json.loads((f.requests / "006.json").read_text()), f.request(6)
    assert meta["path"] == "/images/edits" and body["images"] == [{"image_url": url(MADE)}], (meta, body)

    # Turn-end video cleanup must preserve native items, in memory and after a clean restart.
    expected = f.request(7)["input"] + responses[-1]["native"]["responses"]["items"]
    for index in (8, 9):
        if index == 9:
            pid = server.start(f)
            os.kill(pid, signal.SIGTERM)
            server.gone(pid)
        f.ask("--resume", answer["session"], "Continue the conversation.")
        replayed = f.request(index)["input"]
        assert replayed[:len(expected)] == expected, "native items changed across turns"
    assert not any(e["kind"] == "rebuild" for e in f.record(answer["session"])), "native resume rebuilt instead"

# An API-key route needs no code-mode setting. The loopback stand-in is the explicit HTTP
# proxy, so the shipped openai provider's public hostname is asked while every request stays local.
with flow([done()], model="openai/gpt-6.1-sol") as f:
    config = f.home / ".razor/config.toml"
    original = config.read_text()
    proxy = json.loads(original.split("base_url = ", 1)[1].strip())
    f.env.update({"HTTP_PROXY": proxy, "NO_PROXY": "", "OPENAI_API_KEY": "flow-public-key"})
    config.write_text(original + '\n[providers.openai]\nbase_url = "http://api.openai.com/v1"\n')
    (f.home / ".razor/auth.json").unlink()
    result = f.run("--print=json", "Reply on the API key.")
    assert (f.requests / "001.json").exists(), (result.stdout, result.stderr)
    meta = json.loads((f.requests / "001.json").read_text())
    assert "service_tier" not in f.request(1)
    assert meta["path"] == "http://api.openai.com/v1/responses", meta
    assert meta["headers"]["authorization"] == "Bearer flow-public-key", meta
    assert "chatgpt-account-id" not in meta["headers"], meta
    assert result.returncode == 0 and json.loads(result.stdout)["message"] == "flow done", result
    body = f.request(1)
    assert body["reasoning"].get("summary") == "auto", body["reasoning"]
    names = {t["name"] for t in body["tools"] if t["type"] == "function"}
    assert {"read", "write", "edit", "bash"} <= names and "exec" not in names, names
    config.write_text(config.read_text() + "\n[profiles.codex]\ncode_mode = true\n")
    refused = f.run("--print", "Explicit code mode.")
    assert refused.returncode != 0 and "API-key and chat routes support plain tools only" in refused.stderr, refused
    assert len(f.sent()) == 1, "refused code mode contacted the host"

# Common model limits do not make native routes accept chat connection fields, and a row's old
# connection fields name where they moved, in the detached server's log too, beside a leftover key
# file. Empty request objects count as an override too, and alias collisions and invalid
# fractions refuse locally.
with flow([]) as f:
    config = f.home / ".razor/config.toml"
    original = config.read_text()
    for name in ("opus", "sol"):
        for field in ('base_url = "http://127.0.0.1:1"', 'key = "DUMMY_KEY"', 'request = {}',
                      'variant = {}', 'replay = "streamed"', 'id = "other"'):
            config.write_text(original + f'\n[models.{name}]\n{field}\n')
            refused = f.run("serve")
            said = (f"[models.{name}] cannot hold base_url, key or id: they moved to [providers.<name>] as base_url, key and models, and the model is named <provider>/<id> (docs/configuration.md#providers)."
                    if field.split()[0] in ("base_url", "key", "id") else "remove request, variant, replay and speed_ids")
            assert refused.returncode != 0 and said in refused.stderr, refused
    # The settings still hold the last row, sol's id.
    (f.home / ".razor/key").write_text("flow-old-key")
    assert f.run("--print", "An old row.").returncode != 0
    log = (f.home / ".razor/server.log").read_text()
    assert "[models.sol] cannot hold base_url, key or id" in log and "key is no longer read" in log, log
    for field in ("window = 0", "threshold = 0", "threshold = 1.1", "threshold = nan", "threshold = inf"):
        config.write_text(original + f'\n[models.opus]\n{field}\n')
        refused = f.run("serve")
        assert refused.returncode != 0 and "positive window" in refused.stderr, refused
    config.write_text(original + '\n[models.sol]\nthreshold = 0.5\n[models."GPT Sol"]\nthreshold = 0.6\n')
    refused = f.run("serve")
    assert refused.returncode != 0 and "same model" in refused.stderr, refused
