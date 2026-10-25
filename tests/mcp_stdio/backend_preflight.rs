use super::*;

#[test]
fn unavailable_openvsp_preflights_before_mutation() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let temporary = tempfile::tempdir()?;
            let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
            command.args(["mcp", "serve"]);
            command.current_dir(temporary.path());
            command.env(
                "ACE_OPENVSP_EXECUTABLE",
                "/path/that/does/not/contain/vspscript",
            );
            let client = ().serve(TokioChildProcess::new(command)?).await?;

            let response = client
                .call_tool(CallToolRequestParams {
                    meta: None,
                    name: "generate_report".into(),
                    arguments: Some(arguments(json!({
                        "scenario_path": scenario("c172"),
                        "backend": "openvsp",
                        "format": "json"
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
            assert_eq!(std::fs::read_dir(temporary.path())?.count(), 0);
            client.cancel().await?;
            Ok(())
        })
}
