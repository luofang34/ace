# Aircraft Concept Explorer

Aircraft Concept Explorer is a deterministic Rust application for early-stage
fixed-wing aircraft concept studies. One low-fidelity analysis library powers
both the `aircraft-explorer`/`aex` CLI and its stdio MCP server.

It is not a flight simulator or certification tool. Results are conceptual
estimates and are not suitable for operational flight planning or
safety-critical decisions.

## Quick start

Rust 1.88 or newer is required.

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

The MVP implements ISA through 20 km, scalar and Mach-tabulated parabolic drag
polars with explicit transonic behavior, density-lapsed piston power, simple
and tabulated turbofan thrust/fuel decks, bounded speed and ceiling solves,
mission integration, constraint diagrams, payload-range, requirement margins,
SVG charts, and content-addressed run manifests. Concept-design feasibility
also includes a bounded weight iteration, empirical tail sizing, Breguet
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
