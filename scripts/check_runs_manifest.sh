#!/usr/bin/env bash
# Every top-level entry of runs/ and output/ must be named in that directory's
# committed MANIFEST, so reproducible experiment output does not accumulate
# there. Usage: scripts/check_runs_manifest.sh [repo-root]
set -euo pipefail

root="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
failures=0

for directory in runs output; do
    manifest="${root}/${directory}/MANIFEST"
    if [[ ! -f "${manifest}" ]]; then
        printf 'check_runs_manifest: %s/MANIFEST is missing\n' "${directory}" >&2
        failures=$((failures + 1))
        continue
    fi
    allowed="$(sed -e '/^#/d' -e '/^$/d' "${manifest}" | cut -f1)"
    while IFS= read -r entry; do
        name="$(basename "${entry}")"
        [[ "${name}" == "MANIFEST" ]] && continue
        if ! grep -qxF -- "${name}" <<<"${allowed}"; then
            printf 'check_runs_manifest: %s/%s is not in %s/MANIFEST; move it to .ace/runs/ or list it\n' \
                "${directory}" "${name}" "${directory}" >&2
            failures=$((failures + 1))
        fi
    done < <(find "${root}/${directory}" -mindepth 1 -maxdepth 1 | sort)
done

exit $((failures > 0))
