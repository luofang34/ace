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
