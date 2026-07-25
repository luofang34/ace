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
Sequence entries use stable IDs rather than array positions, for example
`mission.segments.outbound_cruise.mach`.

`payload_drop` mission segments remove declared payload mass without treating
it as fuel burn. This supports payload-out/empty-return radius studies.

`evaluate_feasibility` defaults to `backend: "native"`. Its baseline contains
native geometry, weight closure, drag polar, Breguet estimates, mission
performance, field lengths, ceiling, requirement margins, and failed
constraints. Every geometry or analysis block contains its method, backend,
assumptions, validity range, units, and warnings.

Supported evaluations include `status: "completed"`. If the resolved
component graph uses a canonical component or relationship the selected
backend cannot represent, the tool returns `status: "unsupported"`,
`feasible: null`, code `UNSUPPORTED_BACKEND_TOPOLOGY`, the backend and
topology path, deterministic lists of unsupported kinds, and
`unsupported_features` for count, parameter, or endpoint-shape conflicts. It
does not run the backend or silently omit graph elements.

Use `backend: "openvsp"` only when refinement is requested. The result retains
the native feasibility baseline and adds an OpenVSP geometry/VSPAERO
`refinement`. If OpenVSP cannot launch or produce the explicit completion
protocol, the tool returns an error and does not return a native-only response.
Known backend unavailability is rejected before native analysis, refinement
search, design creation, or artifact creation.

`run_parameter_sweep` accepts the design `scenario_path` and supports metrics
including:

- `geometry.aspect_ratio`
- `geometry.wing_area`
- `aerodynamics.maximum_lift_to_drag_ratio`
- `performance.takeoff_field_length`
- `performance.landing_field_length`
- `mission.breguet_range`
- `mission.breguet_endurance`
- `performance.full_payload_range`
- `performance.zero_payload_ferry_range`
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

Geometry dispatch uses the resolved component graph. Legacy configuration
names still infer the same conventional or lifting-body graph. A lifting-body
component produces the existing two-panel flying-wing `.vsp3` with a reflexed
trailing-edge surrogate and one or two aft engine envelopes. CompGeom wetted
area and VSPAERO results retain explicit BWB-specific validity warnings. The
adapter does not claim inlet-flow, internal-volume, control-system, or
structural-load-path fidelity.

The conventional OpenVSP adapter maps the validated fuselage, lifting surfaces,
propeller, and one- or two-engine layout. Propellers and engine envelopes are
excluded from the VSPAERO lifting set. A graph outside those mappings returns
`UNSUPPORTED_BACKEND_TOPOLOGY`; native feasibility remains available without
an OpenVSP installation.

The structural screen is a sizing guardrail rather than substantiation. It does
not cover detailed loads, joints, buckling, fatigue, flutter, or aeroelasticity.

`generate_report` accepts either a `scenario_path` for an organized concept
report or a `run_id` for an immutable run report. Concept reports include
decision-first sections and ordered chart specifications. See
[concept-design reports](concept-reports.md).

`list_analysis_backends` reports availability, disciplines, fidelity levels,
and supported component and relationship kinds rather than requiring callers
to infer capability from platform paths. `delegated_relationship_kinds` names
relationships consumed by the mandatory native baseline because they are
outside the refinement backend's declared disciplines.

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
