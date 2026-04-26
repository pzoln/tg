use std::process::Command;

#[test]
fn tg_help_smoke_prints_usage_and_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_tg"))
        .arg("--help")
        .output()
        .expect("run tg --help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 help stdout");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("tg FILE"));
    assert!(stdout.contains("Ctrl+S saves in file-backed mode"));
}

#[test]
fn tg_version_smoke_prints_version_and_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_tg"))
        .arg("--version")
        .output()
        .expect("run tg --version");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf8 version stdout");
    assert_eq!(stdout.trim(), format!("tg {}", env!("CARGO_PKG_VERSION")));
}
