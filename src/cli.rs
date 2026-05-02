use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// Parsed process-level command-line options for the `tg` terminal host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliConfig {
    /// Print CLI help and exit before initializing the terminal.
    pub show_help: bool,
    /// Print the package version and exit before initializing the terminal.
    pub show_version: bool,
    /// Optional file path to edit in place; `None` starts scratch mode.
    pub file_path: Option<PathBuf>,
}

impl CliConfig {
    /// Parses `tg` arguments: zero or one file path plus `--help` / `--version`.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, CliError> {
        let mut show_help = false;
        let mut show_version = false;
        let mut file_path = None;

        for arg in args.into_iter().skip(1) {
            if arg == "-h" || arg == "--help" {
                show_help = true;
                continue;
            }

            if arg == "-V" || arg == "--version" {
                show_version = true;
                continue;
            }

            if arg.to_string_lossy().starts_with('-') {
                return Err(CliError::unknown_flag(arg));
            }

            let path = PathBuf::from(&arg);
            if file_path.replace(path).is_some() {
                return Err(CliError::too_many_paths());
            }
        }

        Ok(Self {
            show_help,
            show_version,
            file_path,
        })
    }
}

/// User-facing startup error for invalid CLI arguments or terminal I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    message: String,
}

impl CliError {
    fn unknown_flag(flag: OsString) -> Self {
        Self {
            message: format!(
                "unknown flag: {}\n\n{}",
                PathBuf::from(flag).display(),
                usage_block()
            ),
        }
    }

    fn too_many_paths() -> Self {
        Self {
            message: format!("expected at most one file path\n\n{}", usage_block()),
        }
    }

    /// Builds the error returned when stdin is redirected.
    pub fn stdin_not_tty() -> Self {
        Self {
            message: "stdin redirection is not supported by `tg`; run it on a terminal".to_string(),
        }
    }

    /// Builds the error returned when stdout is redirected.
    pub fn stdout_not_tty() -> Self {
        Self {
            message: "stdout redirection is not supported by `tg`; run it on a terminal"
                .to_string(),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for CliError {}

/// Rejects redirected stdin/stdout so `tg` always owns an interactive terminal.
pub fn validate_startup_io(stdin_is_tty: bool, stdout_is_tty: bool) -> Result<(), CliError> {
    if !stdin_is_tty {
        return Err(CliError::stdin_not_tty());
    }

    if !stdout_is_tty {
        return Err(CliError::stdout_not_tty());
    }

    Ok(())
}

/// Renders process-level help for launching `tg`, distinct from in-app help.
pub fn render_help() -> String {
    format!(
        "\
{description}

Usage:
  tg
  tg FILE
  tg -h | --help
  tg -V | --version

Notes:
  - one file path maximum
  - stdin/stdout redirection is not supported in this CLI
  - FILE behavior follows file content: first recognized `textagram` fence body when present, whole file otherwise
  - Ctrl+S saves in file-backed mode and is blocked in scratch mode
  - q prompts to save in file-backed mode
  - Q writes a repro recording and may discard unsaved file edits
  - press ? in the editor for the full controls reference",
        description = env!("CARGO_PKG_DESCRIPTION")
    )
}

/// Renders the package version shown by `tg --version`.
pub fn render_version() -> String {
    format!("tg {}", env!("CARGO_PKG_VERSION"))
}

fn usage_block() -> &'static str {
    "\
Usage:
  tg
  tg FILE
  tg -h | --help
  tg -V | --version"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_help_version_and_one_file_path() {
        let config = CliConfig::parse([
            OsString::from("tg"),
            OsString::from("-h"),
            OsString::from("-V"),
            OsString::from("diagram.md"),
        ])
        .unwrap();

        assert_eq!(
            config,
            CliConfig {
                show_help: true,
                show_version: true,
                file_path: Some(PathBuf::from("diagram.md")),
            }
        );
    }

    #[test]
    fn parse_rejects_unknown_flags() {
        let error = CliConfig::parse([OsString::from("tg"), OsString::from("--wat")]).unwrap_err();

        assert!(error.to_string().contains("unknown flag: --wat"));
        assert!(error.to_string().contains("Usage:"));
    }

    #[test]
    fn parse_rejects_more_than_one_file_path() {
        let error = CliConfig::parse([
            OsString::from("tg"),
            OsString::from("one.md"),
            OsString::from("two.md"),
        ])
        .unwrap_err();

        assert!(error.to_string().contains("expected at most one file path"));
    }

    #[test]
    fn startup_validation_rejects_redirected_stdin_or_stdout() {
        assert_eq!(
            validate_startup_io(false, true).unwrap_err().to_string(),
            "stdin redirection is not supported by `tg`; run it on a terminal"
        );
        assert_eq!(
            validate_startup_io(true, false).unwrap_err().to_string(),
            "stdout redirection is not supported by `tg`; run it on a terminal"
        );
    }

    #[test]
    fn help_text_mentions_core_cli_behavior() {
        let help = render_help();

        assert!(help.contains("tg FILE"));
        assert!(help.contains("stdin/stdout redirection is not supported"));
        assert!(help.contains("Ctrl+S saves in file-backed mode"));
        assert!(help.contains("press ? in the editor"));
    }

    #[test]
    fn version_text_uses_package_version() {
        assert_eq!(
            render_version(),
            format!("tg {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
