#[test]
fn command_tree_has_consistent_arguments() {
    super::command().debug_assert();
}

#[test]
fn commands_parse_using_the_public_grammar() {
    for arguments in [
        vec![
            "aex",
            "analyze",
            "mission",
            "scenario.yaml",
            "--format",
            "json",
        ],
        vec![
            "aex",
            "sweep",
            "scenario.yaml",
            "--var",
            "wing.area=10:20:3",
            "--metric",
            "range",
        ],
        vec!["aex", "report", "run-id", "--format", "markdown"],
        vec!["aex", "profile", "list"],
    ] {
        let matches = super::command()
            .try_get_matches_from(arguments)
            .expect("valid arguments");
        super::super::parsing::command(matches).expect("typed arguments");
    }
}
