"""A complete call, progress during real work, a correction GPT-Live never hands off in time, the
person's stop holding the voice, and the agent's bounded view of a long call; then refusals after
upgrade, a model and effort chosen during a call, unmetered loss, server stop and a phone
streaming through closure."""
import asyncio
import json
import os
import re
import signal
import socket
import tempfile
import time
from pathlib import Path
from clock import wall_clock
from driver import flow
from live import phone, serve
from responses import PLAIN, call, done, response
import server


release, hushes, jumps = Path(tempfile.mkdtemp()) / "release", [], {}


async def end(ws, event):
    if event.get("type") == "ready":
        await ws.send(bytes(960))
    if event.get("type") == "hush":
        hushes.append(event)
        if len(hushes) == 2:  # The slow check ends during the second hold, so its reply comes held.
            release.touch()
    if event.get("type") == "usage":
        await ws.send('{"type":"end"}')


def voice(f, settings=None):
    response = server.send(f, "GET" if settings is None else "PUT", "/voice", settings)
    body = response.read()
    assert response.status == 200, (response.status, body)
    return json.loads(body)


def jump(seconds):
    """Puts the server's monotonic clock `seconds` ahead, noting when."""
    jumps[seconds] = time.monotonic()
    advance(seconds * 1000, monotonic=True)


script = [
    {"after": "session.input_audio.append", "type": "session.input_transcript.delta",
     "delta": "Please ", "start_ms": 0, "end_ms": 300},
    {"type": "session.input_transcript.delta", "delta": "check the work.", "start_ms": 300, "end_ms": 900},
    {"type": "session.delegation.created", "offset_ms": 900,
     "delegation": {"id": "d1", "target": "client"}},
    # Once the voice acknowledges the request, "wait, stop" holds it: its audio and words go nowhere,
    # and a blank fragment is not the person speaking again.
    {"after": "session.commentary.append", "type": "session.input_transcript.delta",
     "delta": "Wait, stop.", "start_ms": 1000, "end_ms": 1500},
    {"type": "session.output_transcript.delta", "delta": "Okay, pausing.", "start_ms": 1500, "end_ms": 2000},
    {"delay": 3, "type": "session.input_transcript.delta", "delta": " ", "start_ms": 4000, "end_ms": 4100},
    # A correction lifts it and reaches the working agent at its pause; GPT-Live's late hand-off of
    # it adds nothing. After a quiet minute the voice says the agent's step is still running,
    # and after another, once, that it is still on it: each minute passes on the server's
    # monotonic clock, which stands three seconds short of it for longer than a tick of its 1 s
    # timers, and then reaches it.
    {"delay": 3, "type": "session.input_transcript.delta", "delta": "Also check the logs.", "start_ms": 5000, "end_ms": 5500},
    {"delay": 3, "type": "session.delegation.created", "offset_ms": 5600, "delegation": {"id": "d2", "target": "client"}},
    {"delay": .5, "run": lambda: jump(57)}, {"delay": 1.2, "run": lambda: jump(60)},
    {"after": "session.commentary.append"},
    {"delay": .5, "run": lambda: jump(117)}, {"delay": 1.2, "run": lambda: jump(120)},
    # "Wait" holds the voice again while the reply comes; "go on" lifts it and the reply comes silently.
    {"after": "session.commentary.append", "type": "session.input_transcript.delta", "delta": "Wait.",
     "start_ms": 6000, "end_ms": 6300},
    {"delay": 5, "type": "session.input_transcript.delta", "delta": "Go on.", "start_ms": 7000, "end_ms": 7500},
    {"after": "session.thinking.append", "type": "session.output_transcript.delta", "delta": "The check ",
     "start_ms": 8000, "end_ms": 9000},
    {"type": "session.output_transcript.delta", "delta": "passed.", "start_ms": 7000, "end_ms": 8000},
]
slow = response([call("bash", json.dumps({"command": f"until [ -e {release} ]; do sleep .2; done", "wait_ms": 180000, "description": "Run the slow check"}))])
check = response([call("bash", json.dumps({"command": "true", "description": "Checking the result."}), "call_check")])
quick = response([call("bash", json.dumps({"command": "sleep 1"}), "call_quick")])
logs, config = (response([call("bash", json.dumps({"command": f"until [ -e {release}-{n} ]; do sleep .2; done",
    "wait_ms": 60000, "description": f"Read the {n}"}), f"call_{n}")]) for n in ("logs", "config"))
