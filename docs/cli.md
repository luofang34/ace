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
agent-assumption, and profile `PARAMETER_OUTSIDE_TYPICAL` warnings. Comparison
accepts `--strict` as well. The exhaustive decisions for every shipped warning
code are generated in [the strict warning policy](strict-warning-policy.md).

## Error responses

An explicit `--format json` applies to failures as well as successful results.
Every argument, document, quantity, analysis, profile, filesystem, or backend
failure writes exactly one object to stdout and returns a nonzero status:

```json
{
  "error": {
    "code": "ATMOSPHERE_OUTSIDE_VALIDITY",
    "message": "altitude 23774.4 m is outside -2000 to 20000 m",
    "path": "analysis",
    "context": {}
  }
}
```

`code` is stable for machine decisions, `path` identifies the affected
argument, document field, file, or subsystem and may be `null`, and `context`
is always an object. Error objects always use stdout, including requests with
`--output`; stderr remains empty so an agent can parse the complete response
without combining streams. Human-format failures write a concise tracing
diagnostic to stderr and no stdout. Help and version requests remain successful
and keep their normal text output.

Scenario validation and every analysis entry point run the same registered
model-domain preflight. Unsupported declarations return
`MODEL_DOMAIN_UNSUPPORTED`; `context.violations` contains every breach in
deterministic path/model order, including typed bounds, units, inclusivity, and
domain basis. Point and mission analysis also check effective true airspeed and
Mach after applying the same speed-representation precedence and atmosphere
conversion used by the runtime models. Non-finite conditions fail with
`NON_FINITE_VALUE` before simulation or JSON serialization.
