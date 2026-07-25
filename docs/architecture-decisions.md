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
Wing area, span, and aspect ratio reconcile at this boundary: any two define
the third, rounded triples have a 0.5% tolerance, and the canonical ratio is
always recomputed from resolved area and span.

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

The `.vsp3` model retains the complete generated airframe and propulsion
envelopes. CompGeom evaluates that full model, while VSPAERO consumes a named
set containing only lifting surfaces. Profile dimensions take precedence for
engine envelopes; otherwise dry-mass cube-root correlations provide
adapter-internal visual dimensions.

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

## ADR-021 — Studies are shareable schema documents

A study is a schema-version-1 document that declares its baseline, variables,
derived parameters, objectives, constraints, selected snapshots, analysis
policy, and search policy. A baseline is either a relative scenario reference
or a complete embedded document set. Absolute references are rejected so the
same study can be copied, archived, and validated outside its authoring
directory.

Defaults are serialized and deterministic. Variable and objective identities
are stable, integer values are type checked, conditional variables reference
declared variables, and paths are canonical scenario paths. Search execution
is a service concern; the document remains useful without running a search.

## ADR-022 — Evaluations are immutable content-addressed evidence

A candidate identity is the SHA-256 digest of its schema version, baseline
digest, and normalized parameter map. An evaluation identity includes its
candidate and input identity, analysis method and model versions, fidelity,
status, metrics, constraints, diagnostics, provenance, dependencies, and
artifacts. Any result or provenance change therefore creates a new evaluation
rather than rewriting history.

File repositories persist evaluations and study archives as immutable,
digest-sharded JSON records. Repeating the same write is idempotent. Malformed
JSON and content-ID mismatches retain the stored path in a typed error.
Archives reference evaluation IDs and compact candidate descriptors; they do
not create mutable candidate directories.

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

## ADR-025 — Study archives preserve decisions without mutable workspaces

A study archive identifies the study and baseline digests, evaluator
signature, completion state, candidate descriptors, evaluation IDs, and
selected candidate IDs. It also records the optimizer generation, random state,
population, candidate outcomes, scores, and feasible Pareto set. Its content
identity covers every field. Selected candidate snapshots may also travel with
a study document, but immutable evaluation records remain separately
addressable evidence.

Archives record what was evaluated and selected without treating generated
candidate directories as durable state. Workflow tools may reconstruct a
candidate from the baseline and its parameter map, and promotion writes only
explicit user-selected canonical documents. Grid traversal follows declared
variable and value order. Evolutionary traversal is seeded, checkpoints its
random state after every population-construction step, and resumes from the
furthest matching immutable archive. Feasible-first ranking prevents a hard-infeasible candidate from
winning through objective score alone. Conditional combinations count toward
the evaluation limit only after they resolve to a unique descriptor.
Mass-closure derivations apply propulsion dry-mass changes to operating empty
mass and maximum takeoff mass together; fuel-capacity changes affect maximum
takeoff mass without altering operating empty mass.

Study IDs are human-stable names rather than revision identities. Retrieval
uses an archive ID when more than one study, baseline, or evaluator signature
shares a study ID. Promotion resolves the exact signature of the supplied
study document. Archive consumers verify candidate, evaluation, study, and
feasibility links before returning evidence or creating a design.

## ADR-026 — Solver boundaries are not physical roots

Bounded performance solves return a numeric value together with per-metric
validity. Interior roots are valid unless their operating condition requires
model extrapolation. Saturation at an atmosphere or default search boundary is
boundary-limited. A declared aircraft operating limit is an intentional
physical input and remains a valid limiting value.

Requirement evaluation is tri-state. A boundary-limited actual produces an
indeterminate result, with `passed: null` for compatibility, regardless of its
numeric margin. Hard indeterminate requirements and study constraints cannot
satisfy feasibility or enter a feasible Pareto set; evidence assigns them a
positive normalized violation. Extrapolated interior values remain distinct
from boundary-limited values so policy can treat them separately.
