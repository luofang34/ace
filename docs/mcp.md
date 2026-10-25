# MCP reference

Start the stdio server with `aex mcp serve`. The official Rust MCP SDK exposes
structured JSON output for:

- `get_capabilities`
- `validate_document`
- `list_profiles`
- `get_profile`
- `resolve_scenario`
- `calculate_point_performance`
- `generate_report`
- `list_analysis_backends`

`get_capabilities` returns the same manifest as `aex capabilities --format
json`. Agents can discover legal document/profile types, configurations,
mission fields, bindable metrics and declared-only replacements, model
domains, versioned requirement templates and provenance, warning policy, and
backend availability before constructing a request. The stable vocabulary
also generates the
[capability reference](capabilities.md).

All scenario tools accept repository-relative or absolute paths. Overrides are
maps of dotted paths to explicit unit strings. Chart tools return a serializable
chart specification and may write an SVG only when an artifact path is
supplied. No MCP tool invokes a web service or language model.

Scenario tools use `default_unit_system` from the `aircraft-explorer.yaml`
adjacent to `scenario_path`. An optional `units: "si"|"aviation_us"` request
field overrides the project. MCP and CLI responses share the same presentation
serializer: canonical values remain SI, while Aviation-US display metadata uses
kt, ft, lb, and nmi according to quantity semantics.

Scenario resolution and point
requests accept optional `strict` (default `false`). The shared
CLI/MCP [strict warning policy](strict-warning-policy.md) returns
`STRICT_WARNING_FAILURE` when a registered promotable warning is present;
advisory warnings remain in successful responses.

Tool execution failures return a normal MCP `CallToolResult` with
`isError: true`. Its structured content contains `status`, `code`, `message`,
`violating_path`, `valid_range`, `suggested_override`, and `diagnostics`, plus
the CLI-compatible `path` and `context`. Model-domain failures retain every
ordered bound, unit, inclusivity rule, and basis. A suggested override is
included only when the violating request path can be moved directly to an
inclusive model bound; it is a hint and the tool never mutates or reruns the
request. Malformed tool arguments and transport failures remain JSON-RPC
protocol errors.

`synthesize_vehicle` runs every declared vehicle mission phase by phase with
mass, fuel and CG carried across stages, and returns its requirement margins,
closures and per-phase calculation evidence.

## Vehicle evidence and fleet campaigns

`propagate_vehicle_uncertainty` takes `vehicle_path`, `plan_path` and optional
`overrides`, and returns a content-addressed uncertainty record with each
requirement's satisfaction probability, 95% interval and tri-state verdict.
`calibrate_vehicle` takes `vehicle_path`, `case_path` and optional
`overrides`, and returns prior and posterior residuals, consistency and the
calibrated updates with their provenance. `evaluate_fleet_campaign` takes
`campaign_path`, synthesizes every referenced vehicle into a capability record
and returns each fleet's success interval, verdict and cost, together with the
most cost-effective passing fleet. Domain failures are `isError` tool results.

## Vehicle editing

`edit_vehicle` takes `vehicle_path`, typed `edits` (the `ModelEdit`
declarations), `updates` such as `{"wing.span": "37 m"}`, an optional
`expected_revision` and an optional `save_as` path. Edits and assignments are
validated and applied as one transaction, so the file advances by exactly one
model revision. Assigning a computed output returns `COMPUTED_PARAMETER`; a
file at another revision returns `REVISION_CONFLICT`; a file changed during the
edit returns `EXTERNAL_EDIT`. `save_as` writes a new file and never replaces an
existing one. The response carries the saved path, revision, model digest,
affected graph nodes, completion, assessment and requirement margins.

`payload_drop` mission segments remove declared payload mass without treating
it as fuel burn. This supports payload-out/empty-return radius studies.

Scenario feasibility evaluations behind reports default to the native
backend. Their baseline contains
native geometry, weight closure, drag polar, Breguet estimates, mission
performance, field lengths, ceiling, requirement margins, and failed
constraints. Every geometry or analysis block contains its method, backend,
assumptions, validity range, units, and warnings.

Supported evaluations include `status: "completed"`. If the resolved
component graph uses a canonical component or relationship the selected
backend cannot represent, the evaluation returns `status: "unsupported"`,
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

Study constraints and objectives accept the
`feasibility.hard_constraints_passed` metric.

`compare_vehicles` takes `vehicle_paths` and `metrics` (parameter identifiers),
synthesizes each vehicle and returns, per vehicle, its digest, completion,
feasibility, requirement margins and each metric's value record in the
vehicle's declared unit with the producing calculation. A metric the vehicle
does not declare returns `UNKNOWN_COMPARISON_METRIC`.

`vehicle_payload_range` takes `vehicle_path` and the `distance`, `remaining`,
`maximum_takeoff_mass` and `maximum_payload` parameter identifiers and returns
the same corners as `aex payload-range`; an optional `artifact_path` writes
the chart as SVG.

`generate_report` takes `vehicle_path` and returns the same report as
`aex report --vehicle`.

`chart_vehicle_analysis` takes `vehicle_path`, `analysis`, optional
`artifact_path` and `overrides`, and returns the same chart as `aex chart`.

`explain_vehicle_value` takes `vehicle_path`, `parameter`, optional `depth`
(default 2, at most 16) and `overrides`, and returns the same explanation as
`aex explain`.

`promote_vehicle_candidate` takes `archive_path` (a `study_vehicle` archive),
`candidate_id` and `save_as`. It verifies the archive and the candidate's
calculation evidence, requires a satisfied or provisional candidate
(`CANDIDATE_NOT_FEASIBLE` otherwise) and writes the candidate model to a new
file without replacing an existing one. A refine study followed by promotion
is the bounded design search; vehicle closures size coupled quantities.

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

`generate_report` accepts either a `vehicle_path` for a concept report or a
`run_id` for an immutable run report. Concept reports include decision-first
sections and ordered chart specifications. See
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
