# Aircraft Concept Explorer

Aircraft Concept Explorer is a deterministic Rust application for early-stage
fixed-wing aircraft concept studies. One low-fidelity analysis library powers
both the `aircraft-explorer`/`aex` CLI and its stdio MCP server.

ACE and its workspace libraries are distributed under AGPL-3.0-only; see
[LICENSE](LICENSE). Incorporated code retains the notices in [NOTICE](NOTICE)
and any component-specific third-party license files.

It is not a flight simulator or certification tool. Results are conceptual
estimates and are not suitable for operational flight planning or
safety-critical decisions.

## Vehicle synthesis

The Cargo workspace separates vehicle data (`ace-model`), incremental
execution (`ace-engine`), executable geometry, mass, mission and operating
models (`ace-physics`), the sheet/provenance projection (`ace-sheet`), and bounded
design studies (`ace-study`), and replayable engineering drawings (`ace-drawing`). The application package supplies aircraft solvers,
CLI/MCP adapters, charts, and storage. The library crates are independent of
filesystem access, command-line parsing, and aircraft-specific models.

Vehicle documents declare canonical dimensional parameters, expression trees,
requirements, semantic geometry, mass items, missions, operating economics,
and optimization variables/objectives. Analyses map named solver ports onto
those parameters. Coupled components require explicit seeds, dimensional
tolerances, and an iteration limit. Unavailable solvers and failed closures
remain unavailable in the result; each calculation retains its inputs, solver
identity, selected semantic resources, structured results, and source revisions.

```bash
cargo run --bin aex -- synthesize examples/synthesis/transport.json --format json
cargo run --bin aex -- synthesize examples/synthesis/transport.json \
  --set 'wing.span=37 m' --format json
```

The transport synthesis file is an illustrative dependency/closure model, not
an aircraft calibration. The MCP `synthesize_vehicle` tool uses the same
service. Synthesis exports retain declared units and content-addressed evidence;
`--units` display conversion is not applied to these documents. Embedders use
`Session::edit`, `Session::transact` and `Session::run` to recompute only the
affected graph region. Each analysis selects its consumed geometry and
`resources`; their parameter bindings participate in invalidation and closure.
Structural transactions accept typed `ModelEdit` declarations for geometry,
mass, missions, requirements, analyses and operations. References and the new
graph are validated before publication; rejected edits leave the session intact.
Registered `PluginSpec` contracts declare scalar or state-phase invocation,
`ScalarPort` units and optional semantic/reference identities, and executable
`ValidityDomain` intervals. Direct producer/consumer connections require matching
semantics when declared. Semantic ports accept independent inputs or direct typed
producers; expressions do not infer semantic identities. Missing solvers remain
unavailable. Applicability intervals are checked
against actual inputs, phase entry conditions and reported trajectory conditions;
coupled physical limits remain the responsibility of each solver. Contracts and
assumptions are retained in calculation evidence and do not imply calibration.
The Sheet's `/api/edit` accepts either `updates` or an `edits` array, together
with its `expected_revision` token.

Library users can run `ace_study::VerificationCase::run` against a `Session` and
serialize the resulting `VerificationRecord`. `replay` uses the frozen model and
requires the recorded build and installed contracts; changed evidence is rejected.
`VariantMatrix::expand` generates a deterministic, bounded Cartesian matrix of
typed structural edits and parameter overrides, with explicit exclusions and
conflicting writes rejected. It does not perform pairwise sampling.
The `examples/synthesis/constant-rate*.json` files provide a small analytic case
and variant matrix consumed by `cargo test --test solver_verification`.

Native plugins include `vehicle.geometry_envelopes`, `vehicle.usable_volume`,
`vehicle.wetted_area`, `vehicle.mass_properties`, `vehicle.constant_rate_phase`,
`vehicle.mission_sequence`, `vehicle.mission_fuel_requirement`,
`vehicle.mission_cg_envelope`, `vehicle.operations` and `vehicle.trip_cost`.
Envelope clearances and
constant-rate phases are explicitly approximate. Volume and wetted-area
aggregation require explicit measurements for every selected, non-overlapping
component. Missing inertia remains unknown, including changed loadings without
a corresponding tensor.

