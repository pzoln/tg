use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::file_mode::FileMode;

const SAVE_PROMPT_TEXT: &str = "save changes? y/n/esc";
const NO_FILE_TO_SAVE_TEXT: &str = "no file to save";
const SAVE_FAILED_TEXT: &str = "save failed";

/// Result of a plain `q` request in file-backed mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitRequest {
    /// The document is dirty and the host should show the save prompt.
    PromptOpened,
    /// The host can exit immediately without prompting.
    ExitImmediately,
}

/// Result of an explicit save shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveRequestOutcome {
    /// File contents were written successfully.
    Saved,
    /// Scratch mode has no backing file, so the save request was blocked.
    BlockedNoFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuitPromptState {
    Inactive,
    ConfirmSaveOnQuit,
}

/// Host-owned file, save, and quit-prompt state for the terminal app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorState {
    file_mode: FileMode,
    quit_prompt: QuitPromptState,
    transient_message: Option<String>,
}

impl EditorState {
    /// Creates host editor state for a scratch, plain-file, or markdown-backed document.
    pub fn new(file_mode: FileMode) -> Self {
        Self {
            file_mode,
            quit_prompt: QuitPromptState::Inactive,
            transient_message: None,
        }
    }

    /// Returns the current backing document model.
    pub fn file_mode(&self) -> &FileMode {
        &self.file_mode
    }

    /// Returns optional top-center text owned by the host, such as save prompts.
    pub fn center_text(&self) -> Option<&str> {
        match self.quit_prompt {
            QuitPromptState::ConfirmSaveOnQuit => Some(SAVE_PROMPT_TEXT),
            QuitPromptState::Inactive => self.transient_message.as_deref(),
        }
    }

    /// Reports whether the save-on-quit prompt is currently intercepting input.
    pub fn prompt_active(&self) -> bool {
        matches!(self.quit_prompt, QuitPromptState::ConfirmSaveOnQuit)
    }

    /// Clears transient host messages after the next accepted key.
    pub fn clear_transient_message(&mut self) {
        if !self.prompt_active() {
            self.transient_message = None;
        }
    }

    /// Starts quit handling, opening a save prompt only for dirty file-backed documents.
    pub fn begin_quit(&mut self, current_editable_text: &str) -> QuitRequest {
        self.transient_message = None;
        if self.file_mode.path().is_some() && self.file_mode.is_dirty(current_editable_text) {
            self.quit_prompt = QuitPromptState::ConfirmSaveOnQuit;
            QuitRequest::PromptOpened
        } else {
            self.quit_prompt = QuitPromptState::Inactive;
            QuitRequest::ExitImmediately
        }
    }

    /// Dismisses an active save prompt without saving or exiting.
    pub fn cancel_quit_prompt(&mut self) {
        self.quit_prompt = QuitPromptState::Inactive;
    }

    /// Accepts the discard branch of the save prompt and clears prompt state.
    pub fn discard_quit_prompt(&mut self) {
        self.quit_prompt = QuitPromptState::Inactive;
        self.transient_message = None;
    }

    /// Saves the current editable diagram for `Ctrl+S` and keeps editing.
    pub fn save_from_shortcut(
        &mut self,
        current_editable_text: &str,
    ) -> Result<SaveRequestOutcome, FileSaveError> {
        self.transient_message = None;
        if self.file_mode.path().is_none() {
            self.transient_message = Some(NO_FILE_TO_SAVE_TEXT.to_string());
            return Ok(SaveRequestOutcome::BlockedNoFile);
        }

        self.persist_current_text(current_editable_text)?;
        Ok(SaveRequestOutcome::Saved)
    }

    /// Saves from the quit prompt before the host exits.
    pub fn save_from_quit_prompt(
        &mut self,
        current_editable_text: &str,
    ) -> Result<(), FileSaveError> {
        self.quit_prompt = QuitPromptState::Inactive;
        self.transient_message = None;
        self.persist_current_text(current_editable_text)
    }

    fn persist_current_text(&mut self, current_editable_text: &str) -> Result<(), FileSaveError> {
        let path = self
            .file_mode
            .path()
            .map(Path::to_path_buf)
            .expect("file-backed mode should have a save path");
        let payload = self
            .file_mode
            .save_payload(current_editable_text)
            .expect("file-backed mode should always provide a save payload");

        match write_text_file_atomically(&path, &payload) {
            Ok(()) => {
                self.file_mode.mark_saved(current_editable_text.to_string());
                Ok(())
            }
            Err(source) => {
                self.transient_message = Some(SAVE_FAILED_TEXT.to_string());
                Err(FileSaveError { path, source })
            }
        }
    }
}

/// Error returned when a file-backed save fails.
#[derive(Debug)]
pub struct FileSaveError {
    path: PathBuf,
    source: io::Error,
}

impl fmt::Display for FileSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed to save {}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for FileSaveError {}

