#!/usr/bin/env bash
# Local/CI entry point; native suite prerequisites are documented separately.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
python3 tests/test_umu_fixture_support.py
python3 tests/contrast_check.py
cargo deny check advisories bans sources
