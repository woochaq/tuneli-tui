use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Clear, Chart, Dataset, GraphType, Axis},
    symbols,
    Frame,
};

use crate::ui::app::{App, FocusedPanel};

pub fn draw(f: &mut Frame, _app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Main Body (Unified)
                Constraint::Length(3), // Footer
            ]
            .as_ref(),
        )
        .split(f.area());

    draw_header(f, _app, chunks[0]);
    draw_main_body(f, _app, chunks[1]);
    draw_footer(f, _app, chunks[2]);

    // Overlays
    if _app.show_help {
        draw_help_overlay(f);
    }

    if _app.show_config_modal {
        draw_config_modal(f, _app);
    }

    if _app.show_add_config_modal {
        draw_add_config_modal(f, _app);
    }

    if _app.show_delete_modal {
        draw_delete_modal(f, _app);
    }

    _app.sudo_prompt.draw(f, f.area());
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let sudo_status = if crate::engine::runner::SudoRunner::get_password().is_some() {
        Span::styled(" [Sudo: OK] ", Style::default().fg(Color::Green))
    } else {
        Span::styled(" [Sudo: Locked] ", Style::default().fg(Color::DarkGray))
    };

    let geo_span = if let Some(ref geo) = app.geo_info {
        let ping_text = if let Some(p) = app.ping {
            format!(" [Ping: {}ms] ", p.as_millis())
        } else {
            " [Ping: ---] ".to_string()
        };
        Span::styled(
            format!(" {} {} ", geo.public_ip, ping_text),
            Style::default().fg(Color::LightCyan),
        )
    } else {
        Span::styled(" fetching IP… ", Style::default().fg(Color::DarkGray))
    };

    let status_span = if let Some(ref msg) = app.status_message {
        Span::styled(format!(" ⚡ {} ", msg), Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };

    let title_line = Line::from(vec![
        Span::styled(" tuneli-tui ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        sudo_status,
        geo_span,
        status_span,
    ]);

    let header = Paragraph::new(title_line)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, area);
}

