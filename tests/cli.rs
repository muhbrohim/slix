use std::process::{Command, Stdio};

fn slix() -> Command {
    Command::new(env!("CARGO_BIN_EXE_slix"))
}

#[test]
fn help_describes_primary_commands() {
    let output = slix().arg("--help").output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Parse messages from a file"));
    assert!(stdout.contains("Inspect and manage specification projects"));
    assert!(stdout.contains("--project <PROJECT>"));
}

#[test]
fn version_matches_package_version() {
    let output = slix().arg("--version").output().unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("slix {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn missing_active_project_is_an_operational_error() {
    let config = tempfile::tempdir().unwrap();
    let mut child = slix()
        .arg("parse")
        .env("SLIX_CONFIG_HOME", config.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("no active project is configured")
    );
}

#[test]
fn invalid_arguments_use_clap_error_code() {
    let output = slix()
        .args(["parse", "--not-a-real-option"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unexpected argument")
    );
}
