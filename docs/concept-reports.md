# Concept-design reports

A concept report should answer whether the design is worth another iteration
before presenting detailed plots. It is an auditable low-fidelity decision
record, not a certification analysis.

## Section order

1. Decision and failed hard requirements
2. Configuration, geometry, wetted area, and edge-alignment invariants
3. Mass closure, fuel volume, payload offload, and mission completion
4. Aerodynamics and performance
5. Structural and stability screens
6. Method provenance, assumptions, validity ranges, and warnings

OpenVSP results remain a separate refinement inside the report. A failed
OpenVSP run is returned as an error; it does not cause native values to be
substituted. The native feasibility verdict remains identifiable when OpenVSP
data is present.

## Chart order

The `generate_report` MCP tool returns chart specifications in this order:

1. Requirement margins
2. Payload-range
3. Mission mass, including payload-offload steps
4. Constraint diagram
5. Climb envelope
6. Required and available thrust or power
7. Drag polar from the selected refinement, when available

Wetted area, fuel-volume utilization, spar packaging, and edge alignment are
reported as result fields. They should not be plotted merely to fill a
dashboard.

## MCP request

```json
{
  "scenario_path": "examples/designs/model501/candidates/model501_pw812d/scenario.yaml",
  "backend": "native",
  "format": "markdown",
  "sections": []
}
```

Select `openvsp` to add a `.vsp3`, CompGeom wetted area, VSPAERO polar, and
static pitching-moment refinement. The same scenario and report schema are
used.
