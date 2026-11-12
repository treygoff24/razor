"""A pack directory supplies a resident, a skill, a brand and display settings: the person's config
is laid over the pack's key by key, the pack's relative paths count from the pack, a new
conversation lists the pack's, the project's and then the home folder's skills with descriptions and
the `skills` key's folders are omitted, `skill` reads from the last of them, and the status stream carries the brand and the settings,
again when they change, and keeps the last good ones when the pack goes missing
(docs/design/packs.md). The starter's avatar script makes a sheet its own checker accepts."""
import json
import subprocess
import sys
import tempfile
import time
import tomllib
from pathlib import Path
from claude import reply
from driver import REPLIES, Reply, flow
from png import png
from server import post, send, start as start_server, stop, until, wait

ask, body = "Read the lore skill.", "Tell the flow's old story."
done = (REPLIES / "done.sse").read_bytes()
replies = [reply(("toolu_pack_skill", "mcp__razor__skill", {"name": "lore"}), match=ask),
           Reply(done), Reply(done, match="Scout, report.")]
with tempfile.TemporaryDirectory() as scratch:
    pack = Path(scratch) / "pack"
    (pack / "work").mkdir(parents=True)
    def skill(folder, front, body=""):
        folder.mkdir(parents=True)
        (folder / "SKILL.md").write_text(f"---\n{front}\n---\n{body}")
    skill(pack / "skills/greet", "name: greet\ndescription: >-\n  Greets the flow\n  in one line.", "Say hello.\n")
    (pack / "skills/plan").mkdir()  # no SKILL.md, so the project's `plan` is still listed
    skill(pack / "library/lore", "description: Tells old stories.", f"{body}\n")
    (pack / "scout.md").write_text("You are Scout, this pack's resident.")
    config = ('skills = ["library"]\n[brand]\nname = "Acme"\n[brand.colours]\nviolet = "#336699"\n'
              '[ui]\nmotion = false\n'
              '[residents.scout]\ncwd = "work"\nstartup = ["scout.md", "razor"]\n')
    (pack / "config.toml").write_text(config)
    with flow(replies, config=f'pack = {json.dumps(str(pack))}\n[brand]\nname = "Mine"\n'
                              '[ui]\nblocks = "half"\n') as f:
        # 1. The pack's, the project's and the person's own skills are listed with their
        # descriptions, the project's over the person's of the same name, the `skills` folder's
        # are omitted from the first request; `skill` still reads the last.
        skill(f.cwd / ".agents/skills/plan", "description: Plans the flow.")
        skill(f.home / ".claude/skills/notes", "description: Keeps notes.")
        skill(f.home / ".agents/skills/plan", "description: Plans at home.")
        session = f.ask(ask)["session"]
        output = next(e for e in f.record(session) if e["kind"] == "result")
        assert output["outcome"] == "success" and body in output["output"], output
        first = next(body for body in f.sent() if ask in body)
        assert "- greet: Greets the flow in one line." in first, "the pack's skill was not listed"
        assert "- razor:" not in first, "the named pack kept the starter skills"
        assert "- plan: Plans the flow." in first, "the project's skill was not listed"
        assert "- notes: Keeps notes." in first and "Plans at home" not in first, "the home skills were not listed after the project's"
        assert "\\n- lore" not in first and "Tells old stories" not in first, "the opening listed a library skill"
        assert "lore" in [s["name"] for s in json.loads(send(f, "GET", f"/sessions/{session}/skills").read())]
        # 2. The pack's resident starts on its own, in and with paths relative to the pack.
        start = until(f, "scout", lambda es: es)[0]
        assert start["kind"] == "start" and start["cwd"] == str(pack / "work"), start
        post(f, "/sessions/scout/input", {"text": "Scout, report."})
        until(f, "scout", lambda es: any(e["kind"] == "turn_end" for e in es))
        beat = next(body for body in f.sent() if "Scout, report." in body)
        assert "You are Scout" in beat, "the pack's persona missed the model"
        # 3. The status stream opens with the brand: the person's name over the pack's colours.
        with send(f, "GET", "/status") as stream:
            line = json.loads(stream.readline())
            assert line["kind"] == "agents", line
            assert line["brand"] == {"name": "Mine", "colours": {"violet": "#336699"}, "avatars": {}, "resident_avatars": {}}, line["brand"]

            def snapshot(test, why):
                """The next `agents` line passing `test`, within 3 s."""
                deadline = time.monotonic() + 3
                try:
                    while True:
                        line = json.loads(stream.readline())
                        assert time.monotonic() < deadline, why
                        if line["kind"] == "agents" and test(line):
                            return line
                except TimeoutError:
                    raise AssertionError(why) from None

            # 4. A changed pack file reaches the stream without a restart.
            (pack / "config.toml").write_text(config.replace("#336699", "#993366"))
            snapshot(lambda l: l["brand"]["colours"] == {"violet": "#993366"},
                     "the changed brand was not sent within 3 s")
            # 5. A missing pack is refused by name, and the whole last good config stays.
            person = f.home / ".razor/config.toml"
            missing = Path(scratch) / "gone"
            person.write_text(person.read_text().replace(str(pack), str(missing)))
            line = snapshot(lambda l: str(missing) in (l["config"] or ""),
                            "the missing pack was not named within 3 s")
            assert line["brand"] == {"name": "Mine", "colours": {"violet": "#993366"}, "avatars": {}, "resident_avatars": {}}, line["brand"]

