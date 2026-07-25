# X-15-Class Model-Envelope Fixture

This example is an evaluation fixture, not a reference design. It encodes
known gaps documented in `docs/llm-workflow-evaluation.md` and is intentionally
wrong in labeled ways:

- The XLR99 rocket is forced into the turbofan deck (constant-thrust hack:
  zero altitude exponent, zero Mach coefficient, TSFC encoding Isp 279 s)
  because no rocket profile type exists.
- Engine-off segments use `thrust_fraction: 0`, producing zero modeled thrust
  and fuel flow while captive-carry, glide, and landing kinematics continue.
- The captive-carry `fixed_time` altitude sets and propagates the 45,000 ft
  operating point, but there is no explicit air-launch initial state, so the
  first segment still begins from the default ground state.
- The real flight profile (ballistic arc past 80 km, Mach 6.7) is outside the
  quasi-steady solver and the atmosphere domain entirely.

Guardrail tests should assert honest `unsupported` diagnostics for the
inexpressible pieces, and physical behavior (zero burn when engine-off, air
launch honored) once the mission vocabulary supports them.
