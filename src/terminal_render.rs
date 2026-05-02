use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    Frame,
};

use textagram::{
    Area, ChromeOverrides, HintProfile, RenderLine, ScreenFrame, SemanticStyle, Session,
};

pub fn draw_session_frame(session: &Session, frame: &mut Frame<'_>, chrome: &ChromeOverrides) {
    let total_area = frame.size();
    let screen = session.composed_frame_with_overrides(
        area_from_ratatui_rect(total_area),
        HintProfile::Terminal,
        chrome,
    );
    paint_screen_frame(
        frame.buffer_mut(),
        total_area.x,
        total_area.y,
        total_area.width,
        &screen,
    );
}

fn area_from_ratatui_rect(rect: Rect) -> Area {
    Area::new(rect.x, rect.y, rect.width, rect.height)
}

fn paint_screen_frame(buffer: &mut Buffer, x: u16, y: u16, width: u16, screen: &ScreenFrame) {
    for (row_idx, line) in screen.rows.iter().enumerate().take(screen.height as usize) {
        paint_render_line(buffer, x, y + row_idx as u16, width as usize, line);
    }
}

fn paint_render_line(buffer: &mut Buffer, x: u16, y: u16, width: usize, line: &RenderLine) {
    let mut xoff = x;
    let mut rendered_width = 0usize;

    for segment in &line.segments {
        let style = semantic_style_to_ratatui(segment.style);
        for ch in segment.text.chars() {
            if rendered_width == width {
                break;
            }
            let mut encoded = [0; 4];
            buffer.set_stringn(xoff, y, ch.encode_utf8(&mut encoded), 1, style);
            xoff = xoff.saturating_add(1);
            rendered_width += 1;
        }
        if rendered_width == width {
            break;
        }
    }

    if rendered_width < width {
        buffer.set_stringn(
            xoff,
            y,
            " ".repeat(width - rendered_width),
            width - rendered_width,
            Style::default(),
        );
    }
}

fn semantic_style_to_ratatui(style: SemanticStyle) -> Style {
    match style {
        SemanticStyle::Plain => Style::default(),
        SemanticStyle::Grid => Style::default().fg(Color::Gray),
        SemanticStyle::Text => Style::default(),
        SemanticStyle::Connector => Style::default(),
        SemanticStyle::Corruption => Style::default().fg(Color::Red),
        SemanticStyle::Cursor => Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        SemanticStyle::Selection => Style::default().fg(Color::Black).bg(Color::White),
        SemanticStyle::HelpHeading => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        SemanticStyle::HelpKey | SemanticStyle::HintKey => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    }
}