/// Writes a text file via create-new temp file plus same-directory rename.
fn write_text_file_atomically(path: &Path, text: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tg".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    for attempt in 0..16u8 {
        let temp_path = parent.join(format!(
            ".{stem}.tmp-{nanos}-{}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(mut file) => {
                file.write_all(text.as_bytes())?;
                file.sync_all()?;
                drop(file);
                match fs::rename(&temp_path, path) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        let _ = fs::remove_file(&temp_path);
                        return Err(error);
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "failed to allocate a temporary file beside {}",
            path.display()
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_mode::PlainFileDocument;
    use std::fs;

    #[test]
    fn save_shortcut_in_scratch_mode_sets_blocked_message() {
        let mut state = EditorState::new(FileMode::scratch());

        let outcome = state.save_from_shortcut("").unwrap();

        assert_eq!(outcome, SaveRequestOutcome::BlockedNoFile);
        assert_eq!(state.center_text(), Some(NO_FILE_TO_SAVE_TEXT));
        assert!(!state.prompt_active());
    }

    #[test]
    fn plain_file_save_writes_new_contents_and_refreshes_dirty_baseline() {
        let temp_root = unique_temp_dir();
        fs::create_dir_all(&temp_root).unwrap();
        let path = temp_root.join("diagram.tg");
        fs::write(&path, "old").unwrap();
        let mode = FileMode::load_from_path(path.clone()).unwrap();
        let mut state = EditorState::new(mode);

        let outcome = state.save_from_shortcut("new").unwrap();

        assert_eq!(outcome, SaveRequestOutcome::Saved);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(!state.file_mode().is_dirty("new"));
        assert_eq!(state.center_text(), None);
    }

    #[test]
    fn markdown_backed_save_rewrites_only_first_recognized_fence_body() {
        let temp_root = unique_temp_dir();
        fs::create_dir_all(&temp_root).unwrap();
        let path = temp_root.join("diagram.md");
        fs::write(
            &path,
            "before\n```textagram\nold\n```\nmid\n```textagram\nlater\n```\nafter\n",
        )
        .unwrap();
        let mode = FileMode::load_from_path(path.clone()).unwrap();
        let mut state = EditorState::new(mode);

        let outcome = state.save_from_shortcut("new").unwrap();

        assert_eq!(outcome, SaveRequestOutcome::Saved);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "before\n```textagram\nnew\n```\nmid\n```textagram\nlater\n```\nafter\n"
        );
        assert!(!state.file_mode().is_dirty("new"));
    }

    #[test]
    fn clean_quit_exits_immediately_without_prompt() {
        let mode = FileMode::from_existing_text(PathBuf::from("diagram.tg"), "same");
        let mut state = EditorState::new(mode);

        let request = state.begin_quit("same");

        assert_eq!(request, QuitRequest::ExitImmediately);
        assert!(!state.prompt_active());
    }

    #[test]
    fn dirty_quit_prompt_can_be_cancelled_or_discarded() {
        let mode = FileMode::from_existing_text(PathBuf::from("diagram.tg"), "old");
        let mut state = EditorState::new(mode);

        let request = state.begin_quit("new");

        assert_eq!(request, QuitRequest::PromptOpened);
        assert_eq!(state.center_text(), Some(SAVE_PROMPT_TEXT));
        assert!(state.prompt_active());

        state.cancel_quit_prompt();
        assert!(!state.prompt_active());
        assert_eq!(state.center_text(), None);

        let request = state.begin_quit("new");
        assert_eq!(request, QuitRequest::PromptOpened);
        state.discard_quit_prompt();
        assert!(!state.prompt_active());
        assert_eq!(state.center_text(), None);
    }

    #[test]
    fn save_from_quit_prompt_writes_file_and_clears_dirty_state() {
        let temp_root = unique_temp_dir();
        fs::create_dir_all(&temp_root).unwrap();
        let path = temp_root.join("diagram.tg");
        fs::write(&path, "old").unwrap();
        let mode = FileMode::load_from_path(path.clone()).unwrap();
        let mut state = EditorState::new(mode);

        let request = state.begin_quit("new");
        assert_eq!(request, QuitRequest::PromptOpened);

        state.save_from_quit_prompt("new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(!state.file_mode().is_dirty("new"));
        assert!(!state.prompt_active());
        assert_eq!(state.center_text(), None);
    }

    #[test]
    fn save_failure_from_quit_prompt_keeps_document_dirty_and_shows_message() {
        let path = unique_temp_dir().join("missing-dir").join("diagram.tg");
        let mode = FileMode::PlainFile(PlainFileDocument::new(path, "old".to_string()));
        let mut state = EditorState::new(mode);

        let request = state.begin_quit("new");
        assert_eq!(request, QuitRequest::PromptOpened);

        let error = state.save_from_quit_prompt("new").unwrap_err();

        assert!(error.to_string().contains("failed to save"));
        assert_eq!(state.center_text(), Some(SAVE_FAILED_TEXT));
        assert!(!state.prompt_active());
        assert!(state.file_mode().is_dirty("new"));
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tg-editor-state-{nanos}-{}", std::process::id()))
    }
}