A mission analysis selects one mission. State-driven analyses declare
`mode: "phase"` with no scalar outputs. Their adapters receive actual entry
mass, fuel and conditions, including stage controls, and return integration
increments. Each increment applies tank depletion and recomputes mass and CG;
payload releases occur at the stage boundary. Scalar phase adapters can instead
supply `duration`, `distance`, `fuel_burn`, `cost` and optional `state.*` outputs.
Failed transitions retain the last valid stage and revoke downstream results.
Mission `branches` start independently at a nominal stage boundary; their
reserve/alternate consumption and expense do not accumulate into normal trip cost.

The example uses `aircraft.route_cruise_phase` and `aircraft.level_flight_phase`
for cruise and reserve: constant
speed/altitude, parabolic drag, lift equal to weight, thrust equal to drag and
declared installed TSFC/thrust. It rejects exceeded lift/thrust limits and Mach
above 0.9. The `aircraft.propeller_*` and `aircraft.vspaero_propeller_*` level,
route-cruise and energy-path variants replace `tsfc` and `available_thrust` with
`shaft_power` (sea-level maximum), `propeller_efficiency`, `bsfc` (`kg/kWh` or
`kg/J`) and `power_lapse_exponent` n: available thrust is
`efficiency * shaft_power * (rho/rho0)^n / V` and fuel flow is
`bsfc * thrust * V / efficiency`. Efficiency and BSFC stay constant; there is no
propeller map or static-thrust model. Climb, descent and approach integrate mass and fuel along declared straight paths
with linear true airspeed schedules, including potential and kinetic energy.
Paths stay within 0-20 km altitude and a +/-10 deg flight-path angle, so
`path_distance` must be at least the altitude change divided by tan 10 deg
(about 5.7 times it; 25.5 km for a 4500 m climb). Cruise
uses the remaining route distance; reserve descent and loiter retain continuous
states. Ground phases use declared approximate rates and terminal conditions, not
runway performance. Level and route-cruise stages accept an optional
`headwind` input (negative for tailwind): fuel follows airspeed while distance
and route time use ground speed. Climb and descent paths and propulsion
transients ignore wind; infeasible thrust or lift is rejected. A path stage may declare an
`idle_thrust` input: where the path needs less thrust, fuel burns at idle and
the surplus appears as `drag_device_n`, an unmodelled drag device. Without it, a
path needing negative thrust is rejected. Like other stage inputs, `idle_thrust`
remains in effect for later stages.
The capacity-bounded fuel search assumes a monotonic fuel balance and feeds
mission demand into the coupled empty-mass/MTOW closure. It requires usable tank
capacity and terminal reserve, loads tanks in capacity proportion and checks
every declared scenario. An unexecutable search bound remains unavailable.
`fuel.required`, `fuel.loaded`, usable capacity, trip burn, contingency fuel and
terminal remaining fuel are separate quantities.

Synthesis and Sheet assessments separate execution, mandatory feasibility and
evidence quality. A completed run can violate constraints. Missing evidence or
an unmatched hard requirement context is unknown; soft and report-only margins
do not establish infeasibility. Computed results do not imply calibration.
Operations retain airport compatibility alongside trip cost and applicable
hourly or seat-distance metrics.

Geometry placement composes rigid local frames; mass CG and inertia use the
same transforms as envelopes, clearances and sections. Embedded artifacts retain
content digests, media/schema and generator identity. Changing their content
requires updating the digest through a model transaction; external references
must be explicitly verified when read. `aircraft.openvsp_wing` accepts canonical
area/span/taper/sweep/dihedral/twist ports and produces an isolated symmetric
wing artifact and wetted area when OpenVSP is installed. It uses default airfoil
sections and remains conceptual; missing tools produce unavailable outputs.

