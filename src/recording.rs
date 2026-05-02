//! Terminal-specific recording helpers for the `tg` session replay workflow.

use std::error::Error;
use std::fs;
use std::path::Path;

use textagram::app::{
    Action, AppKeyCode, AppKeyEvent, AppKeyModifiers, CanvasSnapshot, SnapshotCropOptions,
};

fn key_event_to_keyspec(key_event: AppKeyEvent) -> Option<String> {
    match key_event.code {
        AppKeyCode::Char(c) => Some(c.to_string()),
        AppKeyCode::Enter => Some("<enter>".to_string()),
        AppKeyCode::Esc => Some("<esc>".to_string()),
        AppKeyCode::Left => {
            if key_event.modifiers.contains(AppKeyModifiers::SHIFT) {
                Some("<shift-left>".to_string())
            } else {
                Some("<left>".to_string())
            }
        }
        AppKeyCode::Right => {
            if key_event.modifiers.contains(AppKeyModifiers::SHIFT) {
                Some("<shift-right>".to_string())
            } else {
                Some("<right>".to_string())
            }
        }
        AppKeyCode::Up => {
            if key_event.modifiers.contains(AppKeyModifiers::SHIFT) {
                Some("<shift-up>".to_string())
            } else {
                Some("<up>".to_string())
            }
        }
        AppKeyCode::Down => {
            if key_event.modifiers.contains(AppKeyModifiers::SHIFT) {
                Some("<shift-down>".to_string())
            } else {
                Some("<down>".to_string())
            }
        }
        AppKeyCode::Backspace => Some("<backspace>".to_string()),
        _ => None,
    }
}

/// Converts a processed key into a replay token, omitting recording-control keys.
pub fn keyspec_for_processed_key(key_event: AppKeyEvent, action: Action) -> Option<String> {
    if action.record_and_exit() || action.reset_recording_baseline() {
        None
    } else {
        key_event_to_keyspec(key_event)
    }
}

#[cfg_attr(test, allow(dead_code))]
/// Writes a markdown e2e scenario that can replay the recorded terminal session.
pub fn write_e2e_recording(
    path: &Path,
    keys: &str,
    given: &[String],
    cursor: (u16, u16),
    clipboard: Option<&str>,
    expect: &[String],
) -> Result<String, Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        format_e2e_recording(keys, given, Some(cursor), clipboard, expect),
    )?;
    Ok(path.display().to_string())
}

/// Formats a recorded session as the repository's markdown e2e scenario format.
pub fn format_e2e_recording(
    keys: &str,
    given: &[String],
    cursor: Option<(u16, u16)>,
    clipboard: Option<&str>,
    expect: &[String],
) -> String {
    let mut content = String::new();
    content.push_str("## Scenario: Recorded session\n\n**Given**\n");
    append_fenced_lines(&mut content, given);
    if let Some((col, row)) = cursor {
        content.push_str(&format!("\n**Cursor** {col} {row}\n"));
    }
    if let Some(clipboard) = clipboard.filter(|clipboard| !clipboard.is_empty()) {
        content.push_str("\n**Clipboard**\n");
        append_fenced_text(&mut content, clipboard);
    }
    content.push_str("\n**Keys**\n");
    append_fenced_text(&mut content, keys);
    content.push_str("\n**Expect**\n");
    append_fenced_lines(&mut content, expect);
    content
}

fn append_fenced_lines(content: &mut String, lines: &[String]) {
    let text = lines.join("\n");
    let fence = markdown_fence_for(&text);
    content.push_str(&fence);
    content.push('\n');
    for line in lines {
        content.push_str(line);
        content.push('\n');
    }
    content.push_str(&fence);
    content.push('\n');
}

fn append_fenced_text(content: &mut String, text: &str) {
    let fence = markdown_fence_for(text);
    content.push_str(&fence);
    content.push('\n');
    content.push_str(text);
    if !text.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&fence);
    content.push('\n');
}

fn markdown_fence_for(text: &str) -> String {
    "`".repeat(max_backtick_run(text).saturating_add(1).max(3))
}

