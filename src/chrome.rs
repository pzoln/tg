use crate::file_mode::FileMode;
use textagram::{ChromeOverrides, RenderLine, RenderSegment, SemanticStyle};

#[cfg(test)]
use textagram::{ScreenFrame, Session};

const TG_TITLE_TEXT: &str = " tg ";
const CHIP_PREFIX_TEXT: &str = "─╴";
const CHIP_SUFFIX_TEXT: &str = "╶";

pub fn chrome_overrides(file_mode: &FileMode, current_editable_text: &str) -> ChromeOverrides {
    chrome_overrides_with_center(file_mode, current_editable_text, None)
}

pub fn chrome_overrides_with_center(
    file_mode: &FileMode,
    current_editable_text: &str,
    center_text: Option<&str>,
) -> ChromeOverrides {
    ChromeOverrides {
        left_title: Some(left_title_line(file_mode, current_editable_text)),
        top_center: center_text.map(center_title_line),
    }
}

fn left_title_line(file_mode: &FileMode, current_editable_text: &str) -> RenderLine {
    let mut segments = vec![plain_segment(TG_TITLE_TEXT)];
    if let Some(file_label) = file_label(file_mode, current_editable_text) {
        push_chip(&mut segments, &file_label);
    }
    RenderLine { segments }
}

fn file_label(file_mode: &FileMode, current_editable_text: &str) -> Option<String> {
    let path = file_mode.path()?;
    let base = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    if file_mode.is_dirty(current_editable_text) {
        Some(format!("{base}*"))
    } else {
        Some(base)
    }
}

fn push_chip(segments: &mut Vec<RenderSegment>, label: &str) {
    segments.push(plain_segment(CHIP_PREFIX_TEXT));
    segments.push(plain_segment(label));
    segments.push(plain_segment(CHIP_SUFFIX_TEXT));
}

fn plain_segment(text: impl Into<String>) -> RenderSegment {
    RenderSegment {
        text: text.into(),
        style: SemanticStyle::Plain,
    }
}

fn center_title_line(text: &str) -> RenderLine {
    RenderLine {
        segments: vec![RenderSegment {
            text: format!(" {text} "),
            style: SemanticStyle::HelpHeading,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_mode::{MarkdownBackedDocument, PlainFileDocument};
    use textagram::{Area, HintProfile};

    #[test]
    fn scratch_left_title_is_just_tg() {
        let overrides = chrome_overrides(&FileMode::scratch(), "");

        assert_eq!(
            flatten_render_line_text(overrides.left_title.as_ref().expect("left title")),
            " tg "
        );
    }

    #[test]
    fn plain_file_left_title_uses_dirty_file_chip() {
        let mode = FileMode::PlainFile(PlainFileDocument::new(
            "docs/MyDiagram.tg".into(),
            "old".to_string(),
        ));

        let overrides = chrome_overrides(&mode, "new");

        assert_eq!(
            flatten_render_line_text(overrides.left_title.as_ref().expect("left title")),
            " tg ─╴MyDiagram.tg*╶"
        );
    }

    #[test]
    fn markdown_file_left_title_uses_same_file_chip_as_plain_file_mode() {
        let mode = FileMode::MarkdownBacked(MarkdownBackedDocument::new(
            "docs/MyDoc.md".into(),
            "before\n".to_string(),
            "```textagram\n".to_string(),
            "body".to_string(),
            "```\n".to_string(),
            "after\n".to_string(),
        ));

        let overrides = chrome_overrides(&mode, "body");

        assert_eq!(
            flatten_render_line_text(overrides.left_title.as_ref().expect("left title")),
            " tg ─╴MyDoc.md╶"
        );
    }

    #[test]
    fn long_left_title_never_overwrites_mode_or_coordinates() {
        let mut session = Session::new(20, 20);
        session.set_cursor_position_in_viewport(10, 10);
        let mode = FileMode::PlainFile(PlainFileDocument::new(
            "docs/AnExtremelyLongDiagramNameThatShouldDisappear.tg".into(),
            String::new(),
        ));
        let current = session.current_document_text();
        let overrides = chrome_overrides(&mode, &current);

        let screen = session.composed_frame_with_overrides(
            Area::new(0, 0, 34, 8),
            HintProfile::Terminal,
            &overrides,
        );

        let top_row = screen_frame_top_row_text(&screen);
        assert!(top_row.contains("Navigating"), "{top_row}");
        assert!(top_row.contains(" 10,10 "), "{top_row}");
    }

    fn flatten_render_line_text(line: &RenderLine) -> String {
        line.segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<String>()
    }

    fn screen_frame_top_row_text(screen: &ScreenFrame) -> String {
        flatten_render_line_text(screen.rows.first().expect("top row"))
    }
}
