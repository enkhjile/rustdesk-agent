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
    for (query, output) in [(Query::Version, "1.4.7\r\n"), (Query::Id, "123456789\n")] {
        assert!(matches!(
            parse_observation(query, true, output.as_bytes(), b""),
            Observation::Observed(_)
        ));
    }
}

#[test]
fn errors_and_blank_output_are_not_observations() {
    for output in [
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

#[test]
fn settings_cli_values_never_claim_a_live_observation() {
    for (query, values) in [
        (Query::ApproveMode, ["password", "click", "password-click"]),
        (
            Query::VerificationMethod,
            [
                "use-temporary-password",
                "use-permanent-password",
                "use-both-passwords",
            ],
        ),
    ] {
        for value in values {
            let result = parse_observation(query, true, format!("{value}\r\n").as_bytes(), b"");
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                serde_json::json!({
                    "status": "unverified_cli_value", "value": value
                })
            );
        }
        for blank in ["", "\n", "\r\n", " \t\r\n"] {
            let result = parse_observation(query, true, blank.as_bytes(), b"");
            assert_eq!(
                serde_json::to_value(result).unwrap(),
                serde_json::json!({
                    "status": "ambiguous_default"
                })
            );
        }
    }
}

#[test]
fn ambiguity_does_not_hide_errors_or_invent_identity() {
    for query in [Query::ApproveMode, Query::VerificationMethod] {
        for output in [b"".as_slice(), b"click", b"use-both-passwords"] {
            assert_eq!(
                parse_observation(query, false, output, b""),
                Observation::CommandFailed
            );
            assert_eq!(
                parse_observation(query, true, output, b"IPC failed"),
                Observation::CommandFailed
            );
        }
        for output in [
            b"Settings are disabled!".as_slice(),
            b"click\npassword",
            &[0xff],
        ] {
            assert_eq!(
                parse_observation(query, true, output, b""),
                Observation::InvalidOutput
            );
        }
    }
    for query in [Query::Version, Query::Id] {
        assert_eq!(
            parse_observation(query, true, b"\r\n", b""),
            Observation::InvalidOutput
        );
    }
}
