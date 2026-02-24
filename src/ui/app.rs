use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use crate::ui::sudo_prompt::SudoPrompt;
use crate::models::{VpnProfile, ProtocolConfig};
use ratatui::widgets::ListState;
use std::io::{stdout, Result};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedPanel {
    Profiles,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProtocolType {
    WireGuard,
    OpenVpn,
}

pub struct AddConfigState {
    pub name: String,
    pub name_cursor: usize,
    pub protocol: ProtocolType,
    pub content: Vec<String>,
    pub content_cursor: (usize, usize), // (col, row)
    pub content_scroll: (u16, u16),     // (vertical, horizontal)
    pub focused_field: usize, // 0: Name, 1: Protocol, 2: Content, 3: Save
}

impl AddConfigState {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            name_cursor: 0,
            protocol: ProtocolType::WireGuard,
            content: vec![String::new()],
            content_cursor: (0, 0),
            content_scroll: (0, 0),
            focused_field: 0,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if self.focused_field == 0 {
            let max_len = match self.protocol {
                ProtocolType::WireGuard => 15,
                ProtocolType::OpenVpn => 50,
            };
            if self.name.chars().count() < max_len {
                let idx = self.name.char_indices().nth(self.name_cursor).map(|(i, _)| i).unwrap_or(self.name.len());
                self.name.insert(idx, c);
                self.name_cursor += 1;
            }
        } else if self.focused_field == 2 {
            if c == '\n' {
                let row = self.content_cursor.1;
                let col = self.content_cursor.0;
                let line = &mut self.content[row];
                let idx = line.char_indices().nth(col).map(|(i, _)| i).unwrap_or(line.len());
                let remainder = line.split_off(idx);
                self.content.insert(row + 1, remainder);
                self.content_cursor.1 += 1;
                self.content_cursor.0 = 0;
            } else {
                let row = self.content_cursor.1;
                let col = self.content_cursor.0;
                let line = &mut self.content[row];
                let idx = line.char_indices().nth(col).map(|(i, _)| i).unwrap_or(line.len());
                line.insert(idx, c);
                self.content_cursor.0 += 1;
            }
        }
    }

    pub fn delete_back(&mut self) {
        if self.focused_field == 0 {
            if self.name_cursor > 0 {
                self.name_cursor -= 1;
                let idx = self.name.char_indices().nth(self.name_cursor).map(|(i, _)| i).unwrap_or(self.name.len());
                self.name.remove(idx);
            }
        } else if self.focused_field == 2 {
            let row = self.content_cursor.1;
            let col = self.content_cursor.0;
            if col > 0 {
                self.content_cursor.0 -= 1;
                let line = &mut self.content[row];
                let idx = line.char_indices().nth(self.content_cursor.0).map(|(i, _)| i).unwrap_or(line.len());
                line.remove(idx);
            } else if row > 0 {
                let current_line = self.content.remove(row);
                self.content_cursor.1 -= 1;
                self.content_cursor.0 = self.content[row - 1].chars().count();
                self.content[row - 1].push_str(&current_line);
            }
        }
    }

    pub fn paste(&mut self, text: &str) {
        for c in text.chars() {
            if c != '\r' {
                self.insert_char(c);
            }
        }
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        if self.focused_field == 0 {
            if dx < 0 && self.name_cursor > 0 { self.name_cursor -= 1; }
            if dx > 0 && self.name_cursor < self.name.chars().count() { self.name_cursor += 1; }
        } else if self.focused_field == 2 {
            let mut row = self.content_cursor.1 as isize + dy;
            if row < 0 { row = 0; }
            if row >= self.content.len() as isize { row = self.content.len() as isize - 1; }
            self.content_cursor.1 = row as usize;
            
            let line_len = self.content[self.content_cursor.1].chars().count() as isize;
            self.content_cursor.0 = std::cmp::min(self.content_cursor.0, line_len as usize);
            
            let col = self.content_cursor.0 as isize + dx;
            if col < 0 {
                if dx < 0 && self.content_cursor.1 > 0 {
                    self.content_cursor.1 -= 1;
                    self.content_cursor.0 = self.content[self.content_cursor.1].chars().count();
                } else {
                    self.content_cursor.0 = 0;
                }
            } else if col > line_len {
                if dx > 0 && self.content_cursor.1 < self.content.len() - 1 {
                    self.content_cursor.1 += 1;
                    self.content_cursor.0 = 0;
                } else {
                    self.content_cursor.0 = line_len as usize;
                }
            } else {
                self.content_cursor.0 = col as usize;
            }
        }
    }

    pub fn get_content_string(&self) -> String {
        self.content.join("\n")
    }

    pub fn toggle_protocol(&mut self) {
        self.protocol = match self.protocol {
            ProtocolType::WireGuard => ProtocolType::OpenVpn,
            ProtocolType::OpenVpn => {
                if self.name.chars().count() > 15 {
                    self.name = self.name.chars().take(15).collect();
                    self.name_cursor = std::cmp::min(self.name_cursor, 15);
                }
                ProtocolType::WireGuard
            },
        };
    }
}

