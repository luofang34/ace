# X-15-Class Model-Envelope Fixture

This example is an evaluation fixture, not a reference design. It encodes
known gaps documented in `docs/llm-workflow-evaluation.md`:

- The XLR99 uses the explicit `rocket_engine` profile type with a thrust/Isp
  table. NASA's 57,000 lbf thrust and greater-than-13,000 lb/min propellant
  flow anchors produce approximately 98 kg/s at full throttle through
  `T/(Isp·g0)`.
- Engine-off segments use `thrust_fraction: 0`, producing zero modeled thrust
  and fuel flow while captive-carry, glide, and landing kinematics continue.
- The explicit initial state starts the captive carry and later drop at
  45,000 ft with 8,500 kg of usable fuel.
- The powered boost uses an ordered energy-climb schedule; the following
  speed run is an engine-off coast. The low-fidelity fixture boost remains in
  the guarded 80–120 second band.
- The real flight profile (ballistic arc past 80 km, Mach 6.7) is outside the
  quasi-steady solver and the atmosphere domain entirely.

Guardrail tests assert honest `unsupported` diagnostics for the remaining
inexpressible trajectory, plus physical engine-off behavior, air-launch state,
and rocket mass flow.
