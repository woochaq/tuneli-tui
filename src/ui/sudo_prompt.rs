use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub struct SudoPrompt {
    pub is_active: bool,
    pub input: String,
    pub error_msg: Option<String>,
    pub is_verifying: bool,
}

impl SudoPrompt {
    pub fn new() -> Self {
        Self {
            is_active: false,
            input: String::new(),
            error_msg: None,
            is_verifying: false,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        if !self.is_active {
            return;
        }

        // Center popup
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(5),
                Constraint::Percentage(40),
            ])
            .split(area);

        let center_y = popup_layout[1];
        let center_x = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(30),
                Constraint::Percentage(40),
                Constraint::Percentage(30),
            ])
            .split(center_y)[1];

        // Clear the background underneath
        f.render_widget(Clear, center_x);

        let block = Block::default()
            .title(" Sudo Authentication Required ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));

        let masked_input = "*".repeat(self.input.len());
        
        let mut text = vec![];
        if self.is_verifying {
            text.push(ratatui::text::Line::from(vec![
                Span::styled("Verifying sudo password... ⠋", Style::default().fg(Color::Yellow)),
            ]));
        } else {
            text.push(ratatui::text::Line::from(vec![
                Span::styled("Password: ", Style::default().fg(Color::Cyan)),
                Span::raw(masked_input),
            ]));
        }

        if let Some(err) = &self.error_msg {
            text.push(ratatui::text::Line::from(""));
            text.push(ratatui::text::Line::from(vec![
                Span::styled(err, Style::default().fg(Color::LightRed))
            ]));
        }

        let paragraph = Paragraph::new(text).block(block);
        f.render_widget(paragraph, center_x);
    }
}