fn draw_main_body(f: &mut Frame, app: &mut App, area: Rect) {
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .split(area);

    // Left: Profiles
    let profiles_border_style = if app.focused_panel == FocusedPanel::Profiles {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let active_name = app.active_profile.as_ref().map(|p| p.name.as_str());

    let sidebar_items: Vec<ListItem> = app.profiles.iter()
        .map(|p| {
            let is_active = active_name == Some(p.name.as_str());
            let (prefix, color) = if is_active {
                ("● ", Color::LightYellow)
            } else {
                ("○ ", Color::LightGreen)
            };
            let (badge, badge_color) = match &p.protocol {
                crate::models::ProtocolConfig::WireGuard { .. } => ("[WG] ", Color::Cyan),
                crate::models::ProtocolConfig::OpenVpn { .. }   => ("[OV] ", Color::Magenta),
            };
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(color)),
                Span::styled(badge, Style::default().fg(badge_color)),
                Span::styled(p.name.clone(), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let sidebar = List::new(sidebar_items)
        .block(Block::default()
            .title(" VPN Profiles ")
            .borders(Borders::ALL)
            .border_style(profiles_border_style))
        .highlight_style(Style::default().bg(Color::Rgb(40, 44, 52)).add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");
    
    f.render_stateful_widget(sidebar, body_chunks[0], &mut app.list_state);

    // Right: Throughput + Logs
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)].as_ref())
        .split(body_chunks[1]);

    // Throughput Chart (always cyan/yellow, not focus dependent currently)
    draw_throughput_widget(f, app, right_chunks[0]);

    // Logs
    let logs_border_style = if app.focused_panel == FocusedPanel::Logs {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let log_area = Block::default()
        .title(" System Logs ")
        .borders(Borders::ALL)
        .border_style(logs_border_style);
    
    let mut iter_lines: Vec<Line> = Vec::new();
    for msg in &app.log_lines {
        let span = if msg.starts_with("[ERROR]") {
            Span::styled(msg, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        } else if msg.starts_with("[cmd]") {
            Span::styled(msg, Style::default().fg(Color::DarkGray))
        } else if msg.starts_with("[geo]") || msg.starts_with("[ping]") {
            Span::styled(msg, Style::default().fg(Color::Blue))
        } else if msg.starts_with("[tuneli-tui]") {
            Span::styled(msg, Style::default().fg(Color::Green))
        } else {
            Span::raw(msg)
        };
        iter_lines.push(Line::from(vec![span]));
    }

    let log_text = Paragraph::new(iter_lines).block(log_area);
    f.render_widget(log_text, right_chunks[1]);
}

fn draw_throughput_widget(f: &mut Frame, app: &App, area: Rect) {
    let inner_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(area);

    let mut rx_points = vec![];
    let mut tx_points = vec![];
    let mut max_rx = 1000.0; // minimum scale 1KB/s
    let mut max_tx = 1000.0;

    for (i, (rx, tx)) in app.throughput_history.iter().enumerate() {
        let x = i as f64;
        rx_points.push((x, *rx));
        tx_points.push((x, *tx));
        if *rx > max_rx { max_rx = *rx; }
        if *tx > max_tx { max_tx = *tx; }
    }

    let rx_last = app.throughput_history.back().map(|(rx, _)| *rx).unwrap_or(0.0);
    let tx_last = app.throughput_history.back().map(|(_, tx)| *tx).unwrap_or(0.0);

    let rx_dataset = vec![Dataset::default()
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::LightBlue))
        .graph_type(GraphType::Line)
        .data(&rx_points)];

    let tx_dataset = vec![Dataset::default()
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::LightYellow))
        .graph_type(GraphType::Line)
        .data(&tx_points)];

    let max_rx_str = format_speed(max_rx);
    let max_tx_str = format_speed(max_tx);
    let rx_width = max_rx_str.len();
    let tx_width = max_tx_str.len();
    
    // To keep bounds properly aligned on the edge of the box without trailing lines:
    let min_rx_str = format!("{:>width$}", "0.0 B/s", width = rx_width);
    let min_tx_str = format!("{:>width$}", "0.0 B/s", width = tx_width);

    let rx_chart = Chart::new(rx_dataset)
        .block(Block::default()
            .title(format!(" Download: {} ", format_speed(rx_last)))
            .borders(Borders::LEFT | Borders::RIGHT | Borders::TOP))
        .x_axis(Axis::default().bounds([0.0, 100.0]))
        .y_axis(Axis::default()
            .bounds([0.0, max_rx * 1.1])
            .labels(vec![
                Span::raw(min_rx_str),
                Span::raw(""),
                Span::styled(max_rx_str, Style::default().add_modifier(Modifier::BOLD)),
            ]));

    let tx_chart = Chart::new(tx_dataset)
        .block(Block::default()
            .title(format!(" Upload:   {} ", format_speed(tx_last)))
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM))
        .x_axis(Axis::default().bounds([0.0, 100.0]))
        .y_axis(Axis::default()
            .bounds([0.0, max_tx * 1.1])
            .labels(vec![
                Span::raw(min_tx_str),
                Span::raw(""),
                Span::styled(max_tx_str, Style::default().add_modifier(Modifier::BOLD)),
            ]));

    f.render_widget(rx_chart, inner_layout[0]);
    f.render_widget(tx_chart, inner_layout[1]);
}

fn format_speed(speed_bps: f64) -> String {
    if speed_bps < 1024.0 {
        format!("{:.1} B/s", speed_bps)
    } else if speed_bps < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", speed_bps / 1024.0)
    } else {
        format!("{:.1} MB/s", speed_bps / (1024.0 * 1024.0))
    }
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let footer_text = if app.quit_pending {
        " ⚠ Press Ctrl+C again to exit (will disconnect VPN) "
    } else {
        " Tab: Cycle Focus  |  j/k: Nav  |  c: Connect  |  v: Config  |  y: Yank  |  a: Add  |  d: Disc  |  r: Recon  |  i: IP  |  ?: Help "
    };
    let footer_style = if app.quit_pending {
        Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let footer = Paragraph::new(Span::styled(footer_text, footer_style))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, area);
}

