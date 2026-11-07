#!/usr/bin/env python3
"""Turnkey ElevenLabs speech program: text stdin, MP3 stdout, optional RAZOR_WORDS metadata.
Requires ELEVENLABS_API_KEY; --voices lists available voices. Without ffmpeg, volume is unchanged
and waveform levels are empty. All vendor knowledge lives in this program, outside Razor core."""
import array
import base64
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

try:
    import ssl
except ImportError:
    sys.exit(f'speech: Python ({sys.executable}) has no working SSL support; set speech = ["/path/to/python3", {json.dumps(str(Path(__file__).absolute()))}] in config.toml to use a Python with SSL.')

BASE = os.environ.get("ELEVENLABS_BASE_URL", "https://api.elevenlabs.io")
# George, an ElevenLabs premade voice, which every account has.
VOICE = os.environ.get("RAZOR_VOICE") or "JBFqnCBsd6RMkjVDRZzb"
MODEL = os.environ.get("ELEVENLABS_MODEL") or "eleven_v4_turbo"
# RAZOR_TEMPO: a voice's speed, made in the recording (eleven_v3 ignores ElevenLabs'); the phone's applies on top.
# A request takes at most 5,000 characters on eleven_v3 (elevenlabs.io/docs/models), so a longer text is
# spoken in pieces, two at a time, which every plan's concurrency allows.
PIECE, TEMPO = 2500, float(os.environ.get("RAZOR_TEMPO") or 1)
TABLE = "There's a table in the reply."
FFMPEG = shutil.which("ffmpeg")

def ear(text):
    """The reply as it should sound: headings as sentences, a link as its words, code blocks and tables
    named rather than read, list and emphasis marks dropped; audio tags such as [pleased] kept."""
    text = re.sub(r"```.*?(```|\Z)", "\nThere's some code in the reply.\n", text, flags=re.S)
    lines = []
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("|"):
            if not lines or lines[-1] != TABLE:
                lines.append(TABLE)
            continue
        line = re.sub(r"^#+\s*(.*?)[\s#]*$", lambda m: m[1].rstrip(".:") + ".", line)
        lines.append(re.sub(r"^(?:[-*+>]|\d+[.)])\s+", "", line))
    text = "\n".join(lines)
    text = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"https?://\S+", "a link", text)
    text = re.sub(r"`([^`]*)`", r"\1", text)
    text = re.sub(r"(?<!\w)(?:~/|/|[A-Z]:\\)[^\s,;!?]+", "a file", text)
    text = re.sub(r"\b(?=[0-9a-f]*[0-9])(?=[0-9a-f]*[a-f])[0-9a-f]{7,64}\b", "a revision", text)
    tags = []
    def tag(match):
        tags.append(match[0])
        return match[0] if len(tags) <= 2 else ""
    text = re.sub(r"\[[^]\n]{1,40}\]", tag, text)
    text = re.sub(r"(\*\*|\*|~~)(?=\S)(.+?)(?<=\S)\1", r"\2", text)
    text = re.sub(r"(?<!\w)(__|_)(?=\S)(.+?)(?<=\S)\1(?!\w)", r"\2", text)
    return re.sub(r"\n{3,}", "\n\n", text).strip()


