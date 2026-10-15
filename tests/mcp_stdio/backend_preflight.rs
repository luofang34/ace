use super::*;

#[test]
fn unavailable_openvsp_preflights_before_mutation() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let temporary = tempfile::tempdir()?;
            let design_root = temporary.path().join("designs");
            let artifact = temporary.path().join("rejected.vsp3");
            let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
            command.args(["mcp", "serve"]);
            command.env(
                "ACE_OPENVSP_EXECUTABLE",
                "/path/that/does/not/contain/vspscript",
            );
            let client = ().serve(TokioChildProcess::new(command)?).await?;

            let response = client
                .call_tool(CallToolRequestParams {
                    meta: None,
                    name: "auto_refine_design".into(),
                    arguments: Some(arguments(json!({
                        "scenario_path": scenario("c172"),
                        "output_design_id": "must_not_exist",
                        "display_name": "Rejected Design",
                        "design_root": design_root,
                        "backend": "openvsp",
                        "artifact_path": artifact,
                        "max_iterations": 12
                    }))?),
                    task: None,
                })
                .await?;

            assert_eq!(response.is_error, Some(true));
            assert_eq!(
                response
                    .structured_content
                    .as_ref()
                    .map(|content| &content["code"]),
                Some(&serde_json::Value::String("BACKEND_UNAVAILABLE".to_owned()))
            );
            assert!(!design_root.exists());
            assert!(!artifact.exists());
            client.cancel().await?;
            Ok(())
        })
}
