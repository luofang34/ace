use super::{OpenVspBackend, positive_marker_number, script_string};
use crate::domain::diagnostic::{AexError, AexResult};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs};

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
    ) -> AexResult<(String, f64)> {
        let directory = tempfile::tempdir().map_err(|source| AexError::Write {
            path: std::env::temp_dir(),
            source,
        })?;
        let path = directory.path().join("wing.vsp3");
        let artifact = script_string(&path)?;
        let mut script =
            String::from("int main() { ClearVSPModel(); string wing=AddGeom(\"WING\",\"\");\n");
        for (key, name, group) in [
            ("area", "TotalArea", "WingGeom"),
            ("aspect_ratio", "TotalAR", "WingGeom"),
            ("taper", "Taper", "XSec_1"),
            ("sweep", "Sweep", "XSec_1"),
            ("dihedral", "Dihedral", "XSec_1"),
            ("twist", "Twist", "XSec_1"),
        ] {
            let value = inputs.get(key).ok_or_else(|| {
                AexError::validation(
                    "MISSING_GEOMETRY_INPUT",
                    key,
                    "required canonical wing input",
                )
            })?;
            script.push_str(&format!(
                "SetParmVal(wing,\"{name}\",\"{group}\",{value:.15});\n"
            ));
        }
        script.push_str(&format!(
            "Update(); WriteVSPFile(\"{artifact}\",SET_ALL);\n"
        ));
        script.push_str("ComputeCompGeom(SET_ALL,false,0); string id=FindLatestResultsID(\"Comp_Geom\"); array<double> areas=GetDoubleResults(id,\"Wet_Area\"); double area=0; for(uint i=0;i<areas.size();i++){area+=areas[i];} Print(\"ACE_WETTED_AREA_M2=\",false); Print(area); while(GetNumTotalErrors()>0){ErrorObj err=PopLastError();Print(\"ACE_ERROR=\"+err.GetErrorString());} Print(\"ACE_COMPLETE=1\"); return 0; }");
        let output = self.run_script_controlled_blocking(
            directory.path(),
            &script,
            "canonical wing geometry",
            "ACE_WETTED_AREA_M2=",
            control,
        )?;
        let area = positive_marker_number(&output, "ACE_WETTED_AREA_M2=")?;
        let content =
            fs::read_to_string(&path).map_err(|source| AexError::Read { path, source })?;
        Ok((content, area))
    }
}