# A missing picture cannot prevent startup; it is omitted and reported.
with flow([]) as f:
    person = f.home / ".razor/config.toml"
    picture = person.with_name("missing.png")
    picture.write_bytes(REPLIES.joinpath("avatar.png").read_bytes())
    person.write_text(person.read_text() + '\n[brand.avatars]\nowl={file="missing.png"}\n')
    start_server(f)
    stop(f)
    picture.unlink()
    result = f.run("config", "get", "ui.blocks")
    assert result.returncode == 0, result.stderr
    start_server(f)
    with send(f, "GET", "/status?avatars=true") as stream:
        line = json.loads(stream.readline())
        assert line["brand"]["avatars"] == {}, line
        assert "brand.avatars.owl.file does not load" in line["config"] and "missing.png" not in line["config"], line

# Avatar pictures are opt-in; reloads carry them only to clients that ask.
with flow([]) as f:
    import base64
    sheet = png(132, 33, (b"\0" + bytes([180,40,40,255])*132)*33, colour=6)
    person = f.home / ".razor/config.toml"
    picture = person.with_name("owl.png")
    picture.write_bytes(sheet)
    broken = [png(33,33,(b"\0"+bytes(33*4))*33,colour=6),
              png(132,33,(b"\0"+bytes(132*3))*33), sheet[:50], sheet+bytes(2049)]
    for n, data in enumerate(broken):
        person.with_name(f"bad{n}.png").write_bytes(data)
    config = person.read_text() + '\n[brand.avatars]\nowl = {file="owl.png", pool=true}\n'
    person.write_text(config)
    start_server(f)
    with send(f, "GET", "/status?avatars=true") as stream, send(f, "GET", "/status") as plain:
        line = json.loads(stream.readline())
        assert line["brand"]["avatars"] == {"owl": base64.b64encode(sheet).decode()}, line
        assert json.loads(plain.readline())["brand"]["avatars"] == {}
        def changed(test):
            deadline = time.monotonic() + 5
            while True:
                line = json.loads(stream.readline())
                assert time.monotonic() < deadline, line
                if line["kind"] == "agents" and test(line):
                    return line
        person.write_text(config.replace("owl =", "bird ="))
        changed(lambda l: "bird" in l["brand"]["avatars"])
        while (line := json.loads(plain.readline()))["kind"] != "agents":
            pass
        assert line["brand"]["avatars"] == {}, line
        for invalid, count, notice in (
                (config.replace("owl.png", "missing.png"), 0, "No such file"),
                (config + "".join(f'a{n} = {{file="owl.png"}}\n' for n in range(16)), 16, "at most 16"),
                *((config.replace("owl.png", f"bad{n}.png"), 0, "2 KiB" if n == 3 else "brand.avatars.owl.file") for n in range(len(broken)))):
            person.write_text(invalid)
            bad = changed(lambda l: bool(l["config"]))
            assert len(bad["brand"]["avatars"]) == count and notice in bad["config"], bad
            person.write_text(config.replace("owl =", "bird ="))
            changed(lambda l: l["config"] is None)
    person.write_text(config)
    for key, value in (("brand.avatars.owl.file", "owl.png"), ("brand.avatars.owl.pool", "false")):
        result = f.run("config", "set", key, value)
        assert result.returncode == 0, result.stderr
    assert tomllib.loads(person.read_text())["brand"]["avatars"]["owl"] == {"file": "owl.png", "pool": False}
    # Pack-relative paths are resolved from the pack.
    pack = person.parent / "pack"
    pack.mkdir()
    pack.joinpath("face.png").write_bytes(sheet)
    pack.joinpath("config.toml").write_text('[brand.avatars]\npack={file="face.png"}\n')
    assert f.run("config", "unset", "brand.avatars").returncode == 0
    result = f.run("config", "set", "pack", str(pack))
    assert result.returncode == 0, result.stderr
    value = f.run("config", "get", "brand.avatars.pack.file")
    assert json.loads(value.stdout.splitlines()[0]) == str(pack / "face.png"), value.stderr

