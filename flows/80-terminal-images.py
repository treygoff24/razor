"""A saved attachment reaches the real PTY as Kitty graphics after protocol detection, or
coloured half blocks without an answer; small viewports keep the chip and scrolling clears it.
Real tool results display too; unsafe codec paths and metadata bounds keep their chips.
This observes emitted protocol and text cells, not a terminal emulator's actual graphics pixels.
"""
import base64
import hashlib
import json
import os
import re
import struct
import threading

from driver import REPLIES, Reply, flow
from png import chunk, png
from responses import PLAIN, message, response
import server
from terminal import END, Terminal


# Two saturated colours with a diagonal edge: half-block pairs differ at any useful size.
RED, BLUE = bytes((240, 20, 40)), bytes((20, 60, 230))
pixels = b"".join(b"\0" + b"".join(RED if x + y < 96 else BLUE for x in range(96))
                  for y in range(96))
PNG = png(96, 96, pixels)
small = png(48, 24, b"".join(b"\0" + b"".join(RED if x + 2 * y < 48 else BLUE for x in range(48))
                            for y in range(24)))


def graphics(data):
    """The real app's Kitty commands, including chunked payloads, from captured PTY bytes."""
    return [(dict(field.split(b"=", 1) for field in control.split(b",") if b"=" in field), payload)
            for control, payload in re.findall(rb"\x1b_G([^;\x1b]*);?([^\x1b]*)\x1b\\", data)]


def placements(t):
    """Keep the subset of Kitty placement state used by the app; deletes retire visible ids."""
    visible = set()
    for control, _ in graphics(t.output):
        action, image = control.get(b"a"), control.get(b"i")
        if action in (b"p", b"T"):
            visible.add(image)
        elif action == b"d":
            if control.get(b"d", b"a").lower() == b"a":
                visible.clear()
            elif control.get(b"d", b"a").lower() == b"i":
                visible.discard(image)
    return visible


def transfers(data):
    """Complete direct Kitty transmissions, whose continuation chunks omit their image id."""
    active, pieces = None, []
    for control, payload in graphics(data):
        if control.get(b"a") in (b"t", b"T"):
            active, pieces = control, []
        if active is not None:
            pieces.append(payload)
            if control.get(b"m", b"0") == b"0":
                yield active, base64.b64decode(b"".join(pieces))
                active = None


def rows(t):
    """Picture rows sent, excluding the multipart startup probe."""
    return [(c, p) for c, p in transfers(t.output) if c.get(b"i") != t.probe]


def final_chunks(data):
    """Each completed multipart transfer's first and final controls."""
    first = None
    for control, _ in graphics(data):
        if control.get(b"a") == b"t":
            first = control
        if first is not None and control.get(b"m") == b"0":
            yield first, control
            first = None


def gave_up(t, number):
    return any(c.get(b"a") == b"d" and c.get(b"d") == b"I" and c.get(b"i") == number
               for c, _ in graphics(t.output))


