# CLI reference

Both executable names run the same command tree:

```text
aircraft-explorer
aex
```

Primary commands:

```text
aex validate DOCUMENT
aex resolve SCENARIO
aex analyze point SCENARIO --altitude VALUE (--speed VALUE | --mach VALUE)
aex analyze performance SCENARIO
aex analyze mission SCENARIO
aex analyze constraints SCENARIO
aex analyze payload-range SCENARIO
aex sweep SCENARIO --var PATH=START:STOP:COUNT --metric METRIC
aex compare SCENARIO... --metric METRIC
aex profile list|show|validate
aex plot drag-polar|power-curves|thrust-curves|climb-envelope
         |payload-range|constraints|mission-mass|requirement-margins SCENARIO
aex report RUN_ID
aex mcp serve
```

`aex validate` accepts aircraft, mission, requirements, profile, scenario, and
study documents. For a study with a relative scenario baseline, it validates
both the study contract and the resolved referenced scenario.

Analysis common flags include `--format table|json|yaml|csv`, `--output`,
`--units si|aviation_us`, `--strict`, `--explain`, `--no-cache`, `--seed`, and
repeated `--set PATH=VALUE`. Without `--units`, scenario commands use
`default_unit_system` from the adjacent `aircraft-explorer.yaml`; projects that
omit that file use SI. The option changes only `display_value` and
`display_unit`, never canonical SI values or persisted run identities. Plot
`--output` names the SVG and `--spec-output` writes the structured chart
response separately. Strict mode promotes model extrapolation,
agent-assumption, and profile `PARAMETER_OUTSIDE_TYPICAL` warnings.
