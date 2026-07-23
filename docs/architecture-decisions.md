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