fn max_backtick_run(text: &str) -> usize {
    let mut max_run = 0usize;
    let mut current_run = 0usize;
    for ch in text.chars() {
        if ch == '`' {
            current_run = current_run.saturating_add(1);
            max_run = max_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    max_run
}

#[cfg(test)]
/// Returns the cropped occupied size for a canvas snapshot.
pub fn occupied_recording_size(snapshot: &CanvasSnapshot, crop: SnapshotCropOptions) -> (u16, u16) {
    snapshot.crop(crop).size
}

/// Returns the occupied snapshot size, expanded to include the recorded cursor cell.
pub fn occupied_recording_size_including_cursor(
    snapshot: &CanvasSnapshot,
    cursor: (u16, u16),
    crop: SnapshotCropOptions,
) -> (u16, u16) {
    let cropped = snapshot.crop(crop);
    let mut width = cropped.size.0;
    let mut height = cropped.size.1;

    if cursor.0 >= cropped.origin.0 {
        width = width.max(cursor.0.saturating_sub(cropped.origin.0).saturating_add(1));
    }
    if cursor.1 >= cropped.origin.1 {
        height = height.max(cursor.1.saturating_sub(cropped.origin.1).saturating_add(1));
    }

    (width, height)
}

#[cfg(test)]
/// Grows a recorded size to include the current occupied snapshot size.
pub fn grow_recording_size(
    recorded_size: &mut (u16, u16),
    snapshot: &CanvasSnapshot,
    crop: SnapshotCropOptions,
) {
    let size = occupied_recording_size(snapshot, crop);
    recorded_size.0 = recorded_size.0.max(size.0);
    recorded_size.1 = recorded_size.1.max(size.1);
}

/// Grows a recorded size to include both the current snapshot and cursor cell.
pub fn grow_recording_size_including_cursor(
    recorded_size: &mut (u16, u16),
    snapshot: &CanvasSnapshot,
    cursor: (u16, u16),
    crop: SnapshotCropOptions,
) {
    let size = occupied_recording_size_including_cursor(snapshot, cursor, crop);
    recorded_size.0 = recorded_size.0.max(size.0);
    recorded_size.1 = recorded_size.1.max(size.1);
}

/// Renders a snapshot into fixed-size e2e rows, using dots for blank cells.
pub fn recording_lines_for_size(snapshot: &CanvasSnapshot, size: (u16, u16)) -> Vec<String> {
    let mut lines = Vec::with_capacity(size.1 as usize);
    for row in 0..size.1 as usize {
        let chars = snapshot
            .glyphs
            .get(row)
            .map(|line| line.chars().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut line = String::with_capacity(size.0 as usize);
        for col in 0..size.0 as usize {
            let ch = chars.get(col).copied().unwrap_or(' ');
            line.push(if ch == ' ' { '.' } else { ch });
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use textagram::app::{
        CanvasSnapshot, RecordingDirective, SnapshotCropMode, SnapshotCropOptions,
    };

    #[test]
    fn recording_lines_pad_to_requested_size() {
        let snapshot = CanvasSnapshot {
            glyphs: vec!["ab".to_string(), "c ".to_string()],
            highlights: None,
            origin: (0, 0),
            size: (2, 2),
        };

        let lines = recording_lines_for_size(&snapshot, (4, 3));

        assert_eq!(lines, vec!["ab..", "c...", "...."]);
    }

    #[test]
    fn grow_recording_size_tracks_largest_origin_preserving_crop() {
        let crop = SnapshotCropOptions {
            mode: SnapshotCropMode::OriginPreserving,
            padding: 0,
        };
        let mut recorded_size = (1, 1);
        let snapshot = CanvasSnapshot {
            glyphs: vec!["ab ".to_string(), " c ".to_string(), "   ".to_string()],
            highlights: None,
            origin: (0, 0),
            size: (3, 3),
        };

        grow_recording_size(&mut recorded_size, &snapshot, crop);

        assert_eq!(recorded_size, (2, 2));
    }

    #[test]
    fn occupied_recording_size_including_cursor_preserves_blank_cell_to_the_right() {
        let crop = SnapshotCropOptions {
            mode: SnapshotCropMode::OriginPreserving,
            padding: 0,
        };
        let snapshot = CanvasSnapshot {
            glyphs: vec!["abcd    ".to_string()],
            highlights: None,
            origin: (0, 0),
            size: (8, 1),
        };

        let size = occupied_recording_size_including_cursor(&snapshot, (4, 0), crop);

        assert_eq!(size, (5, 1));
    }

    #[test]
    fn grow_recording_size_including_cursor_tracks_blank_cursor_column() {
        let crop = SnapshotCropOptions {
            mode: SnapshotCropMode::OriginPreserving,
            padding: 0,
        };
        let mut recorded_size = (1, 1);
        let snapshot = CanvasSnapshot {
            glyphs: vec!["abcd    ".to_string()],
            highlights: None,
            origin: (0, 0),
            size: (8, 1),
        };

        grow_recording_size_including_cursor(&mut recorded_size, &snapshot, (4, 0), crop);

        assert_eq!(recorded_size, (5, 1));
    }

    #[test]
    fn keyspec_for_processed_key_omits_global_record_shortcut() {
        let key = AppKeyEvent::new(AppKeyCode::Char('Q'), AppKeyModifiers::NONE);
        assert_eq!(keyspec_for_processed_key(key, Action::RecordAndExit), None);
    }

    #[test]
    fn keyspec_for_processed_key_omits_recording_reset_shortcut() {
        let key = AppKeyEvent::new(AppKeyCode::Char('R'), AppKeyModifiers::NONE);
        assert_eq!(
            keyspec_for_processed_key(
                key,
                Action::Continue.with_recording(RecordingDirective::ResetBaseline)
            ),
            None
        );
    }

    #[test]
    fn keyspec_for_processed_key_keeps_literal_q_in_text_flow() {
        let key = AppKeyEvent::new(AppKeyCode::Char('Q'), AppKeyModifiers::NONE);
        assert_eq!(
            keyspec_for_processed_key(key, Action::Continue),
            Some("Q".to_string())
        );
    }

    #[test]
    fn format_e2e_recording_includes_clipboard_block_when_present() {
        let content = format_e2e_recording(
            "hjkl",
            &["abc".to_string()],
            Some((2, 3)),
            Some("copied"),
            &["xyz".to_string()],
        );

        assert!(content.contains("**Cursor** 2 3"));
        assert!(content.contains("**Clipboard**"));
        assert!(content.contains("```\ncopied\n```"));
    }

    #[test]
    fn format_e2e_recording_uses_longer_clipboard_fence_when_needed() {
        let content = format_e2e_recording(
            "p",
            &["...".to_string()],
            Some((0, 0)),
            Some("```textagram\n┏━┓\n```"),
            &["┏━┓".to_string()],
        );

        assert!(content.contains("**Clipboard**\n````\n```textagram\n┏━┓\n```\n````"));
    }

    #[test]
    fn format_e2e_recording_omits_clipboard_block_when_empty() {
        let content = format_e2e_recording(
            "hjkl",
            &["abc".to_string()],
            Some((0, 0)),
            Some(""),
            &["xyz".to_string()],
        );

        assert!(!content.contains("**Clipboard**"));
    }
}