fn draw_help_overlay(f: &mut Frame) {
    let area = centered_rect(60, 60, f.area());
    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(""),
        Line::from(vec![Span::styled("  Navigation", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
        Line::from("  j / ↓    Move cursor down"),
        Line::from("  k / ↑    Move cursor up"),
        Line::from("  Tab      Switch focus between Profiles and Logs"),
        Line::from(""),
        Line::from(vec![Span::styled("  Actions", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
        Line::from("  c / Enter Connect to selected profile"),
        Line::from("  d         Disconnect active connection"),
        Line::from("  r         Reconnect selected profile"),
        Line::from("  v         View Configuration Path & Content"),
        Line::from("  y         Yank (Copy) config to clipboard"),
        Line::from("  a         Add New Configuration (Import)"),
        Line::from("  i         Refresh public IP"),
        Line::from(""),
        Line::from(vec![Span::styled("  System", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))]),
        Line::from("  s            Enter sudo password"),
        Line::from("  ?            Toggle this help"),
        Line::from("  Ctrl+C ×2   Disconnect VPN and exit"),
        Line::from("  Esc          Close help / Modals"),
    ];

    let help_block = Block::default()
        .title(" tuneli-tui Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let help_para = Paragraph::new(help_text).block(help_block);
    f.render_widget(help_para, area);
}

fn draw_config_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 70, f.area());
    f.render_widget(Clear, area);

    let path = app.config_path.as_deref().unwrap_or("Unknown path");
    let content = app.config_content.as_deref().unwrap_or("Loading configuration...");

    let modal_block = Block::default()
        .title(format!(" Configuration (y: Yank): {} ", path))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightBlue));

    let para = Paragraph::new(content).block(modal_block);
    f.render_widget(para, area);
}

fn draw_add_config_modal(f: &mut Frame, app: &mut App) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);

    let main_block = Block::default()
        .title(" Add New VPN Configuration ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    
    let inner_area = main_block.inner(area);
    f.render_widget(main_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Name
            Constraint::Length(3), // Protocol
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Save button
        ].as_ref())
        .split(inner_area);

    // Name
    let name_style = if app.add_config_state.focused_field == 0 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Gray) };
    let name_input = Paragraph::new(app.add_config_state.name.as_str())
        .block(Block::default().title(" Profile Name ").borders(Borders::ALL).border_style(name_style));
    f.render_widget(name_input, chunks[0]);
    if app.add_config_state.focused_field == 0 {
        let name_inner = chunks[0].inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });
        let nx = app.add_config_state.name_cursor as u16;
        f.set_cursor_position(ratatui::layout::Position {
            x: name_inner.x + std::cmp::min(nx, name_inner.width.saturating_sub(1)),
            y: name_inner.y,
        });
    }

    // Protocol
    let proto_style = if app.add_config_state.focused_field == 1 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Gray) };
    let wg_style = if app.add_config_state.protocol == crate::ui::app::ProtocolType::WireGuard { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };
    let ov_style = if app.add_config_state.protocol == crate::ui::app::ProtocolType::OpenVpn { Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) };

    let proto_line = Line::from(vec![
        Span::styled(" [ WireGuard ] ", wg_style),
        Span::raw("    "),
        Span::styled(" [ OpenVPN ] ", ov_style),
        Span::raw("   (Space/Arrows to toggle)"),
    ]);
    let proto_input = Paragraph::new(proto_line)
        .block(Block::default().title(" Protocol ").borders(Borders::ALL).border_style(proto_style));
    f.render_widget(proto_input, chunks[1]);

    // Content
    let content_style = if app.add_config_state.focused_field == 2 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::Gray) };

    let content_inner = chunks[2].inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });
    if app.add_config_state.focused_field == 2 {
        let cx = app.add_config_state.content_cursor.0 as u16;
        let cy = app.add_config_state.content_cursor.1 as u16;

        let mut scroll_v = app.add_config_state.content_scroll.0;
        let mut scroll_h = app.add_config_state.content_scroll.1;

        if cy < scroll_v {
            scroll_v = cy;
        } else if cy >= scroll_v + content_inner.height {
            scroll_v = cy.saturating_sub(content_inner.height).saturating_add(1);
        }

        if cx < scroll_h {
            scroll_h = cx;
        } else if cx >= scroll_h + content_inner.width {
            scroll_h = cx.saturating_sub(content_inner.width).saturating_add(1);
        }

        app.add_config_state.content_scroll = (scroll_v, scroll_h);
        
        f.set_cursor_position(ratatui::layout::Position {
            x: content_inner.x + cx.saturating_sub(scroll_h),
            y: content_inner.y + cy.saturating_sub(scroll_v),
        });
    }

    let content_input = Paragraph::new(app.add_config_state.get_content_string())
        .block(Block::default().title(" Configuration Content (Paste here) ").borders(Borders::ALL).border_style(content_style))
        .scroll((app.add_config_state.content_scroll.0, app.add_config_state.content_scroll.1));
    f.render_widget(content_input, chunks[2]);

    // Save Button
    let save_style = if app.add_config_state.focused_field == 3 {
        Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let save_button = Paragraph::new(" [ SAVE CONFIGURATION ] ")
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(save_style));
    f.render_widget(save_button, chunks[3]);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);

    let profile_name = app.profile_to_delete.as_ref().map(|p| p.name.as_str()).unwrap_or("Unknown");
    
    let modal_block = Block::default()
        .title(" Confirm Deletion ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD));

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw(" Are you sure you want to delete "),
            Span::styled(profile_name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(" [Y/Enter] Confirm  |  [N/Esc] Cancel "),
    ];

    let para = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .block(modal_block);

    f.render_widget(para, area);
}
