use rustdesk_agent::detection::{Observation, Query, parse_observation};

#[test]
fn only_fixed_read_commands_are_exposed() {
    assert_eq!(Query::Version.args(), ["--version"]);
    assert_eq!(Query::Id.args(), ["--get-id"]);
    assert_eq!(Query::ApproveMode.args(), ["--option", "approve-mode"]);
    assert_eq!(
        Query::VerificationMethod.args(),
        ["--option", "verification-method"]
    );
}

#[test]
fn accepts_only_expected_output() {
    for (query, output) in [
        (Query::Version, "1.4.7\r\n"),
        (Query::Id, "123456789\n"),
        (Query::ApproveMode, "password-click\n"),
        (Query::VerificationMethod, "use-both-passwords\n"),
    ] {
        assert!(matches!(
            parse_observation(query, true, output.as_bytes(), b""),
            Observation::Observed(_)
        ));
    }
}

#[test]
fn errors_and_blank_output_are_not_observations() {
    for output in [
        "",
        "Settings are disabled!",
        "Installation and administrative privileges required!",
        "click\npassword",
    ] {
        assert_eq!(
            parse_observation(Query::ApproveMode, true, output.as_bytes(), b""),
            Observation::InvalidOutput
        );
    }
    assert_eq!(
        parse_observation(Query::Id, false, b"123456789", b""),
        Observation::CommandFailed
    );
    assert_eq!(
        parse_observation(Query::Id, true, b"123456789", b"IPC failed"),
        Observation::CommandFailed
    );
    assert_eq!(
        parse_observation(Query::Version, true, b"unknown", b""),
        Observation::InvalidOutput
    );
    assert_eq!(
        parse_observation(Query::Id, true, &[0xff], b""),
        Observation::InvalidOutput
    );
}
