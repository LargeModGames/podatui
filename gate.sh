#!/usr/bin/env bash
# Local checks on Linux: fmt, clippy and tests for every crate that builds here.
set -euo pipefail
cd "$(dirname "$0")"

for d in core; do
    echo "==> $d"
    (cd "$d" && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test)
done
