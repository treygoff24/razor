"""A pack directory supplies a resident, a skill, a brand and display settings: the person's config
is laid over the pack's key by key, the pack's relative paths count from the pack, a new
conversation lists the pack's, the project's and then the home folder's skills with descriptions and
the `skills` key's folders are omitted, `skill` reads from the last of them, and the status stream carries the brand and the settings,
again when they change, and keeps the last good ones when the pack goes missing
(docs/design/packs.md)."""
import json
import sys
import tempfile
import time
import tomllib
import urllib.request
from pathlib import Path
from claude import reply
from driver import REPLIES, Reply, flow
from server import send, until, wait

ask, body = "Read the lore skill.", "Tell the flow's old story."
done = (REPLIES / "done.sse").read_bytes()
replies = [reply(("toolu_pack_skill", "mcp__razor__skill", {"name": "lore"}), match=ask),
           Reply(done), Reply(done, match="Scout heartbeat.")]
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
    due = time.strftime("%H:%M:%S", time.localtime(time.time() + 6))
    config = ('skills = ["library"]\n[brand]\nname = "Acme"\n[brand.colours]\nviolet = "#336699"\n'
              '[ui]\nmotion = false\n'
              '[residents.scout]\ncwd = "work"\nstartup = ["scout.md", "razor"]\n'
              f'[residents.scout.heartbeat]\nat = ["{due}"]\ntext = "Scout heartbeat."\n')
    (pack / "config.toml").write_text(config)
    with flow(replies, config=f'pack = {json.dumps(str(pack))}\n[brand]\nname = "Mine"\n'
                              '[ui]\nblocks = "half"\n'
                              '[server]\naddress = "127.0.0.1:0"') as f:
        # 1. The pack's, the project's and the person's own skills are listed with their
        # descriptions, the project's over the person's of the same name, the `skills` folder's
        # are omitted from the first request; `skill` still reads the last.
        skill(f.cwd / ".agents/skills/plan", "description: Plans the flow.")
        skill(f.home / ".claude/skills/notes", "description: Keeps notes.")
        skill(f.home / ".agents/skills/plan", "description: Plans at home.")
        result = f.run("--print=json", ask)
        assert result.returncode == 0, result.stderr
        session = json.loads(result.stdout)["session"]
        output = next(e for e in f.record(session) if e["kind"] == "result")
        assert output["outcome"] == "success" and body in output["output"], output
        first = next(r.read_text() for r in sorted(f.requests.glob("*.body")) if ask in r.read_text())
        assert "- greet: Greets the flow in one line." in first, "the pack's skill was not listed"
        assert "- razor-settings:" not in first, "the named pack kept the starter skills"
        assert "- plan: Plans the flow." in first, "the project's skill was not listed"
        assert "- notes: Keeps notes." in first and "Plans at home" not in first, "the home skills were not listed after the project's"
        assert "\\n- lore" not in first and "Tells old stories" not in first, "the opening listed a library skill"
        assert "lore" in [s["name"] for s in json.loads(send(f, "GET", f"/sessions/{session}/skills").read())]
        # 2. The pack's resident starts on its own, in and with paths relative to the pack.
        until(f, "scout", lambda es: any(e["kind"] == "turn_end" for e in es), timeout=40)
        start = next(f.record("scout"))
        assert start["cwd"] == str(pack / "work"), start
        beat = next(r.read_text() for r in f.requests.glob("*.body") if "Scout heartbeat." in r.read_text())
        assert "You are Scout" in beat, "the pack's persona missed the model"
        # 3. The status stream opens with the brand: the person's name over the pack's colours.
        server = json.loads((f.home / ".razor/server.json").read_text())
        key = (f.home / ".razor/device.key").read_text().strip()
        status = urllib.request.Request(f"http://{server['address']}/v1/status",
                                        headers={"Authorization": f"Bearer {key}"})
        with urllib.request.urlopen(status, timeout=10) as stream:
            line = json.loads(stream.readline())
            assert line["kind"] == "agents", line
            assert line["brand"] == {"name": "Mine", "colours": {"violet": "#336699"}}, line["brand"]

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
            assert line["brand"] == {"name": "Mine", "colours": {"violet": "#993366"}}, line["brand"]

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
    result = f.run("--print=json", "Ask the person.")
    assert result.returncode == 0, result.stderr
    first = json.loads((f.requests / "001.body").read_text())
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
    session = json.loads(result.stdout)["session"]
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
    config("set", "models.sol.window", "12345")
    assert config("get", "models.sol.window").splitlines()[0] == "12345"
    assert str(person) in config("get", 'models."gpt-6.1-sol".window')
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
    (pack / "config.toml").write_text('[models.custom]\nbase_url="http://127.0.0.1:1"\nvariant="plain.json"\n')
    target.write_text(f'pack={json.dumps(str(pack))}\n')
    result = f.run("config", "set", "models.custom.window", "12345")
    check("variant retains file spelling", result.returncode == 0 and
          tomllib.loads(person.read_text())["models"]["custom"]["variant"] == "plain.json")
    if not person.is_symlink():
        person.unlink()
        person.symlink_to(target)
    target.write_text('[models."gpt-6.1-sol"]\nwindow=12345\n')
    result = f.run("config", "set", "models.sol.window", "12346")
    check("model alias explains existing spelling", result.returncode != 0 and
          "gpt-6.1-sol" in result.stderr)
    target.write_text('')
    result = f.run("config", "set", "mcp.other.enabled", "false")
    check("intermediate tables are implicit", result.returncode == 0 and
          "[mcp]\n" not in target.read_text())
    assert not failures, failures
