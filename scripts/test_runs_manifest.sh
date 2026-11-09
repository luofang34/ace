#!/usr/bin/env bash
# Exercises scripts/check_runs_manifest.sh on a scratch tree.
set -euo pipefail

check="$(cd "$(dirname "$0")" && pwd)/check_runs_manifest.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

mkdir -p "${scratch}/runs/kept" "${scratch}/output"
printf '# comment\nkept\tpurpose\n' >"${scratch}/runs/MANIFEST"
printf '# nothing kept\n' >"${scratch}/output/MANIFEST"
bash "${check}" "${scratch}" || { echo 'listed entries were refused' >&2; exit 1; }

mkdir "${scratch}/runs/2026-07-23T233929Z-24b8c994"
if bash "${check}" "${scratch}" 2>/dev/null; then
    echo 'an unlisted run directory was accepted' >&2
    exit 1
fi
rmdir "${scratch}/runs/2026-07-23T233929Z-24b8c994"

touch "${scratch}/output/drawing.pdf"
if bash "${check}" "${scratch}" 2>/dev/null; then
    echo 'an unlisted output file was accepted' >&2
    exit 1
fi
rm "${scratch}/output/drawing.pdf"

rm "${scratch}/runs/MANIFEST"
if bash "${check}" "${scratch}" 2>/dev/null; then
    echo 'a missing MANIFEST was accepted' >&2
    exit 1
fi
