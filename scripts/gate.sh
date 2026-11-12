#!/usr/bin/env bash
# The gate: the line budgets, DESIGN.md's colours, the builds and their lints, every numbered flow
# on the debug build (not built with --static), then the performance budgets on the release build
# (docs/design/perf.md).
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
[[ $# == 0 || $# == 1 && $1 == --static ]] || { echo 'Usage: scripts/gate.sh [--static]' >&2; exit 2; }

source scripts/flows.sh
setup_flows

python3 scripts/size.py
python3 scripts/design-colours.py
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release
npm --prefix bridge exec -- tsc --noEmit -p bridge
shellcheck scripts/gate.sh scripts/flows.sh

if [[ ${1:-} != --static ]]; then
    build_flows
    "$flow_python" scripts/flows.py
fi
RAZOR_BIN=$target/release/razor "$flow_python" scripts/perf.py
