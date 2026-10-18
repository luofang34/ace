//! Workspace package metadata enforces the project's distribution license.
#![cfg(test)]

#[test]
fn every_workspace_package_declares_agpl_blocking() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::process::Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let members = metadata["workspace_members"]
        .as_array()
        .ok_or("missing members")?;
    let packages = metadata["packages"].as_array().ok_or("missing packages")?;
    for member in members {
        let package = packages
            .iter()
            .find(|package| &package["id"] == member)
            .ok_or("missing package")?;
        assert_eq!(package["license"], "AGPL-3.0-only", "{}", package["name"]);
        let manifest = std::path::Path::new(
            package["manifest_path"]
                .as_str()
                .ok_or("missing manifest path")?,
        );
        let directory = manifest.parent().ok_or("missing package directory")?;
        assert_eq!(
            std::fs::read_to_string(directory.join("LICENSE"))?,
            include_str!("../LICENSE")
        );
        if package["name"] != "ace-cfd" {
            assert_eq!(
                std::fs::read_to_string(directory.join("NOTICE"))?,
                include_str!("../NOTICE")
            );
        }
    }
    Ok(())
}
