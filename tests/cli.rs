use std::process::Command;

#[test]
fn help_and_version_do_not_require_a_tty() {
    for argument in ["--help", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_chronogit"))
            .arg(argument)
            .output()
            .unwrap_or_else(|error| panic!("could not run chronogit {argument}: {error}"));
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("chronogit"));
    }
}

#[test]
fn non_repository_is_a_contextual_error() {
    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("could not create temporary directory: {error}"));
    let output = Command::new(env!("CARGO_BIN_EXE_chronogit"))
        .arg(directory.path())
        .output()
        .unwrap_or_else(|error| panic!("could not run chronogit: {error}"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Git operation failed"));
}

#[test]
fn repository_without_tty_is_rejected_before_terminal_setup() {
    let output = Command::new(env!("CARGO_BIN_EXE_chronogit"))
        .arg(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|error| panic!("could not run chronogit: {error}"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("interactive TTY"));
}

#[test]
fn missing_git_executable_has_a_contextual_error_chain() {
    let output = Command::new(env!("CARGO_BIN_EXE_chronogit"))
        .arg(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", "")
        .output()
        .unwrap_or_else(|error| panic!("could not run chronogit: {error}"));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Git operation failed"));
    assert!(stderr.contains("could not discover repository"));
}

#[cfg(unix)]
#[test]
fn permission_denied_git_executable_has_a_contextual_error_chain() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir()
        .unwrap_or_else(|error| panic!("could not create temporary directory: {error}"));
    let git = directory.path().join("git");
    fs::write(&git, b"not executable")
        .unwrap_or_else(|error| panic!("could not create fake Git: {error}"));
    fs::set_permissions(&git, fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|error| panic!("could not set fake Git permissions: {error}"));

    let output = Command::new(env!("CARGO_BIN_EXE_chronogit"))
        .arg(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", directory.path())
        .output()
        .unwrap_or_else(|error| panic!("could not run chronogit: {error}"));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Git operation failed"));
    assert!(stderr.contains("could not discover repository"));
}