class Kitty(Terminal):
    """Answers the probe and each finished transmission by client ID; `probe` and `bands` are "ok", "error",
    "none" (never) or "late" (when `late` is called)."""

    def __init__(self, *args, probe="ok", bands="ok", pause=False, **kwargs):
        self.modes, self.apc, self.loading, self.held = {"probe": probe, "bands": bands}, b"", None, []
        self.assigned, self.live, self.waiting, self.probe = {}, set(), set(), None
        self.pause, self.paused, self.pause_pending, self.pause_tail = pause, threading.Event(), False, b""
        super().__init__(*args, **kwargs)

    def answer(self, chunk):
        super().answer(chunk)
        if self.pause_pending:
            self.pause_tail += chunk
            if END in self.pause_tail:
                self.reading = False
                self.paused.set()
            self.pause_tail = self.pause_tail[-len(END):]
        self.apc += chunk
        end = 0
        for m in re.finditer(rb"\x1b_G([^;\x1b]*)[^\x1b]*\x1b\\", self.apc):
            self.command(dict(f.split(b"=", 1) for f in m[1].split(b",") if b"=" in f))
            end = m.end()
        rest = self.apc[end:]
        self.apc = rest[rest.rfind(b"\x1b"):] if b"\x1b" in rest else b""

    def command(self, c):
        action = c.get(b"a", b"t")
        if action == b"t" and b"i" in c and self.loading is None:
            self.loading = int(c[b"i"])
            if c.get(b"s") == b"769" and c.get(b"v") == b"1":
                self.probe = c[b"i"]
        if action == b"t" and self.loading is not None and c.get(b"m", b"0") == b"0":
            self.finished(self.loading)
            self.loading = None
        elif action == b"d" and c.get(b"d") == b"I":
            number = int(c[b"i"])
            self.waiting.discard(number)
            self.live.discard(number)

    def finished(self, number):
        mode = self.modes["probe" if str(number).encode() == self.probe else "bands"]
        self.waiting.add(number)
        if mode == "none":
            if self.pause and str(number).encode() != self.probe:
                self.pause_pending = True
            return
        id = number
        self.assigned[number] = id
        if mode != "error":
            self.live.add(id)
        reply = f"\x1b_Gi={id};{'EINVAL:refused' if mode == 'error' else 'OK'}\x1b\\".encode()
        if mode == "late":
            self.held.append(reply)
        else:
            self.waiting.discard(number)
            self.send(reply)

    def late(self):
        while self.held:
            self.send(self.held.pop(0))


def coloured(t, channels=(0, 2)):
    """Picture hues survive interpolation; the clock and successful theme lack these hues."""
    def picture(rgb):
        if not re.fullmatch("[0-9a-f]{6}", rgb):
            return False
        values = bytes.fromhex(rgb)
        return any(values[c] > max(values[i] for i in range(3) if i != c) + 100 for c in channels)

    return [cell for row in t.screen.buffer.values() for cell in row.values()
            if cell.data in (" ", "▀", "▄") and (picture(cell.bg) or cell.data != " " and picture(cell.fg))]


def send(f, session, **body):
    admitted = server.post(f, f"/sessions/{session}/input", body)["entry"]
    return server.until(f, session, lambda es: any(e["kind"] == "turn_end" and admitted in e["inputs"] for e in es))


