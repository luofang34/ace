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
cargo run --bin aex -- validate examples/c172/scenario.yaml --format json
cargo run --bin aex -- analyze point examples/c172/scenario.yaml \
  --altitude "8000 ft" --speed "115 kt" --format json
cargo run --bin aex -- analyze mission examples/c172/scenario.yaml --format json
cargo run --bin aex -- analyze point examples/b777/scenario.yaml \
  --altitude "35000 ft" --mach 0.84 --format json
cargo run --bin aex -- analyze mission examples/b777/scenario.yaml --format json
```

SI values remain authoritative inside the solver and persisted runs. Range and
completed-distance fields display in nautical miles by default in every output
format, including when other display fields remain SI:

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
two-dimensional sweeps, comparison, explanation, and report tools. It writes
protocol messages only to stdout; diagnostics use `tracing` on stderr.

## Model scope

The MVP implements ISA through 20 km, a parabolic drag polar with optional
transonic drag rise, density-lapsed piston power, a simple turbofan thrust/TSFC
deck, bounded speed and ceiling solves, segment mission integration,
constraint diagrams, payload-range, requirement margins, SVG charts, and
content-addressed run manifests.

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

