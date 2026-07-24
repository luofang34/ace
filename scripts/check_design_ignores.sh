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
