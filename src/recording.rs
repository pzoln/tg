//! Terminal-only recording helpers.
//!
//! These support the current `tg` workflow and are intentionally kept out of
//! the long-term core-library surface while the repo is still a single crate.

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

pub fn keyspec_for_processed_key(key_event: AppKeyEvent, action: Action) -> Option<String> {
    if action.record_and_exit() || action.reset_recording_baseline() {
        None
    } else {
        key_event_to_keyspec(key_event)
    }
}

#[cfg_attr(test, allow(dead_code))]
pub fn write_e2e_recording(
    path: &Path,
    keys: &str,
    given: &[String],
    clipboard: Option<&str>,
    expect: &[String],
) -> Result<String, Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, format_e2e_recording(keys, given, clipboard, expect))?;
    Ok(path.display().to_string())
}

pub fn format_e2e_recording(
    keys: &str,
    given: &[String],
    clipboard: Option<&str>,
    expect: &[String],
) -> String {
    let mut content = String::new();
    content.push_str("## Scenario: Recorded session\n\n**Given**\n```\n");
    for line in given {
        content.push_str(line);
        content.push('\n');
    }
    content.push_str("```\n");
    if let Some(clipboard) = clipboard.filter(|clipboard| !clipboard.is_empty()) {
        content.push_str("\n**Clipboard**\n```\n");
        content.push_str(clipboard);
        if !clipboard.ends_with('\n') {
            content.push('\n');
        }
        content.push_str("```\n");
    }
    content.push_str(&format!("\n**Keys**\n```\n{keys}\n```\n"));
    content.push_str("\n**Expect**\n```\n");
    for line in expect {
        content.push_str(line);
        content.push('\n');
    }
    content.push_str("```\n");
    content
}

#[cfg(test)]
pub fn occupied_recording_size(snapshot: &CanvasSnapshot, crop: SnapshotCropOptions) -> (u16, u16) {
    snapshot.crop(crop).size
}

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
pub fn grow_recording_size(
    recorded_size: &mut (u16, u16),
    snapshot: &CanvasSnapshot,
    crop: SnapshotCropOptions,
) {
    let size = occupied_recording_size(snapshot, crop);
    recorded_size.0 = recorded_size.0.max(size.0);
    recorded_size.1 = recorded_size.1.max(size.1);
}

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
            Some("copied"),
            &["xyz".to_string()],
        );

        assert!(content.contains("**Clipboard**"));
        assert!(content.contains("```\ncopied\n```"));
    }

    #[test]
    fn format_e2e_recording_omits_clipboard_block_when_empty() {
        let content =
            format_e2e_recording("hjkl", &["abc".to_string()], Some(""), &["xyz".to_string()]);

        assert!(!content.contains("**Clipboard**"));
    }
}
