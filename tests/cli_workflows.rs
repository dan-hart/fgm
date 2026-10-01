use std::process::Command;

#[test]
fn new_command_help_is_accessible_without_authentication() {
    for name in [
        "find",
        "capture",
        "review",
        "pack",
        "variables",
        "check-map",
        "compare-url",
        "export",
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_fgm"))
            .args([name, "--help"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn variables_import_generates_every_target_without_authentication() {
    let dir = tempfile::tempdir().unwrap();
    for target in ["json", "swift", "kotlin", "css"] {
        let path = dir.path().join(target);
        let out = Command::new(env!("CARGO_BIN_EXE_fgm"))
            .args([
                "variables",
                "--import",
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/variables.json"),
                "--target",
                target,
                "--output",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!std::fs::read_to_string(path).unwrap().is_empty());
    }
}

#[test]
fn invalid_account_and_conflicting_capture_inputs_fail_early() {
    for args in [
        vec!["--account", "../outside", "config", "show"],
        vec![
            "capture",
            "--simulator",
            "booted",
            "--android",
            "connected",
            "-o",
            "unused.png",
        ],
        vec![
            "review",
            "design.png",
            "--screenshot",
            "actual.png",
            "--android",
            "connected",
        ],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_fgm"))
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
    }
}

#[test]
fn named_accounts_have_distinct_config_and_cache_locations() {
    for command in [vec!["config", "path"], vec!["cache", "status"]] {
        let outputs: Vec<_> = ["example-one", "example-two"]
            .into_iter()
            .map(|name| {
                let out = Command::new(env!("CARGO_BIN_EXE_fgm"))
                    .args(["--account", name, "--no-keychain"])
                    .args(&command)
                    .output()
                    .unwrap();
                assert!(
                    out.status.success(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let text = String::from_utf8(out.stdout).unwrap();
                assert!(text.contains(name));
                text
            })
            .collect();
        assert_ne!(outputs[0], outputs[1]);
    }
}