def pieces(text):
    """The text in pieces of at most PIECE characters, each ending a paragraph, else a sentence, else a word."""
    while len(text) > PIECE:
        ends = (max((m.end() for m in re.finditer(p, text[:PIECE + 1])), default=0)
                for p in (r"\n\s*\n", r"[.!?][\"')\]]*\s", r"\s"))
        cut = next((end for end in ends if end > PIECE // 2), PIECE)
        yield text[:cut].rstrip()
        text = text[cut:].lstrip()
    yield text



def call(path, body=None):
    key = os.environ.get("ELEVENLABS_API_KEY") or sys.exit("speech: there is no ElevenLabs key; run razor keys to add one.")
    headers = {"xi-api-key": key, "Content-Type": "application/json"}
    data = None if body is None else json.dumps(body).encode()
    try:
        with urllib.request.urlopen(urllib.request.Request(BASE + path, data, headers), timeout=60) as answer:
            return json.load(answer)
    except urllib.request.HTTPError as e:
        detail = json.loads(text).get("detail") if (text := e.read()).startswith(b"{") else None  # only JSON names a permission
        if isinstance(detail, dict) and detail.get("status") == "missing_permissions":
            sys.exit(f"speech: {detail.get('message', 'the ElevenLabs key lacks a permission.')} The key needs Text to Speech and Voices (read).")
        raise


def voices():
    rows, token = [], ""
    while len(rows) < 1000:
        page = call("/v2/voices?" + urllib.parse.urlencode({"page_size":100,"next_page_token":token}))
        rows.extend({"id":v["voice_id"], "name":v["name"], "description":v.get("description") or "",
                     "preview_url":v.get("preview_url"), "labels":v.get("labels") or {}} for v in page["voices"])
        if not page.get("has_more"):
            break
        token = page["next_page_token"]
    print(json.dumps(rows[:1000]))


def audio(mp3s):
    """The pieces as one louder MP3, its levels and each piece's seconds; without ffmpeg, joined as they came."""
    if not FFMPEG:
        return b"".join(mp3s), [], None
    gain = float(os.environ.get("RAZOR_GAIN_DB", "9.5"))
    loud = f"volume={gain}dB,alimiter=limit=0.891:attack=5:release=80:level=disabled"
    def run(*args):
        result = subprocess.run([FFMPEG,"-v","error",*args,"pipe:1"], capture_output=True, timeout=60)
        if result.returncode:
            raise ValueError("ffmpeg could not process speech")
        return result.stdout
    try:
        # Each piece decoded alone drops its encoder padding, so the joined audio and the seconds agree.
        with tempfile.TemporaryDirectory(prefix="speech-") as folder:
            files = [Path(folder)/f"{i}.mp3" for i in range(len(mp3s))]
            for file,mp3 in zip(files,mp3s):
                file.write_bytes(mp3)
            pcms = [run("-i",file,"-af",loud,"-ac","1","-ar","8000","-f","s16le") for file in files]
            joined = "".join(f"[{i}:a]" for i in range(len(files)))+f"concat=n={len(files)}:v=0:a=1,"+(f"atempo={TEMPO}," if TEMPO != 1 else "")+loud
            mp3 = run(*(arg for file in files for arg in ("-i",file)),"-filter_complex",joined,"-b:a","64k","-f","mp3")
        pcm = array.array("h",b"".join(pcms))
        if sys.byteorder != "little":
            pcm.byteswap()
        step = max(1,(len(pcm)+63)//64)
        levels = [min(255,round(max(abs(x) for x in pcm[i:i+step])/32768*255))
                  for i in range(0,len(pcm),step)]
        return mp3, levels, [len(p)/16000 for p in pcms]
    except (OSError,ValueError,subprocess.TimeoutExpired):
        print("speech: ffmpeg failed; keeping original audio volume and no waveform.",file=sys.stderr)
        return b"".join(mp3s), [], None


def metadata(alignments, seconds, tempo=1):
    """The words and duration across the pieces' alignments, each piece after the seconds of those before it."""
    words, offset, last = [], 0, 0
    ms = lambda at: round(at*1000/tempo)
    for alignment,length in zip(alignments,seconds):
        starts, ends = alignment["character_start_times_seconds"], alignment["character_end_times_seconds"]
        words += [{"start_ms":ms(offset+starts[m.start()]),"end_ms":ms(offset+ends[m.end()-1]),"text":m[0]}
                  for m in re.finditer(r"\S+","".join(alignment["characters"]))]
        last, offset = offset+(ends or [0])[-1], offset+length
    return {"duration_ms":ms(last),"words":words}


def main():
    if sys.argv[1:] == ["--voices"]:
        return voices()
    text = ear(sys.stdin.read())
    if not text:
        sys.exit("speech: the reply has no words to read.")
    settings = {"stability":1-float(os.environ.get("RAZOR_EXPRESSIVENESS","0.5")),"similarity_boost":0.75}
    texts = list(pieces(text))
    def speak(i):
        body = {"text":texts[i],"model_id":MODEL,"voice_settings":settings}
        if MODEL != "eleven_v3":  # which refuses the neighbouring words that keep the voice even across pieces
            body |= ({"previous_text":texts[i-1]} if i else {})|({"next_text":texts[i+1]} if i+1 < len(texts) else {})
        return call(f"/v1/text-to-speech/{urllib.parse.quote(VOICE,safe='')}/with-timestamps?output_format=mp3_44100_64",body)
    results = list(ThreadPoolExecutor(2).map(speak,range(len(texts))))
    mp3, levels, seconds = audio([base64.b64decode(r["audio_base64"]) for r in results])
    alignments = [r.get("normalized_alignment") or r.get("alignment") for r in results]
    if (path := os.environ.get("RAZOR_WORDS")) and all(alignments):
        seconds = seconds or [(a["character_end_times_seconds"] or [0])[-1] for a in alignments]
        meta = metadata(alignments, seconds, TEMPO if levels else 1)
        Path(path).parent.mkdir(parents=True,exist_ok=True)
        Path(path).write_text(json.dumps({**meta,"levels":levels}))
    sys.stdout.buffer.write(mp3)
    print(f"speech: {sys.argv[-1] if sys.argv[1:] else 'a session'}: {len(text):,} characters spoken"+(f" in {len(texts)} pieces" if len(texts) > 1 else ""),file=sys.stderr)


if __name__ == "__main__":
    try:
        main()
    except (OSError,KeyError,ValueError) as error:
        sys.exit(f"speech: {error}")
