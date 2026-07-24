# Model 501-10 engine and geometry trade

This example interprets the requirements as:

- 35,000 lb maximum takeoff mass
- 4,200 lb payload carried 3,500 nmi outbound
- payload offloaded after landing
- empty-aircraft return mission
- 8,000 nmi zero-payload ferry target

The mission contains an explicit `payload_drop` segment. A 7,000 nmi mission
with payload retained on both legs is a different and more demanding
requirement.

## Edge discipline

The center body has one independent edge angle, `θ`. Its leading edge is `+θ`
and its trailing edge is `-θ`, so the two edges remain parallel in projection.
The outer wing has one quarter-chord sweep. Left and right geometry is
symmetric. The planform therefore uses two independent sweep numbers rather
than separate values for each edge or side.

## MCP-native screening result

| Candidate | Engines | Full-payload range | Ferry range | Native wetted area | L/D max | Structural screen |
|---|---:|---:|---:|---:|---:|---|
| `model501_pw812d` | 1 × PW812D | 4,000 nmi | 4,141 nmi | 184.8 m² | 24.23 | pass |
| `model501_twin_pw306d1` | 2 × PW306D1 | 3,450 nmi | 3,567 nmi | 193.6 m² | 23.55 | pass |
| `model501_passport20` | 1 × Passport 20-17 | 3,148 nmi | 3,258 nmi | 198.0 m² | 23.08 | pass |

All three concepts fail overall feasibility. The single PW812D is the best of
this set and passes the 3,500 nmi full-payload point, but it exhausts fuel
during the return cruise and misses the 8,000 nmi ferry target by about 48%.

OpenVSP refinement of the PW812D candidate returned 179.14 m² wetted area,
maximum sampled L/D of 30.77, and a static pitching-moment slope of
-0.00893/deg. The slope is statically stable in the low-order VSPAERO sweep.
These refined values do not replace the failed native mission verdict.

An MCP sweep spanning clean `CD0 = 0.009–0.013` and Oswald efficiency
`0.86–0.94` did not close the ferry requirement. Even the most optimistic
combination returned about 5,767 nmi. The 8,000 nmi target is therefore not a
credible 35,000 lb requirement under these mass and engine assumptions. The
next trade should change a primary constraint—MTOW, empty mass, fuel fraction,
ferry tank allowance, speed schedule, or engine cycle—not merely polish the
planform.

The structural result is a conceptual outer-panel bending and
volume-packaging screen. It does not establish pressure-cabin integration,
cutout loads, torsion, buckling, fatigue, flutter, crashworthiness, systems
routing, or certification compliance.

## Engine data status

Rated thrust, dimensions, and dry mass come from manufacturer data or the
applicable type-certificate data sheet. TSFC, thrust lapse, bypass ratio, inlet
loss, and nacelle drag are explicit low-confidence assumptions because public
engine decks are insufficient for this study.

- [Pratt & Whitney PW800 product data](https://www.prattwhitney.com/en/products/business-aviation-engines/pw800)
- [EASA PW800 TCDS E.081](https://www.easa.europa.eu/en/downloads/33453/en)
- [EASA PW300 TCDS E.008](https://www.easa.europa.eu/en/downloads/7711/en)
- [GE Aerospace Passport overview](https://www.geaerospace.com/news/press-releases/regional-business-engines/ge-passport-achieves-faa-certification-business-jet)
- [EASA Passport TCDS E.109](https://www.easa.europa.eu/en/downloads/68115/en)

The PW306D/D1 approval basis specifies multiple-engine installation. The
single-engine PW812D and Passport integrations here are experimental aircraft
concepts and are not claims of an approved installation.

## OpenVSP inspection

Evaluate a candidate with the `openvsp` backend and provide an artifact path:

```json
{
  "scenario_path": "examples/designs/model501/candidates/model501_pw812d/scenario.yaml",
  "backend": "openvsp",
  "artifact_path": "runs/model501-study/model501-pw812d.vsp3"
}
```

Open the resulting file with:

```sh
open -a OpenVSP runs/model501-study/model501-pw812d.vsp3
```

Inspect the top view for equal-and-opposite center edges, section reflex,
engine-envelope placement, the wetted-area component table, and VSPAERO
reference quantities. OpenVSP improves geometry and low-order aerodynamic
evidence; it does not make the structural screen authoritative.
