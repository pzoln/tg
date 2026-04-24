mod recording;
mod terminal_input;
mod terminal_render;

use std::error::Error;
use std::io;
use std::time::Duration;

use arboard::Clipboard;
use crossterm::{
    event::Event,
    event::{self, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::CrosstermBackend, Terminal};
use recording::{
    grow_recording_size_including_cursor, keyspec_for_processed_key,
    occupied_recording_size_including_cursor, recording_lines_for_size, write_e2e_recording,
};
use terminal_input::{
    app_key_event_from_crossterm, terminal_document_clipboard_intent_from_crossterm,
};
use terminal_render::draw_session_frame;
use textagram::app::debug::init_tracing_to_file;
use textagram::session::{ClipboardIntent, Session};
use textagram::{
    Action, AppKeyCode, AppKeyEvent, AppKeyModifiers, Mode, SnapshotCropMode, SnapshotCropOptions,
    TimerDirective,
};

fn main() -> Result<(), Box<dyn Error>> {
    let trace_path = init_tracing_to_file(None);
    let mut session = TerminalSession::new()?;
    run_app(&mut session, &trace_path)
}

fn run_app(
    session: &mut TerminalSession,
    trace_path: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    let size = session.terminal_mut().size()?;
    let mut app = Session::new(size.width, size.height);
    let record_crop = SnapshotCropOptions {
        mode: SnapshotCropMode::OriginPreserving,
        padding: 0,
    };
    let mut baseline_snapshot = app.canvas_snapshot_data_full();
    let mut baseline_clipboard = app.clipboard_text();
    let mut recorded_size = occupied_recording_size_including_cursor(
        &baseline_snapshot,
        recording_cursor(&app),
        record_crop,
    );
    let mut recorded_keys = String::new();
    let mut next_tick_delay_ms = app.next_ui_tick_delay_ms();

    loop {
        {
            let terminal = session.terminal_mut();
            terminal.draw(|f| draw_session_frame(&app, f))?;
        }

        let mut events = Vec::new();
        if let Some(delay_ms) = next_tick_delay_ms {
            if event::poll(Duration::from_millis(delay_ms))? {
                events.push(event::read()?);
                while event::poll(Duration::from_millis(0))? {
                    events.push(event::read()?);
                }
            } else {
                tracing::trace!(
                    delay_ms,
                    mode = ?app.mode(),
                    revision = app.revision(),
                    "terminal_tick"
                );
                let action = app.tick_ui(delay_ms);
                let updated_tick_delay_ms = apply_timer_directive(next_tick_delay_ms, action.timer);
                tracing::trace!(
                    delay_ms,
                    ?action,
                    mode = ?app.mode(),
                    revision = app.revision(),
                    next_tick_delay_ms = updated_tick_delay_ms,
                    "terminal_tick_action"
                );
                next_tick_delay_ms = updated_tick_delay_ms;
                continue;
            }
        } else {
            events.push(event::read()?);
            while event::poll(Duration::from_millis(0))? {
                events.push(event::read()?);
            }
        }

        let mut i = 0usize;
        while i < events.len() {
            match events[i] {
                Event::Resize(width, height) => {
                    app.set_frame_size(width, height);
                    let snapshot = app.canvas_snapshot_data_full();
                    grow_recording_size_including_cursor(
                        &mut recorded_size,
                        &snapshot,
                        recording_cursor(&app),
                        record_crop,
                    );
                    i += 1;
                }
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    let mut to_process = key_event;
                    if movement_coalescing_mode(app.mode()) && is_movement_key(key_event.code) {
                        let mut j = i + 1;
                        while j < events.len() {
                            match events[j] {
                                Event::Key(next)
                                    if next.kind == KeyEventKind::Press
                                        && is_movement_key(next.code) =>
                                {
                                    to_process = next;
                                    j += 1;
                                }
                                _ => break,
                            }
                        }
                        i = j;
                    } else {
                        i += 1;
                    }

                    let app_key_event = app_key_event_from_crossterm(to_process);
                    tracing::trace!(?to_process, ?app_key_event, mode = ?app.mode(), "terminal_key");
                    let clipboard_before = app.clipboard_text();
                    refresh_terminal_selection_clipboard_from_os_if_needed(&mut app, app_key_event);
                    let action = if app.mode() == Mode::Nav {
                        if let Some(intent) =
                            terminal_document_clipboard_intent_from_crossterm(to_process)
                        {
                            handle_terminal_document_clipboard(&mut app, intent)
                        } else {
                            app.handle_key(app_key_event)
                        }
                    } else {
                        app.handle_key(app_key_event)
                    };
                    tracing::trace!(
                        ?app_key_event,
                        ?action,
                        mode = ?app.mode(),
                        revision = app.revision(),
                        "terminal_action"
                    );
                    sync_terminal_selection_clipboard_to_os_if_needed(
                        app_key_event,
                        clipboard_before.as_deref(),
                        app.clipboard_text().as_deref(),
                    );
                    next_tick_delay_ms = apply_timer_directive(next_tick_delay_ms, action.timer);
                    if let Some(token) = keyspec_for_processed_key(app_key_event, action) {
                        recorded_keys.push_str(&token);
                    }
                    if action.reset_recording_baseline() {
                        baseline_snapshot = app.canvas_snapshot_data_full();
                        baseline_clipboard = app.clipboard_text();
                        recorded_size = occupied_recording_size_including_cursor(
                            &baseline_snapshot,
                            recording_cursor(&app),
                            record_crop,
                        );
                        recorded_keys.clear();
                        continue;
                    }
                    if action.record_and_exit() {
                        let final_snapshot = app.canvas_snapshot_data_full();
                        grow_recording_size_including_cursor(
                            &mut recorded_size,
                            &final_snapshot,
                            recording_cursor(&app),
                            record_crop,
                        );
                        let initial_lines =
                            recording_lines_for_size(&baseline_snapshot, recorded_size);
                        let final_lines = recording_lines_for_size(&final_snapshot, recorded_size);
                        let path = write_e2e_recording(
                            &recorded_keys,
                            &initial_lines,
                            baseline_clipboard.as_deref(),
                            &final_lines,
                        )?;
                        eprintln!("Recorded session saved to {path}");
                        eprintln!("Debug trace saved to {}", trace_path.display());
                        return Ok(());
                    }
                    if action.exit_requested() {
                        return Ok(());
                    }
                    let snapshot = app.canvas_snapshot_data_full();
                    grow_recording_size_including_cursor(
                        &mut recorded_size,
                        &snapshot,
                        recording_cursor(&app),
                        record_crop,
                    );
                }
                _ => {
                    i += 1;
                }
            }
        }
    }
}

