"""One HTTP/2 stand-in serves Apple, FCM and service-account OAuth."""
from contextlib import contextmanager
from urllib.parse import parse_qs
import base64
import json
import subprocess
import time
from apns import Apple, listening


class Firebase(Apple):
    def __init__(self, folder):
        super().__init__(folder, "FLOWKEY48", "FLOWTEAM48")
        self.credentials, self.private, self.rsa_public = (folder / n for n in ("account.json", "rsa.pem", "rsa-public.pem"))
        for args in [("genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048", "-out", str(self.private)),
                     ("pkey", "-in", str(self.private), "-pubout", "-out", str(self.rsa_public))]:
            subprocess.run(["openssl", *args], check=True, capture_output=True)
        self.messages, self.failures, self.grants = [], {}, 0

    def answer(self, headers, raw):
        if headers[":path"].startswith("/3/device/"):
            return super().answer(headers, raw)
        if headers[":path"] == "/token":
            self.grants += 1
            form = parse_qs(raw.decode())
            assert form["grant_type"] == ["urn:ietf:params:oauth:grant-type:jwt-bearer"]
            head, claims, signature = form["assertion"][0].split(".")
            decode = lambda s: base64.urlsafe_b64decode(s + "=" * (-len(s) % 4))
            assert json.loads(decode(head)) == {"alg": "RS256", "typ": "JWT", "kid": "FCMKEY"}
            claims_json = json.loads(decode(claims))
            assert claims_json == {"iss": "flow@example.iam.gserviceaccount.com", "aud": self.url + "/token",
                "scope": "https://www.googleapis.com/auth/firebase.messaging", "iat": claims_json["iat"],
                "exp": claims_json["iat"] + 3600}
            assert abs(claims_json["iat"] - time.time()) < 30
            (self.folder / "rsa-signed").write_bytes(f"{head}.{claims}".encode())
            (self.folder / "rsa-signature").write_bytes(decode(signature))
            checked = subprocess.run(["openssl", "dgst", "-sha256", "-verify", str(self.rsa_public),
                "-signature", str(self.folder / "rsa-signature"), str(self.folder / "rsa-signed")], capture_output=True)
            assert checked.returncode == 0, "FCM assertion signature did not verify"
            return 200, b'{"access_token":"FLOW_FCM_ACCESS","expires_in":3600,"token_type":"Bearer"}'
        assert headers[":path"] == "/v1/projects/flow-project/messages:send"
        assert headers["authorization"] == "Bearer FLOW_FCM_ACCESS"
        message = json.loads(raw)["message"]
        assert set(message) == {"token", "data", "android"}, message
        assert message["android"]["ttl"] == "86400s"
        assert all(isinstance(v, str) for v in message["data"].values()), message
        assert len(json.dumps(message["data"], separators=(",", ":")).encode()) <= 4096
        self.messages.append(message)
        code = self.failures.get(message["token"])
        if code:
            detail = {"@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError", "errorCode": code}
            if code == "payload":
                detail = {"@type": "type.googleapis.com/google.rpc.BadRequest", "fieldViolations": [{"field": "message.data"}]}
            return (404 if code == "UNREGISTERED" else 400), json.dumps({"error": {"details": [detail]}}).encode()
        return 200, b'{}'


@contextmanager
def firebase(folder):
    with listening(Firebase(folder)) as fcm:
        fcm.credentials.write_text(json.dumps({"project_id": "flow-project", "private_key_id": "FCMKEY",
            "client_email": "flow@example.iam.gserviceaccount.com", "private_key": fcm.private.read_text(),
            "token_uri": fcm.url + "/token"}))
        yield fcm