`vehicle.openvsp_geometry` generates every selected geometry object that declares
a native `shape` as one OpenVSP model in its composed placement frame: bodies
become lofted elliptical fuselages and wings become NACA four-series trapezoidal
surfaces. It publishes the canonicalized `.vsp3`, a STEP AP203 surface model and
a Wavefront OBJ tessellation as content-addressed artifacts, and reports the
CompGeom intersected wetted area and enclosed volume. Kernel identifiers and
export time stamps are normalized, so identical inputs reproduce identical
digests. Symmetric surfaces must keep their local y axis parallel to the vehicle
y axis at zero lateral offset; other placements and shapes outside the native
concept domain fail explicitly. Envelope-only objects are not exported.
`examples/synthesis/kernel-transport.json` adds this analysis to the concept
transport.

Parameters may declare an `uncertainty` with a `normal` or `uniform`
distribution about their value, and requirements may declare a `confidence`.
`ace_study::UncertaintyPlan::run` samples every uncertain independent input
jointly from a seeded, version-stable generator, redraws values outside declared
parameter bounds, and evaluates the whole graph per draw. The record reports
metric statistics and each requirement's satisfaction probability with a 95%
Wilson interval: it passes only when the interval lies at or above the declared
confidence, fails when it lies below, and is otherwise indeterminate. Draws whose
evidence is missing count as unsatisfied. Sampled feasibility is at best
provisional, and a hard requirement without a confidence leaves it unknown.

`examples/synthesis/vspaero-transport.json` connects `aircraft.vspaero_wing_polar`
to the state-driven climb, cruise, descent, approach and reserve phases:

```bash
cargo run --bin aex -- synthesize examples/synthesis/vspaero-transport.json --format json
```

The adapter requires both `vspscript` and its sibling `vspaero` executable.
It retains the generated geometry, raw solver output and fitted polar with both
executable digests. It reduces VSPAERO's induced drag to `k(Mach) * CL²` at Mach
0, 0.2, 0.4, 0.6 and 0.8 (ports `k_000` to `k_080`); the supplied parasite `CD0`
remains separate. Every sampled point must meet an absolute 0.00002 plus
relative 5% fit tolerance. Flight phases interpolate the table from the
incompressible sample upward, so low-speed phases need no extrapolation; they
reject Mach above 0.8 and CL above the common sampled limit. This adapter
supports isolated, untwisted wings with symmetric NACA 0012 airfoils;
unsupported shapes fail explicitly. Mesh convergence, experimental calibration,
whole-aircraft interference, trim, stall and transonic shocks remain unverified.
Use the same model file with `sheet`, `study` or `drawing` to project its results.

Rate-based operations require one currency, price year and cost scope. Native
rates use USD, block hours, cycles, turnaround time and annual ownership cost.
The `crew_flight_hour`, `maintenance_flight_hour` and annual `flight_time` fields
are applied to normal-trip block time, including declared ground phases; they
do not use a separate airborne-time measure. Annual block/operating time limits
determine ownership allocation. Required route scenarios retain airport
compatibility independently of CASM; load factor
does not change its available-seat denominator. Price-year/currency conversion
and uncertainty propagation are not implemented.

For non-passenger (for example UAS) accounting, omit `operations.seats`,
`load_factor` and `revenue_per_passenger` (or set seats to zero);
`load_factor` is required only when seats are declared. An optional
`operations.payload` mass adds `cost_per_payload_kg_block_hour_usd`, the
block-hour cost per kilogram of payload. `vehicle.trip_cost` version `1` selects
`resources: {"operations": true}` and publishes `trip_cost` (USD),
`cost_per_hour` (USD/h) and `network_compatible` (1). Bind a positive
`operations.block_duration` to normal-trip elapsed time; rate-based cases use
each scenario's own `block_duration`. Reserve/alternate time and turnaround
are excluded from this denominator. Zero distance is allowed. CASM and occupied
seat cost are null when seats or distance are zero; the `vehicle.operations`
plugin requires both to be positive and fails otherwise. Missing duration leaves
hourly cost unavailable; neither an artificial seat nor a zero CASM is inserted.

