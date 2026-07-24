#!/usr/bin/env bash
set -euo pipefail

source_root="${1:-.}"

if [[ ! -d "${source_root}" ]]; then
    printf 'Rust size-check root is not a directory: %s\n' "${source_root}" >&2
    exit 2
fi

status=0
while IFS= read -r source_file; do
    line_count="$(wc -l < "${source_file}")"
    line_count="${line_count//[[:space:]]/}"

    limit=500
    if [[ "$(basename "${source_file}")" == "lib.rs" ]]; then
        limit=100
    fi

    if (( line_count > limit )); then
        relative_path="${source_file#"${source_root}"/}"
        printf '%s has %d lines; limit is %d\n' \
            "${relative_path}" "${line_count}" "${limit}" >&2
        status=1
    fi
done < <(
    find "${source_root}" -type f -name '*.rs' \
        -not -path '*/target/*' \
        -not -path '*/.git/*' \
        | LC_ALL=C sort
)

exit "${status}"
