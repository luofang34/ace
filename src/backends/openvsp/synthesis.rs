use super::{OpenVspBackend, positive_marker_number, script_string};
use crate::domain::diagnostic::{AexError, AexResult};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

pub(crate) struct SynthesisWing {
    pub content: String,
    pub wetted_area: f64,
    pub measured: BTreeMap<String, f64>,
}

impl OpenVspBackend {
    pub(crate) fn binary_identity_blocking(&self) -> AexResult<String> {
        let bytes = fs::read(&self.executable).map_err(|source| AexError::Read {
            path: self.executable.clone(),
            source,
        })?;
        Ok(format!(
            "openvsp:{}:sha256:{:x}",
            self.version.as_deref().unwrap_or("unknown"),
            Sha256::digest(bytes)
        ))
    }

    pub(crate) fn generate_synthesis_wing_blocking(
        &self,
        inputs: &BTreeMap<String, f64>,
        control: &ace_engine::RunControl,
    ) -> AexResult<SynthesisWing> {
        let directory = tempfile::tempdir().map_err(|source| AexError::Write {
            path: std::env::temp_dir(),
            source,
        })?;
        let path = directory.path().join("wing.vsp3");
        let settings = wing_settings(inputs)?;
        let script = wing_script(&settings, &script_string(&path)?);
        let output = self.run_script_controlled_blocking(
            directory.path(),
            &script,
            "canonical wing geometry",
            "ACE_WETTED_AREA_M2=",
            control,
        )?;
        let measured = validate_readback(&output, &settings)?;
        let wetted_area = positive_marker_number(&output, "ACE_WETTED_AREA_M2=")?;
        let content =
            fs::read_to_string(&path).map_err(|source| AexError::Read { path, source })?;
        Ok(SynthesisWing {
            content,
            wetted_area,
            measured,
        })
    }
}

fn wing_settings(inputs: &BTreeMap<String, f64>) -> AexResult<BTreeMap<String, f64>> {
    let read = |key| {
        inputs.get(key).copied().ok_or_else(|| {
            AexError::validation(
                "MISSING_GEOMETRY_INPUT",
                key,
                "required canonical wing input",
            )
        })
    };
    let area = read("area")?;
    let span = read("span")?;
    let taper = read("taper")?;
    let dihedral = read("dihedral")?;
    let cos_dihedral = dihedral.to_radians().cos();
    let root = 2.0 * area / (span * (1.0 + taper));
    // Canonical geometry uses horizontal span, untwisted projected area and leading-edge sweep.
    // OpenVSP section span and sweep are measured in the inclined panel plane.
    Ok(BTreeMap::from([
        ("Span".into(), span / (2.0 * cos_dihedral)),
        ("Root_Chord".into(), root),
        ("Tip_Chord".into(), root * taper),
        ("Sweep_Location".into(), 0.0),
        (
            "Sweep".into(),
            (read("sweep")?.to_radians().tan() * cos_dihedral)
                .atan()
                .to_degrees(),
        ),
        ("Dihedral".into(), dihedral),
        ("Twist".into(), read("twist")?),
        ("Taper".into(), taper),
        ("TotalArea".into(), area / cos_dihedral),
        ("TotalProjectedSpan".into(), span),
    ]))
}

fn wing_script(settings: &BTreeMap<String, f64>, artifact: &str) -> String {
    let mut script = String::from(
        "int main() { ClearVSPModel(); string wing=AddGeom(\"WING\",\"\"); SetDriverGroup(wing,1,SPAN_WSECT_DRIVER,ROOTC_WSECT_DRIVER,TIPC_WSECT_DRIVER); Update(); SetParmVal(wing,\"Sym_Planar_Flag\",\"Sym\",SYM_XZ);\n",
    );
    for (name, value) in settings {
        if !name.starts_with("Total") && name != "Taper" {
            script.push_str(&format!(
                "SetParmVal(wing,\"{name}\",\"XSec_1\",{value:.15});\n"
            ));
        }
    }
    script.push_str("Update();\n");
    for name in settings.keys() {
        let group = if name.starts_with("Total") {
            "WingGeom"
        } else {
            "XSec_1"
        };
        script.push_str(&format!("Print(\"ACE_GEOMETRY_{name}=\",false); Print(GetParmVal(wing,\"{name}\",\"{group}\"));\n"));
    }
    script.push_str(&format!("WriteVSPFile(\"{artifact}\",SET_ALL);\n"));
    script.push_str("ComputeCompGeom(SET_ALL,false,0); string id=FindLatestResultsID(\"Comp_Geom\"); array<double> areas=GetDoubleResults(id,\"Wet_Area\"); double area=0; for(uint i=0;i<areas.size();i++){area+=areas[i];} Print(\"ACE_WETTED_AREA_M2=\",false); Print(area); while(GetNumTotalErrors()>0){ErrorObj err=PopLastError();Print(\"ACE_ERROR=\"+err.GetErrorString());} Print(\"ACE_COMPLETE=1\"); return 0; }");
    script
}

fn validate_readback(
    output: &str,
    expected: &BTreeMap<String, f64>,
) -> AexResult<BTreeMap<String, f64>> {
    expected
        .iter()
        .map(|(name, expected)| {
            let actual = super::parsing::marker_number(output, &format!("ACE_GEOMETRY_{name}="))?;
            // OpenVSP's script Print(double) rounds to six decimal places.
            if (actual - expected).abs() > 1e-6_f64.max(expected.abs() * 1e-8) {
                return Err(AexError::validation(
                    "OPENVSP_GEOMETRY_MISMATCH",
                    name,
                    format!(
                        "requested {expected}, generated {actual}; refusing inconsistent geometry"
                    ),
                ));
            }
            Ok((name.clone(), actual))
        })
        .collect()
}

#[cfg(test)]
mod tests;
