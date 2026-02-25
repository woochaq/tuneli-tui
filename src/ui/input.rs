use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use crate::ui::add_config::FocusedPanel;
use crate::ui::app::App;

pub async fn handle_event(app: &mut App, ev: Event) -> bool {
    let mut request_redraw = false;

    if let Event::Key(key) = ev {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        if app.show_add_config_modal {
            match key.code {
                KeyCode::Esc => app.show_add_config_modal = false,
                KeyCode::Tab => {
                    app.add_config_state.focused_field = (app.add_config_state.focused_field + 1) % 4;
                }
                KeyCode::BackTab => {
                    app.add_config_state.focused_field = (app.add_config_state.focused_field + 3) % 4;
                }
                KeyCode::Char(c) => match app.add_config_state.focused_field {
                    0 | 2 => app.add_config_state.insert_char(c),
                    1 => {
                         if c == ' ' || c == 'p' {
                             app.add_config_state.toggle_protocol();
                         }
                    }
                    _ => {}
                },
                KeyCode::Backspace => app.add_config_state.delete_back(),
                KeyCode::Enter => {
                    if app.add_config_state.focused_field == 3 {
                        let _ = app.save_new_config().await;
                    } else if app.add_config_state.focused_field == 2 {
                         app.add_config_state.insert_char('\n');
                    }
                }
                KeyCode::Left => {
                    if app.add_config_state.focused_field == 1 {
                        app.add_config_state.toggle_protocol();
                    } else {
                        app.add_config_state.move_cursor(-1, 0);
                    }
                }
                KeyCode::Right => {
                    if app.add_config_state.focused_field == 1 {
                        app.add_config_state.toggle_protocol();
                    } else {
                        app.add_config_state.move_cursor(1, 0);
                    }
                }
                KeyCode::Up => app.add_config_state.move_cursor(0, -1),
                KeyCode::Down => app.add_config_state.move_cursor(0, 1),
                _ => {}
            }
            return true;
        }

        if app.show_delete_modal {
            match key.code {
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    app.show_delete_modal = false;
                    app.profile_to_delete = None;
                }
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(profile) = app.profile_to_delete.take() {
                        let _ = app.delete_profile(profile).await;
                    }
                    app.show_delete_modal = false;
                }
                _ => {}
            }
            return true;
        }

        // Ctrl+C
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if app.quit_pending {
                app.should_quit = true;
            } else {
                app.quit_pending = true;
                app.quit_pending_time = Some(std::time::Instant::now());
                app.status_message = Some("Press Ctrl+C again within 3s to exit…".to_string());
            }
            return true;
        }

        if app.sudo_prompt.is_active {
            match key.code {
                KeyCode::Char(c) => app.sudo_prompt.input.push(c),
                KeyCode::Backspace => { app.sudo_prompt.input.pop(); },
                KeyCode::Esc => app.sudo_prompt.is_active = false,
                KeyCode::Enter => {
                    if !app.sudo_prompt.input.is_empty() {
                        let text = app.sudo_prompt.input.clone();
                        
                        // Check what type of password this is by examining the prompt message
                        let is_auth = app.sudo_prompt.error_msg.as_deref().unwrap_or("").contains("Auth [user:pass]");
                        let is_private_key = app.sudo_prompt.error_msg.as_deref().unwrap_or("").contains("Private Key");
                        
                        if is_auth {
                            if text.contains(':') && app.openvpn_cmd_tx.is_some() {
                                let parts: Vec<&str> = text.splitn(2, ':').collect();
                                if parts.len() == 2 {
                                    let user_cmd = format!("username 'Auth' \"{}\"", parts[0]);
                                    let pass_cmd = format!("password 'Auth' \"{}\"", parts[1]);
                                    
                                    if let Some(ref tx) = app.openvpn_cmd_tx {
                                        let _ = tx.try_send(user_cmd);
                                        let _ = tx.try_send(pass_cmd);
                                        app.log_lines.push("[tuneli-tui] Sent Auth credentials to OpenVPN management.".to_string());
                                    }
                                }
                            }
                            app.sudo_prompt.is_active = false;
                            app.sudo_prompt.error_msg = None;
                            app.sudo_prompt.input.clear();
                        } else if is_private_key {
                            if let Some(ref tx) = app.openvpn_cmd_tx {
                                let _ = tx.try_send(format!("password 'Private Key' \"{}\"", text));
                                app.log_lines.push("[tuneli-tui] Sent Private Key password to OpenVPN management.".to_string());
                            }
                            app.sudo_prompt.is_active = false;
                            app.sudo_prompt.error_msg = None;
                            app.sudo_prompt.input.clear();
                        } else {
                            // Standard Sudo Password Authentication
                            app.sudo_prompt.is_verifying = true;
                            let is_valid = crate::engine::runner::SudoRunner::validate_password(&text).await;
                            if is_valid {
                                crate::engine::runner::SudoRunner::set_password(text);
                                app.sudo_prompt.is_active = false;
                                app.sudo_prompt.error_msg = None;
                                app.refresh_profiles().await;
                            } else {
                                app.sudo_prompt.error_msg = Some("Incorrect sudo password. Try again.".to_string());
                                app.sudo_prompt.input.clear();
                            }
                            app.sudo_prompt.is_verifying = false;
                        }
                    } else {
                        app.sudo_prompt.is_active = false;
                    }
                }
                _ => {}
            }
            return true;
        }

        match key.code {
            KeyCode::Esc => {
                if app.show_help { app.show_help = false; }
                else if app.show_config_modal { app.show_config_modal = false; }
                else if app.show_add_config_modal { app.show_add_config_modal = false; }
                else if app.show_delete_modal { app.show_delete_modal = false; }
            }
            KeyCode::Tab => {
                app.focused_panel = match app.focused_panel {
                    FocusedPanel::Profiles => FocusedPanel::Logs,
                    FocusedPanel::Logs => FocusedPanel::Profiles,
                };
            }
            KeyCode::Char('?') => app.show_help = !app.show_help,
            KeyCode::Char('v') => {
                if let Some(idx) = app.list_state.selected() {
                    if let Some(profile) = app.profiles.get(idx) {
                        app.config_path = Some(profile.path.clone());
                        app.show_config_modal = true;
                        app.load_config_for_selected().await;
                    }
                }
            }
            KeyCode::Char('y') => {
                if app.show_config_modal {
                    app.copy_config_to_clipboard();
                }
            }
            KeyCode::Char('s') => {
                app.sudo_prompt.input.clear();
                app.sudo_prompt.error_msg = None;
                app.sudo_prompt.is_active = true;
            },
            KeyCode::Char('U') => {
                app.trigger_update();
            },
            KeyCode::Char('a') => {
                app.show_add_config_modal = true;
                app.add_config_state = crate::ui::add_config::AddConfigState::new();
            },
            KeyCode::Down | KeyCode::Char('j') => {
                if app.focused_panel == FocusedPanel::Profiles {
                    app.next_profile().await;
                } else if app.focused_panel == FocusedPanel::Logs {
                    app.log_scroll_offset = app.log_scroll_offset.saturating_sub(1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if app.focused_panel == FocusedPanel::Profiles {
                    app.previous_profile().await;
                } else if app.focused_panel == FocusedPanel::Logs {
                    app.log_scroll_offset = app.log_scroll_offset.saturating_add(1);
                    let max_scroll = app.log_lines.len().saturating_sub(1) as u16;
                    if app.log_scroll_offset > max_scroll {
                        app.log_scroll_offset = max_scroll;
                    }
                }
            }
            KeyCode::Right | KeyCode::Char('l') => app.focused_panel = FocusedPanel::Logs,
            KeyCode::Left | KeyCode::Char('h') => app.focused_panel = FocusedPanel::Profiles,
            KeyCode::PageUp => {
                app.log_scroll_offset = app.log_scroll_offset.saturating_add(10);
                let max_scroll = app.log_lines.len().saturating_sub(1) as u16;
                if app.log_scroll_offset > max_scroll {
                    app.log_scroll_offset = max_scroll;
                }
            }
            KeyCode::PageDown => {
                app.log_scroll_offset = app.log_scroll_offset.saturating_sub(10);
            }
            KeyCode::Char('c') | KeyCode::Enter => {
                if app.focused_panel == FocusedPanel::Profiles {
                    if let Some(selected) = app.list_state.selected() {
                        if let Some(profile) = app.profiles.get(selected).cloned() {
                            app.connect_profile(profile).await;
                        }
                    }
                } else if app.focused_panel == FocusedPanel::Logs {
                    app.log_scroll_offset = 0;
                }
            }
            KeyCode::Char('d') => {
                app.disconnect_selected().await;
            }
            KeyCode::Delete | KeyCode::Char('x') => {
                if let Some(idx) = app.list_state.selected() {
                    if let Some(profile) = app.profiles.get(idx).cloned() {
                        app.profile_to_delete = Some(profile);
                        app.show_delete_modal = true;
                    }
                }
            }
            KeyCode::Char('r') => app.reconnect_selected().await,
            KeyCode::Char('i') => {
                app.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(60);
                app.status_message = Some("Refreshing public IP...".to_string());
            }
            _ => {}
        }
        request_redraw = true;
    } else if let Event::Mouse(mouse_event) = ev {
        let terminal_width = crossterm::terminal::size().unwrap_or((80, 24)).0;
        let is_over_logs = mouse_event.column > (terminal_width / 2);

        match mouse_event.kind {
            crossterm::event::MouseEventKind::ScrollDown => {
                if app.show_add_config_modal && app.add_config_state.focused_field == 2 {
                    app.add_config_state.move_cursor(0, 1);
                } else if !app.show_add_config_modal && !app.show_config_modal && !app.show_help {
                    if is_over_logs {
                        app.log_scroll_offset = app.log_scroll_offset.saturating_sub(3);
                    } else {
                        app.next_profile().await;
                    }
                }
            }
            crossterm::event::MouseEventKind::ScrollUp => {
                if app.show_add_config_modal && app.add_config_state.focused_field == 2 {
                    app.add_config_state.move_cursor(0, -1);
                } else if !app.show_add_config_modal && !app.show_config_modal && !app.show_help {
                    if is_over_logs {
                        app.log_scroll_offset = app.log_scroll_offset.saturating_add(3);
                        let max_scroll = app.log_lines.len().saturating_sub(1) as u16;
                        if app.log_scroll_offset > max_scroll {
                            app.log_scroll_offset = max_scroll;
                        }
                    } else {
                        app.previous_profile().await;
                    }
                }
            }
            _ => {}
        }
        request_redraw = true;
    } else if let Event::Paste(text) = ev {
        if app.show_add_config_modal {
            app.add_config_state.paste(&text);
            request_redraw = true;
        }
    }

    request_redraw
}