said = lambda text: response([{"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]}])
replies = [done(), slow, check, done(), done(), quick, done(), done(), logs, done(), config, done(),
           done(), said("Checked."), said("Fast."), done(), response([call("bash", '{"command": "until [ -e go ]; do sleep .05; done"}')]), done()]
with wall_clock(still=False) as (_, env, advance), flow(replies, model="sol", env=env, config=PLAIN + '\n[providers.slow]\nbase_url="{standin}"') as f:
    capture = f.cwd / "live"
    tone = f.cwd / "tone.pcm"
    tone.write_bytes(bytes(range(240)) * 8)
    script.insert(6, {"audio": str(tone)})  # While held: the phone hears the tone once, after "go on".
    script += [{"audio": str(tone)}, {"type": "session.usage.updated", "usage": {"seconds": 2}}]
    session = f.ask("Seed the call.")["session"]
    config = f.home / ".razor/config.toml"

    async def refused(which, reason):
        seen = await phone(f, which)
        assert len(seen) == 1 and seen[0].get("type") == "closed" and seen[0].get("reason") == reason, ("refusal after upgrade", reason, seen)
    asyncio.run(refused(session, "key"))  # No live URL and no OPENAI_API_KEY.
    asyncio.run(refused("missing", "error"))
    with serve(script, capture) as url:
        config.write_text(config.read_text() + f'\n[live]\nurl={json.dumps(url)}\n')
        # A session started for a call runs on the call's model and effort, Razor's or the config's;
        # a model or effort the phone sends wins, and Razor tells the phone the call's.
        def start(**choice):
            first = next(f.record(server.post(f, "/sessions", {"cwd": str(f.cwd), "call": True, **choice})["session"]))
            return first["model"], first["effort"]
        assert start() == ("terra", "low"), "a call's own session is not on terra at low effort"
        assert start(model="luna", effort="high") == ("luna", "high"), "the phone's choice lost"
        config.write_text('call_model = "sol"\ncall_effort = "medium"\n' + config.read_text())
        assert start() == ("sol", "medium"), "the configured call model or effort did not apply"
        assert {"call_model": "sol", "call_effort": "medium"}.items() <= json.loads(server.send(f, "GET", "/models").read()).items(), "/models"
        seen = asyncio.run(phone(f, session, end))
        assert b"".join(e["audio"] for e in seen if "audio" in e) == tone.read_bytes(), "voice PCM changed, vanished or was heard while held"
        assert [e for e in seen if e.get("type") == "hush"] == [{"type": "hush"}] * 2 and not any(e.get("text") == "Okay, pausing." for e in seen), "stop did not hush the phone or hold the voice's words"
        assert seen[-1].get("type") == "closed" and seen[-1]["reason"] == "ended", ("end was not acknowledged", seen[-1])
        sent = list(map(json.loads, (capture / "commands.jsonl").read_text().splitlines()))
        assert sent[0]["session"]["delegation"] == {"type": "client", "acknowledgement_delay_ms": 800}, "acknowledgement not delayed"
        assert any(c["type"] == "session.input_audio.append" and c["audio"] == 960 for c in sent), "phone PCM never reached voice"
        silent = [c for c in sent if c["type"] == "session.thinking.append"]
        spoken = [c for c in sent if c["type"] == "session.commentary.append"]
        assert [c["content"].rsplit(": ", 1)[-1] for c in spoken] == ["Please check the work.", "Run the slow check", "Say in two or three words that you're still here and still on it; the line has been quiet."], "acknowledgement did not name the request, or the step or its reminder was not said once"
        assert spoken[1]["_at"] > jumps[60], "the step was spoken before a quiet minute"
        assert spoken[2]["_at"] > jumps[120], "the reminder came before another quiet minute"
        assert [c["content"] for c in silent] == ["flow done"], "held reply spoken, or a step given to the voice"
        assert [c["delegation_id"] for c in spoken + silent] == ["d1", "d2", "d2", "d2"], "agent's progress did not reach the voice's latest delegation"
        entries = server.until(f, session, lambda es: sum(e["kind"] == "turn_end" for e in es) == 2)
        facts = [e for e in entries if e["kind"] == "voice_call"]
        assert [e["event"] for e in facts] == ["started"] + ["line"] * 6 + ["ended"] and facts[-1]["reason"] == "ended", "call facts or utterance boundaries were lost"
        assert [(e["who"], e["text"]) for e in facts if e["event"] == "line"] == [("person", "Please check the work."), ("person", "Wait, stop."),
            ("person", "Also check the logs."), ("person", "Wait."),
            ("person", "Go on."), ("voice", "The check passed.")], "transcript deltas did not form whole utterances, or held words were recorded"
        handed = [e for e in entries if e.get("voice_call")]
        assert [(e["text"], e["voice_call"]) for e in handed] == [(facts[n]["text"], {"id": facts[0]["call"], "lines": [facts[n]["id"]]})
            for n in (1, 3)], "spoken request or correction was lost, duplicated, or detached from its record"
        assert json.loads(server.send(f, "GET", "/voice").read())["call_used_today"] == 2 / 1200, "usage update and final usage charged the call twice"

        # A second call has more utterances than the agent's view holds. The record keeps all of them.
        script[:] = [{"type": "session." + ("input" if n % 2 == 0 else "output") + "_transcript.delta",
                      "delta": f"line{n:03d}.", "start_ms": n * 30, "end_ms": (n + 1) * 30} for n in range(300)]
        script += [{"type": "session.usage.updated", "usage": {"seconds": 1}}]
        asyncio.run(phone(f, session, end))
        entries = list(f.record(session))
        long_call = [e for e in entries if e["kind"] == "voice_call" and e["call"] != facts[0]["call"]]
        assert [e["text"] for e in long_call if e["event"] == "line"] == [f"line{n:03d}." for n in range(300)], "long call lost record lines"
        server.post(f, f"/sessions/{session}/input", {"text": "Continue by message."})
        server.until(f, session, lambda es: sum(e["kind"] == "turn_end" for e in es) == 3)
        users = [i for i in f.request(5)["input"] if i.get("role") == "user"]
        view = "\n".join(p.get("text", "") for p in users[-1]["content"])
        shown = re.findall(r"line(\d{3})\.", view)
        omitted = re.findall(r"\b\d+\b", view)
        # The prior call's three lines after its turn are also unread: 303 lines, of which 47 no longer fit.
        assert shown == [f"{n:03d}" for n in range(44, 300)] and omitted == ["47"], "agent did not receive newest 256 lines and one accurate omission count"

        # Words the voice talked over and the agent's turn ended before their pause start the next turn.
        script[:] = [
            {"type": "session.input_transcript.delta", "delta": "Run the quick check.", "start_ms": 0, "end_ms": 500},
            {"type": "session.delegation.created", "offset_ms": 500, "delegation": {"id": "d3", "target": "client"}},
            {"after": "session.commentary.append", "type": "session.input_transcript.delta", "delta": "Then summarize it.",
             "start_ms": 600, "end_ms": 900},
            {"type": "session.output_transcript.delta", "delta": "Mm-hm.", "start_ms": 900, "end_ms": 1000},
            {"delay": 4, "type": "session.usage.updated", "usage": {"seconds": 1}}]
        asyncio.run(phone(f, session, end))
        entries = server.until(f, session, lambda es: sum(e["kind"] == "turn_end" for e in es) == 5)
        asked = [e["text"] for e in entries if e.get("voice_call") and e["voice_call"]["id"] not in (facts[0]["call"], long_call[0]["call"])]
        assert asked == ["Run the quick check.", "Then summarize it."], ("words said as the turn ended were lost", asked)

        # The person talks on after the voice spoke over them: the server waits for their pause
        # without spinning, and the turn ending meanwhile sends their whole line once they pause. A
        # backchannel said as the next turn ends starts nothing, and neither do words begun after
        # GPT-Live's late hand-off of words already sent.
        script[:] = [
            {"type": "session.input_transcript.delta", "delta": "Look into the logs.", "start_ms": 0, "end_ms": 500},
            {"type": "session.delegation.created", "offset_ms": 500, "delegation": {"id": "d4", "target": "client"}},
            {"after": "session.commentary.append", "type": "session.input_transcript.delta", "delta": "And the",
             "start_ms": 600, "end_ms": 800},
            {"type": "session.output_transcript.delta", "delta": "Sure.", "start_ms": 800, "end_ms": 900},
            *({"delay": .5, "type": "session.input_transcript.delta", "delta": f" part{n}",
               "start_ms": 1000 + n * 500, "end_ms": 1400 + n * 500} for n in range(10)),
            {"delay": 4, "type": "session.input_transcript.delta", "delta": "Mm-hm.", "start_ms": 9000, "end_ms": 9200},
            {"delay": 3, "type": "session.delegation.created", "offset_ms": 9300, "delegation": {"id": "d5", "target": "client"}},
            {"delay": 1, "type": "session.input_transcript.delta", "delta": "Thanks, that's all for now.",
             "start_ms": 11000, "end_ms": 11500},
            {"delay": 3, "type": "session.usage.updated", "usage": {"seconds": 1}}]
        pid, cpu = json.loads((f.home / ".razor/server.json").read_text())["pid"], []

        async def talk(ws, event):
            await end(ws, event)
            if event.get("type") == "caption" and event["text"] in (" part0", " part8"):
                cpu.append(sum(map(int, Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[11:13])))
            for word, n in ((" part8", "logs"), ("Mm-hm.", "config")):
                if event.get("type") == "caption" and event["text"] == word:
                    Path(f"{release}-{n}").touch()
        asyncio.run(phone(f, session, talk))
        assert (cpu[1] - cpu[0]) / os.sysconf("SC_CLK_TCK") < .5, ("server spun while the person talked on", cpu)
        entries = server.until(f, session, lambda es: sum(e["kind"] == "turn_end" for e in es) == 7)
        latest = [e["text"] for e in entries if e.get("voice_call")][-2:]
        assert latest == ["Look into the logs.", "And the part0 part1 part2 part3 part4 part5 part6 part7 part8 part9"], (
            "a line talked on past the turn's end was split, or a backchannel or words after a late hand-off were admitted", latest)

    # A partial settings update retains every omitted field, including the call budget. Invalid
    # shapes and values cannot change the saved file.
    saved = voice(f, {"call_daily_dollars": 4.5, "call_voice": "chosen", "memos": True})
    changed = voice(f, {"voice": "another"})
    assert changed == {**saved, "voice": "another"}, (saved, changed)
    path = f.home / ".razor/voice.json"
    before = path.read_bytes()
    for body, status in (([], 422), ({"voice": None}, 400)):
        answer = server.send(f, "PUT", "/voice", body)
        assert answer.status == status, (answer.status, answer.read())
        assert path.read_bytes() == before and voice(f) == changed

    session = f.ask("Seed the call.")["session"]
    # A refused choice after a held one changes nothing; the hand-off carries the held one, a
    # handover, and the new model's rebuilt request ends with the person's words.
    script = [{"after": "session.input_audio.append", "type": "session.input_transcript.delta", "delta": "Check it.", "start_ms": 0, "end_ms": 900}, {"type": "session.delegation.created", "offset_ms": 900, "delegation": {"id": "c1", "target": "client"}}]
    async def choose(ws, event):
        if reply := {"ready": '{"type":"choose","model":"terra","effort":"high","handover":true}', "chose": '{"type":"choose","model":"nope"}', "refused": bytes(960), "working": event.get("on") and '{"type":"end"}'}.get(event.get("type")):
            await ws.send(reply)
    with serve(script) as endpoint:
        config.write_text(config.read_text().replace(url, endpoint))
        seen = [e for e in asyncio.run(phone(f, session, choose)) if e.get("type") in ("ready", "chose", "refused")]
    assert [e["type"] for e in seen] == ["ready", "chose", "refused"] and seen[0]["model"] == "sol" and seen[1]["model"] == "terra" and seen[1]["handover"], seen
    handed = [(e.get("model"), e.get("effort"), e.get("handover")) for e in server.until(f, session, lambda es: es[-2].get("text") == "Checked.") if e.get("voice_call")]
    assert handed == [("terra", "high", True)], handed
    asked = json.loads(f.sent()[-1])
    parts = [p["text"] for m in asked["input"] if m.get("role") == "user" for p in m["content"]]
    assert asked["model"] == "gpt-6.1-terra" and parts[-1].endswith("Check it.") and "Check it." not in (rest := "".join(parts[:-1])) and "context was rebuilt" in rest, parts
    # A held choice the session no longer takes (Fast came after it) never costs the person's words:
    # they go without it, and the phone hears why.
    async def fallback(ws, event):
        if event.get("type") == "chose":
            server.post(f, f"/sessions/{session}/input", {"text": "Go fast.", "speed": "fast"})
            server.until(f, session, lambda es: es[-2].get("text") == "Fast.")
        if reply := {"ready": '{"type":"choose","model":"slow/slow"}', "chose": bytes(960), "refused": '{"type":"end"}'}.get(event.get("type")):
            await ws.send(reply)
    with serve(script) as again:
        config.write_text(config.read_text().replace(endpoint, again))
        seen = [e for e in asyncio.run(phone(f, session, fallback)) if e.get("type") == "refused"]
    entries = server.until(f, session, lambda es: sum(e.get("text") == "flow done" for e in es) == 2)
    assert "unavailable" in seen[0]["sentence"] and [(e["text"], e.get("model")) for e in entries if e.get("voice_call")][-1] == ("Check it.", None), (seen, entries)
    # 63 inputs, the first naming a model and handover, join the running turn on its
    # current model, with the call's end not yet sent; each is read and answered once.
    async def flood(ws, event):
        if event.get("type") == "ready":
            server.post(f, f"/sessions/{session}/input", {"text": "Hold."})
            server.until(f, session, lambda es: any(e["kind"] == "job" for e in es))
            held.extend(server.post(f, f"/sessions/{session}/input", {"text": f"Held {i}.", **({"model": "sol", "handover": True} if i == 0 else {})})["entry"] for i in range(63))
            await ws.send('{"type":"end"}')
    with serve([]) as quiet:
        config.write_text(config.read_text().replace(again, quiet))
        held = []
        asyncio.run(phone(f, session, flood))
    (f.cwd / "go").touch()
    ends = [e for e in server.until(f, session, lambda es: set(held) <= {i for e in es if e["kind"] == "turn_end" for i in e["inputs"]}) if e["kind"] == "turn_end"]
    assert ends[-1]["end"] == "done" and set(held) <= set(ends[-1]["inputs"]), ends[-1]
    assert all(sum(e["inputs"].count(i) for e in ends) == 1 for i in held), ends
    current = json.loads(f.sent()[-1])
    assert current["model"] == "gpt-6.1-terra" and all(json.dumps(current).count(f"Held {i}.") == 1 for i in range(63)), current
    with serve([], refuse=401) as url:
        config.write_text(config.read_text().replace(quiet, url))
        settings = voice(f)
        settings["call_daily_dollars"] = 0
        voice(f, settings)
        asyncio.run(refused(session, "cap"))
        settings["call_daily_dollars"] = 10
        voice(f, settings)
        asyncio.run(refused(session, "key"))
    # Hold a port without listening: connection refusal without a race to reuse the port.
    with socket.socket() as unavailable:
        unavailable.bind(("127.0.0.1", 0))
        lost = f"ws://127.0.0.1:{unavailable.getsockname()[1]}"
        config.write_text(config.read_text().replace(url, lost))
        asyncio.run(refused(session, "lost"))

    for mode in ("drop", "stop", "streaming"):
        script = ([{"after": "session.input_audio.append", "delay": .3, "drop": True}] if mode == "drop" else
                  [{"after": "session.input_audio.append", "type": "session.input_transcript.delta",
                    "delta": "Half a sentence", "start_ms": 0, "end_ms": 200}] if mode == "stop" else
                  [{"after": "session.input_audio.append", "type": "session.usage.updated", "usage": {"seconds": 30}}])
        with serve(script) as endpoint:
            config.write_text(config.read_text().replace(lost, endpoint))
            lost = endpoint
            settings["call_daily_dollars"] = .01 if mode == "streaming" else 10
            used = voice(f, settings)["call_used_today"]  # The calls above charged their usage.
            async def act(ws, event):
                if event.get("type") == "ready":
                    await ws.send(bytes(960))
                if mode == "stop" and event.get("type") == "caption":
                    os.kill(json.loads((f.home / ".razor/server.json").read_text())["pid"], signal.SIGTERM)
            before = list(f.record(session))
            pid = json.loads((f.home / ".razor/server.json").read_text())["pid"]
            opened = time.monotonic()
            seen = asyncio.run(phone(f, session, act, streaming=mode == "streaming"))
            elapsed = time.monotonic() - opened
            assert seen[0].get("type") == "ready" and seen[-1].get("type") == "closed" and seen[-1].get("reason") == ("cap" if mode == "streaming" else "lost"), (mode, "phone lost its ending", seen)
            added = [e for e in f.record(session) if e["id"] > before[-1]["id"] and e["kind"] == "voice_call"]
            assert added[0]["event"] == "started" and added[-1]["event"] == "ended" and added[-1]["reason"] == seen[-1]["reason"], (mode, "call ending not recorded", added)
            if mode == "drop":
                charged = (voice(f)["call_used_today"] - used) * 1200
                assert .25 <= charged <= elapsed + .1, ("unmetered call not charged its wall-clock length", charged, elapsed)
            if mode == "stop":
                assert [e["text"] for e in added if e["event"] == "line"] == ["Half a sentence"], "server stop lost the last utterance"
                server.gone(pid)
                assert f.run("devices").returncode == 0, "scratch server could not restart after call ended"

