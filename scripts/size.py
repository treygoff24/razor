#!/usr/bin/env python3
"""Count tracked source per part; exit 1 when the total exceeds this release's ceiling in size.tsv."""
from pathlib import Path
import re
import subprocess
import sys

# Each part's directories, named as size.tsv's columns; a file directly in src/ is its own.
PARTS = {"Protocol": "src/protocol", "Controller": "src/controller", "Record": "src/record",
         "Context": "src/profiles", "Models": "src/responses src/claude bridge",
         "Tools": "src/tools src/code_mode", "Shell": "src/shell",
         "Children": "src/children src/residents src/memory src/launcher.rs",
         "API": "src/api src/config src/devices.rs src/devices src/main.rs src/lib.rs", "Extensions": "src/extensions",
         "Terminal": "src/terminal", "Network": "network", "Flows": "flows scripts eval", "Performance": "perf",
         "Skills": "packs"}
# Parts that are not the product: their growth is not reported, and the release's rewrite never takes them.
NOT_PRODUCT = {"Flows", "Performance", "Skills"}
COMMENTS = {".rs": "//", ".ts": "//", ".js": "//", ".py": "#", ".sh": "#", ".swift": "//"}

root = Path(__file__).resolve().parents[1]
# The last row is the release being built; the one before it, the last release, measured.
try:
    header, *releases = (line.split("\t") for line in (root / "scripts/size.tsv").read_text().splitlines())
    last, building = (dict(zip(header, row)) for row in releases[-2:])
    if len(last) < len(header):
        raise ValueError("the last release's row is missing a column")
    released, ceiling = last["release"], int(building["ceiling"])
    history = {part: int(last[part]) for part in PARTS if part not in NOT_PRODUCT}
except (OSError, ValueError, KeyError) as error:
    sys.exit(f"FAIL size: scripts/size.tsv needs a header, the last release's counts and a numeric ceiling: {error}")
modules = {directory: part for part, directories in PARTS.items() for directory in directories.split()}
counts = dict.fromkeys(PARTS, 0)
failures = []
for name in subprocess.check_output(["git", "ls-files", "-z"], cwd=root, text=True).split("\0"):
    path = Path(name)
    # An eval task's repository is a fixture the model edits, not our source; its check counts.
    # Vendored third-party code, never edited here and named by its folder's UPSTREAM file, is not
    # ours either; each such folder is listed here by name.
    if (not name or path.parts[0] in ("planning", "docs") or path.suffix not in COMMENTS
            or path.parts[0] == "eval" and path.parts[3:4] == ("repo",)
            or name.startswith("packs/starter/skills/impeccable/")):
        continue
    text = (root / path).read_text()
    if path.parts[0] != "flows" and ("tests" in path.parts or path.name.startswith("test_")
                                     or ".test." in name or ".spec." in name
                                     or path.suffix == ".rs" and re.search(r"^\s*#\[(\w+::)?test\b", text, re.M)):
        failures.append(f"test outside flows {name}")
    part = modules.get("/".join(path.parts[:2]) if path.parts[0] == "src" else path.parts[0])
    if part is None:
        failures.append(f"unmapped source {name}")
        continue
    counts[part] += sum(bool(line.strip()) and not line.lstrip().startswith(COMMENTS[path.suffix])
                       for line in text.splitlines())
growth = {part: counts[part] - count for part, count in history.items()}
for part, count in counts.items():
    print(f"size {part}: {count:,}" + (f" ({growth[part]:+,} since {released})" if part in growth else ""))
most = max(growth, key=growth.get)
print(f"size grew most since {released}: {most} ({growth[most]:+,}), the part this release's rewrite takes")
total = sum(counts.values())
print(f"{'PASS' if total <= ceiling else 'FAIL'} size Total: {total:,}/{ceiling:,}")
if total > ceiling:
    failures.append("the total exceeds this release's ceiling")
for failure in failures:
    print(f"FAIL size: {failure}")
sys.exit(bool(failures))
