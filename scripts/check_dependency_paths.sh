#!/usr/bin/env bash
# Every inline `{ path = ... }` dependency in a Cargo.toml must stay inside this repository,
# so a fresh clone builds without sibling checkouts; outside crates are git
# dependencies pinned by rev. Usage: scripts/check_dependency_paths.sh [repo-root]
set -euo pipefail

root="$(cd "${1:-$(dirname "$0")/..}" && pwd -P)"
failures=0

while IFS= read -r manifest; do
    directory="$(dirname "${manifest}")"
    while IFS= read -r relative; do
        target="$(cd "${directory}" && cd "${relative}" 2>/dev/null && pwd -P || true)"
        if [[ -z "${target}" || "${target}/" != "${root}/"* ]]; then
            printf 'check_dependency_paths: %s depends on path %s outside the repository\n' \
                "${manifest#"${root}/"}" "${relative}" >&2
            failures=$((failures + 1))
        fi
    done < <(grep -oE '\{[^}]*[^_a-z]path[[:space:]]*=[[:space:]]*"[^"]+"' "${manifest}" |
        sed -E 's/.*path[[:space:]]*=[[:space:]]*"([^"]+)"$/\1/')
done < <(find "${root}" -name Cargo.toml -not -path '*/target/*' -not -path '*/.ace/*' -not -path '*/runs/*' | sort)

exit $((failures > 0))