fn movement_coalescing_mode(mode: Mode) -> bool {
    matches!(mode, Mode::Nav | Mode::Shape | Mode::Connector)
}

fn is_movement_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Left
            | KeyCode::Right
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Char('h')
            | KeyCode::Char('j')
            | KeyCode::Char('k')
            | KeyCode::Char('l')
            | KeyCode::Char('a')
            | KeyCode::Char('s')
            | KeyCode::Char('d')
            | KeyCode::Char('w')
    )
}

fn apply_timer_directive(current: Option<u64>, directive: TimerDirective) -> Option<u64> {
    match directive {
        TimerDirective::Unchanged => current,
        TimerDirective::Schedule(delay_ms) => Some(delay_ms),
        TimerDirective::Clear => None,
    }
}

fn recording_cursor(session: &Session) -> (u16, u16) {
    let viewport = session.viewport();
    let (col, row) = session.cursor_position_in_viewport();
    (
        viewport.origin.0.saturating_add(col),
        viewport.origin.1.saturating_add(row),
    )
}

fn refresh_terminal_selection_clipboard_from_os_if_needed(
    session: &mut Session,
    event: AppKeyEvent,
) {
    if should_refresh_terminal_selection_clipboard_from_os(session.mode(), event) {
        if let Some(text) = maybe_read_terminal_os_clipboard() {
            session.set_clipboard_text(text);
        }
    }
}

