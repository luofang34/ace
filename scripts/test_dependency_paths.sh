#!/usr/bin/env bash
# Exercises scripts/check_dependency_paths.sh on a scratch workspace.
set -euo pipefail

check="$(cd "$(dirname "$0")" && pwd)/check_dependency_paths.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

mkdir -p "${scratch}/repo/crates/a" "${scratch}/repo/crates/b" "${scratch}/sibling/crate"
printf '[dependencies]\nb = { path = "../b" }\n' >"${scratch}/repo/crates/a/Cargo.toml"
printf '[package]\nname = "b"\n' >"${scratch}/repo/crates/b/Cargo.toml"
bash "${check}" "${scratch}/repo" || { echo 'an in-repository path was refused' >&2; exit 1; }

printf '[dependencies]\nc = { path = "../../../sibling/crate" }\n' >"${scratch}/repo/crates/b/Cargo.toml"
if bash "${check}" "${scratch}/repo" 2>/dev/null; then
    echo 'a path outside the repository was accepted' >&2
    exit 1
fi
