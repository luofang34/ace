#!/usr/bin/env bash
set -euo pipefail

fixture_root="$(mktemp -d)"
trap 'rm -rf "${fixture_root}"' EXIT

mkdir -p "${fixture_root}/src"
awk 'BEGIN { for (i = 1; i <= 500; i++) print "// line" }' \
    > "${fixture_root}/src/model.rs"
awk 'BEGIN { for (i = 1; i <= 100; i++) print "// line" }' \
    > "${fixture_root}/src/lib.rs"
bash scripts/check_rust_size_limits.sh "${fixture_root}"

awk 'BEGIN { for (i = 1; i <= 501; i++) print "// line" }' \
    > "${fixture_root}/src/model.rs"
if bash scripts/check_rust_size_limits.sh "${fixture_root}" >/dev/null 2>&1; then
    printf 'Rust size guard accepted a 501-line source file\n' >&2
    exit 1
fi

awk 'BEGIN { for (i = 1; i <= 500; i++) print "// line" }' \
    > "${fixture_root}/src/model.rs"
printf '%s' '// final line' >> "${fixture_root}/src/model.rs"
if bash scripts/check_rust_size_limits.sh "${fixture_root}" >/dev/null 2>&1; then
    printf 'Rust size guard accepted an unterminated 501-line source file\n' >&2
    exit 1
fi

awk 'BEGIN { for (i = 1; i <= 500; i++) print "// line" }' \
    > "${fixture_root}/src/model.rs"
awk 'BEGIN { for (i = 1; i <= 101; i++) print "// line" }' \
    > "${fixture_root}/src/lib.rs"
if bash scripts/check_rust_size_limits.sh "${fixture_root}" >/dev/null 2>&1; then
    printf 'Rust size guard accepted a 101-line lib.rs\n' >&2
    exit 1
fi

awk 'BEGIN { for (i = 1; i <= 100; i++) print "// line" }' \
    > "${fixture_root}/src/lib.rs"
printf '%s' '// final line' >> "${fixture_root}/src/lib.rs"
if bash scripts/check_rust_size_limits.sh "${fixture_root}" >/dev/null 2>&1; then
    printf 'Rust size guard accepted an unterminated 101-line lib.rs\n' >&2
    exit 1
fi
