# Strict warning policy

Strict mode promotes only warnings marked `yes` to `STRICT_WARNING_FAILURE`.
The wire codes and decisions below are generated from the runtime registry.

| Warning code | Promoted | Meaning |
| --- | --- | --- |
| `AGENT_ASSUMPTION` | yes | A value was supplied by an agent assumption rather than declared source data. |
| `APPROXIMATE_TAKEOFF_DISTANCE` | no | Takeoff distance uses the documented conceptual energy approximation. |
| `CERTIFICATION_USE_NOT_PROHIBITED` | no | Example metadata does not explicitly prohibit certification use. |
| `CROSS_CLASS_COMPARISON` | no | A comparison mixes propulsion classes with non-equivalent loading metrics. |
| `CRUISE_ALTITUDE_LIMIT_EXCEEDED` | no | A declared cruise condition exceeds an aircraft operating limit. |
| `CRUISE_CAPABILITY_UNAVAILABLE` | no | Installed capability cannot produce a supported achieved-cruise result. |
| `CRUISE_CONDITION_UNSUPPORTED` | no | A declared cruise condition cannot be evaluated by the selected model. |
| `FUEL_CAPACITY_EXCEEDED` | no | The requested initial fuel load exceeds tank capacity. |
| `FUEL_EXHAUSTED` | no | Usable fuel is depleted before the mission completes. |
| `INDETERMINATE_REQUIREMENT` | no | A boundary-limited requirement margin cannot be plotted as pass or fail. |
| `LOW_FIDELITY_MODEL` | no | The result uses a documented conceptual-fidelity model. |
| `MODEL_EXTRAPOLATION` | yes | A model is evaluated outside its calibrated or declared range. |
| `NATIVE_STABILITY_NOT_MODELED` | no | The native backend does not estimate stability derivatives. |
| `PARAMETER_OUTSIDE_TYPICAL` | yes | A resolved profile parameter is outside its registered typical range. |
| `SIMPLIFIED_PAYLOAD_RANGE` | no | Payload-range values use a representative cruise approximation. |
| `STUDY_CHART_PROJECTED` | no | A higher-dimensional study chart displays a declared two-axis projection. |
| `TRANSONIC_DRAG_APPROXIMATION` | no | Wave drag uses the documented conceptual power-law correction. |
| `ZERO_BREGUET_FUEL_BURN` | no | The Breguet estimate is zero because the simulated mission burns no fuel. |
