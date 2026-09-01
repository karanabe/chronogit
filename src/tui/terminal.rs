//! RAII ownership of raw mode, the alternate screen, and panic restoration.

use std::io::{self, Stdout, Write, stdout};
use std::panic;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// An active ratatui terminal session that restores process terminal state on drop.
///
/// Construction enables raw mode, enters the alternate screen, enables mouse
/// capture, and hides the cursor. Every construction failure and [`Drop`] runs
/// the same best-effort restoration sequence.
pub struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalSession {
    /// Enters raw alternate-screen mode and creates the terminal backend.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when raw mode, terminal control sequences, or the
    /// ratatui backend cannot be initialized. Partial setup is restored before
    /// returning the error.
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut output = stdout();
        if let Err(error) = execute!(output, EnterAlternateScreen, EnableMouseCapture, Hide) {
            restore_terminal();
            return Err(error);
        }
        let backend = CrosstermBackend::new(output);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                restore_terminal();
                return Err(error);
            }
        };
        Ok(Self { terminal })
    }

    /// Draws one frame using ratatui's buffered terminal backend.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the frame cannot be flushed to standard output.
    pub fn draw<F>(&mut self, render: F) -> io::Result<()>
    where
        F: FnOnce(&mut ratatui::Frame<'_>),
    {
        self.terminal.draw(render).map(|_| ())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Installs a panic hook that restores terminal state before reporting a panic.
///
/// The previously installed hook is retained and invoked after restoration.
/// Install this immediately before [`TerminalSession::enter`] so startup errors
/// printed earlier do not need terminal cleanup.
pub fn install_panic_hook() {
    install_panic_hook_with(restore_terminal);
}

fn install_panic_hook_with<F>(restore: F)
where
    F: Fn() + Send + Sync + 'static,
{
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |information| {
        restore();
        previous(information);
    }));
}

fn restore_terminal() {
    let _ignored = disable_raw_mode();
    let _ignored = emit_restore(stdout());
}

fn emit_restore<W: Write>(mut output: W) -> io::Result<()> {
    execute!(output, Show, DisableMouseCapture, LeaveAlternateScreen)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::{emit_restore, install_panic_hook_with};

    #[test]
    fn restore_sequence_shows_the_cursor_and_leaves_the_alternate_screen() {
        let mut output = Vec::new();
        emit_restore(&mut output)
            .unwrap_or_else(|error| panic!("could not emit restore sequence: {error}"));
        let sequence = String::from_utf8_lossy(&output);
        assert!(sequence.contains("?25h"));
        assert!(sequence.contains("?1049l"));
    }

    #[test]
    fn panic_hook_probe() {
        if std::env::var_os("CHRONOGIT_PANIC_HOOK_PROBE").is_none() {
            return;
        }
        install_panic_hook_with(|| eprintln!("CHRONOGIT_TERMINAL_RESTORED"));
        panic!("intentional terminal restoration probe");
    }

    #[test]
    fn panic_hook_restores_the_terminal_before_process_exit() {
        let output = Command::new(
            std::env::current_exe()
                .unwrap_or_else(|error| panic!("could not locate test executable: {error}")),
        )
        .args([
            "--exact",
            "tui::terminal::tests::panic_hook_probe",
            "--nocapture",
        ])
        .env("CHRONOGIT_PANIC_HOOK_PROBE", "1")
        .output()
        .unwrap_or_else(|error| panic!("could not run panic-hook probe: {error}"));

        assert!(
            !output.status.success(),
            "the probe must terminate by panic"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let restored = stderr
            .find("CHRONOGIT_TERMINAL_RESTORED")
            .unwrap_or_else(|| panic!("restore marker missing from probe stderr: {stderr}"));
        let panicked = stderr
            .find("intentional terminal restoration probe")
            .unwrap_or_else(|| panic!("panic marker missing from probe stderr: {stderr}"));
        assert!(
            restored < panicked,
            "restoration must precede panic reporting"
        );
    }
}
