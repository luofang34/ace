#!/usr/bin/env bash
set -euo pipefail

generated_path="scratch/designs/candidate/scenario.yaml"
example_path="examples/designs/candidate/scenario.yaml"

if ! git check-ignore --quiet --no-index "${generated_path}"; then
    printf 'generated design path is not ignored: %s\n' "${generated_path}" >&2
    exit 1
fi

if git check-ignore --quiet --no-index "${example_path}"; then
    printf 'example design path is unexpectedly ignored: %s\n' "${example_path}" >&2
    exit 1
fi

for kept in runs/MANIFEST output/MANIFEST; do
    if git check-ignore --quiet --no-index "${kept}"; then
        printf 'retention manifest is unexpectedly ignored: %s\n' "${kept}" >&2
        exit 1
    fi
done

for scratch in runs/2026-01-01T000000Z-00000000 output/drawing.pdf .ace/runs/run; do
    if ! git check-ignore --quiet --no-index "${scratch}"; then
        printf 'run output is not ignored: %s\n' "${scratch}" >&2
        exit 1
    fi
done
