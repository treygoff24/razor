"""Machine skills update through real Git and a stand-in vendor CLI, under one cross-home lock.
The pack stays immutable and its moving skill is published through a stable override link.
"""
import json
import subprocess
import shutil
import sys
import time
from claude import reply
from driver import BINARY, REPLIES, Reply, flow
from server import send, start, stop, wait


def git(directory, *args):
    result = subprocess.run(["git", "-c", "user.name=Flow", "-c", "user.email=flow@example.invalid",
                             *args], cwd=directory, capture_output=True, text=True, check=True)
    return result.stdout.strip()


def skill(path, text):
    path.mkdir(parents=True, exist_ok=True)
    (path / "SKILL.md").write_text(f"---\ndescription: Flow skill.\n---\n{text}\n")


with flow([reply(("toolu_update", "mcp__razor__skill", {"name": "moving"})),
           Reply((REPLIES / "done.sse").read_bytes())] * 2, config='pack="../../pack"\n') as f:
    root = f.home.parent
    upstream, pack, library = (root / name for name in ("upstream", "pack", "library"))
    upstream.mkdir()
    git(upstream, "init", "-b", "main")
    skill(upstream / "moving", "original")
    (upstream / "moving/link").symlink_to("../../decoy.txt")
    (upstream / ".gitignore").write_text("ignored\n")
    git(upstream, "add", ".")
    git(upstream, "commit", "-m", "original")
    original = git(upstream, "rev-parse", "HEAD")
    git(root, "clone", "--bare", str(upstream), str(root / "remote.git"))
    git(upstream, "remote", "add", "origin", str(root / "remote.git"))
    checkouts = {}
    for name in ("clean", "dirty", "ignored", "detached", "untracked", "diverged", "race-dirty", "race-branch"):
        checkout = root / name
        git(root, "clone", str(root / "remote.git"), str(checkout))
        checkouts[name] = checkout
        (f.home / ".agents/skills").mkdir(parents=True, exist_ok=True)
        (f.home / ".agents/skills" / name).symlink_to(checkout / "moving")
    (checkouts["dirty"] / "moving/SKILL.md").write_text("local edit")
    (checkouts["ignored"] / "ignored").write_text("local ignored file")
    git(checkouts["detached"], "checkout", "--detach")
    git(checkouts["untracked"], "branch", "--unset-upstream")
    (checkouts["diverged"] / "local").write_text("local commit")
    git(checkouts["diverged"], "add", ".")
    git(checkouts["diverged"], "commit", "-m", "local")
    diverged = git(checkouts["diverged"], "rev-parse", "HEAD")
    (f.home / ".claude/skills").mkdir(parents=True)
    (f.home / ".claude/skills/alias").symlink_to(checkouts["clean"] / "moving")
    skill(library / "vendor", "locally tailored, vendor owns replacement")
    (library / "broken").symlink_to("broken")
    skill(library / "unmanaged", "keep")
    skill(library / "-option", "keep")
    pack.mkdir()
    (pack / "config.toml").write_text("")
    skill(pack / "skills/moving", "bundled")
    skill(pack / "skills/pinned", "pinned")
    for name, ref in (("moving", "main"), ("pinned", original)):
        (pack / "skills" / name / "UPSTREAM").write_text(f"{root / 'remote.git'} {ref} moving {original}\n")
    skill(f.cwd / ".agents/skills/project", "project")
    (f.cwd / "skills-lock.json").write_text(json.dumps({"version": 1, "skills": {"project": {}}}))
    (f.home / ".agents/.skill-lock.json").write_text(json.dumps({"version": 3, "skills": {
        name: {} for name in ("vendor", "missing", "clean", "alias", "project", "-option")}}))
    programs = root / "bin"
    programs.mkdir()
    (programs / "npx").write_text(f'''#!{sys.executable}
import json, os, subprocess, sys, time
from pathlib import Path
root = Path({str(root)!r})
with (root / "calls").open("a") as file:
    file.write(json.dumps({{"argv":sys.argv[1:], "scripts":os.environ.get("npm_config_ignore_scripts"), "telemetry":os.environ.get("DISABLE_TELEMETRY"), "color":os.environ.get("NO_COLOR"), "cwd":os.getcwd()}})+"\\n")
if (root / "hang").exists():
    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(600)"])
    (root / "child").write_text(str(child.pid))
    time.sleep(600)
Path({str(library / 'vendor/SKILL.md')!r}).write_text("vendor replacement")
print("stand-in vendor stdout", flush=True)
print("stand-in vendor failure", file=sys.stderr)
sys.exit(42)
''')
    (programs / "npx").chmod(0o755)
    # A real fetch can finish after a manual edit or branch switch. Git's merge alone permits both.
    (programs / "git").write_text(f'''#!{sys.executable}
import subprocess, sys
from pathlib import Path
real = {shutil.which('git')!r}
result = subprocess.run([real, *sys.argv[1:]])
if result.returncode == 0 and sys.argv[1:2] == ["fetch"]:
    if Path.cwd().name == "race-dirty":
        Path("ignored").write_text("edit during fetch")
        Path("untracked").write_text("edit during fetch")
    elif Path.cwd().name == "race-branch":
        subprocess.run([real, "switch", "-c", "other"], check=True)
sys.exit(result.returncode)
''')
    (programs / "git").chmod(0o755)
    f.env["PATH"] = f"{programs}:{f.env['PATH']}"
    config = f.home / ".razor/config.toml"
    # Remove the fixture's opt-out: the product's real default must enable this run.
    config.write_text(config.read_text().replace("skill_updates = false\n", "") +
                      f'\n[residents.clock]\ncwd={json.dumps(str(f.cwd))}\n')
    config.write_text('skills = [' + json.dumps(str(library)) + ']\n' + config.read_text())
    skill(upstream / "moving", "updated upstream")
    git(upstream, "add", ".")
    git(upstream, "commit", "-m", "update")
    git(upstream, "push", "origin", "main")
    updated = git(upstream, "rev-parse", "HEAD")
    log = f.home / ".razor/server.log"
    # launchd connects only stderr to the log; inherited stdout would lose vendor results.
    with log.open("a") as output:
        running = subprocess.Popen([BINARY, "serve"], env=f.env, cwd=f.cwd,
                                   stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
    try:
        override = f.home / ".razor/skills/moving"
        wait(lambda: override.is_symlink(), 30, "moving starter was not published")
        assert git(checkouts["clean"], "rev-parse", "HEAD") == updated
        assert (checkouts["clean"] / "moving/SKILL.md").read_text() == (upstream / "moving/SKILL.md").read_text()
        unchanged = ("dirty", "ignored", "detached", "untracked", "race-dirty", "race-branch")
        moved = [name for name in unchanged if git(checkouts[name], "rev-parse", "HEAD") != original]
        assert not moved, moved
        assert (checkouts["dirty"] / "moving/SKILL.md").read_text() == "local edit"
        assert git(checkouts["diverged"], "rev-parse", "HEAD") == diverged
        assert git(checkouts["race-branch"], "symbolic-ref", "HEAD") == "refs/heads/other"
        assert log.read_text().count(f"{checkouts['clean']} {original} -> {updated}") == 1
        calls = [json.loads(line) for line in (root / "calls").read_text().splitlines()]
        assert calls == [{"argv": ["--yes", "skills@1.7.6", "update", "--global", "--yes", "vendor"],
                          "scripts": "true", "telemetry": "1", "color": "1", "cwd": str(f.home)}], calls
        assert (library / "vendor/SKILL.md").read_text() == "vendor replacement"
        assert "stand-in vendor stdout" in log.read_text()
        assert "stand-in vendor failure" in log.read_text() and "42" in log.read_text()
        assert (pack / "skills/moving/SKILL.md").read_text().endswith("bundled\n")
        assert updated in (override / "UPSTREAM").read_text()
        assert not (override / "link").is_symlink()
        assert (override / "link").read_text() == "../../decoy.txt"
        assert not (override.parent / "pinned").exists()
        # Later skill lookup sees the published folder, while a retained path follows replacements.
        session = f.ask("Read the moving skill.")["session"]
        loaded = next(entry for entry in f.record(session) if entry["kind"] == "result")
        assert "updated upstream" in loaded["output"] and str(override) in loaded["output"], loaded
        catalog = json.loads(send(f, "GET", "/sessions/clock/skills").read())
        assert "moving" in [entry["name"] for entry in catalog]
        rejected = f.run("config", "set", "skill_updates", "false", "--project")
        assert rejected.returncode != 0 and "cannot set skill_updates" in rejected.stderr
        # Opting out is live: the next load restores the pack, and a restart launches no updater.
        enabled = config.read_text()
        config.write_text("skill_updates = false\n" + enabled)
        loaded = next(entry for entry in f.record(f.ask("Read the moving skill again.")["session"]) if entry["kind"] == "result")
        assert "bundled" in loaded["output"] and str(override) not in loaded["output"], loaded
    finally:
        running.terminate()
        running.wait(timeout=30)
    start(f)
    assert send(f, "GET", "/residents").status == 200
    # Watch long enough for an accidentally enabled asynchronous run to reach the CLI.
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        assert len((root / "calls").read_text().splitlines()) == 1, "disabled updater ran"
        time.sleep(0.05)
    stop(f)
    config.write_text(enabled)
    # A release replaces the immutable pack; the existing override survives and moves again.
    (pack / "skills/moving/SKILL.md").write_text("new release bundle")
    skill(upstream / "moving", "second upstream")
    git(upstream, "add", ".")
    git(upstream, "commit", "-m", "second update")
    git(upstream, "push", "origin", "main")
    previous = override.resolve()
    # A real second Razor home shares the user's lock while a subprocess is running.
    (root / "hang").touch()
    start(f)
    wait(lambda: (root / "child").exists(), 20, "vendor stand-in never started")
    other = f.home / "other-razor"
    other.mkdir()
    (other / "config.toml").write_text(config.read_text())
    try:
        result = subprocess.run([BINARY, "devices"], cwd=f.cwd,
                                env={**f.env, "RAZOR_HOME": str(other)}, capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, result.stderr
        wait(lambda: "holds the lock" in (other / "server.log").read_text(), 10, "second home did not skip")
        assert len((root / "calls").read_text().splitlines()) == 2
        # Timeout kills the entire process group; the server still answers a resident request.
        wait(lambda: "exceeded two minutes" in log.read_text(), 135, "vendor timeout was not reported")
        child = int((root / "child").read_text())
        wait(lambda: subprocess.run(["ps", "-p", str(child), "-o", "stat="], capture_output=True,
                                    text=True).stdout.strip().strip("s+") in ("", "Z"), 10, "timed-out descendant survived")
        assert send(f, "GET", "/residents").status == 200
        wait(lambda: "second upstream" in (override / "SKILL.md").read_text(), 20, "override did not move")
        assert previous.is_dir() and previous != override.resolve()
        assert (pack / "skills/moving/SKILL.md").read_text() == "new release bundle"
    finally:
        stop(other)
    stop(f)
    # Unsupported vendor provenance skips that tool; a third starter version prunes only the oldest.
    (root / "hang").unlink()
    (f.home / ".agents/.skill-lock.json").write_text('{"version":99,"skills":{"vendor":{}}}')
    skill(upstream / "moving", "third upstream")
    git(upstream, "add", ".")
    git(upstream, "commit", "-m", "third update")
    git(upstream, "push", "origin", "main")
    current = override.resolve()
    start(f)
    wait(lambda: "third upstream" in (override / "SKILL.md").read_text(), 20, "third override was not published")
    wait(lambda: not previous.exists(), 10, "oldest override was not pruned")
    assert current.is_dir() and len(list(override.parent.glob(".moving@*"))) == 2
    assert len((root / "calls").read_text().splitlines()) == 2
    assert "unsupported global lock" in log.read_text()
    stop(f)
    # A new pack can pin, unmark or remove skills; only updater-owned copies are retired.
    for name in ("unmarked", "removed"):
        version = override.parent / f".{name}@{original}"
        shutil.copytree(override.resolve(), version)
        (override.parent / name).symlink_to(version.name)
    skill(pack / "skills/unmarked", "unmarked bundle")
    (pack / "skills/moving/UPSTREAM").write_text(f"{root / 'remote.git'} {original} moving {original}\n")
    skill(override.parent / "personal", "keep personal folder")
    (override.parent / "external").symlink_to(library / "unmanaged")
    start(f)
    retired = ("moving", "unmarked", "removed")
    wait(lambda: all(not (override.parent / name).is_symlink() and
                     not list(override.parent.glob(f".{name}@*")) for name in retired),
         20, "obsolete starter copies still hide the pack")
    assert (override.parent / "personal/SKILL.md").is_file()
    assert (override.parent / "external").is_symlink()
