use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RECOGNIZED_FENCE_LABEL: &str = "textagram";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScratchDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlainFileDocument {
    path: PathBuf,
    original_editable_text: String,
}

impl PlainFileDocument {
    pub fn new(path: PathBuf, original_editable_text: String) -> Self {
        Self {
            path,
            original_editable_text,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn original_editable_text(&self) -> &str {
        &self.original_editable_text
    }

    pub fn is_dirty(&self, current_editable_text: &str) -> bool {
        self.original_editable_text != current_editable_text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownBackedDocument {
    path: PathBuf,
    before_first_fence: String,
    opening_fence_line: String,
    original_editable_text: String,
    closing_fence_line: String,
    after_first_fence: String,
}

impl MarkdownBackedDocument {
    fn new(
        path: PathBuf,
        before_first_fence: String,
        opening_fence_line: String,
        original_editable_text: String,
        closing_fence_line: String,
        after_first_fence: String,
    ) -> Self {
        Self {
            path,
            before_first_fence,
            opening_fence_line,
            original_editable_text,
            closing_fence_line,
            after_first_fence,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn before_first_fence(&self) -> &str {
        &self.before_first_fence
    }

    pub fn opening_fence_line(&self) -> &str {
        &self.opening_fence_line
    }

    pub fn original_editable_text(&self) -> &str {
        &self.original_editable_text
    }

    pub fn closing_fence_line(&self) -> &str {
        &self.closing_fence_line
    }

    pub fn after_first_fence(&self) -> &str {
        &self.after_first_fence
    }

    pub fn is_dirty(&self, current_editable_text: &str) -> bool {
        self.original_editable_text != current_editable_text
    }

    pub fn reconstruct_with(&self, current_editable_text: &str) -> String {
        let mut rebuilt = String::new();
        rebuilt.push_str(&self.before_first_fence);
        rebuilt.push_str(&self.opening_fence_line);
        if !current_editable_text.is_empty() {
            rebuilt.push_str(current_editable_text);
            if !current_editable_text.ends_with('\n') {
                rebuilt.push('\n');
            }
        }
        rebuilt.push_str(&self.closing_fence_line);
        rebuilt.push_str(&self.after_first_fence);
        rebuilt
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        path: PathBuf,
        before_first_fence: String,
        opening_fence_line: String,
        original_editable_text: String,
        closing_fence_line: String,
        after_first_fence: String,
    ) -> Self {
        Self::new(
            path,
            before_first_fence,
            opening_fence_line,
            original_editable_text,
            closing_fence_line,
            after_first_fence,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileMode {
    Scratch(ScratchDocument),
    PlainFile(PlainFileDocument),
    MarkdownBacked(MarkdownBackedDocument),
}

impl FileMode {
    pub fn scratch() -> Self {
        Self::Scratch(ScratchDocument)
    }

    pub fn load_from_path(path: PathBuf) -> Result<Self, FileLoadError> {
        match fs::metadata(&path) {
            Ok(metadata) => {
                if metadata.is_dir() {
                    return Err(FileLoadError::Directory(path));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::PlainFile(PlainFileDocument::new(path, String::new())));
            }
            Err(error) => {
                return Err(FileLoadError::Read {
                    path,
                    source: error,
                })
            }
        }

        let text = fs::read_to_string(&path).map_err(|source| FileLoadError::Read {
            path: path.clone(),
            source,
        })?;
        Ok(Self::from_existing_text(path, &text))
    }

    pub fn from_existing_text(path: PathBuf, text: &str) -> Self {
        let normalized = normalize_newlines(text);
        if let Some(parsed) = parse_markdown_backed_document(path.clone(), &normalized) {
            Self::MarkdownBacked(parsed)
        } else {
            Self::PlainFile(PlainFileDocument::new(path, normalized))
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Scratch(_) => None,
            Self::PlainFile(doc) => Some(doc.path()),
            Self::MarkdownBacked(doc) => Some(doc.path()),
        }
    }

    pub fn original_editable_text(&self) -> &str {
        match self {
            Self::Scratch(_) => "",
            Self::PlainFile(doc) => doc.original_editable_text(),
            Self::MarkdownBacked(doc) => doc.original_editable_text(),
        }
    }

    pub fn is_dirty(&self, current_editable_text: &str) -> bool {
        match self {
            Self::Scratch(_) => false,
            Self::PlainFile(doc) => doc.is_dirty(current_editable_text),
            Self::MarkdownBacked(doc) => doc.is_dirty(current_editable_text),
        }
    }

    pub fn save_payload(&self, current_editable_text: &str) -> Option<String> {
        match self {
            Self::Scratch(_) => None,
            Self::PlainFile(_) => Some(current_editable_text.to_string()),
            Self::MarkdownBacked(doc) => Some(doc.reconstruct_with(current_editable_text)),
        }
    }

    pub fn mark_saved(&mut self, current_editable_text: String) {
        match self {
            Self::Scratch(_) => {}
            Self::PlainFile(doc) => {
                doc.original_editable_text = current_editable_text;
            }
            Self::MarkdownBacked(doc) => {
                doc.original_editable_text = current_editable_text;
            }
        }
    }
}

#[derive(Debug)]
pub enum FileLoadError {
    Directory(PathBuf),
    Read { path: PathBuf, source: io::Error },
}

impl fmt::Display for FileLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Directory(path) => {
                write!(f, "{} is a directory; expected a text file", path.display())
            }
            Self::Read { path, source } => write!(f, "failed to read {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for FileLoadError {}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn parse_markdown_backed_document(path: PathBuf, text: &str) -> Option<MarkdownBackedDocument> {
    let fence = find_first_recognized_fence(text)?;
    Some(MarkdownBackedDocument::new(
        path,
        text[..fence.opening_start].to_string(),
        text[fence.opening_start..fence.opening_end].to_string(),
        fence.body(text),
        text[fence.closing_start..fence.closing_end].to_string(),
        text[fence.closing_end..].to_string(),
    ))
}

fn find_first_recognized_fence(text: &str) -> Option<FenceBounds> {
    let mut offset = 0usize;
    let mut opening_bounds = None;

    for line_segment in text.split_inclusive('\n') {
        let line = line_segment.strip_suffix('\n').unwrap_or(line_segment);

        if opening_bounds.is_none() {
            if let Some(rest) = line.trim_start().strip_prefix("```") {
                if fenced_block_label(rest.trim()).is_some_and(is_recognized_diagram_fence) {
                    opening_bounds = Some((offset, offset + line_segment.len()));
                }
            }
        } else if line.trim_start().starts_with("```") {
            let (opening_start, opening_end) = opening_bounds.take().unwrap();
            return Some(FenceBounds {
                opening_start,
                opening_end,
                closing_start: offset,
                closing_end: offset + line_segment.len(),
            });
        }

        offset += line_segment.len();
    }

    None
}

fn fenced_block_label(info_string: &str) -> Option<&str> {
    info_string.split_whitespace().next()
}

fn is_recognized_diagram_fence(label: &str) -> bool {
    label.eq_ignore_ascii_case(RECOGNIZED_FENCE_LABEL)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FenceBounds {
    opening_start: usize,
    opening_end: usize,
    closing_start: usize,
    closing_end: usize,
}

impl FenceBounds {
    fn body(self, text: &str) -> String {
        let mut body = text[self.opening_end..self.closing_start].to_string();
        if body.ends_with('\n') {
            body.pop();
        }
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn existing_file_with_no_recognized_fence_loads_as_plain_text() {
        let mode = FileMode::from_existing_text(
            PathBuf::from("notes.md"),
            "before\n```rust\nlet x = 1;\n```\nafter",
        );

        assert_eq!(
            mode,
            FileMode::PlainFile(PlainFileDocument::new(
                PathBuf::from("notes.md"),
                "before\n```rust\nlet x = 1;\n```\nafter".to_string()
            ))
        );
    }

    #[test]
    fn existing_file_with_recognized_fence_loads_first_fence_body_only() {
        let mode = FileMode::from_existing_text(
            PathBuf::from("diagram.md"),
            "intro\n```textagram demo\n╭╮\n▽│\n━╯\n```\noutro\n",
        );

        let FileMode::MarkdownBacked(doc) = mode else {
            panic!("expected markdown-backed document");
        };

        assert_eq!(doc.before_first_fence(), "intro\n");
        assert_eq!(doc.opening_fence_line(), "```textagram demo\n");
        assert_eq!(doc.original_editable_text(), "╭╮\n▽│\n━╯");
        assert_eq!(doc.closing_fence_line(), "```\n");
        assert_eq!(doc.after_first_fence(), "outro\n");
    }

    #[test]
    fn existing_file_with_multiple_recognized_fences_uses_first_one_only() {
        let mode = FileMode::from_existing_text(
            PathBuf::from("diagram.md"),
            "```textagram\nfirst\n```\nmid\n```TEXTAGRAM\nsecond\n```\n",
        );

        let FileMode::MarkdownBacked(doc) = mode else {
            panic!("expected markdown-backed document");
        };

        assert_eq!(doc.original_editable_text(), "first");
        assert_eq!(doc.after_first_fence(), "mid\n```TEXTAGRAM\nsecond\n```\n");
    }

    #[test]
    fn closing_fence_on_final_line_without_trailing_newline_is_parsed_once() {
        let mode = FileMode::from_existing_text(
            PathBuf::from("diagram.md"),
            "before\n```textagram\nbody\n```",
        );

        let FileMode::MarkdownBacked(doc) = mode else {
            panic!("expected markdown-backed document");
        };

        assert_eq!(doc.before_first_fence(), "before\n");
        assert_eq!(doc.opening_fence_line(), "```textagram\n");
        assert_eq!(doc.original_editable_text(), "body");
        assert_eq!(doc.closing_fence_line(), "```");
        assert_eq!(doc.after_first_fence(), "");
    }

    #[test]
    fn unclosed_recognized_fence_falls_back_to_plain_text() {
        let mode = FileMode::from_existing_text(
            PathBuf::from("diagram.md"),
            "before\n```textagram\nbody\nstill body",
        );

        assert_eq!(
            mode,
            FileMode::PlainFile(PlainFileDocument::new(
                PathBuf::from("diagram.md"),
                "before\n```textagram\nbody\nstill body".to_string()
            ))
        );
    }

    #[test]
    fn markdown_reconstruction_preserves_unedited_regions() {
        let mode = FileMode::from_existing_text(
            PathBuf::from("diagram.md"),
            "before\n```textagram\nold\n```\nmid\n```textagram\nlater\n```\nafter\n",
        );

        let FileMode::MarkdownBacked(doc) = mode else {
            panic!("expected markdown-backed document");
        };

        assert_eq!(
            doc.reconstruct_with("new"),
            "before\n```textagram\nnew\n```\nmid\n```textagram\nlater\n```\nafter\n"
        );
    }

    #[test]
    fn missing_file_starts_as_blank_plain_diagram_document() {
        let temp_root = unique_temp_dir();
        fs::create_dir_all(&temp_root).unwrap();
        let path = temp_root.join("fresh.md");

        let mode = FileMode::load_from_path(path.clone()).unwrap();

        assert_eq!(
            mode,
            FileMode::PlainFile(PlainFileDocument::new(path, String::new()))
        );
    }

    #[test]
    fn directory_path_is_rejected() {
        let path = unique_temp_dir();
        fs::create_dir_all(&path).unwrap();

        let error = FileMode::load_from_path(path.clone()).unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("{} is a directory; expected a text file", path.display())
        );
    }

    #[test]
    fn newline_normalization_happens_on_load() {
        let mode = FileMode::from_existing_text(PathBuf::from("diagram.txt"), "one\r\ntwo\rthree");

        assert_eq!(mode.original_editable_text(), "one\ntwo\nthree");
    }

    #[test]
    fn dirty_tracking_compares_current_text_to_original_editable_text() {
        let plain = FileMode::from_existing_text(PathBuf::from("diagram.txt"), "abc");
        assert!(!plain.is_dirty("abc"));
        assert!(plain.is_dirty("abd"));

        let markdown =
            FileMode::from_existing_text(PathBuf::from("diagram.md"), "```textagram\nabc\n```\n");
        assert!(!markdown.is_dirty("abc"));
        assert!(markdown.is_dirty("abd"));

        let scratch = FileMode::scratch();
        assert!(!scratch.is_dirty("anything"));
    }

    #[test]
    fn save_payload_uses_whole_text_for_plain_files() {
        let mode = FileMode::from_existing_text(PathBuf::from("diagram.txt"), "old");

        assert_eq!(mode.save_payload("new"), Some("new".to_string()));
    }

    #[test]
    fn save_payload_reconstructs_only_first_recognized_fence_body() {
        let mode = FileMode::from_existing_text(
            PathBuf::from("diagram.md"),
            "before\n```textagram\nold\n```\nafter\n",
        );

        assert_eq!(
            mode.save_payload("new"),
            Some("before\n```textagram\nnew\n```\nafter\n".to_string())
        );
    }

    #[test]
    fn mark_saved_refreshes_dirty_baseline() {
        let mut mode = FileMode::from_existing_text(PathBuf::from("diagram.txt"), "old");

        assert!(mode.is_dirty("new"));
        mode.mark_saved("new".to_string());
        assert!(!mode.is_dirty("new"));
    }

    #[test]
    fn non_utf8_file_is_rejected() {
        let temp_root = unique_temp_dir();
        fs::create_dir_all(&temp_root).unwrap();
        let path = temp_root.join("bad.txt");
        fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();

        let error = FileMode::load_from_path(path.clone()).unwrap_err();

        assert!(error
            .to_string()
            .contains(&format!("failed to read {}", path.display())));
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tg-file-mode-{nanos}-{}", std::process::id()))
    }
}