# With no named pack a stranger gets the starter, including the about word, skills and actual
# notification/speech programs. Both desktop commands record here, ahead of driver's no-ops.
with flow([reply((None, "mcp__razor__message", {"to": "parent", "type": "question",
                 "text": "QUESTION_STARTER_30", "options": ["yes", "no"]})),
           reply(text="waiting")]) as f:
    programs, notice = f.home / "bin", f.home / "notice.json"
    programs.mkdir()
    for name in ("osascript", "notify-send"):
        program = programs / name
        program.write_text(f'#!{sys.executable}\nimport json,sys\nfrom pathlib import Path\nPath({str(notice)!r}).write_text(json.dumps(sys.argv[1:]))\n')
        program.chmod(0o755)
    f.env["PATH"] = f"{programs}:{f.env['PATH']}"
    (f.home / ".razor/about.md").write_text("ABOUT_STARTER_30 knows the project.")
    session = f.ask("Ask the person.")["session"]
    first = f.request(1)
    opening = json.dumps(first)
    starter = Path(__file__).resolve().parents[1] / "packs/starter"
    # These are prompt data, compared to the files, so editing prose does not break the flow.
    for file in ("persona.md", "rules.md"):
        assert (starter / file).read_text().strip() in "\n".join(
            block.get("text", "") for block in first["system"]), f"starter {file} missed the model"
    assert "ABOUT_STARTER_30" in opening, "about.md missed the model"
    for skill in (path.name for path in (starter / "skills").iterdir()):  # each one listed
        assert f"- {skill}:" in opening, f"starter skill {skill} was not listed"
    wait(notice.exists, 10, "starter question never reached the desktop program")
    assert "QUESTION_STARTER_30" in " ".join(json.loads(notice.read_text())), "question text missed notifier"
    said = next(e for e in f.record(session) if e["kind"] == "text")
    answer = send(f, "GET", f"/sessions/{session}/speech/{said['id']}")
    why = answer.read().decode()
    assert answer.status == 500 and "razor keys" in why and "ElevenLabs" in why, why

