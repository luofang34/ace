# SR-71-Class Model-Envelope Fixture

This example is an evaluation fixture, not a reference design. It encodes
known gaps documented in `docs/llm-workflow-evaluation.md` and labels every
remaining surrogate:

- The J58 uses the explicit `turbojet_engine` profile type and a versioned
  installed thrust/TSFC table. NASA YF-12 material anchors the engine type and
  32,500 lbf static rating; intermediate off-design cells remain disclosed
  conceptual data. The subsonic required-thrust check is 3–5 t/hr and the
  Mach 3.2 / 78,000 ft point is inside table support.
- The authentic mission cruises at 78,000 ft, above the atmosphere model's
  20 km domain, and must fail statically at validate/resolve rather than at
  runtime.
- The clean polar uses separate subsonic, transonic, and supersonic
  breakpoints. Its maximum-L/D guardrails are 8–10 subsonically and 5–6.5 at
  Mach 3.2; values outside Mach 0–3.3 are explicit extrapolations.
- `maximum_fuel_mass` is raised to 46,180 kg from NASA's documented
  80,280 lb (36,414 kg) capacity. The current mission model starts on the
  runway with one fuel load and cannot represent the SR-71's operational
  post-takeoff refueling, so this is an explicit mission-completion surrogate,
  not a capacity claim. See
  [NASA's SR-71 fact page](https://www.nasa.gov/image-article/sr-71-takeoff-with-afterburner/).

Guardrail tests assert that remaining unsupported or extrapolated conditions
are explicit and never silently produce a valid result.
