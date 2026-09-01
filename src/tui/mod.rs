mod graph;
pub mod keymap;
pub mod render;
pub mod terminal;

use std::io;

use crossterm::event::{Event as TerminalEvent, EventStream, KeyEventKind};
use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::app::{AppState, EffectExecutor, GitEffect};
use crate::error::AppError;
use crate::git::GitRunner;
use crate::tui::keymap::KeyMapper;
use crate::tui::terminal::TerminalSession;

pub async fn run<R: GitRunner>(
    mut state: AppState,
    executor: EffectExecutor<R>,
    mut keymap: KeyMapper,
) -> Result<(), AppError> {
    let mut terminal = TerminalSession::enter()?;
    let mut events = EventStream::new();
    let (sender, mut receiver) = mpsc::channel(64);
    dispatch_all(&executor, &sender, state.start());
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
                            let effects = state.handle_action(action);
                            dispatch_all(&executor, &sender, effects);
                        }
                    }
                    Some(Ok(TerminalEvent::Resize(_, _))) => {}
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(AppError::Io(error)),
                    None => return Err(AppError::Io(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "terminal event stream ended",
                    ))),
                }
            }
            Some(event) = receiver.recv() => {
                let effects = state.handle_event(event);
                dispatch_all(&executor, &sender, effects);
            }
            _ = tick.tick() => {
                let effects = state.handle_action(crate::app::Action::Tick);
                dispatch_all(&executor, &sender, effects);
            }
            signal = tokio::signal::ctrl_c() => {
                signal.map_err(AppError::Io)?;
                state.handle_action(crate::app::Action::Quit);
            }
        }
    }
    Ok(())
}

fn dispatch_all<R: GitRunner>(
    executor: &EffectExecutor<R>,
    sender: &mpsc::Sender<crate::app::Event>,
    effects: Vec<GitEffect>,
) {
    for effect in effects {
        executor.dispatch(effect, sender.clone());
    }
}