/// Returns true if the process is running with effective UID 0 (root/sudo).
fn is_running_as_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .map(|o| o.stdout.trim_ascii() == b"0")
        .unwrap_or(false)
}

pub struct App {
    pub should_quit: bool,
    pub quit_pending: bool,
    pub quit_pending_time: Option<std::time::Instant>,
    pub sudo_prompt: SudoPrompt,
    pub profiles: Vec<VpnProfile>,
    pub list_state: ListState,
    pub active_profile: Option<VpnProfile>,
    pub status_message: Option<String>,
    pub log_lines: Vec<String>,
    pub config_content: Option<String>,
    pub last_refresh: std::time::Instant,
    pub last_geo_refresh: std::time::Instant,
    pub geo_info: Option<crate::telemetry::geo::GeoInfo>,
    pub geo_fetch_handle: Option<tokio::task::JoinHandle<Option<crate::telemetry::geo::GeoInfo>>>,
    pub show_help: bool,
    
    // New fields for Phase 6
    pub focused_panel: FocusedPanel,
    pub throughput_history: std::collections::VecDeque<(f64, f64)>, // (rx, tx) bytes per second
    pub last_net_stats: Option<crate::telemetry::network::NetStats>,
    pub last_throughput_update: std::time::Instant,
    pub show_config_modal: bool,
    pub config_path: Option<String>,
    pub clipboard: Option<arboard::Clipboard>,
    pub show_add_config_modal: bool,
    pub add_config_state: AddConfigState,
    
    // New fields for Phase 7 (UX/Features)
    pub show_delete_modal: bool,
    pub profile_to_delete: Option<VpnProfile>,
    pub ping: Option<std::time::Duration>,
    pub last_ping_refresh: std::time::Instant,
    pub ping_fetch_handle: Option<tokio::task::JoinHandle<Option<std::time::Duration>>>,
}

impl App {
    pub async fn new() -> Self {
        let root = is_running_as_root();
        let mut app = Self { 
            should_quit: false,
            quit_pending: false,
            quit_pending_time: None,
            sudo_prompt: SudoPrompt::new(),
            profiles: vec![],
            list_state: ListState::default(),
            active_profile: None,
            status_message: None,
            log_lines: vec!["[tuneli-tui] Ready.".to_string()],
            config_content: None,
            last_refresh: std::time::Instant::now(),
            last_geo_refresh: std::time::Instant::now() - std::time::Duration::from_secs(60),
            geo_info: None,
            geo_fetch_handle: None,
            show_help: false,
            focused_panel: FocusedPanel::Profiles,
            throughput_history: std::collections::VecDeque::with_capacity(100),
            last_net_stats: None,
            last_throughput_update: std::time::Instant::now(),
            show_config_modal: false,
            config_path: None,
            clipboard: arboard::Clipboard::new().ok(),
            show_add_config_modal: false,
            add_config_state: AddConfigState::new(),
            show_delete_modal: false,
            profile_to_delete: None,
            ping: None,
            last_ping_refresh: std::time::Instant::now() - std::time::Duration::from_secs(60),
            ping_fetch_handle: None,
        };

        if root {
            // Running as root — set a sentinel so SudoRunner passes an empty password
            crate::engine::runner::SudoRunner::set_password(String::new());
            app.log_lines.push("[tuneli-tui] Running as root — no sudo password needed.".to_string());
            app.refresh_profiles().await;
        } else if crate::engine::runner::SudoRunner::get_password().is_none() {
            app.sudo_prompt.is_active = true;
            app.sudo_prompt.error_msg = Some("Please enter sudo password to load profiles.".to_string());
        } else {
            app.refresh_profiles().await;
        }
        app
    }

