//! Binary composition root and top-level error reporting for ChronoGit.

use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;

use chronogit::app::{AppState, EffectExecutor};
use chronogit::cli::Cli;
use chronogit::error::AppError;
use chronogit::git::{GitService, SystemGitRunner};
use chronogit::tui;
use chronogit::tui::keymap::KeyMapper;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("chronogit: {}", safe_diagnostic(&error.to_string()));
            let mut source = std::error::Error::source(&error);
            while let Some(cause) = source {
                eprintln!("  caused by: {}", safe_diagnostic(&cause.to_string()));
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn safe_diagnostic(value: &str) -> String {
    let mut safe = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\n' => safe.push('\n'),
            '\t' => safe.push_str("    "),
            character if character.is_control() => {
                safe.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
            }
            character => safe.push(character),
        }
    }
    safe
}

async fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    let runner = SystemGitRunner;
    let service = Arc::new(GitService::discover(runner, cli.path())?);
    let keymap = KeyMapper::load(cli.keymap())?;

    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(AppError::NonInteractiveTerminal);
    }

    tui::terminal::install_panic_hook();
    let state = AppState::new(service.root().clone(), cli.initial_view());
    let executor = EffectExecutor::new(service);
    tui::run(state, executor, keymap).await
}

#[cfg(test)]
mod tests {
    use super::safe_diagnostic;

    #[test]
    fn diagnostic_escapes_terminal_controls() {
        assert_eq!(safe_diagnostic("bad\u{1b}[2J"), "bad\\u{1b}[2J");
    }
}