# The local CLI preserves comments, uses the loader's layers and refuses a bad edit before save.
with flow([], config='# keep this comment\n[ui]\nmotion = false # keep this too\n') as f:
    person = f.home / ".razor/config.toml"
    f.base()

    def config(*words):
        result = f.run("config", *words)
        assert result.returncode == 0, result.stderr
        return result.stdout

    assert config("path").strip() == str(person)
    config("set", "ui.blocks", "half")
    assert tomllib.loads(person.read_text())["ui"]["blocks"] == "half"
    assert "# keep this comment" in person.read_text() and "# keep this too" in person.read_text()
    assert config("get", "ui.blocks").splitlines()[0] == '\"half\"'
    assert str(person) in config("get", "ui.blocks")
    for key, value in (("ui.blocks", "wrong"), ("ui.motion", "1"),
                       ("ui.motin", "false"), ("brand.nam", "wrong"),
                       ("phone.detai", "wrong"), ("rebuild.threshold", "0"),
                       ("unknown", "true")):
        before = person.read_bytes()
        refused = f.run("config", "set", key, value)
        assert refused.returncode != 0 and person.read_bytes() == before, (key, refused.stderr)
        assert str(person) in refused.stderr, refused.stderr
    refused = f.run("config", "set", "brand.resident_avatars.scout", '"owl"')
    assert refused.returncode != 0 and "residents.<name>.avatar" in refused.stderr and person.read_bytes() == before, refused
    config("set", "models.sol.window", "12345")
    assert config("get", "models.sol.window").splitlines()[0] == "12345"
    assert str(person) in config("get", 'models."GPT 6.1 Sol".window')
    config("unset", "ui.blocks")
    assert "blocks" not in tomllib.loads(person.read_text())["ui"]
    assert config("get", "ui.blocks").splitlines()[0] == '\"auto\"'
    assert "built-in default" in config("get", "ui.blocks")
    shared, private = f.cwd / ".razor/config.toml", f.cwd / ".razor/config.local.toml"
    config("set", "children.max", "8", "--project")
    assert str(shared) in config("get", "children.max")
    config("set", "children.max", "3", "--local")
    assert config("get", "children.max").splitlines()[0] == "3"
    assert str(private) in config("get", "children.max")
    assert ".razor/config.local.toml" in (f.cwd / ".git/info/exclude").read_text()
    before = shared.read_bytes()
    refused = f.run("config", "set", "ui.blocks", "half", "--project")
    assert refused.returncode != 0 and "cannot set ui" in refused.stderr, refused.stderr
    assert shared.read_bytes() == before
    config("unset", "children.max", "--local")
    assert config("get", "children.max").splitlines()[0] == "8"
    config("set", "tasks.remote", "origin", "--local")
    assert tomllib.loads(private.read_text())["tasks"]["remote"] == "origin"
    # The pack's whole ui.keys entry must survive a nested override in the person's file.
    pack = f.home / "pack"
    pack.mkdir()
    (pack / "config.toml").write_text('[ui.keys.global]\nstatus=["Ctrl-Y"]\nprefix=["Ctrl-B"]\n')
    config("set", "pack", str(pack))
    config("set", "ui.keys.global.status", '["F8"]')
    keys = tomllib.loads(person.read_text())["ui"]["keys"]["global"]
    assert keys == {"status": ["F8"], "prefix": ["Ctrl-B"]}, keys
    assert config("get", "ui.keys.global.prefix").splitlines()[0] == '["Ctrl-B"]'
    # A non-existent pack and a misspelled nested MCP field are rejected by the real loader.
    for key, value in (("pack", str(f.home / "missing-pack")),
                       ("mcp.typo", '{ command="echo", commnad="echo" }')):
        before = person.read_bytes()
        refused = f.run("config", "set", key, value)
        assert refused.returncode != 0 and person.read_bytes() == before, refused.stderr
    assert not (f.home / ".razor/server.json").exists(), "config started a server"