`examples/synthesis/trip-cost.json` is a standalone seatless accounting example
with no flight-performance assumptions: USD 10,000 over two block hours yields
USD 5,000/h, or USD 100/h per kilogram of its 50 kg payload.
Use it with `synthesize` or `sheet` to inspect the cost breakdown and provenance.

Open an interactive synthesis sheet with:

```bash
cargo run --bin aex -- sheet examples/synthesis/transport.json --format json
```

Open the printed local URL in a browser. The sheet projects requirements,
arrangement, critical sections, mass properties, mission, operating costs, and
closure results from one model revision. Click values or geometry to inspect
their sources and calculation evidence. Parameter edits recompute only affected
nodes; wingtip drags and design-variable sliders show an isolated conceptual
preview before committing on release. Approximate evidence remains marked,
including violated constraints. Results from unavailable adapters remain unknown.
Applications can bind a formal solver to a fast adapter with
`Registry::register_preview`; named ports and units must match. Only preview
branches use the replacement, retain its actual solver identity, and mark its
outputs approximate. Formal runs keep the model's selected solver.

CG constraints select the solver's limiting loading case. Requirements associated
with semantic volumes prioritize their smallest available envelope sections;
these remain bounding-box evidence, not internal CAD cross-sections. Invalidated
results lose this focus until fresh calculations establish it again.

The mission selector switches normal and contingency trajectories. CG envelopes
are piecewise linear versus mass in vehicle axes; every integrated and released
state is checked, without extrapolation. These are sampled trajectories, not a
continuous stability or certification proof. Economic scenario selection changes
the displayed case; every required scenario still participates in feasibility.

Formal jobs have IDs, revision guards and a 60-second deadline; previews have a
15-second deadline. **Cancel run** requests cleanup before reporting cancellation.
Edits supersede older jobs. Native adapters must checkpoint `RunControl` between
bounded units of work; arbitrary blocking native code cannot be forcibly stopped
inside the process. The OpenVSP adapter supervises owned Unix process groups,
kills timed-out/canceled children and waits for cleanup. Shutdown names and aborts
execution monitors that exceed its grace period.

Edits stay in memory until **Save model** is selected. Saving refuses to overwrite
external changes to the input file. **Export JSON** downloads the current model;
the browser print command produces the sheet layout. Stop the local server with
Ctrl-C. `--port` selects a loopback port; the default chooses an available port.

Export an engineering synthesis drawing from a supplied concept or an explicitly
selected bounded-study candidate:

```bash
cargo run --bin aex -- drawing examples/synthesis/concept-transport.json \
  --plan examples/synthesis/drawing-study.json --candidate 1 \
  --directory output/pdf/concept --paper A3 --format json
cargo run --bin aex -- drawing output/pdf/concept/drawing.json --replay \
  --directory output/pdf/replayed --format json
```