fn should_refresh_terminal_selection_clipboard_from_os(mode: Mode, event: AppKeyEvent) -> bool {
    mode == Mode::Nav
        && event.modifiers == AppKeyModifiers::NONE
        && matches!(event.code, AppKeyCode::Char('p'))
}

fn sync_terminal_selection_clipboard_to_os_if_needed(
    event: AppKeyEvent,
    before: Option<&str>,
    after: Option<&str>,
) {
    if should_sync_terminal_selection_clipboard_to_os(event) && before != after {
        if let Some(text) = after {
            maybe_write_terminal_os_clipboard(text);
        }
    }
}

fn should_sync_terminal_selection_clipboard_to_os(event: AppKeyEvent) -> bool {
    event.modifiers == AppKeyModifiers::NONE
        && matches!(
            event.code,
            AppKeyCode::Char('x') | AppKeyCode::Char('y') | AppKeyCode::Char('c')
        )
}

fn handle_terminal_document_clipboard(session: &mut Session, intent: ClipboardIntent) -> Action {
    if session.clipboard_shortcut_suppressed(intent) {
        tracing::trace!(?intent, mode = ?session.mode(), "terminal_document_clipboard_suppressed");
        return Action::Continue;
    }
    let before = session.revision();
    match intent {
        ClipboardIntent::CopyDocument => {
            if let Some(text) = session.clipboard_copy(intent) {
                tracing::trace!(
                    ?intent,
                    text_len = text.len(),
                    "terminal_document_clipboard_copy"
                );
                maybe_write_terminal_os_clipboard(&text);
            } else {
                tracing::trace!(?intent, "terminal_document_clipboard_copy_empty");
            }
        }
        ClipboardIntent::PasteDocument => {
            let text = maybe_read_terminal_os_clipboard().or_else(|| {
                let fallback = session.clipboard_text();
                if fallback.is_some() {
                    tracing::trace!(
                        ?intent,
                        "terminal_document_clipboard_using_internal_fallback"
                    );
                }
                fallback
            });
            let text = text.unwrap_or_default();
            tracing::trace!(
                ?intent,
                text_len = text.len(),
                "terminal_document_clipboard_paste"
            );
            session.clipboard_paste(intent, text);
        }
        ClipboardIntent::CutDocument => {
            if let Some(text) = session.clipboard_cut(intent) {
                tracing::trace!(
                    ?intent,
                    text_len = text.len(),
                    "terminal_document_clipboard_cut"
                );
                maybe_write_terminal_os_clipboard(&text);
            } else {
                tracing::trace!(?intent, "terminal_document_clipboard_cut_empty");
            }
        }
        ClipboardIntent::CopySelectionOrDocument
        | ClipboardIntent::CutSelection
        | ClipboardIntent::PasteSelection => {}
    }
    Action::Continue.with_document_changed(session.revision() != before)
}

fn terminal_os_clipboard_enabled() -> bool {
    std::env::var("TEXTAGRAM_DISABLE_OS_CLIPBOARD").is_err()
}

fn maybe_write_terminal_os_clipboard(text: &str) {
    if terminal_os_clipboard_enabled() {
        match Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(error) = clipboard.set_text(text.to_string()) {
                    tracing::trace!(?error, "terminal_os_clipboard_write_failed");
                }
            }
            Err(error) => {
                tracing::trace!(?error, "terminal_os_clipboard_open_for_write_failed");
            }
        }
    }
}

fn maybe_read_terminal_os_clipboard() -> Option<String> {
    if terminal_os_clipboard_enabled() {
        match Clipboard::new() {
            Ok(mut clipboard) => match clipboard.get_text() {
                Ok(text) => return Some(text),
                Err(error) => {
                    tracing::trace!(?error, "terminal_os_clipboard_read_failed");
                }
            },
            Err(error) => {
                tracing::trace!(?error, "terminal_os_clipboard_open_for_read_failed");
            }
        }
    }
    None
}

struct TerminalSession {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalSession {
    fn new() -> Result<Self, Box<dyn Error>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let backend = self.terminal.backend_mut();
        let _ = execute!(backend, LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
