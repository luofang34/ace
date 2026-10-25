# Concept-design reports

A concept report should answer whether the design is worth another iteration
before presenting detailed plots. It is an auditable low-fidelity decision
record, not a certification analysis.

`aex report --vehicle VEHICLE` and the `generate_report` MCP tool with
`vehicle_path` synthesize the vehicle and report only what its own calculation
evidence supports.

## Section order

1. Feasibility, and hard requirements counted as satisfied, failed or unknown;
   an unavailable margin is never counted as met
2. Every requirement margin with its achieved value, or the reason it is
   unavailable
3. Charts drawn from the run evidence

## Chart order

1. Requirement margins, with unknown and not-applicable margins labelled
2. Each `aircraft.constraint_diagram` analysis
3. Mission mass by stage, including payload releases

An analysis that did not compute contributes no chart. Payload-range corners
come from `aex payload-range`, which names its distance and mass parameters
explicitly.

## MCP request

```json
{
  "vehicle_path": "examples/b777/vehicle.json",
  "format": "markdown",
  "sections": []
}
```