Use a new output directory; publication is atomic and existing directories are
not overwritten. The directory contains a frozen `drawing.json`, vector SVG/PDF, and a PNG preview
from the same millimetre scene. `--paper A1` doubles the drawing's linear size.
`--paper 16:9` uses a separate presentation composition. `--from-snapshot` reads
a synthesis report, drawing input, or verified drawing archive without running
engineering solvers. `--drawing-data profile.json` adds typed, English-only
geometry paths, section stations, curves, result callouts and documentary facts.
External calculation/test/readiness claims require the matching model digest and
revision; configuration definitions can bind directly to canonical parameters.
Supplied structure is drawn only where declared, and material hatching requires
a closed polygon in the section plane. The evidence register retains full text,
equations, conditions and all curve samples when the sheet shows an abbreviated selection.
The `DrawingData` and `FigureSpec` public Rust types define the import format.
Caller-defined `figures` select geometry, compatible imported curves, documentary
facts or supplied diagram linework, with independent IDs and pixel dimensions.
Each figure exports SVG/PDF, frozen JSON and PNG at four times its requested display
resolution. `manifest.json` records file paths, revision, units, evidence and missing
inputs. Missing geometry/results remain N/C and make presentation completeness false.
`--candidate` is a zero-based explicit selection, not a claim of optimality;
without `--plan`, the supplied design is calculated and studies remain uncomputed.
`--brand "Your Organization" --drawing-title "Concept Study" customizes the heading
and PDF metadata; the default presentation has no tool branding. Printable ASCII
text is supported; logos and embedded Unicode fonts are unavailable. Branding is
frozen with the page without changing the vehicle or calculation evidence.
`--mission` selects the mission/operating case. `--set` changes canonical inputs
before calculation; replay preserves the frozen page and rejects edits.
Native `vehicle.concept_geometry` generates ellipsoidal body ends and trapezoidal
diamond-section lifting surfaces from `geometry.*.shape` bindings, using the same
rigid frames as mass and envelope checks. The drawing distinguishes these concept
surfaces from dashed occupancy envelopes; body A(x) is not total aircraft area or
wave-drag analysis. Surface-area sums include intersecting part/root surfaces.
Fine mechanisms, structural sizing, calibrated airfoils and continuous performance
boundaries are not inferred by the renderer. Evidence keys on SVG
objects resolve through the frozen document to source parameters and calculations.

Study the model's declared variables and objectives with:

```bash
cargo run --bin aex -- study examples/synthesis/transport.json \
  --plan examples/synthesis/span-study.json --format json --output span-study.json
```

Use `examples/synthesis/route-study.json` for a bounded payload/range grid.
Expand **Design studies** in Sheet and import its JSON archive to plot any two
canonical metrics, including payload, block fuel and CASM against stage length.
Imports require the exact baseline model and implementation build and verify
archive/candidate/run hashes. The interactive view accepts up to 64 candidates
and 128 MiB per archive. Points retain hard-constraint status and exact calculation
inputs; sparse samples do not imply a continuous payload-range boundary.

A plan uses `schema_version: 1`, a positive `max_evaluations`, and a `method`.
The `grid` method provides unit-bearing samples in `axes`; `refine` provides
`levels` and an optional `variables` list, then halves coordinate steps around
the retained Pareto designs. Both methods enforce the vehicle's declared bounds
and reuse unaffected calculations. Refinement is a bounded search, not a global
optimality certificate. `stop_reason` distinguishes budget exhaustion from
exhausting the requested grid or refinement levels.

Archives retain each candidate model, current values, margins, and shared
calculation evidence. `pareto` contains nondominated designs with current
comparison evidence; `provisional_pareto` is separate. Unknown constraints,
unmatched mandatory contexts, failed closures, and hard violations are excluded
from both fronts. Soft violations remain visible without excluding a design.
Use `StudyArchive::snapshot` to verify and restore a candidate's evidence for
`ace_sheet::project`. The MCP `study_vehicle` tool accepts `vehicle_path`,
`plan_path`, and optional overrides through the same service.

## Quick start

Development tracks the current stable Rust toolchain selected by `rust-toolchain.toml`; no minimum version is declared.

```bash
cargo build
cargo run --bin aex -- capabilities --format json
cargo run --bin aex -- validate examples/c172/scenario.yaml --format json
cargo run --bin aex -- analyze point examples/c172/scenario.yaml \
  --altitude "8000 ft" --speed "115 kt" --format json
cargo run --bin aex -- analyze mission examples/c172/scenario.yaml --format json
cargo run --bin aex -- analyze point examples/b777/scenario.yaml \
  --altitude "35000 ft" --mach 0.84 --format json