    pub async fn refresh_profiles(&mut self) {
        if let Ok(profiles) = crate::engine::discovery::list_all_profiles().await {
            self.profiles = profiles;

            // Reconcile active_profile with real system state (WireGuard only).
            // Never overwrite an active OpenVPN connection — openvpn runs as a daemon
            // and won't show up in `wg show interfaces`.
            let active_is_ovpn = matches!(
                self.active_profile.as_ref().map(|p| &p.protocol),
                Some(crate::models::ProtocolConfig::OpenVpn { .. })
            );
            if !active_is_ovpn {
                if let Some(pwd) = crate::engine::runner::SudoRunner::get_password() {
                    let active_ifaces = crate::engine::runner::SudoRunner::get_active_wg_interfaces(&pwd).await;

                    let newly_active = self.profiles.iter().find(|p| {
                        active_ifaces.iter().any(|iface| iface == &p.name)
                    }).cloned();

                    if self.active_profile.as_ref().map(|p| &p.name) != newly_active.as_ref().map(|p| &p.name) {
                        self.active_profile = newly_active;
                    }
                }
            }

            // Adjust list_state selection if out of bounds
            if let Some(i) = self.list_state.selected() {
                if self.profiles.is_empty() {
                    self.list_state.select(None);
                } else if i >= self.profiles.len() {
                    self.list_state.select(Some(self.profiles.len() - 1));
                }
            } else if !self.profiles.is_empty() {
                self.list_state.select(Some(0));
            }
        }
    }

