use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use textagram::{AppKeyCode, AppKeyEvent, AppKeyModifiers, ClipboardIntent};

/// Translates crossterm key events into Textagram's host-neutral key type.
pub fn app_key_event_from_crossterm(key_event: KeyEvent) -> AppKeyEvent {
    let mut modifiers = AppKeyModifiers::NONE;
    if key_event.modifiers.contains(KeyModifiers::SHIFT) {
        modifiers |= AppKeyModifiers::SHIFT;
    }
    if key_event.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers |= AppKeyModifiers::CONTROL;
    }
    if key_event.modifiers.contains(KeyModifiers::ALT) {
        modifiers |= AppKeyModifiers::ALT;
    }

    let code = match key_event.code {
        KeyCode::Char(ch) => {
            AppKeyCode::Char(normalized_shifted_alpha_char(ch, key_event.modifiers))
        }
        KeyCode::Enter => AppKeyCode::Enter,
        KeyCode::Esc => AppKeyCode::Esc,
        KeyCode::Left => AppKeyCode::Left,
        KeyCode::Right => AppKeyCode::Right,
        KeyCode::Up => AppKeyCode::Up,
        KeyCode::Down => AppKeyCode::Down,
        KeyCode::Backspace => AppKeyCode::Backspace,
        KeyCode::Tab => AppKeyCode::Tab,
        KeyCode::PageUp => AppKeyCode::PageUp,
        KeyCode::PageDown => AppKeyCode::PageDown,
        _ => AppKeyCode::Null,
    };

    AppKeyEvent::new(code, modifiers)
}

fn normalized_shifted_alpha_char(ch: char, modifiers: KeyModifiers) -> char {
    if modifiers.contains(KeyModifiers::SHIFT) && ch.is_ascii_lowercase() {
        ch.to_ascii_uppercase()
    } else {
        ch
    }
}

/// Maps terminal-only shifted document clipboard keys (`Y`/`P`/`X`) to intents.
pub fn terminal_document_clipboard_intent_from_crossterm(
    key_event: KeyEvent,
) -> Option<ClipboardIntent> {
    if !key_event.modifiers.contains(KeyModifiers::SHIFT) {
        return None;
    }

    match key_event.code {
        KeyCode::Char('Y') | KeyCode::Char('y') => Some(ClipboardIntent::CopyDocument),
        KeyCode::Char('X') | KeyCode::Char('x') => Some(ClipboardIntent::CutDocument),
        KeyCode::Char('P') | KeyCode::Char('p') => Some(ClipboardIntent::PasteDocument),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossterm_translation_preserves_shifted_document_shortcut_key() {
        let event = KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT);
        let translated = app_key_event_from_crossterm(event);
        assert_eq!(translated.code, AppKeyCode::Char('Y'));
        assert!(translated.modifiers.contains(AppKeyModifiers::SHIFT));
    }

    #[test]
    fn crossterm_translation_normalizes_lowercase_shifted_alpha_to_uppercase() {
        let event = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::SHIFT);
        let translated = app_key_event_from_crossterm(event);
        assert_eq!(translated.code, AppKeyCode::Char('L'));
        assert!(translated.modifiers.contains(AppKeyModifiers::SHIFT));
    }

    #[test]
    fn crossterm_translation_preserves_shifted_arrow_precision_input() {
        let event = KeyEvent::new(KeyCode::Left, KeyModifiers::SHIFT);
        let translated = app_key_event_from_crossterm(event);
        assert_eq!(translated.code, AppKeyCode::Left);
        assert!(translated.modifiers.contains(AppKeyModifiers::SHIFT));
    }

    #[test]
    fn terminal_document_clipboard_intent_accepts_shifted_terminal_shortcuts() {
        let event = KeyEvent::new(KeyCode::Char('P'), KeyModifiers::SHIFT);

        assert_eq!(
            terminal_document_clipboard_intent_from_crossterm(event),
            Some(ClipboardIntent::PasteDocument)
        );
    }

    #[test]
    fn terminal_document_clipboard_intent_recognizes_cut_document_shortcut() {
        let event = KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT);

        assert_eq!(
            terminal_document_clipboard_intent_from_crossterm(event),
            Some(ClipboardIntent::CutDocument)
        );
    }
}
