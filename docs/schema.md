# Schema and units

Documents are YAML or JSON mappings with `schema_version: 1` and one envelope:
`aircraft`, `mission`, `requirements`, `profile`, or `scenario`.

Physical values must include a unit:

```yaml
maximum_takeoff_mass: 1111 kg
area: 174 ft^2
cruise_speed: 122 kt
distance: 350 nmi
```

Bare numbers are reserved for dimensionless Mach, coefficients, ratios,
counts, and enumerated values. Unsupported or incompatible units return a
typed `Quantity`/validation error rather than guessing.

The resolver normalizes SI values and records scalar sources in an assumptions
ledger. Interface quantity objects contain `value`, `unit`, `display_value`,
and `display_unit`. Range fields use metres for `value` and nautical miles for
display by default.

Scenario documents may contain an `overrides` map. The design MCP tools use
this map as an editable overlay instead of rewriting aircraft, mission, or
requirements documents:

```yaml
scenario:
  id: c172_experiment
  name: C172 Experiment
  aircraft: aircraft.yaml
  mission: mission.yaml
  requirements: requirements.yaml
  overrides:
    aircraft.geometry.wing.area: 18.0 m^2
    aircraft.geometry.wing.aspect_ratio: "8.4"
    aircraft.propulsion.sizing_factor: "1.10"
```

Call-specific overrides take precedence over persisted scenario overrides.
Both are validated against the same typed canonical models.
Sequence entries are addressed by their stable `id`, not by array position:

```yaml
scenario:
  overrides:
    mission.segments.outbound_cruise.mach: "0.58"
```

Mission segments with `type: payload_drop` require `payload_mass`. The
simulator removes that mass without recording fuel burn, enabling an explicit
payload-delivery and empty-return mission.

`aircraft.geometry.wing.center_body_edge_sweep` is an optional angle for
tailless blended-wing-body geometry. It denotes the positive magnitude of
opposed center-body leading- and trailing-edge sweeps. OpenVSP translates it
to the backend's quarter-chord representation without exposing backend
parameter identifiers.

Turbofan profiles may provide `dimensions.overall_length` and
`dimensions.maximum_diameter`. OpenVSP uses them for the engine envelope and
retains conservative defaults when they are absent.

## Aircraft topology

`aircraft.topology` is an optional, backend-neutral component graph. Components
have stable IDs, registered kinds, optional reusable definitions, counts,
analysis roles, and data parameters. Relationships express attachment,
symmetry, repetition, alignment, parallelism, continuity, and load paths.
Backend geometry IDs, meshes, and solver entities are not schema data.

When topology is absent, schema-version-1 aircraft retain their behavior
through a resolved graph inferred from `aircraft.configuration`, engine count,
and propeller presence. An explicit graph takes precedence over the legacy
configuration label.

Canonical component kinds include fuselage, wing, horizontal and vertical
tails, canard, lifting body, boom, engine, propeller, fuel system, and payload.
A backend advertises the subset it can analyze. A canonical graph outside that
subset resolves normally, but feasibility returns `status: "unsupported"`
with code `UNSUPPORTED_BACKEND_TOPOLOGY`, unsupported kinds, and any unsupported
count, parameter, or relationship-endpoint features. Unknown vocabulary
remains a validation error. Component definitions and roles are descriptive;
backend-relevant counts, parameters, and relationships must be consumed or
rejected before analysis. A refinement descriptor may delegate a relationship
outside its disciplines only when the mandatory native baseline consumes it.
