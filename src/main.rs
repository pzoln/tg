mod recording;
mod terminal_input;
mod terminal_render;

use std::error::Error;
use std::ffi::OsString;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::path::{Path, PathBuf};
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
use tg::chrome;
use tg::cli;
use tg::editor_state::{EditorState, QuitRequest, SaveRequestOutcome};
use tg::file_mode::FileMode;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = cli::CliConfig::parse(std::env::args_os())?;
    if cli.show_help {
        println!("{}", cli::render_help());
        return Ok(());
    }
    if cli.show_version {
        println!("{}", cli::render_version());
        return Ok(());
    }
    cli::validate_startup_io(io::stdin().is_terminal(), io::stdout().is_terminal())?;

    let file_mode = if let Some(path) = cli.file_path {
        FileMode::load_from_path(path)?
    } else {
        FileMode::scratch()
    };
    let trace_path = init_tracing_to_file(default_trace_path().as_deref());
    let recording_path = default_recording_path();
    let mut session = TerminalSession::new()?;
    let mut editor_state = EditorState::new(file_mode);
    let _ = run_app(
        &mut session,
        trace_path.as_deref(),
        &recording_path,
        &mut editor_state,
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunOutcome {
    Exit,
    RecordAndExit,
    SaveAndExit,
}

fn run_app(
    session: &mut TerminalSession,
    trace_path: Option<&Path>,
    recording_path: &Path,
    editor_state: &mut EditorState,
) -> Result<RunOutcome, Box<dyn Error>> {
    let size = session.terminal_mut().size()?;
    let mut app = Session::new(size.width, size.height);
    if !matches!(editor_state.file_mode(), FileMode::Scratch(_)) {
        app.load(editor_state.file_mode().original_editable_text());
    }
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
            let current_text = active_editable_text(&app);
            let chrome = chrome::chrome_overrides_with_center(
                editor_state.file_mode(),
                &current_text,
                editor_state.center_text(),
            );
            let terminal = session.terminal_mut();
            terminal.draw(|f| draw_session_frame(&app, f, &chrome))?;
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
                    match handle_host_key(session, editor_state, &mut app, app_key_event)? {
                        HostKeyDecision::Consumed => continue,
                        HostKeyDecision::Exit(outcome) => return Ok(outcome),
                        HostKeyDecision::ForwardToCore => {}
                    }
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
                            recording_path,
                            &recorded_keys,
                            &initial_lines,
                            baseline_clipboard.as_deref(),
                            &final_lines,
                        )?;
                        eprintln!("Recorded session saved to {path}");
                        if let Some(trace_path) = trace_path {
                            eprintln!("Debug trace saved to {}", trace_path.display());
                        }
                        return Ok(RunOutcome::RecordAndExit);
                    }
                    if action.exit_requested() {
                        return Ok(RunOutcome::Exit);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostKeyDecision {
    Consumed,
    ForwardToCore,
    Exit(RunOutcome),
}

fn handle_host_key(
    session: &mut TerminalSession,
    editor_state: &mut EditorState,
    app: &mut Session,
    event: AppKeyEvent,
) -> Result<HostKeyDecision, Box<dyn Error>> {
    if editor_state.prompt_active() {
        if prompt_should_bypass_to_core(app.mode(), event) {
            return Ok(HostKeyDecision::ForwardToCore);
        }
        return match prompt_key_intent(event) {
            Some(PromptKeyIntent::SaveAndExit) => {
                let current_text = active_editable_text_for_save(app);
                match editor_state.save_from_quit_prompt(&current_text) {
                    Ok(()) => Ok(HostKeyDecision::Exit(RunOutcome::SaveAndExit)),
                    Err(error) => {
                        tracing::trace!(?error, "tg_save_from_quit_prompt_failed");
                        Ok(HostKeyDecision::Consumed)
                    }
                }
            }
            Some(PromptKeyIntent::DiscardAndExit) => {
                editor_state.discard_quit_prompt();
                Ok(HostKeyDecision::Exit(RunOutcome::Exit))
            }
            Some(PromptKeyIntent::Cancel) => {
                editor_state.cancel_quit_prompt();
                Ok(HostKeyDecision::Consumed)
            }
            None => {
                session.ring_bell()?;
                Ok(HostKeyDecision::Consumed)
            }
        };
    }

    editor_state.clear_transient_message();

    if is_save_shortcut(event) {
        let current_text = active_editable_text_for_save(app);
        match editor_state.save_from_shortcut(&current_text) {
            Ok(SaveRequestOutcome::Saved) => {}
            Ok(SaveRequestOutcome::BlockedNoFile) => {
                session.ring_bell()?;
            }
            Err(error) => {
                tracing::trace!(?error, "tg_save_shortcut_failed");
            }
        }
        return Ok(HostKeyDecision::Consumed);
    }

    if should_intercept_plain_file_quit(app.mode(), event, editor_state.file_mode()) {
        let current_text = active_editable_text(app);
        return Ok(match editor_state.begin_quit(&current_text) {
            QuitRequest::PromptOpened => HostKeyDecision::Consumed,
            QuitRequest::ExitImmediately => HostKeyDecision::Exit(RunOutcome::Exit),
        });
    }

    Ok(HostKeyDecision::ForwardToCore)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptKeyIntent {
    SaveAndExit,
    DiscardAndExit,
    Cancel,
}

fn prompt_key_intent(event: AppKeyEvent) -> Option<PromptKeyIntent> {
    if matches!(event.code, AppKeyCode::Esc) {
        return Some(PromptKeyIntent::Cancel);
    }
    if !is_plain_or_shift_only(event.modifiers) {
        return None;
    }
    match event.code {
        AppKeyCode::Char(ch) if ch.eq_ignore_ascii_case(&'y') => Some(PromptKeyIntent::SaveAndExit),
        AppKeyCode::Char(ch) if ch.eq_ignore_ascii_case(&'n') => {
            Some(PromptKeyIntent::DiscardAndExit)
        }
        _ => None,
    }
}

fn is_save_shortcut(event: AppKeyEvent) -> bool {
    event.modifiers.contains(AppKeyModifiers::CONTROL)
        && matches!(event.code, AppKeyCode::Char('s') | AppKeyCode::Char('S'))
}

fn should_intercept_plain_file_quit(mode: Mode, event: AppKeyEvent, file_mode: &FileMode) -> bool {
    file_mode.path().is_some()
        && mode != Mode::Text
        && event.modifiers == AppKeyModifiers::NONE
        && matches!(event.code, AppKeyCode::Char('q'))
}

fn prompt_should_bypass_to_core(mode: Mode, event: AppKeyEvent) -> bool {
    if event.modifiers.contains(AppKeyModifiers::CONTROL) {
        return matches!(
            event.code,
            AppKeyCode::Char('c')
                | AppKeyCode::Char('C')
                | AppKeyCode::Char('q')
                | AppKeyCode::Char('Q')
        );
    }

    mode != Mode::Text
        && is_plain_or_shift_only(event.modifiers)
        && matches!(event.code, AppKeyCode::Char('Q'))
}

fn is_plain_or_shift_only(modifiers: AppKeyModifiers) -> bool {
    modifiers == AppKeyModifiers::NONE || modifiers == AppKeyModifiers::SHIFT
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

fn active_editable_text(session: &Session) -> String {
    session.current_document_text()
}

fn active_editable_text_for_save(session: &mut Session) -> String {
    session.export_for_host_save()
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

    fn ring_bell(&mut self) -> io::Result<()> {
        let backend = self.terminal.backend_mut();
        backend.write_all(b"\x07")?;
        backend.flush()
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

fn textagram_dev_env_enabled() -> bool {
    textagram_dev_env_enabled_from(std::env::var_os("TEXTAGRAM_DEV_ENV"))
}

fn textagram_dev_env_enabled_from(value: Option<OsString>) -> bool {
    matches!(value, Some(value) if !value.is_empty())
}

fn default_trace_path() -> Option<PathBuf> {
    default_trace_path_for(
        textagram_dev_env_enabled(),
        std::env::current_dir().ok().as_deref(),
    )
}

fn default_trace_path_for(dev_env: bool, cwd: Option<&Path>) -> Option<PathBuf> {
    dev_env.then(|| base_output_dir(cwd).join("spec/e2e/recording.trace"))
}

fn default_recording_path() -> PathBuf {
    default_recording_path_for(
        textagram_dev_env_enabled(),
        std::env::current_dir().ok().as_deref(),
    )
}

fn default_recording_path_for(dev_env: bool, cwd: Option<&Path>) -> PathBuf {
    if dev_env {
        base_output_dir(cwd).join("spec/e2e/recording.md")
    } else {
        base_output_dir(cwd).join("tg-recording.md")
    }
}

fn base_output_dir(cwd: Option<&Path>) -> PathBuf {
    cwd.map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textagram_dev_env_is_disabled_when_unset_or_empty() {
        assert!(!textagram_dev_env_enabled_from(None));
        assert!(!textagram_dev_env_enabled_from(Some(OsString::new())));
    }

    #[test]
    fn textagram_dev_env_is_enabled_when_present_and_non_empty() {
        assert!(textagram_dev_env_enabled_from(Some(OsString::from("1"))));
    }

    #[test]
    fn default_trace_path_is_disabled_outside_dev_env() {
        assert_eq!(
            default_trace_path_for(false, Some(Path::new("/tmp/textagram-dev"))),
            None
        );
    }

    #[test]
    fn default_trace_path_uses_repo_relative_location_in_dev_env() {
        assert_eq!(
            default_trace_path_for(true, Some(Path::new("/tmp/textagram-dev"))),
            Some(PathBuf::from("/tmp/textagram-dev/spec/e2e/recording.trace"))
        );
    }

    #[test]
    fn default_recording_path_uses_cwd_by_default() {
        assert_eq!(
            default_recording_path_for(false, Some(Path::new("/tmp/textagram-dev"))),
            PathBuf::from("/tmp/textagram-dev/tg-recording.md")
        );
    }

    #[test]
    fn default_recording_path_uses_repo_relative_location_in_dev_env() {
        assert_eq!(
            default_recording_path_for(true, Some(Path::new("/tmp/textagram-dev"))),
            PathBuf::from("/tmp/textagram-dev/spec/e2e/recording.md")
        );
    }
}
