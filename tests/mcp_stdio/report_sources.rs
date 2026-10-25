use super::*;

#[test]
fn scenario_concept_reports_are_retired_without_side_effects() -> Result<(), Box<dyn Error>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let temporary = tempfile::tempdir()?;
            let mut command = tokio::process::Command::new(assert_cmd::cargo::cargo_bin!("aex"));
            command.args(["mcp", "serve"]);
            command.current_dir(temporary.path());
            let client = ().serve(TokioChildProcess::new(command)?).await?;

            // A scenario path is no longer a report source; vehicle reports replace it.
            let response = client
                .call_tool(CallToolRequestParams {
                    meta: None,
                    name: "generate_report".into(),
                    arguments: Some(arguments(json!({
                        "scenario_path": scenario("c172"),
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
                Some(&serde_json::Value::String(
                    "MISSING_REPORT_SOURCE".to_owned()
                ))
            );
            assert_eq!(std::fs::read_dir(temporary.path())?.count(), 0);
            client.cancel().await?;
            Ok(())
        })
}
