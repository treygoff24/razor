"""A large real PNG is fitted by the terminal before it reaches the model request."""
import base64
import hashlib
import json
import struct
import time

from driver import flow
from mcp import PNG
from png import png
from responses import PLAIN, call, message, response
import server
from terminal import Terminal


side = 2048
noise = hashlib.shake_256(b"fit-attachment").digest(side * side * 3)
rows = b"".join(b"\0" + noise[y * side * 3:(y + 1) * side * 3] for y in range(side))
original = png(side, side, rows)
assert len(original) > 10_000_000
with flow([response([message("FIT_READY_82")]), response([message("PHOTO_READY_82")]), response([message("POINTER_READY_82")]), response([message("JPEG_READY_82")]), response([message("FALLBACK_READY_82")])], model="sol", config=PLAIN) as f:
    image = f.cwd / "large.png"
    image.write_bytes(original)
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    with Terminal(f, "--resume", session) as t:
        t.shown("Message the agent")
        started = time.monotonic()
        t.send("\x1b[200~" + str(image) + "\x1b[201~")
        t.shown("preparing", timeout=10)
        t.send(" inspect")
        t.shown("inspect", "preparing", timeout=2)
        t.shown("[Image #1]", "scaled", timeout=120)
        print(f"large attachment ready in {time.monotonic() - started:.3f}s", flush=True)
        t.send("\r")
        t.shown("FIT_READY_82", timeout=90)
        person = next(e for e in f.record(session) if e["kind"] == "input" and e["from"] == "person")
        assert person["text"] == "[Image #1] inspect" and str(image) not in t.text(), (person, t.text())
        width, height = 4000, 3000
        photo_noise = hashlib.shake_256(b"twelve-megapixels").digest(width * height * 3).translate(
            bytes(n & 31 for n in range(256)))
        photo_rows = b"".join(b"\0" + photo_noise[y * width * 3:(y + 1) * width * 3] for y in range(height))
        photo = png(width, height, photo_rows)
        assert 7_500_000 < len(photo) < 32 << 20
        photo_path = f.cwd / "photo.png"
        photo_path.write_bytes(photo)
        del photo_noise, photo_rows, photo
        t.send("\x1b[200~" + str(photo_path) + "\x1b[201~")
        t.shown("preparing", timeout=10)
        t.until(lambda t: "scaled" in "\n".join(t.screen.display[-8:]) and "preparing" not in t.text(), timeout=120)
        t.send("\r")
        t.shown("PHOTO_READY_82", timeout=90)
        too_large = f.cwd / "pointer.png"
        too_large.write_bytes(original + bytes(34 << 20))
        t.send("\x1b[200~" + str(too_large) + "\x1b[201~")
        t.until(lambda t: "pointer" in "\n".join(t.screen.display[-8:]) and "preparing" not in t.text(), timeout=30)
        t.send("\r")
        t.shown("POINTER_READY_82", timeout=90)
        jpeg_path = f.cwd / "Screenshot 12.38.45\u202fAM.jpg"
        # One grayscale component; every 8×8 block is zero DC and EOB.
        def segment(marker, data):
            return b"\xff" + bytes([marker]) + struct.pack(">H", len(data) + 2) + data

        table = bytes([1] + [0] * 15) + b"\0"
        jpeg = (b"\xff\xd8" + segment(0xdb, b"\0" + bytes([1] * 64))
                + segment(0xc0, struct.pack(">BHHBBBB", 8, 3024, 4032, 1, 1, 0x11, 0))
                + segment(0xc4, b"\0" + table + b"\x10" + table)
                + segment(0xda, b"\x01\x01\x00\x00\x3f\x00")
                + bytes((4032 // 8) * (3024 // 8) // 4) + b"\xff\xd9")
        jpeg_path.write_bytes(jpeg)
        t.send("\x1b[200~" + str(jpeg_path) + "\x1b[201~")
        t.until(lambda t: "preparing" not in t.text() and ("scaled" in "\n".join(t.screen.display[-8:]) or "pointer" in "\n".join(t.screen.display[-8:])), timeout=120)
        t.send("\r")
        t.shown("JPEG_READY_82", timeout=90)
        fallback_path = f.cwd / "fallback.jpg"
        jpeg = jpeg_path.read_bytes()
        fallback = jpeg[:2] + (b"\xff\xe1\x00\x03x" * 65) + jpeg[2:]
        fallback_path.write_bytes(fallback)
        t.send("\x1b[200~" + str(fallback_path) + "\x1b[201~")
        t.until(lambda t: "[Image #1]" in "\n".join(t.screen.display[-8:]) and "preparing" not in t.text(), timeout=30)
        t.send("\r")
        t.shown("FALLBACK_READY_82", timeout=90)
    assert image.read_bytes() == original, "original changed"
    request = f.request(1)
    images = [part["image_url"] for item in request["input"] for part in item.get("content", [])
              if part.get("type") == "input_image"]
    assert len(images) == 1, "model did not receive the fitted image"
    encoded = images[0].split(",", 1)[1]
    fitted = base64.b64decode(encoded, validate=True)
    assert len(encoded) <= 10_000_000 and fitted != original, "image was not fitted"

    photo_request = f.request(2)
    photo_images = [part["image_url"] for item in photo_request["input"] for part in item.get("content", [])
                    if part.get("type") == "input_image"]
    assert len(photo_images) == 2 and len(photo_images[-1].split(",", 1)[1]) <= 10_000_000, "12 MP photo was not fitted"
    pointer_request = f.request(3)
    parts = [part for item in pointer_request["input"] for part in item.get("content", [])]
    assert any("pointer only" in part.get("text", "") and str(too_large) in part["text"] for part in parts), parts
    assert sum(part.get("type") == "input_image" for part in parts) == 2, "pointer became an image"

    jpeg_request = f.request(4)
    parts = [part for item in jpeg_request["input"] for part in item.get("content", [])]
    jpeg_images = [part["image_url"] for part in parts if part.get("type") == "input_image"]
    assert len(jpeg_images) == 3, "12 MP baseline JPEG did not reach the model as an image"
    assert jpeg_images[-1].startswith("data:image/jpeg;"), "JPEG was not encoded as JPEG"
    assert base64.b64decode(jpeg_images[-1].split(",", 1)[1]) != jpeg, "baseline JPEG was not fitted"
    assert any(str(jpeg_path) in part.get("text", "") for part in parts), "JPEG origin path changed"

    fallback_request = f.request(5)
    parts = [part for item in fallback_request["input"] for part in item.get("content", [])]
    image_urls = [part["image_url"] for part in parts if part.get("type") == "input_image"]
    assert base64.b64decode(image_urls[-1].split(",", 1)[1]) == fallback, "original fallback was not sent"
    assert not any(str(fallback_path) in part.get("text", "") for part in parts), "whole original got an origin label"

# An origin belongs to the model's input, including a fresh context; the person's words stay clean.
with flow([response([call("handover", json.dumps({"note": "Continue inspecting the image."}), "image_note")]),
           response([message("REBUILT_82")])], model="sol", config=PLAIN) as f:
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    origin = {"number": 1, "path": "/sending-device/original82.png", "device": "phone82", "copy": "scaled"}
    asked = server.post(f, f"/sessions/{session}/input", {"text": "Inspect this picture.", "images": [PNG], "image_origins": [origin]})["entry"]
    entries = server.until(f, session, lambda es: any(e["kind"] == "turn_end" and asked in e["inputs"] for e in es))
    person = next(e for e in entries if e["kind"] == "input" and e["from"] == "person")
    assert person["text"] == "Inspect this picture." and person["image_origins"] == [origin], person
    assert any(e["kind"] == "rebuild" for e in entries), entries
    for index in (1, 2):
        sent = f.request(index)
        assert origin["path"] in json.dumps(sent["input"]), f"image origin missing in request {index}"

# From a paired device, an origin without a device names the device that sent the input; one naming a device keeps it.
with flow([response([message("PHONE_82")])], model="sol", config=PLAIN) as f:
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    phone = f.run("devices", "add", "phone").stdout.splitlines()[1]
    origins = [{"number": 1, "path": "IMG_82.HEIC in Photos", "device": "", "copy": "scaled"},
               {"number": 2, "path": "s82/drop.png", "device": "mac82", "copy": "scaled"}]
    sent = server.send(f, "POST", f"/sessions/{session}/input", {"text": "From the phone.", "images": [PNG, PNG], "image_origins": origins}, key=phone)
    assert sent.status == 200, sent.read()
    entries = server.until(f, session, lambda es: any(e["kind"] == "turn_end" for e in es))
    person = next(e for e in entries if e["kind"] == "input" and e["from"] == "person")
    assert person["image_origins"] == [{**origins[0], "device": "phone"}, origins[1]], person