    pub async fn load_config_for_selected(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            if let Some(profile) = self.profiles.get(selected).cloned() {
                let password = crate::engine::runner::SudoRunner::get_password();
                
                if let Some(pwd) = password {
                    if let Ok(content) = crate::engine::runner::SudoRunner::run_with_sudo(
                        &pwd,
                        "cat",
                        &[&profile.path]
                    ).await {
                        self.config_content = Some(content);
                        return;
                    }
                }
                
                // Fallback if no sudo password or somehow it failed
                if let Ok(content) = std::fs::read_to_string(&profile.path) {
                    self.config_content = Some(content);
                } else {
                    self.config_content = Some("Permission denied or file unreadable. Sudo required.".to_string());
                }
            }
        }
    }

    pub async fn next_profile(&mut self) {
        if self.profiles.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.profiles.len() - 1 { 0 } else { i + 1 }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.load_config_for_selected().await;
    }

    pub async fn previous_profile(&mut self) {
        if self.profiles.is_empty() { return; }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 { self.profiles.len() - 1 } else { i - 1 }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
        self.load_config_for_selected().await;
    }

    pub async fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;
        stdout().execute(crossterm::event::EnableBracketedPaste)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        stdout().execute(crossterm::event::EnableMouseCapture)?;

        while !self.should_quit {
            // Expire "Press Ctrl+C again to exit" after 3 seconds
            if let Some(t) = self.quit_pending_time {
                if t.elapsed().as_secs() >= 3 {
                    self.quit_pending = false;
                    self.quit_pending_time = None;
                    self.status_message = None;
                }
            }

            if self.last_refresh.elapsed().as_secs() >= 3 {
                self.last_refresh = std::time::Instant::now();
                self.refresh_profiles().await;
            }

            // Non-blocking geo fetch — spawn in background and poll each tick
            if let Some(handle) = self.geo_fetch_handle.take_if(|h| h.is_finished()) {
                if let Ok(Some(geo)) = handle.await {
                    self.log_lines.push(format!("[geo] IP: {}", geo.public_ip));
                    self.geo_info = Some(geo);
                    if self.status_message.as_deref() == Some("Refreshing public IP...") {
                        self.status_message = None;
                    }
                }
            }
            if self.geo_fetch_handle.is_none() && self.last_geo_refresh.elapsed().as_secs() >= 30 {
                self.last_geo_refresh = std::time::Instant::now();
                self.geo_fetch_handle = Some(tokio::spawn(crate::telemetry::geo::fetch_geo_info()));
            }

            // Continuous Ping background task (every 5 seconds)
            if let Some(handle) = self.ping_fetch_handle.take_if(|h| h.is_finished()) {
                if let Ok(ping_result) = handle.await {
                    self.ping = ping_result;
                    if let Some(p) = ping_result {
                        self.log_lines.push(format!("[ping] Latency to 1.1.1.1 is {}ms", p.as_millis()));
                    }
                }
            }
            if self.ping_fetch_handle.is_none() && self.last_ping_refresh.elapsed().as_secs() >= 5 {
                self.last_ping_refresh = std::time::Instant::now();
                self.ping_fetch_handle = Some(tokio::spawn(crate::telemetry::ping::measure_latency("1.1.1.1")));
            }

            // Network throughput monitoring
            if self.last_throughput_update.elapsed().as_millis() >= 1000 {
                let current_stats = crate::telemetry::network::get_net_stats();
                if let (Some(prev), Some(curr)) = (self.last_net_stats, current_stats) {
                    let throughput = crate::telemetry::network::calculate_throughput(&prev, &curr);
                    self.throughput_history.push_back((throughput.rx_bps, throughput.tx_bps));
                    if self.throughput_history.len() > 100 {
                        self.throughput_history.pop_front();
                    }
                }
                self.last_net_stats = current_stats;
                self.last_throughput_update = std::time::Instant::now();
            }

            terminal.draw(|f| {
                crate::ui::layout::draw(f, self);
            })?;

            while event::poll(std::time::Duration::from_millis(0))? {
                let ev = event::read()?;
                if let Event::Key(key) = ev {
                    if key.kind == KeyEventKind::Press {
                        if self.show_add_config_modal {
                            match key.code {
                                KeyCode::Esc => self.show_add_config_modal = false,
                                KeyCode::Tab => {
                                    self.add_config_state.focused_field = (self.add_config_state.focused_field + 1) % 4;
                                }
                                KeyCode::BackTab => {
                                    self.add_config_state.focused_field = (self.add_config_state.focused_field + 3) % 4;
                                }
                                KeyCode::Char(c) => match self.add_config_state.focused_field {
                                    0 | 2 => self.add_config_state.insert_char(c),
                                    1 => {
                                         if c == ' ' || c == 'p' {
                                             self.add_config_state.toggle_protocol();
                                         }
                                    }
                                    _ => {}
                                },
                                KeyCode::Backspace => self.add_config_state.delete_back(),
                                KeyCode::Enter => {
                                    if self.add_config_state.focused_field == 3 {
                                        self.save_new_config().await.ok();
                                    } else if self.add_config_state.focused_field == 2 {
                                         self.add_config_state.insert_char('\n');
                                    }
                                }
                                KeyCode::Left => {
                                    if self.add_config_state.focused_field == 1 {
                                        self.add_config_state.toggle_protocol();
                                    } else {
                                        self.add_config_state.move_cursor(-1, 0);
                                    }
                                }
                                KeyCode::Right => {
                                    if self.add_config_state.focused_field == 1 {
                                        self.add_config_state.toggle_protocol();
                                    } else {
                                        self.add_config_state.move_cursor(1, 0);
                                    }
                                }
                                KeyCode::Up => self.add_config_state.move_cursor(0, -1),
                                KeyCode::Down => self.add_config_state.move_cursor(0, 1),
                                _ => {}
                            }
                            continue;
                        }

                        if self.show_delete_modal {
                            match key.code {
                                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                                    self.show_delete_modal = false;
                                    self.profile_to_delete = None;
                                }
                                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                                    if let Some(profile) = self.profile_to_delete.take() {
                                        self.delete_profile(profile).await.ok();
                                    }
                                    self.show_delete_modal = false;
                                }
                                _ => {}
                            }
                            continue;
                        }

                        // Ctrl+C — always handled, even inside sudo prompt
                        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                            if self.quit_pending {
                                self.should_quit = true;
                            } else {
                                self.quit_pending = true;
                                self.quit_pending_time = Some(std::time::Instant::now());
                                self.status_message = Some("Press Ctrl+C again within 3s to exit…".to_string());
                            }
                            continue;
                        }

                        if self.sudo_prompt.is_active {
                            match key.code {
                                KeyCode::Char(c) => self.sudo_prompt.input.push(c),
                                KeyCode::Backspace => { self.sudo_prompt.input.pop(); },
                                KeyCode::Esc => self.sudo_prompt.is_active = false,
                                KeyCode::Enter => {
                                    if !self.sudo_prompt.input.is_empty() {
                                        let pwd = self.sudo_prompt.input.clone();
                                        self.sudo_prompt.is_verifying = true;
                                        terminal.draw(|f| { crate::ui::layout::draw(f, self); }).ok();
                                        if crate::engine::runner::SudoRunner::validate_password(&pwd).await {
                                            crate::engine::runner::SudoRunner::set_password(pwd);
                                            self.sudo_prompt.is_active = false;
                                            self.sudo_prompt.error_msg = None;
                                            self.refresh_profiles().await;
                                        } else {
                                            self.sudo_prompt.error_msg = Some("Incorrect sudo password. Try again.".to_string());
                                            self.sudo_prompt.input.clear();
                                        }
                                        self.sudo_prompt.is_verifying = false;
                                    } else {
                                        self.sudo_prompt.is_active = false;
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Esc => {
                                    if self.show_help {
                                        self.show_help = false;
                                    } else if self.show_config_modal {
                                        self.show_config_modal = false;
                                    } else if self.show_add_config_modal {
                                        self.show_add_config_modal = false;
                                    } else if self.show_delete_modal {
                                        self.show_delete_modal = false;
                                    }
                                }
                                KeyCode::Tab => {
                                    self.focused_panel = match self.focused_panel {
                                        FocusedPanel::Profiles => FocusedPanel::Logs,
                                        FocusedPanel::Logs => FocusedPanel::Profiles,
                                    };
                                }
                                KeyCode::Char('?') => self.show_help = !self.show_help,
                                KeyCode::Char('v') => {
                                    if let Some(idx) = self.list_state.selected() {
                                        if let Some(profile) = self.profiles.get(idx) {
                                            self.config_path = Some(profile.path.clone());
                                            self.show_config_modal = true;
                                            // Ensure config is loaded
                                            self.load_config_for_selected().await;
                                        }
                                    }
                                }
                                KeyCode::Char('y') => {
                                    if self.show_config_modal {
                                        self.copy_config_to_clipboard();
                                    }
                                }
                                KeyCode::Char('s') => {
                                    self.sudo_prompt.input.clear();
                                    self.sudo_prompt.error_msg = None;
                                    self.sudo_prompt.is_active = true;
                                },
                                KeyCode::Char('a') => {
                                    self.show_add_config_modal = true;
                                    self.add_config_state = AddConfigState::new();
                                },
                                KeyCode::Down | KeyCode::Char('j') => self.next_profile().await,
                                KeyCode::Up | KeyCode::Char('k') => self.previous_profile().await,
                                KeyCode::Char('c') | KeyCode::Enter => {
                                    if let Some(selected) = self.list_state.selected() {
                                        if let Some(profile) = self.profiles.get(selected).cloned() {
                                            self.connect_profile(profile).await;
                                        }
                                    }
                                }
                                KeyCode::Char('d') => {
                                    self.disconnect_active().await;
                                }
                                KeyCode::Delete | KeyCode::Char('x') => {
                                    if let Some(idx) = self.list_state.selected() {
                                        if let Some(profile) = self.profiles.get(idx) {
                                            self.profile_to_delete = Some(profile.clone());
                                            self.show_delete_modal = true;
                                        }
                                    }
                                }
                                KeyCode::Char('r') => {
                                    self.reconnect_selected().await;
                                }
                                KeyCode::Char('i') => {
                                    // Trigger IP refresh in background
                                    self.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(60);
                                    self.status_message = Some("Refreshing public IP...".to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                } else if let Event::Mouse(mouse_event) = ev {
                    match mouse_event.kind {
                        event::MouseEventKind::ScrollDown => {
                            if self.show_add_config_modal && self.add_config_state.focused_field == 2 {
                                self.add_config_state.move_cursor(0, 1);
                            } else if !self.show_add_config_modal && !self.show_config_modal && !self.show_help {
                                if self.focused_panel == FocusedPanel::Profiles {
                                    self.next_profile().await;
                                }
                            }
                        }
                        event::MouseEventKind::ScrollUp => {
                            if self.show_add_config_modal && self.add_config_state.focused_field == 2 {
                                self.add_config_state.move_cursor(0, -1);
                            } else if !self.show_add_config_modal && !self.show_config_modal && !self.show_help {
                                if self.focused_panel == FocusedPanel::Profiles {
                                    self.previous_profile().await;
                                }
                            }
                        }
                        _ => {}
                    }
                } else if let Event::Paste(text) = ev {
                    if self.show_add_config_modal {
                        self.add_config_state.paste(&text);
                    }
                }
            }
            
            // Limit FPS slightly to avoid 100% CPU but stay responsive
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // --- Cleanup on exit ---
        stdout().execute(crossterm::event::DisableMouseCapture).ok();
        stdout().execute(crossterm::event::DisableBracketedPaste).ok();
        self.status_message = Some("Disconnecting VPN before exit…".to_string());
        terminal.draw(|f| { crate::ui::layout::draw(f, self); }).ok();
        self.disconnect_active().await;

        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;
        Ok(())
    }

    async fn connect_profile(&mut self, profile: VpnProfile) {
        let password = match crate::engine::runner::SudoRunner::get_password() {
            Some(p) => p,
            None => {
                self.sudo_prompt.is_active = true;
                self.sudo_prompt.error_msg = Some("Sudo password required.".to_string());
                return;
            }
        };

        // Handle disconnect if there's an active profile
        if let Some(active) = self.active_profile.clone() {
            let _ = crate::engine::firewall::Firewall::disable_killswitch(&password).await;
            
            if let ProtocolConfig::WireGuard { .. } = &active.protocol {
                let cmd_str = format!("sudo wg-quick down {}", active.name);
                self.log_lines.push(format!("[cmd] {}", cmd_str));
                self.status_message = Some(format!("Disconnecting {}...", active.name));
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password,
                    "wg-quick",
                    &["down", &active.name]
                ).await;
            } else if let ProtocolConfig::OpenVpn { .. } = &active.protocol {
                self.log_lines.push("[cmd] sudo killall -9 openvpn".to_string());
                self.status_message = Some(format!("Disconnecting OpenVPN {}...", active.name));
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password, "killall", &["-9", "openvpn"]
                ).await;
            }

            self.active_profile = None;
            self.status_message = Some(format!("Disconnected {}", active.name));

            // If we just clicked the currently active profile, toggle it off and return
            if active.name == profile.name {
                // Refresh IP
                self.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(60);
                return;
            }
        }

        if let ProtocolConfig::WireGuard { .. } = &profile.protocol {
            let cmd_str = format!("sudo wg-quick up {}", profile.name);
            self.log_lines.push(format!("[cmd] {}", cmd_str));
            self.status_message = Some(format!("Connecting {}...", profile.name));
            match crate::engine::runner::SudoRunner::run_with_sudo(
                &password, 
                "wg-quick", 
                &["up", &profile.name]
            ).await {
                Ok(_) => {
                    self.active_profile = Some(profile.clone());
                    self.status_message = Some(format!("Connected to {}. Enabling killswitch...", profile.name));
                    self.log_lines.push(format!("[tuneli-tui] Connected to {}", profile.name));
                    
                    if let Err(ks_err) = crate::engine::firewall::Firewall::enable_killswitch(&password, &profile.name).await {
                        self.status_message = Some(format!("KS Failed: {}", ks_err));
                        self.log_lines.push(format!("[ERROR] Killswitch failed: {}", ks_err));
                    } else {
                        self.status_message = Some(format!("Connected to {} (Killswitch ON)", profile.name));
                        self.log_lines.push("[tuneli-tui] Killswitch enabled.".to_string());
                    }
                    
                    // Trigger IP refresh shortly after connection
                    self.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(45);
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    
                    if err_msg.to_lowercase().contains("incorrect password") || err_msg.to_lowercase().contains("try again") {
                        crate::engine::runner::SudoRunner::clear_password();
                        self.sudo_prompt.error_msg = Some("Incorrect sudo password. Please try again.".to_string());
                        self.sudo_prompt.is_active = true;
                        self.status_message = Some("Sudo authentication failed.".to_string());
                    } else {
                        self.status_message = Some("Connection failed. See logs.".to_string());
                        self.log_lines.push(format!("[ERROR] {}", err_msg));
                    }
                }
            }
        } else if let ProtocolConfig::OpenVpn { .. } = &profile.protocol {
            let cmd_str = format!("sudo openvpn --daemon --config {}", profile.path);
            self.log_lines.push(format!("[cmd] {}", cmd_str));
            self.status_message = Some(format!("Connecting OpenVPN {}...", profile.name));
            match crate::engine::runner::SudoRunner::run_with_sudo(
                &password,
                "openvpn",
                &["--daemon", "--config", &profile.path],
            ).await {
                Ok(_) => {
                    self.active_profile = Some(profile.clone());
                    self.status_message = Some(format!("Connected to {} (OpenVPN)", profile.name));
                    self.log_lines.push(format!("[tuneli-tui] OpenVPN connected: {}", profile.name));

                    // Trigger IP refresh shortly after connection
                    self.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(45);
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    self.status_message = Some("OpenVPN failed. See logs.".to_string());
                    self.log_lines.push(format!("[ERROR] OpenVPN: {}", err_msg));
                }
            }
        }
    }

    pub async fn disconnect_active(&mut self) {
        let password = match crate::engine::runner::SudoRunner::get_password() {
            Some(p) => p,
            None => {
                self.sudo_prompt.is_active = true;
                self.sudo_prompt.error_msg = Some("Sudo password required.".to_string());
                return;
            }
        };

        if let Some(active) = self.active_profile.clone() {
            let _ = crate::engine::firewall::Firewall::disable_killswitch(&password).await;
            if let ProtocolConfig::WireGuard { .. } = &active.protocol {
                let cmd_str = format!("sudo wg-quick down {}", active.name);
                self.log_lines.push(format!("[cmd] {}", cmd_str));
                self.status_message = Some(format!("Disconnecting {}...", active.name));
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password, "wg-quick", &["down", &active.name]
                ).await;
            } else if let ProtocolConfig::OpenVpn { .. } = &active.protocol {
                self.log_lines.push("[cmd] sudo killall -9 openvpn".to_string());
                self.status_message = Some(format!("Disconnecting OpenVPN {}...", active.name));
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password, "killall", &["-9", "openvpn"]
                ).await;
            }
            self.active_profile = None;
            self.status_message = Some(format!("Disconnected {}", active.name));
            self.log_lines.push(format!("[tuneli-tui] Disconnected {}", active.name));
            
            // Refresh IP
            self.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(60);
        } else {
            self.status_message = Some("No active connection to disconnect.".to_string());
        }
    }

    pub async fn reconnect_selected(&mut self) {
        let profile = match self.list_state.selected()
            .and_then(|i| self.profiles.get(i).cloned())
        {
            Some(p) => p,
            None => {
                self.status_message = Some("No profile selected.".to_string());
                return;
            }
        };

        self.disconnect_active().await;
        self.connect_profile(profile).await;
    }

    pub async fn save_new_config(&mut self) -> anyhow::Result<()> {
        let name = self.add_config_state.name.trim();
        let content_str = self.add_config_state.get_content_string();
        let content = content_str.trim();
        if name.is_empty() || content.is_empty() {
            self.status_message = Some("Name and content cannot be empty!".to_string());
            return Ok(());
        }

        let extension = match self.add_config_state.protocol {
            ProtocolType::WireGuard => "conf",
            ProtocolType::OpenVpn => "ovpn",
        };

        let filename = format!("{}.{}", name, extension);
        
        let base_path = if cfg!(target_os = "macos") {
            match self.add_config_state.protocol {
                ProtocolType::WireGuard => "/opt/homebrew/etc/wireguard",
                ProtocolType::OpenVpn => "/opt/homebrew/etc/openvpn",
            }
        } else {
            match self.add_config_state.protocol {
                ProtocolType::WireGuard => "/etc/wireguard",
                ProtocolType::OpenVpn => "/etc/openvpn",
            }
        };

        let full_path = std::path::Path::new(base_path).join(filename);
        let path_str = full_path.to_string_lossy().to_string();

        let password = match crate::engine::runner::SudoRunner::get_password() {
            Some(p) => p,
            None => {
                self.status_message = Some("Sudo password required to save config.".to_string());
                self.sudo_prompt.is_active = true;
                return Ok(());
            }
        };

        // Write content to a temp file first
        let tmp_path = format!("/tmp/tuneli_new_config.{}", extension);
        if let Err(e) = tokio::fs::write(&tmp_path, content).await {
             self.status_message = Some(format!("Failed to write temp file: {}", e));
             return Ok(());
        }

        // Use sudo mv to move it to the final location
        let mv_res = crate::engine::runner::SudoRunner::run_with_sudo(
            &password, "mv", &[&tmp_path, &path_str]
        ).await;

        match mv_res {
            Ok(_) => {
                self.status_message = Some(format!("Saved config to {}", path_str));
                self.show_add_config_modal = false;
                self.refresh_profiles().await;
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to save config: {}", e));
            }
        }

        Ok(())
    }

    pub async fn delete_profile(&mut self, profile: VpnProfile) -> anyhow::Result<()> {
        let password = match crate::engine::runner::SudoRunner::get_password() {
            Some(p) => p,
            None => {
                self.status_message = Some("Sudo password required to delete config.".to_string());
                self.sudo_prompt.is_active = true;
                return Ok(());
            }
        };

        // If the profile is active, disconnect it first
        if self.active_profile.as_ref().map(|p| p.name.as_str()) == Some(profile.name.as_str()) {
            self.disconnect_active().await;
        }

        let path_str = profile.path.clone();
        
        let rm_res = crate::engine::runner::SudoRunner::run_with_sudo(
            &password, "rm", &["-f", &path_str]
        ).await;

        match rm_res {
            Ok(_) => {
                self.status_message = Some(format!("Deleted config {}", path_str));
                self.refresh_profiles().await;
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to delete config: {}", e));
            }
        }
        
        Ok(())
    }

    pub fn copy_config_to_clipboard(&mut self) {
        let content = match &self.config_content {
            Some(c) => c,
            None => return,
        };

        // 1. Try system-level clipboard tools (Atomic, persistent)
        if self.try_shell_copy(content) {
            self.status_message = Some("Copied to clipboard (system tool)!".to_string());
            return;
        }

        // 2. Fallback to arboard
        if let Some(ref mut clipboard) = self.clipboard {
            if let Err(e) = clipboard.set_text(content.clone()) {
                self.status_message = Some(format!("Clipboard error: {}", e));
            } else {
                self.status_message = Some("Copied to clipboard (arboard)!".to_string());
            }
        } else {
            self.status_message = Some("Clipboard not available.".to_string());
        }
    }

    fn try_shell_copy(&self, text: &str) -> bool {
        use std::process::{Command, Stdio};
        use std::io::Write;

        let commands = if cfg!(target_os = "macos") {
            vec![("pbcopy", vec![])]
        } else {
            vec![
                ("wl-copy", vec![]),
                ("xclip", vec!["-selection", "clipboard"]),
                ("xsel", vec!["--clipboard", "--input"]),
            ]
        };

        for (cmd, args) in commands {
            if let Ok(mut child) = Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn() 
            {
                if let Some(mut stdin) = child.stdin.take() {
                    if stdin.write_all(text.as_bytes()).is_ok() {
                        drop(stdin);
                        if let Ok(status) = child.wait() {
                            if status.success() {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}
