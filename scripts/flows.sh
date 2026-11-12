#!/usr/bin/env bash
# Run numbered flows; source this file to share setup with the gate.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

setup_flows() {
    target=$(cargo metadata --no-deps --format-version 1 |
        python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')
    if ! cmp -s bridge/package.json bridge/node_modules/.razor-package.json ||
        ! cmp -s bridge/package-lock.json bridge/node_modules/.razor-package-lock.json; then
        npm --prefix bridge ci
        cp bridge/package.json bridge/node_modules/.razor-package.json
        cp bridge/package-lock.json bridge/node_modules/.razor-package-lock.json
    fi
    flow_python=$target/flows-venv/bin/python
    if [[ ! -x $flow_python ]] ||
        ! cmp -s flows/requirements.txt "$target/flows-venv/requirements.txt"; then
        python3 -m venv "$target/flows-venv"
        "$flow_python" -m pip install -r flows/requirements.txt
        cp flows/requirements.txt "$target/flows-venv/requirements.txt"
    fi
}

build_flows() {
    cargo build --workspace
    export RAZOR_BIN=${RAZOR_BIN:-$target/debug/razor} FLOW_PEER_BIN=$target/debug/flow-peer
    # The relay the network flows connect through, at the version Razor's iroh uses.
    relay_version=$(awk '$0 == "name = \"iroh-relay\"" {getline; print $3}' Cargo.lock | tr -d '"')
    export RAZOR_RELAY_BIN=$target/flows-tools/bin/iroh-relay
    if [[ $("$RAZOR_RELAY_BIN" --version 2>/dev/null) != "iroh-relay $relay_version" ]]; then
        cargo install iroh-relay --version "=$relay_version" --features server --locked --root "$target/flows-tools"
    fi
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    setup_flows
    build_flows
    "$flow_python" scripts/flows.py "$@"
fi
