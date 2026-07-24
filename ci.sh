#!/usr/bin/env bash
set -euo pipefail

bash scripts/check_design_ignores.sh
bash scripts/test_rust_size_limits.sh
bash scripts/check_rust_size_limits.sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc --no-deps
cargo build --release
