# MCP reference

Start the stdio server with `aex mcp serve`. The official Rust MCP SDK exposes
structured JSON output for:

- `validate_document`
- `list_profiles`
- `get_profile`
- `resolve_scenario`
- `calculate_point_performance`
- `simulate_mission`
- `generate_constraint_diagram`
- `generate_payload_range`
- `run_parameter_sweep`
- `compare_scenarios`
- `explain_result`
- `generate_report`
- `create_design`
- `update_design_parameters`
- `evaluate_feasibility`
- `auto_refine_design`
- `compare_designs`
- `list_analysis_backends`

All scenario tools accept repository-relative or absolute paths. Overrides are
maps of dotted paths to explicit unit strings. Chart tools return a serializable
chart specification and may write an SVG only when an artifact path is
supplied. No MCP tool invokes a web service or language model.

## Design experiments

`create_design` copies a canonical baseline into an editable design directory.
Use `baseline: "c172"` or `baseline: "transport"`, or provide
`source_scenario_path`. Initial `parameters` and later
`update_design_parameters.updates` are dotted paths such as:

```json
{
  "aircraft.geometry.wing.area": "18.0 m^2",
  "aircraft.geometry.wing.aspect_ratio": "8.4",
  "aircraft.mass.maximum_fuel_mass": "165 kg",
  "mission.payload.mass": "240 kg"
}
```

Physical values retain explicit units. Dimensionless values are strings in the
MCP request and are restored to numeric YAML scalars by the scenario resolver.
The canonical aircraft, mission, requirements, profiles, and design overrides
remain YAML data; OpenVSP parameter identifiers never enter the interface.

`evaluate_feasibility` defaults to `backend: "native"`. Its baseline contains
native geometry, weight closure, drag polar, Breguet estimates, mission
performance, field lengths, ceiling, requirement margins, and failed
constraints. Every geometry or analysis block contains its method, backend,
assumptions, validity range, units, and warnings.

Use `backend: "openvsp"` only when refinement is requested. The result retains
the native feasibility baseline and adds an OpenVSP geometry/VSPAERO
`refinement`. If OpenVSP cannot launch or produce the explicit completion
protocol, the tool returns an error and does not return a native-only response.

`run_parameter_sweep` accepts the design `scenario_path` and supports metrics
including:

- `geometry.aspect_ratio`
- `geometry.wing_area`
- `aerodynamics.maximum_lift_to_drag_ratio`
- `performance.takeoff_field_length`
- `performance.landing_field_length`
- `mission.breguet_range`
- `mission.breguet_endurance`
- `feasibility.hard_constraints_passed`

`auto_refine_design` runs a deterministic bounded native search over wing area,
aspect ratio, propulsion sizing, and fuel capacity. Wing span remains consistent
with area and aspect ratio, and powerplant dry-mass growth is charged to operating
empty mass. Convergence requires implemented requirements, mission power margins,
tail-volume bounds, aspect-ratio bounds, and a conceptual wing-spar packaging
screen to pass. Mission points retain a 3% installed-reference-power reserve.
When `backend: "openvsp"` is selected, OpenVSP runs once on the chosen design;
static pitch stability becomes a final verification gate, and any adapter
failure is returned without fallback.

The structural screen is a sizing guardrail rather than substantiation. It does
not cover detailed loads, joints, buckling, fatigue, flutter, or aeroelasticity.

`list_analysis_backends` reports availability rather than requiring callers to
infer it from platform paths.

## OpenVSP subprocess adapter

The adapter uses the installed headless `vspscript` process and finds
`vspaero` beside it through the subprocess search path. Set
`ACE_OPENVSP_EXECUTABLE` to override discovery. On macOS the standard
application location is detected automatically.

The integration follows the OpenVSP API model described by the
[Python API documentation](https://openvsp.org/pyapi_docs/latest/) while
keeping the language boundary at a generated AngelScript subprocess. OpenVSP
describes itself as a parametric aircraft geometry tool suitable for producing
analysis geometry; the native solver remains authoritative for the canonical
design workflow.
