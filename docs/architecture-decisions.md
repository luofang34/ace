# Architecture decisions

Status: accepted for the Rust MVP.

## ADR-001 — Deterministic core with thin adapters

All equations, root solves, mission integration, and chart data generation live
below the domain/model/service boundary. CLI and MCP handlers parse requests
and invoke the same `ApplicationService`; neither adapter invents performance
values.

## ADR-002 — Rust implementation

The independent implementation targets Rust 1.88+ and forbids unsafe code.
Serde provides interface schemas, `uom` normalizes physical quantities, Rayon
orders local sweeps, Plotters renders SVG, and the official `rmcp` SDK serves
MCP over stdio.

## ADR-003 — SI storage, explicit interface units

Core values use kg, N, m, m², m/s, W, Pa, kg/s, and seconds. Dimensional
inputs require a unit string. Outputs retain SI values with display metadata.
Range and completed distance display in `nmi` by default.

## ADR-004 — Provenance and diagnostics are data

Resolved scalar inputs become assumption-ledger rows with source, confidence,
explicit/profile/default flags, and units. Every warning has a stable code,
severity, path, message, and context object. Strict mode promotes model
extrapolation and agent-assumption warnings to failures.

## ADR-005 — Immutable content-addressed runs

Analyses write through an injected run repository. A completed directory is
never modified. The manifest identifies software, models, seed, artifacts, and
a SHA-256 hash over normalized content.

## ADR-006 — Explicit fidelity and validity

Every numerical result identifies model ID, version, fidelity level, and
validity status. Implemented models are fidelity 0 or 1. Approximations and
extrapolation remain visible regardless of numeric precision.

## ADR-007 — Registered code, data-only profiles

Aircraft select known deterministic model IDs. Engine and propeller profiles
are YAML data loaded through an injected repository; profiles cannot execute
code.

## ADR-008 — Separate aircraft, mission, requirements, and scenario

Scenario resolution combines independent versioned documents. Dotted-path
overrides apply before normalization, allowing reuse and declarative sweeps.

## ADR-009 — Segment-based mission

The quasi-steady solver supports taxi/fixed-time, takeoff, climb, cruise,
loiter/reserve, descent, landing, and fixed-fuel segments. Each segment records
start/end mass, fuel, distance, duration, altitude, and warnings.

## ADR-010 — Parabolic baseline aerodynamics

The baseline is `CD = CD0 + k CL² + CDadditional + CDwave`, with
`k = 1/(πeAR)`. The optional power-law wave-drag term is labeled approximate
and warns in transonic use.

## ADR-011 — Power and thrust propulsion capabilities

Piston-propeller profiles expose shaft/propulsive power, bounded low-speed
thrust, and BSFC fuel flow. Turbofans expose altitude/Mach-lapsed thrust and
TSFC fuel flow. Shared performance code consumes either capability.

## ADR-012 — Explicit requirement semantics

Requirements use hard, soft, or report-only severity with `ge`, `le`, or `eq`
operators. Evaluations include actual/required quantities, pass state, absolute
and percentage margin, and tolerance warning state.

## ADR-013 — Serializable chart specifications

Analyses return renderer-independent axes, series, annotations, and warnings.
The SVG renderer consumes only this specification and never performs aircraft
calculations.

## ADR-014 — Declarative deterministic sweeps

One- and two-dimensional linear, logarithmic, explicit-value, and categorical
profile sweeps expand into a stable Cartesian order. Local workers share a
sub-run cache without holding a lock across asynchronous work.

## ADR-015 — Bounded scalar closure

Weight closure and envelope intersections require a sign-changing bracket and
use bounded iteration. Missing brackets and non-convergence are typed failures
with no silent clamping.

## ADR-016 — Calibration examples, not digital twins

The C172-class and B777-300ER-class examples demonstrate different scale and
propulsion modes. Broad acceptance bands test software behavior; no result
claims handbook, manufacturer, operational, or certification authority.

## ADR-017 — Backend-neutral geometry and analysis

Geometry generation and low-order analysis implement separate backend traits.
The native backend is always registered. Optional adapters consume resolved
canonical scenarios and return backend-neutral geometry, scalar, polar,
stability, provenance, unit, and warning structures. Adapter-specific
parameter identifiers remain private implementation details.

## ADR-018 — OpenVSP is an explicit subprocess refinement

OpenVSP runs through its headless script executable and produces `.vsp3`,
CompGeom wetted-area, and VSPAERO polar data. Native feasibility remains a
separate baseline in the response. Requesting OpenVSP is explicit, and any
launch, analysis, protocol, or parsing failure fails the request rather than
silently returning a native substitute.

## ADR-019 — Editable designs use validated scenario overlays

An editable design is a normal scenario directory plus a persisted
backend-neutral `overrides` map. Creating a design copies its canonical
aircraft, mission, requirements, and profiles. Updating parameters validates
the prospective resolved scenario before atomically replacing the overlay.

## ADR-020 — Automatic refinement is bounded and independently verified

Automatic refinement uses deterministic coordinate search over canonical wing
area, aspect ratio, propulsion sizing, and fuel capacity. Wing span preserves
the planform identity, and propulsion growth carries an empty-mass penalty.
Convergence requires all requirements, mission power reserve, tail-volume,
aspect-ratio, and wing-spar packaging screens to pass. Optional OpenVSP runs
only on the selected candidate; its static-pitch result is an independent
verification gate and is not substituted into native feasibility.

## ADR-023 — Aircraft configurations use components and relationships

The canonical aircraft model represents configurations as stable component
instances and explicit relationships rather than deriving topology from one
configuration label. Components have identifiers, registered kinds, optional
reusable definitions, counts, data parameters, and analysis roles.
Relationships describe attachment, symmetry, repetition, alignment,
parallelism, continuity, and load-path intent. Backend-native geometry
identifiers, meshes, and solver entities do not enter this graph.

Missing topology in a schema-version-1 aircraft resolves to a versioned graph
inferred from the legacy configuration, engine count, and propeller presence.
Explicit topology takes precedence. Each backend advertises the component and
relationship kinds it supports. A canonical but unsupported graph returns an
explicit unsupported result; adapters do not flatten, drop, or reinterpret
components silently.

Backend support covers both vocabulary and graph shape. Component definitions
and roles are descriptive metadata, while counts, parameters, and relationship
endpoints must be consumed or rejected. Native conventional analysis requires
one fuselage, wing, horizontal tail, and vertical tail, and propulsion counts
must agree with the resolved propulsion model. OpenVSP advertises only the
component and relationship mappings implemented by its geometry adapter,
including conventional propellers and one- or two-engine envelopes. A
relationship outside a refinement backend's disciplines may be declared as
delegated only when the mandatory native baseline consumes it. OpenVSP
delegates `carries_load_to` and does not claim structural-load-path fidelity.

## ADR-024 — Refinement is a discipline-aware analysis graph

Geometry, aerodynamics, propulsion, structures, mission, controls, signatures,
and other disciplines are independent capabilities. Backend descriptors name
their disciplines, fidelity levels, and topology support. A refinement result
remains separate from native feasibility and never overwrites its provenance
or verdict.

Multi-fidelity policies decide which candidates advance to expensive analyses.
A failed, unavailable, unsupported, or out-of-validity refinement remains a
distinct state and does not cause silent substitution by a lower-fidelity
value. Solver decks, meshes, and detailed model files are artifacts rather
than canonical aircraft state.
