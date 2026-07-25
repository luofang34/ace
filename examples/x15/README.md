# X-15-Class Model-Envelope Fixture

This example is an evaluation fixture, not a reference design. It encodes
known gaps documented in `docs/llm-workflow-evaluation.md` and is intentionally
wrong in labeled ways:

- The XLR99 rocket is forced into the turbofan deck (constant-thrust hack:
  zero altitude exponent, zero Mach coefficient, TSFC encoding Isp 279 s)
  because no rocket profile type exists.
- Engine-off segments use `thrust_fraction: 0.01` because the schema forbids
  exactly zero; the fake idle burns propellant during captive carry and glide,
  which is why the mission fails mid-"glide".
- `altitude` on the captive-carry `fixed_time` segment is currently accepted
  and ignored (the mission starts at 0 m); there is no air-launch initial
  state.
- The real flight profile (ballistic arc past 80 km, Mach 6.7) is outside the
  quasi-steady solver and the atmosphere domain entirely.

Guardrail tests should assert honest `unsupported` diagnostics for the
inexpressible pieces, and physical behavior (zero burn when engine-off, air
launch honored) once the mission vocabulary supports them.
