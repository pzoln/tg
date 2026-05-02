/// Host-owned title chrome for the terminal UI.
pub mod chrome;
/// Process-level CLI parsing and help/version rendering.
pub mod cli;
/// File save and quit-prompt state for the terminal host.
pub mod editor_state;
/// File-backed document models for plain files and markdown fences.
pub mod file_mode;
/// Crossterm-to-Textagram key translation.
pub mod terminal_input;

#[cfg(test)]
mod recording;