# Scope boundaries, repair of invalid files, and preservation of file spelling and comments.
with flow([], config='[mcp.gh]\ncommand="echo"\n[mcp.gh.env]\nGITHUB_TOKEN="fixture-secret"\n') as f:
    f.base()
    person = f.home / ".razor/config.toml"
    shared = f.cwd / ".razor/config.toml"
    private = f.cwd / ".razor/config.local.toml"
    failures = []
    def check(name, condition):
        if not condition:
            failures.append(name)
    result = f.run("config", "set", "mcp.gh.enabled", "false", "--project")
    check("shared excludes personal credentials", result.returncode == 0 and
          "fixture-secret" not in shared.read_text())
    shared.write_text('[mcp.project]\ncommand="./project-server"\n')
    result = f.run("config", "set", "mcp.project.enabled", "false")
    check("personal excludes project entries", result.returncode == 0 and
          "project-server" not in person.read_text())
    shared.write_text('[mcp.local]\ncommand="./server"\n')
    result = f.run("config", "set", "mcp.local.enabled", "false", "--local")
    check("private preserves source spelling", result.returncode == 0 and
          tomllib.loads(private.read_text())["mcp"]["local"]["command"] == "./server")
    result = f.run("config", "set", "mcp.gh.enabled", "false", "--local")
    check("private inherits personal entry", result.returncode == 0 and
          tomllib.loads(private.read_text())["mcp"]["gh"]["env"]["GITHUB_TOKEN"] == "fixture-secret")
    person.write_text('[ui]\nmotin=false\n')
    result = f.run("config", "unset", "ui.motin")
    check("unset repairs personal typo", result.returncode == 0 and "motin" not in person.read_text())
    person.write_text('')
    shared.write_text('[children]\nmx=1\n')
    result = f.run("config", "set", "ui.motion", "false")
    check("bad project does not block personal edit", result.returncode == 0)
    shared.write_text('[children] # keep parent\nmax=2\n')
    result = f.run("config", "unset", "children.max", "--project")
    check("empty parent comment survives", result.returncode == 0 and "# keep parent" in shared.read_text())
    target = person.parent / "target.toml"
    person.rename(target)
    person.symlink_to(target)
    result = f.run("config", "set", "ui.motion", "true")
    check("symlink survives", result.returncode == 0 and person.is_symlink() and
          tomllib.loads(target.read_text())["ui"]["motion"] is True)
    if not person.is_symlink():
        person.unlink()
        person.symlink_to(target)
    pack = f.home / "test-pack"
    pack.mkdir()
    (pack / "plain.json").write_text('{}')
    (target.parent / "plain.json").write_text('{}')
    (pack / "config.toml").write_text('[providers.custom]\nbase_url="http://127.0.0.1:1"\n[models."custom/custom"]\nvariant="plain.json"\n')
    target.write_text(f'pack={json.dumps(str(pack))}\n')
    result = f.run("config", "set", 'models."custom/custom".window', "12345")
    check("variant retains file spelling", result.returncode == 0 and
          tomllib.loads(person.read_text())["models"]["custom/custom"]["variant"] == "plain.json")
    if not person.is_symlink():
        person.unlink()
        person.symlink_to(target)
    target.write_text('[models.sol]\nwindow=12345\n')
    result = f.run("config", "set", 'models."GPT Sol".window', "12346")
    check("model alias explains existing spelling", result.returncode != 0 and
          "as sol;" in result.stderr)
    target.write_text('')
    result = f.run("config", "set", "mcp.other.enabled", "false")
    check("intermediate tables are implicit", result.returncode == 0 and
          "[mcp]\n" not in target.read_text())
    assert not failures, failures

# The avatar skill's script turns a face on a plain ground into a sheet its own checker accepts and
# refuses a picture of the wrong size and a face with no eyes, each with its reason.
def face(eyes):
    """A 64 × 64 RGBA PNG: a face, with two dark eyes when `eyes`, on a plain blue ground."""
    def colour(x, y):
        if eyes and 22 <= y < 30 and (18 <= x < 26 or 38 <= x < 46):
            return (20, 20, 30, 255)
        return (240, 200, 150, 255) if 8 <= x < 56 and 8 <= y < 56 else (40, 60, 200, 255)
    return png(64, 64, b"".join(b"\0" + b"".join(bytes(colour(x, y)) for x in range(64)) for y in range(64)), colour=6)

script = Path(__file__).resolve().parents[1] / "packs/starter/skills/agent-avatar/avatar.js"
with tempfile.TemporaryDirectory() as scratch:
    def avatar(*words):
        return subprocess.run(["node", script, *words], capture_output=True, text=True, cwd=scratch)
    for name, eyes in (("face", True), ("blank", False)):
        (Path(scratch) / f"{name}.png").write_bytes(face(eyes))
    made = avatar("make", "face.png", "character", "sheet.png")
    assert made.returncode == 0 and (Path(scratch) / "sheet-preview.png").exists(), made.stderr
    checked = avatar("check", "sheet.png")
    assert checked.returncode == 0 and json.loads(checked.stdout)["bytes"] <= 2048, checked.stderr
    for words, reason in ((("check", "face.png"), "132"), (("make", "blank.png", "character", "x.png"), "eyes")):
        refused = avatar(*words)
        assert refused.returncode == 1 and reason in refused.stderr, refused.stderr
