//! Terminal input, rendering, lifecycle management, and the interactive loop.
//!
//! [`run`] owns the event loop: it draws [`AppState`], translates terminal input
//! through [`keymap::KeyMapper`], and dispatches typed Git effects. Terminal raw
//! mode and alternate-screen restoration remain isolated in [`terminal`].
//!
//! [`AppState`]: crate::app::AppState

mod graph;
mod highlight;
pub mod keymap;
pub mod render;
pub mod terminal;

use std::io;

use crossterm::event::{Event as TerminalEvent, EventStream, KeyEventKind};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::app::{AppEffect, AppState, EffectExecutor};
use crate::error::AppError;
use crate::git::GitRunner;
use crate::tui::keymap::KeyMapper;
use crate::tui::terminal::TerminalSession;

/// Runs the interactive terminal loop until the state requests shutdown.
///
/// The loop owns terminal setup, drawing, crossterm input, periodic ticks,
/// Ctrl-C handling, effect dispatch, and completion events. Dropping its
/// terminal session restores the alternate screen on every return path.
///
/// # Errors
///
/// Returns [`AppError`] when terminal setup, drawing, input streaming, or
/// Ctrl-C registration fails. Repository errors produced after startup remain
/// recoverable application load states and are rendered in their owning pane.
pub async fn run<R: GitRunner>(
    mut state: AppState,
    executor: EffectExecutor<R>,
    mut keymap: KeyMapper,
) -> Result<(), AppError> {
    let mut terminal = TerminalSession::enter()?;
    if let Ok((width, height)) = crossterm::terminal::size() {
        state.set_terminal_size(width, height);
    }
    let mut events = EventStream::new();
    let (sender, mut receiver) = mpsc::channel(64);
    dispatch_all(&executor, &sender, state.start_effects());
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));

    while !state.should_quit() {
        terminal.draw(|frame| render::render(frame, &state))?;
        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(TerminalEvent::Key(key)))
                        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                    {
                        if let Some(action) = keymap.map(key, state.is_search_input_active()) {
                            let effects = state.handle_app_action(action);
                            dispatch_all(&executor, &sender, effects);
                        }
                    }
                    Some(Ok(TerminalEvent::Resize(width, height))) => {
                        state.set_terminal_size(width, height);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(AppError::Io(error)),
                    None => return Err(AppError::Io(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "terminal event stream ended",
                    ))),
                }
            }
            Some(event) = receiver.recv() => {
                let effects = state.handle_app_event(event);
                dispatch_all(&executor, &sender, effects);
            }
            _ = tick.tick() => {
                let effects = state.handle_app_action(crate::app::Action::Tick);
                dispatch_all(&executor, &sender, effects);
            }
            signal = tokio::signal::ctrl_c() => {
                signal.map_err(AppError::Io)?;
                state.handle_app_action(crate::app::Action::Quit);
            }
        }
    }
    drop(terminal);
    executor.shutdown().await;
    Ok(())
}

fn dispatch_all<R: GitRunner>(
    executor: &EffectExecutor<R>,
    sender: &mpsc::Sender<crate::app::Event>,
    effects: Vec<AppEffect>,
) {
    for effect in effects {
        executor.dispatch_app(effect, sender.clone());
    }
}