older = "```\n" + "\n".join(f"BEFORE_IMAGE_{n:03}" for n in range(100)) + "\n```"
with flow([response([message(older)]), response([message("PICTURE_READY_80")]),
           response([message("STALL_READY_80")])],
          model="sol", config='[ui]\nmotion = false\n[ui.keys.global]\nstatus = ["Alt-_"]\n' + PLAIN) as f:
    server.start(f)
    session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
    send(f, session, text="Older transcript")
    entries = send(f, session, text="PICTURE_80 \U000f0001", images=[base64.b64encode(small).decode()])
    image = next(e for e in entries if e["kind"] == "input" and e.get("images"))["images"][0]
    saved = f.home / ".razor/sessions" / session / image["path"]
    assert saved.read_bytes() == small, image
    # The terminal must fetch the session-owned file after attach; it cannot read the user's cwd.
    assert not list(f.cwd.glob("*.png"))

    with Terminal(f, "--resume", session) as t:
        t.shown("PICTURE_80", "\U000f0001", "PICTURE_READY_80")
        t.send("\x1b_")
        t.shown("── USAGE")
        t.send("\x1b_")
        t.until(lambda t: "── USAGE" not in t.text())
        t.until(lambda t: len(coloured(t)) >= 16)
        assert any(c.data in ("▀", "▄") for c in coloured(t)), "no coloured half-block pixels"
        assert not placements(t), "an unanswered terminal received Kitty placements"
        # The smallest usable transcript keeps an image's text chip instead of a sliver.
        before = t.frames
        t.resize(12, 24)
        t.until(lambda t: t.frames > before and "[Image #1]" in t.text())
        y, x = next((y, line.index("[Image #1]")) for y, line in enumerate(t.screen.display) if "[Image #1]" in line)
        assert t.screen.buffer[y][x].bg != t.screen.buffer[y][x + 10].bg, "the image label lost its chip styling"
        assert not coloured(t), "a too-small viewport still painted pixels"
        t.resize(40, 120)
        t.until(lambda t: len(coloured(t)) >= 16)
        t.send("\x1b[5~" * 12)
        t.shown("BEFORE_IMAGE_000")
        assert not coloured(t), "half-block pixels remained after scrolling away"

    with Kitty(f, "--resume", session) as t:
        t.shown("PICTURE_80", "PICTURE_READY_80")
        # A real Alt-_ immediately before a reply must not turn the reply into typed text.
        t.send(b"\x1b_\x1b_Gi=31;OK\x1b\\")
        t.shown("── USAGE")
        t.send("\x1b_")
        t.until(lambda t: "── USAGE" not in t.text())
        assert "Gi=" not in t.text(), "reply after Alt-_ reached the draft"

    with Terminal(f, "--resume", session) as t:
        t.shown("PICTURE_80", "PICTURE_READY_80")
        t.send("draft-before80")
        t.shown("draft-before80")
        # The reply is split between actual PTY writes, beside ordinary user keystrokes.
        t.send(b"\x1b")
        t.pump(0.02)  # separate reads, within the terminal's 50 ms lone-ESC interval
        for part in (b"_", b"G"):
            t.send(part)
        t.pump(0.08)  # after ESC _ G identifies the answer, its remaining bytes may be delayed
        for part in (b"i=123;", b"OK"):
            t.send(part)
        t.send(b"\x1b")
        t.pump(0.02)  # ST can also straddle reads
        t.send(b"\\")
        t.send("-after80")
        t.shown("draft-before80-after80")
        assert ";OK" not in t.text() and "Gi=" not in t.text(), "Kitty response reached the composer"

    with Kitty(f, "--resume", session) as t:
        t.shown("PICTURE_80", "PICTURE_READY_80")
        t.until(placements)
        assert all(int(c.get(b"z", b"0")) > 0 for c, _ in graphics(t.output) if c.get(b"a") == b"p"), \
            "a Kitty placement is not above text"
        assert ";OK" not in t.text() and "Gi=" not in t.text(), "Kitty response reached the composer"
        # The 48×24 image is enlarged to 96×48, three 16-pixel cell rows.
        t.until(lambda t: len(rows(t)) == 3 and len(placements(t)) == 3)
        assert not coloured(t), "half-block pixels remained beneath Kitty graphics"
        uploads = rows(t)
        assert uploads, "Kitty placement had no image transfer"
        completed = list(final_chunks(t.output))
        assert any(first.get(b"i") == t.probe and final.get(b"i") == t.probe
                   and first.get(b"m") == b"1" for first, final in completed), "probe was not multipart by ID"
        assert all(first.get(b"i") == final.get(b"i") for first, final in completed), \
            "a transfer's final chunk lost its ID"
        numbers = [int(c[b"i"]) for c, _ in uploads]
        assert numbers == list(range(numbers[0], numbers[0] + 3)), f"rows not sent once, in order: {numbers}"
        assert all(b"i" in c and b"q" not in c for c, _ in uploads), "a row was not sent by ID for an answer"
        assert all(c.get(b"f") == b"32" and int(c[b"s"]) == 96 and int(c[b"v"]) == 16
                   for c, _ in uploads), [c for c, _ in uploads]
        expected = b"".join((RED if x // 2 + 2 * (y // 2) < 48 else BLUE) + b"\xff"
                            for y in range(48) for x in range(96))
        assert b"".join(p for _, p in uploads) == expected, "Kitty pixels differed from the saved image"
        assert placements(t) == {str(t.assigned[n]).encode() for n in numbers}, "a row was not placed by its answered id"
        t.send("\x03")
        t.shown("Message the agent")
        visible = placements(t)
        before = len(t.output)
        t.send("\x1b[5~" * 12)
        t.shown("BEFORE_IMAGE_000")
        assert not placements(t), f"Kitty placements survived scrolling away: {visible}"
        frames = re.findall(rb"\x1b\[\?2026h(.*?)\x1b\[\?2026l", t.output[before:], re.S)
        assert any(c.get(b"a") == b"d" for frame in frames for c, _ in graphics(frame)), \
            "no graphics delete inside a synchronized frame after scrolling the image away"

    # A probe never answered or refused leaves half blocks and its response stays out of the draft.
    for probe in ("none", "error"):
        with Kitty(f, "--resume", session, probe=probe) as t:
            t.shown("PICTURE_80", "PICTURE_READY_80")
            t.send("draft80")
            t.shown("draft80")
            t.until(lambda t: len(coloured(t)) >= 16)
            assert not rows(t) and not placements(t), f"probe {probe}: Kitty pictures without its answer"
            assert "Gi=" not in t.text() and ";OK" not in t.text(), "an answer reached the draft"

    # A row unanswered: no row follows it; it is given up on by its number, the picture keeps its
    # half blocks, a resize reads it no more, and its late answer is consumed, its id not deleted.
    with Kitty(f, "--resume", session, bands="late") as t:
        t.shown("PICTURE_80", "PICTURE_READY_80")
        t.until(rows)
        number = rows(t)[0][0][b"i"]
        t.send("pending80")
        t.shown("pending80")
        assert not gave_up(t, number), "typing waited for the unanswered row to time out"
        t.send("\x03")
        t.until(lambda t: "pending80" not in t.text())
        t.send(b"\x1b_Gi=0;OK\x1b\\")
        t.pump(0.5)
        assert len(rows(t)) == 1, "an anonymous reply acknowledged the pending row"
        t.until(lambda t: gave_up(t, number), timeout=5)
        t.until(lambda t: len(coloured(t)) >= 16)
        assert not placements(t), "an unanswered row was placed"
        for size in ((30, 100), (40, 120)):
            before = t.frames
            t.resize(*size)
            # A frame at the new width prevents old, clipped cells from surviving the next resize.
            t.until(lambda t: t.frames > before and t.screen.display[0].endswith("┐") and len(coloured(t)) >= 16)
        t.pump(1.5)
        assert len(rows(t)) == 1, "a picture given up on was read and sent again after a resize"
        late, before = str(t.assigned[int(number)]).encode(), len(t.output)
        t.late()
        t.send("after-late80")
        t.shown("after-late80")
        after = graphics(t.output[before:])
        assert not any(c.get(b"a") == b"d" and c.get(b"i") == late for c, _ in after), \
            "the late answer's id was deleted, though it may name another row by then"
        assert "Gi=" not in t.text(), "the late answer reached the draft"

    # A row refused: no row follows it, nothing is placed, and the half blocks stay.
    with Kitty(f, "--resume", session, bands="error") as t:
        t.shown("PICTURE_80", "PICTURE_READY_80")
        t.until(rows)
        t.until(lambda t: len(coloured(t)) >= 16)
        t.pump(1.5)
        assert len(rows(t)) == 1 and not placements(t), "a refused picture went on sending or was placed"

    # Quit with a row unanswered waits at most its patience, then gives it up by number.
    with Kitty(f, "--resume", session, bands="none") as t:
        t.shown("PICTURE_80", "PICTURE_READY_80")
        t.until(rows)
        number = rows(t)[0][0][b"i"]
        t.quit()
        assert gave_up(t, number), "quit left an unanswered row undeleted"

    # Stop the real PTY reader with a row unanswered, fill its output on resize, and keep typing.
    # The overdue row's deadline must not spin while the writer waits for the terminal to read.
    with Kitty(f, "--resume", session, bands="none", pause=True) as t:
        t.shown("PICTURE_80", "PICTURE_READY_80")
        assert t.paused.wait(30), "no Kitty row arrived before the stall"
        t.reader.join()
        try:
            # Fill the scratch PTY's output as a slow terminal would, without draining it.
            filler = os.open(t.device, os.O_WRONLY | os.O_NONBLOCK | os.O_NOCTTY)
            try:
                for _ in range(1024):
                    try:
                        os.write(filler, b" " * 1024)
                    except BlockingIOError:
                        break
                else:
                    raise AssertionError("scratch PTY output did not fill")
            finally:
                os.close(filler)
            t.resize(250, 500)
            t.send("STALLED_INPUT_80\r")
            server.until(f, session, lambda es: any(e["kind"] == "input" and
                         "STALLED_INPUT_80" in e.get("text", "") for e in es), timeout=10)
            def cpu():
                fields = (f"/proc/{t.pid}/stat")
                with open(fields) as stat:
                    return sum(map(int, stat.read().rsplit(")", 1)[1].split()[11:13])) / os.sysconf("SC_CLK_TCK")
            before = cpu()
            threading.Event().wait(3)
            assert cpu() - before < 0.5, "terminal spun while the reader was stopped"
        finally:
            t.pause_pending = False
            t.reading = True
            t.reader = threading.Thread(target=t.read, daemon=True)
            t.reader.start()
            t.resize(40, 120)


def cell(name, source):
    return response([{"type": "custom_tool_call", "id": "ct_" + name, "call_id": name,
                      "name": "exec", "input": source}])


# Actual pixel data crosses the model's inline-image bound, with no ancillary padding. Low
# random bits preserve the visible red/blue hues after interpolation into the terminal's cells.
side = 2048
noise = bytearray(hashlib.shake_256(b"razor-terminal-image80").digest(side * side * 3).translate(
    bytes(n & 31 for n in range(256))))
middle, high = len(noise) // 2, bytes(n | 224 for n in range(256))
noise[:middle:3] = noise[:middle:3].translate(high)
noise[middle + 2::3] = noise[middle + 2::3].translate(high)
raw = b"".join(b"\0" + noise[y * side * 3:(y + 1) * side * 3] for y in range(side))
large = png(side, side, raw)
del noise, raw
replies = [cell("screenshot80", 'image(await tools.view_image({path: "screen.png"}));'),
           response([message("SCREENSHOT_READY_80")]),
           cell("generated80", 'generatedImage(await tools.image_gen__imagegen({prompt: "A diagonal"}));'),
           Reply(json.dumps({"data": [{"b64_json": base64.b64encode(large).decode()}]}).encode(),
                 match='"gpt-image-2"'), response([message("GENERATED_READY_80")])]
with flow(replies, model="sol", config="[ui]\nmotion = false\n[profiles.codex]\nimage_gen = true\n") as f:
    (f.cwd / "screen.png").write_bytes(PNG)
    server.start(f)
    for name, marker, expected in (("screenshot80", "SCREENSHOT_READY_80", PNG),
                                   ("generated80", "GENERATED_READY_80", large)):
        session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        entries = send(f, session, text=name)
        result = next(e for e in entries if e["kind"] == "result" and e["call"] == name + ":1")
        assert result["outcome"] == "success", result
        output = json.loads(result["output"])
        image = output["images"][0]["path"] if output["images"] else output["path"]
        assert (f.home / ".razor/sessions" / session / image).read_bytes() == expected
        if name == "screenshot80":
            (f.cwd / "screen.png").unlink()  # only the session-owned copy can be fetched now
        else:
            assert output["images"] == [] and len(expected) > 7_500_000, output
        with Terminal(f, "--resume", session) as t:
            t.shown(marker)
            t.until(lambda t: len(coloured(t)) >= 16)
            assert any(c.data in ("▀", "▄") for c in coloured(t)), f"{name} showed no image pixels"


# Diagonal still frames use ffmpeg's libwebp encoder at quality 100; animation/Exif use Pillow 13.
# VP8L and ALPH method 1 both enter the unsafe lossless decoder, including in nested frames.
webps = ("lossy", "metadata", "unknown", "lossless", "lossless-alpha", "compressed-alpha", "animated-lossless", "malformed")
cases = [(kind, (REPLIES / f"terminal-{kind}.webp").read_bytes(), kind in ("lossy", "metadata", "unknown"))
         for kind in webps]
# A valid 16-bit image above the old half-sized pixel cap but within the shared cap.
deep_row = b"\0" + bytes((255, 255, 0, 0, 0, 0)) * 2048
cases.append(("png-16bit", png(2048, 2200, deep_row * 2200, depth=16), True))
# A harmless one-pixel canvas mismatch, keeping the actual compressed image at 96×96.
extended = (REPLIES / "terminal-metadata.webp").read_bytes()
cases.append(("webp-dimensions", extended[:24] + (96).to_bytes(3, "little") + extended[27:], False))
# Aggregate ancillary bytes, not merely one chunk: both chunks are individually below 1 MiB.
for name, size, allowed in (("png-metadata", (1 << 19) - 64, True),
                            ("png-metadata-over", (1 << 19) + 64, False)):
    cases.append((name, PNG[:-12] + chunk(b"raZr", b"x" * size) * 2 + PNG[-12:], allowed))
jpeg = (REPLIES / "terminal.jpg").read_bytes()  # metadata stripped; decoded back by ffmpeg
exif = b"Exif\0\0MM\0*" + struct.pack(">IH", 8, 1) + struct.pack(">HHIH", 0x112, 3, 1, 6) + b"\0\0" + struct.pack(">I", 0)
rotated = jpeg[:2] + b"\xff\xe1" + struct.pack(">H", len(exif) + 2) + exif + jpeg[2:]
cases.extend((("jpeg-baseline", jpeg, True), ("jpeg-rotated", rotated, True)))
cases.extend((name, jpeg[:2] + marker + jpeg[2:], False)
             for name, marker in (("jpeg-tem", b"\xff\x01"), ("jpeg-rst", b"\xff\xd0")))
for name, count, size, allowed in (("jpeg-segments", 64, 1, True), ("jpeg-segments-over", 65, 1, False),
                                   ("jpeg-metadata", 16, 65_500, True), ("jpeg-metadata-over", 17, 65_500, False)):
    metadata = (b"\xff\xef" + struct.pack(">H", size + 2) + b"x" * size) * count
    cases.append((name, jpeg[:2] + metadata + jpeg[2:], allowed))
green = png(96, 96, (b"\0" + bytes((20, 240, 20)) * 96) * 96)
thin = png(1, 96, b"".join(b"\0" + (RED if y < 48 else BLUE) for y in range(96)))
cases.append(("thin-png", thin, False))
blue_positions = {}
with flow([response([message("FORMAT_READY_80")]) for _ in cases], model="sol",
          config="[ui]\nmotion = false\n" + PLAIN) as f:
    server.start(f)
    for kind, data, allowed in cases:
        session = server.post(f, "/sessions", {"cwd": str(f.cwd)})["session"]
        images = [data] if allowed else [data, green]
        send(f, session, text=kind, images=[base64.b64encode(image).decode() for image in images])
        with Kitty(f, "--resume", session, rows=700 if kind == "thin-png" else 80) as t:
            t.shown("FORMAT_READY_80")
            if kind == "thin-png":
                # 1×96 is harmless even with the old enlargement (48×4608), which this tall
                # viewport can expose. The bounded size cannot meet the six-column floor.
                t.until(placements)
                for control, payload in transfers(t.output):
                    if control.get(b"f") == b"32":
                        assert max(int(control[b"s"]), int(control[b"v"])) <= 2048, control
                        assert len(payload) <= 2048 * 2048 * 4, "Kitty enlargement exceeded its pixel bound"
                row = bytes((20, 240, 20, 255)) * 96 * 16
                assert any(payload == row for _, payload in transfers(t.output)), "the second image was not sent"
                t.shown("[Image #1]")
            if allowed:
                t.until(lambda t: len(coloured(t)) >= 16)
                if kind in ("jpeg-baseline", "jpeg-rotated"):
                    blue = lambda rgb: bool(re.fullmatch("[0-9a-f]{6}", rgb)) and int(rgb[4:6], 16) > int(rgb[:2], 16) + 100
                    xs = [x for row in t.screen.buffer.values() for x, c in row.items()
                          if c.data in (" ", "▀", "▄") and (blue(c.bg) or c.data != " " and blue(c.fg))]
                    assert xs, f"{kind} showed no blue pixels"
                    blue_positions[kind] = sum(xs) / len(xs)
            elif kind != "thin-png":
                # One decode runs at a time in visible row order: the second image's green
                # pixels prove the first has finished, so its loading chip cannot pass this.
                t.until(lambda t: len(coloured(t, (1,))) >= 16)
                t.shown("[Image #1]")
                assert not coloured(t), f"unsafe {kind} decoded instead of retaining its chip"
assert blue_positions["jpeg-rotated"] < blue_positions["jpeg-baseline"], "JPEG orientation was not applied"