cargo run --bin aex -- analyze mission examples/b777/scenario.yaml --format json
```

With `--format json`, failures also return one machine-readable
`{"error":{"code","message","path","context"}}` object on stdout with a
nonzero status. See [the CLI error contract](docs/cli.md#error-responses).

SI values remain authoritative inside the solver and persisted runs. The
example projects select Aviation-US display metadata, so ranges use nautical
miles, speeds use knots, altitudes use feet, and masses use pounds:

```json
{
  "total_distance": {
    "value": 13871100.0,
    "unit": "m",
    "display_value": 7490.9,
    "display_unit": "nmi"
  }
}
```

## Exploration

```bash
cargo run --bin aex -- plot power-curves examples/c172/scenario.yaml \
  --output c172-power.svg --format json
cargo run --bin aex -- plot payload-range examples/b777/scenario.yaml \
  --output b777-payload-range.svg --format json
cargo run --bin aex -- sweep examples/c172/scenario.yaml \
  --var 'aircraft.geometry.wing.area=14 m^2:20 m^2:25' \
  --metric performance.stall_speed_landing \
  --metric mission.total_fuel --format json
cargo run --bin aex -- sweep examples/b777/scenario.yaml \
  --var 'mission.payload.mass=30000 kg:68000 kg:20' \
  --metric mission.completed_distance \
  --metric mission.total_fuel --format json
```

Every successful analysis is stored below `runs/<run-id>/` with original and
resolved input, result JSON, warnings, assumptions CSV, model manifest, and a
SHA-256 content hash.

## MCP

Configure an MCP client to launch:

```bash
cargo run --bin aex -- mcp serve
```

The server exposes validation, profile lookup, scenario resolution, point
performance, mission simulation, constraints, payload-range, one- and
two-dimensional sweeps, comparison, explanation, report, and editable
aircraft-design experiment tools. `auto_refine_design` performs a bounded
native search, requires conceptual aerodynamic, structural, mission-power, and
requirement gates to pass, writes a new design, and can run an explicit final
OpenVSP verification. Portable studies can be loaded, run or resumed, queried
by immutable evidence ID, and explicitly promoted into one editable design.
Mission simulation is compact by default and returns an immutable run
reference; `detail: true` or `generate_report` retrieves the full result.
The shared capability manifest advertises versioned light-aircraft and
transport requirement templates with achieved metrics and provenance.
Study execution stores content-addressed evaluations and checkpoints under
`.ace/studies/`; it does not generate candidate directories. The server writes
protocol messages only to stdout; diagnostics use `tracing` on stderr.

Native conceptual geometry and performance analysis are always available.
OpenVSP is discovered automatically when its headless `vspscript` executable
is installed, or it can be configured with `ACE_OPENVSP_EXECUTABLE`. Selecting
the `openvsp` backend generates a `.vsp3` artifact, computes wetted area, and
runs a VSPAERO polar refinement. An OpenVSP failure is returned as a backend
error; it never causes an implicit substitution with native estimates.

## Model scope

The MVP implements the 1976 standard atmosphere through 86 km, scalar and
Mach-tabulated parabolic drag polars with explicit transonic behavior,
density-lapsed piston power, simple
and tabulated turbofan thrust/fuel decks, bounded speed and ceiling solves,
mission integration, constraint diagrams, payload-range, requirement margins,
SVG charts, and content-addressed run manifests. Concept-design feasibility
also includes a bounded weight iteration, resolved conventional geometry with
versioned statistical completion, Breguet
range/endurance, takeoff/landing estimates, mission operating-point power
reserve, and a conceptual wing-spar/tail-volume structural screen with explicit provenance
and validity limits.

The C172-class and B777-300ER-class files are calibration examples rather than
digital twins. Their metadata explicitly prohibits certification use.

See [architecture decisions](docs/architecture-decisions.md),
[acceptance evidence](docs/acceptance.md), [models](docs/models.md),
[CLI reference](docs/cli.md), and [MCP reference](docs/mcp.md).

## Verification

```bash
./ci.sh
```

The offline gate runs formatting, Clippy with warnings denied, all unit and
integration tests, missing-doc and intra-doc-link checks, and a release build.
