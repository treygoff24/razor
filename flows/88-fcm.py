"""Both push services share alert payloads and follows; FCM caches signed OAuth and
forgets invalid registrations without mistaking payload errors or follow ends for them."""
from pathlib import Path
import json
import tempfile
import time
from fcm import firebase
from driver import flow
from responses import PLAIN, reply as turn
from server import post, send, start, until, wait


def put(f, path, key, body):
    answer = send(f, "PUT", path, body, key=key)
    result = json.loads(answer.read())
    assert answer.status == 200, (path, answer.status, result)
    return result


with tempfile.TemporaryDirectory(prefix="razor-fcm-") as scratch, firebase(Path(scratch)) as fcm:
    config = PLAIN + f'''[push]
team = "FLOWTEAM48"
bundle = "com.example.razor"
endpoint = "{fcm.url}"
fcm_credentials = {json.dumps(str(fcm.credentials))}
[push.development]
key_id = "FLOWKEY48"
key = {json.dumps(str(fcm.key))}
'''
    replies = [turn("FCMQUESTION", "message", type="question", text="SECRET_FCM", options=["ok"]),
               turn("FCMQUESTION", "asked"), turn("FCMQUESTION", "answered")] + [turn("FCMFINISH", "finished") for _ in range(5)]
    with flow(replies, model="gpt-6.1-sol", config=config) as f:
        start(f)
        state = f.home / ".razor"
        iphone, android = [post(f, "/devices", {"name": name})["key"] for name in ("iphone", "android")]
        token = {"token": "FCM_device:registration-1", "service": "fcm"}
        old = {"token": "ab12", "environment": "development"}
        assert put(f, "/devices/self/push", iphone, old) == {}  # iPhone omits service
        wrong = send(f, "PUT", "/devices/self/push", dict(token, environment="development"), key=android)
        assert wrong.status == 400
        wrong.read()
        assert put(f, "/devices/self/push", android, token) == {}
        for key in (iphone, android):
            put(f, "/devices/self/notifications", key, {"level": "all"})

        def made():
            return post(f, "/sessions", {"cwd": str(f.cwd)})["session"]

        def run(session, text="FCMFINISH"):
            post(f, f"/sessions/{session}/input", {"text": text})
            return until(f, session, lambda es: any(e["kind"] == "turn_end" for e in es))

        def messages(session, event):
            return [m for m in fcm.messages if (p := json.loads(m["data"]["payload"]))
                    and (p.get("razor", {}).get("session"), p.get("razor", {}).get("kind")) == (session, event)
                    or m["data"].get("session") == session and p["aps"]["event"] == event]

        def saved():
            return next(d for d in json.loads((state / "devices.json").read_text()) if d["name"] == "android")

        def alert(session, kind, priority):
            message = wait(lambda: messages(session, kind))[0]
            reference = wait(lambda: [r["body"] for r in fcm.received
                if r["body"].get("razor", {}).get("session") == session and r["body"]["razor"]["kind"] == kind])[0]
            assert json.loads(message["data"]["payload"]) == reference
            assert message["android"]["priority"] == priority and "collapse_key" not in message["android"]

        def activity(event, priority, date):
            message = wait(lambda: messages(question, event))[0]
            aps = json.loads(message["data"]["payload"])["aps"]
            reference = wait(lambda: [r["body"]["aps"] for r in fcm.received
                if r["path"].endswith("cd34") and r["body"]["aps"]["event"] == event])[0]
            assert aps["content-state"] == reference["content-state"]
            assert abs(aps["timestamp"] - time.time()) < 10 and aps[date] - aps["timestamp"] == 900
            assert message["android"]["priority"] == priority and message["android"]["collapse_key"] == question

        question, other = made(), made()
        for session in (question, other):
            assert put(f, f"/sessions/{session}/follow", android, token) == {}
            wait(lambda: messages(session, "update"))
        put(f, f"/sessions/{question}/follow", iphone, dict(old, token="cd34"))
        activity("update", "NORMAL", "stale-date")
        credentials = fcm.credentials.read_text()
        fcm.credentials.unlink()  # Cached sends must not read the account again.
        entries = run(question, "FCMQUESTION")
        alert(question, "question", "HIGH")
        urgent = wait(lambda: [m for m in messages(question, "update")
            if json.loads(m["data"]["payload"])["aps"]["content-state"]["asks"] == 1])[0]
        assert urgent["android"]["priority"] == "HIGH"
        ask = next(e["id"] for e in entries if e["kind"] == "call")
        post(f, f"/sessions/{question}/asks/{ask}", {"option": 0})
        alert(question, "finished", "HIGH")
        activity("end", "HIGH", "dismissal-date")
        wait(lambda: len(saved().get("follows", [])) == 1)
        assert saved()["follows"][0]["session"] == other and saved()["push"]["token"] == token["token"]
        run(other)
        wait(lambda: messages(other, "end"))
        # These later alerts prove ending both follows kept the alert registration.
        for code in ("payload", "UNREGISTERED", "INVALID_ARGUMENT"):
            if code == "INVALID_ARGUMENT":
                fcm.credentials.write_text(credentials)
                token = dict(token, token="FCM_new:registration-2")
                assert put(f, "/devices/self/push", android, token) == {}
            fcm.failures[token["token"]] = code
            session = made()
            run(session)
            wait(lambda: messages(session, "finished"))
            if code == "payload":
                assert saved()["push"]["token"] == token["token"]
            else:
                wait(lambda: "push" not in saved())
        count, last = len(fcm.messages), made()
        run(last)
        wait(lambda: any(r["body"].get("razor", {}).get("session") == last for r in fcm.received))
        assert len(fcm.messages) == count and fcm.grants == 1
        assert "SECRET_FCM" not in json.dumps(fcm.messages)
        private = [token["token"], "FCM_device:registration-1", "FLOW_FCM_ACCESS", fcm.private.read_text().splitlines()[1]]
        files = [state / "server.log", *state.glob("sessions/*/record.jsonl")]
        assert not any(secret in p.read_text() for p in files if p.exists() for secret in private)
