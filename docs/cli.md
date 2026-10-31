# CLI reference

Both executable names run the same command tree:

```text
aircraft-explorer
aex
```

Vehicle commands:

```text
aex synthesize VEHICLE
aex study VEHICLE --plan PLAN [--chart PARAMETER [--artifact SVG]]
aex explain VEHICLE PARAMETER [--depth N]
aex promote ARCHIVE CANDIDATE --save-as PATH
aex edit VEHICLE [--edits FILE] [--set PARAMETER=VALUE]... [--expected-revision N] [--save-as PATH]
aex compare VEHICLE... --metric PARAMETER
aex payload-range VEHICLE --distance P --remaining P --maximum-takeoff-mass P --maximum-payload P [--artifact SVG]
aex chart VEHICLE ANALYSIS [--artifact SVG]
aex uncertainty VEHICLE --plan PLAN
aex calibrate VEHICLE --case CASE
aex campaign CAMPAIGN
aex sheet VEHICLE [--port PORT]
```

`aex explain` synthesizes the vehicle and returns the parameter's declaration,
value record and producing calculation (solver identity, contract, consumed
inputs and details), the producers of those inputs up to `--depth` levels
(default 2, at most 16), the analyses and expressions that read the parameter,
and its requirement margins.

`aex payload-range` flies the vehicle's own mission at four loadings: the
design loading, maximum payload (fuel limited by maximum takeoff mass),
maximum fuel (payload limited by maximum takeoff mass) and ferry. Payload and
fuel are the parameters bound to the single `payload` and `fuel` mass items,
fuel capacity is the fuel item's capacity, and the empty mass is the sum of the
other items. For each loading it brackets and refines the route distance
`--distance` until `--remaining` (fuel left on the most demanding normal or
reserve path) reaches zero, to a relative bracket width of 1e-3. A loading with
no fuel below maximum takeoff mass reports zero range with a note; a mission
that fails for a reason other than fuel is an error. `--artifact` also writes
the payload-against-range chart as SVG.

`aex chart` synthesizes the vehicle and charts one declared analysis from its
run details; `aircraft.constraint_diagram` analyses draw the constraint curves
and feasible loading region. Other plugins return `UNSUPPORTED_ANALYSIS_CHART`.

Scenario commands:

```text
aex capabilities
aex validate DOCUMENT
aex resolve SCENARIO
aex analyze point SCENARIO --altitude VALUE (--speed VALUE | --mach VALUE) [--mass VALUE] [--cg VALUE]
aex analyze performance SCENARIO [--mass VALUE] [--landing-mass VALUE]
aex analyze constraints SCENARIO
aex analyze payload-range SCENARIO
aex profile list|show|validate
aex plot drag-polar|power-curves|thrust-curves|climb-envelope
         |payload-range|constraints|mission-mass|requirement-margins SCENARIO
aex report RUN_ID
aex mcp serve
```

`aex capabilities --format json` returns the versioned machine-readable
vocabulary accepted by validation and analysis, plus runtime backend status,
registered model domains, and strict-warning decisions. The stable sections
generate the [capability reference](capabilities.md).

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

`analyze point --mass` sets the point's mass (default maximum takeoff mass).
`analyze point --cg VALUE` trims the point at that longitudinal CG station
(from the geometry datum): the wing then carries the tail-trimmed lift, which
sets stall speed and induced drag, and the result reports
`trim_wing_lift_fraction`. It requires horizontal-tail and fuselage geometry.
`analyze performance --mass` sets `mission.field.takeoff_mass`, the mass of the
whole performance summary including ceilings and takeoff field length;
`--landing-mass` sets `mission.field.landing_mass` for the landing field
length. Field elevation, obstacle height, braking coefficient and the ISA
offset are set with `--set mission.field.*` and
`--set mission.atmosphere.isa_offset`; see [the schema](schema.md).

## Error responses

An explicit `--format json` applies to failures as well as successful results.
Every argument, document, quantity, analysis, profile, filesystem, or backend
failure writes exactly one object to stdout and returns a nonzero status:

```json
{
  "error": {
    "code": "ATMOSPHERE_OUTSIDE_VALIDITY",
    "message": "altitude 90000 m is outside -2000 to 86000 m",
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

A point speed below stall returns `SPEED_BELOW_STALL` with
`context.minimum_flyable_speed_m_s` for the requested altitude, mass and
configuration, so scripts can read the per-altitude minimum speed instead
of searching for it.

Scenario validation and every analysis entry point run the same registered
model-domain preflight. Unsupported declarations return
`MODEL_DOMAIN_UNSUPPORTED`; `context.violations` contains every breach in
deterministic path/model order, including typed bounds, units, inclusivity, and
domain basis. Point and mission analysis also check effective true airspeed and
Mach after applying the same speed-representation precedence and atmosphere
conversion used by the runtime models. Non-finite conditions fail with
`NON_FINITE_VALUE` before simulation or JSON serialization.
