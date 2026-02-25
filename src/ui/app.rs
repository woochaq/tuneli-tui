use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use crossterm::{
    event,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use crate::ui::sudo_prompt::SudoPrompt;
use crate::models::{VpnProfile, ProtocolConfig};
use crate::ui::add_config::{AddConfigState, ProtocolType, FocusedPanel};
use ratatui::widgets::ListState;
use std::io::{stdout, Result};



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
    pub active_profiles: Vec<VpnProfile>,
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
    pub update_task: Option<tokio::task::JoinHandle<anyhow::Result<String>>>,
    pub openvpn_events: Option<tokio::sync::mpsc::Receiver<crate::engine::openvpn::OpenVpnEvent>>,
    pub openvpn_cmd_tx: Option<tokio::sync::mpsc::Sender<String>>,
    pub log_scroll_offset: u16,
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
            active_profiles: vec![],
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
            update_task: None,
            openvpn_events: None,
            openvpn_cmd_tx: None,
            log_scroll_offset: 0,
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

            // Reconcile active_profiles with real system state (WireGuard only).
            // Never overwrite an active OpenVPN connection — openvpn runs as a daemon
            // and won't show up in `wg show interfaces`.
            if let Some(pwd) = crate::engine::runner::SudoRunner::get_password() {
                let active_ifaces = crate::engine::runner::SudoRunner::get_active_wg_interfaces(&pwd).await;
                
                for profile in &self.profiles {
                    if let ProtocolConfig::WireGuard { .. } = &profile.protocol {
                        let is_wg_active = active_ifaces.iter().any(|iface| iface == &profile.name);
                        let is_tracked = self.active_profiles.iter().any(|p| p.name == profile.name);
                        
                        if is_wg_active && !is_tracked {
                            self.active_profiles.push(profile.clone());
                        } else if !is_wg_active && is_tracked {
                            self.active_profiles.retain(|p| p.name != profile.name);
                        }
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

            // Polling Update Background Task
            if let Some(handle) = self.update_task.take_if(|h| h.is_finished()) {
                match handle.await {
                    Ok(Ok(msg)) => {
                        self.status_message = Some(msg.clone());
                        self.log_lines.push(format!("[tuneli-tui] {}", msg));
                    }
                    Ok(Err(e)) => {
                        self.status_message = Some("Update failed. See logs.".into());
                        self.log_lines.push(format!("[ERROR] Update failed: {}", e));
                    }
                    Err(e) => {
                        self.status_message = Some("Update thread panicked.".into());
                        self.log_lines.push(format!("[ERROR] Update thread panic: {}", e));
                    }
                }
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

            // Poll OpenVPN Events — drain into a Vec first to avoid holding a
            // borrow on self.openvpn_events while we mutate other fields.
            let openvpn_events: Vec<_> = if let Some(ref mut rx) = self.openvpn_events {
                let mut events = Vec::new();
                while let Ok(ev) = rx.try_recv() {
                    events.push(ev);
                }
                events
            } else {
                Vec::new()
            };
            for ev in openvpn_events {
                match ev {
                        crate::engine::openvpn::OpenVpnEvent::Log(msg) => {
                            self.log_lines.push(msg);
                        }
                        crate::engine::openvpn::OpenVpnEvent::NeedAuth => {
                            self.status_message = Some("OpenVPN requires Auth".to_string());
                            self.log_lines.push("[tuneli-tui] OpenVPN requested Username/Password prompt.".to_string());
                            self.sudo_prompt.error_msg = Some("OpenVPN Auth [user:pass]".to_string());
                            self.sudo_prompt.is_active = true;
                            self.sudo_prompt.input.clear();
                        }
                        crate::engine::openvpn::OpenVpnEvent::NeedPrivateKeyAuth => {
                            self.status_message = Some("OpenVPN requires Private Key Password".to_string());
                            self.log_lines.push("[tuneli-tui] OpenVPN requested Private Key Password.".to_string());
                            self.sudo_prompt.error_msg = Some("Private Key Password:".to_string());
                            self.sudo_prompt.is_active = true;
                            self.sudo_prompt.input.clear();
                        }
                        crate::engine::openvpn::OpenVpnEvent::AuthFailed(msg, name) => {
                            self.status_message = Some("OpenVPN Auth Failed!".to_string());
                            self.log_lines.push(format!("[ERROR] OpenVPN Auth failed: {}", msg));
                            self.active_profiles.retain(|p| p.name != name);
                        }
                        crate::engine::openvpn::OpenVpnEvent::Connected(name) => {
                            self.status_message = Some(format!("Connected to {} (OpenVPN)", name));
                            self.log_lines.push("[tuneli-tui] OpenVPN Management reported CONNECTED.".to_string());
                            self.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(45);
                            self.sudo_prompt.is_active = false;
                            
                            // If WireGuard is active, add ip rule to bypass WireGuard policy routing
                            // for OpenVPN subnets (which are more specific routes in the main table).
                            let has_wg = self.active_profiles.iter().any(|p| {
                                matches!(&p.protocol, ProtocolConfig::WireGuard { .. })
                            });
                            if has_wg {
                                if let Some(pwd) = crate::engine::runner::SudoRunner::get_password() {
                                    self.log_lines.push("[tuneli-tui] WireGuard active — adjusting routing priority for OpenVPN subnets.".to_string());
                                    let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                                        &pwd, "ip", &["rule", "add", "lookup", "main", "suppress_prefixlength", "0", "priority", "10"]
                                    ).await;
                                }
                            }
                        }
                        crate::engine::openvpn::OpenVpnEvent::Disconnected => {
                            self.log_lines.push("[tuneli-tui] OpenVPN Management Disconnected.".to_string());
                            // We no longer indiscriminately drop active OpenVPN profiles here, because `openvpn` 
                            // may close the management interface intentionally on some setups.
                            // Profiles are instead dropped when the user initiates `disconnect_selected`.
                        }
                    }
            }

            terminal.draw(|f| {
                crate::ui::layout::draw(f, self);
            })?;

            while event::poll(std::time::Duration::from_millis(0))? {
                let ev = event::read()?;
                if crate::ui::input::handle_event(self, ev).await {
                    if self.sudo_prompt.is_verifying {
                        terminal.draw(|f| { crate::ui::layout::draw(f, self); }).ok();
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
        self.disconnect_all().await;

        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;
        Ok(())
    }

    pub async fn connect_profile(&mut self, profile: VpnProfile) {
        let password = match crate::engine::runner::SudoRunner::get_password() {
            Some(p) => p,
            None => {
                self.sudo_prompt.is_active = true;
                self.sudo_prompt.error_msg = Some("Sudo password required.".to_string());
                return;
            }
        };

        if self.active_profiles.iter().any(|p| p.name == profile.name) {
            self.status_message = Some(format!("{} is already connected.", profile.name));
            return;
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
                    self.active_profiles.push(profile.clone());
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
            // Use Unix domain socket for management
            let mgmt_sock = format!("/tmp/tuneli_mgmt_{}.sock", profile.name.replace(" ", "_"));
            let _ = std::fs::remove_file(&mgmt_sock);
                
            self.status_message = Some(format!("Connecting OpenVPN {}...", profile.name));
            let pid_file = format!("/tmp/tuneli_{}.pid", profile.name.replace(" ", "_"));
            // Assign a decreasing metric so newer connections override the default route.
            let route_metric = 100_usize.saturating_sub(self.active_profiles.len() * 10).max(1);
            let route_metric_str = route_metric.to_string();

            // Always use management-query-passwords for interactive auth.
            let args = vec![
                "--daemon", 
                "--config", &profile.path, 
                "--management", &mgmt_sock, "unix", 
                "--management-hold",
                "--writepid", &pid_file,
                "--route-metric", &route_metric_str,
                "--redirect-gateway", "def1",
                "--management-query-passwords",
            ];
            
            self.log_lines.push(format!("[cmd] sudo openvpn {}", args.join(" ")));
            
            match crate::engine::runner::SudoRunner::run_with_sudo(
                &password,
                "openvpn",
                &args,
            ).await {
                Ok(_) => {
                    self.active_profiles.push(profile.clone());
                    self.status_message = Some(format!("Connecting to {} (OpenVPN)", profile.name));
                    self.log_lines.push(format!("[tuneli-tui] OpenVPN spawned, attaching to management interface: {}", profile.name));
                    
                    // Start parsing management logic
                    match crate::engine::openvpn::start_management_client(mgmt_sock, profile.name.clone()).await {
                        Ok((rx, tx)) => {
                            self.openvpn_events = Some(rx);
                            self.openvpn_cmd_tx = Some(tx);
                        }
                        Err(e) => {
                            self.log_lines.push(format!("[ERROR] Management connection failed: {}", e));
                        }
                    }

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

    pub async fn disconnect_selected(&mut self) {
        let password = match crate::engine::runner::SudoRunner::get_password() {
            Some(p) => p,
            None => {
                self.sudo_prompt.is_active = true;
                self.sudo_prompt.error_msg = Some("Sudo password required.".to_string());
                return;
            }
        };

        let profile = match self.list_state.selected().and_then(|i| self.profiles.get(i).cloned()) {
            Some(p) => p,
            None => return,
        };
        
        if !self.active_profiles.iter().any(|p| p.name == profile.name) {
            self.status_message = Some(format!("Profile {} is not connected.", profile.name));
            return;
        }

        let _ = crate::engine::firewall::Firewall::disable_killswitch(&password).await;

        if let ProtocolConfig::WireGuard { .. } = &profile.protocol {
            let cmd_str = format!("sudo wg-quick down {}", profile.name);
            self.log_lines.push(format!("[cmd] {}", cmd_str));
            self.status_message = Some(format!("Disconnecting {}...", profile.name));
            let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                &password, "wg-quick", &["down", &profile.name]
            ).await;
        } else if let ProtocolConfig::OpenVpn { .. } = &profile.protocol {
            let pid_file = format!("/tmp/tuneli_{}.pid", profile.name.replace(" ", "_"));
            let mgmt_sock = format!("/tmp/tuneli_mgmt_{}.sock", profile.name.replace(" ", "_"));
            self.log_lines.push(format!("[cmd] kill OpenVPN {}", profile.name));
            self.status_message = Some(format!("Disconnecting OpenVPN {}...", profile.name));
            let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                &password, "sh", &["-c", &format!("kill $(cat {}) 2>/dev/null; rm -f {} {}", pid_file, pid_file, mgmt_sock)]
            ).await;
            // Clean up the ip rule we added for WireGuard coexistence
            let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                &password, "ip", &["rule", "del", "lookup", "main", "suppress_prefixlength", "0", "priority", "10"]
            ).await;
        }
        
        self.active_profiles.retain(|p| p.name != profile.name);
        self.status_message = Some(format!("Disconnected {}", profile.name));
        self.log_lines.push(format!("[tuneli-tui] Disconnected {}", profile.name));
        
        self.last_geo_refresh = std::time::Instant::now() - std::time::Duration::from_secs(60);
    }
    
    pub async fn disconnect_all(&mut self) {
        let password = match crate::engine::runner::SudoRunner::get_password() {
            Some(p) => p,
            None => return,
        };

        let active = self.active_profiles.clone();
        for profile in active {
            let _ = crate::engine::firewall::Firewall::disable_killswitch(&password).await;
            if let ProtocolConfig::WireGuard { .. } = &profile.protocol {
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(&password, "wg-quick", &["down", &profile.name]).await;
            } else if let ProtocolConfig::OpenVpn { .. } = &profile.protocol {
                let pid_file = format!("/tmp/tuneli_{}.pid", profile.name.replace(" ", "_"));
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(&password, "sh", &["-c", &format!("kill $(cat {}) 2>/dev/null; rm -f {}", pid_file, pid_file)]).await;
            }
        }
        self.active_profiles.clear();
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

        self.disconnect_selected().await;
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
        if self.active_profiles.iter().any(|p| p.name == profile.name) {
            let _ = crate::engine::firewall::Firewall::disable_killswitch(&password).await;

            if let ProtocolConfig::WireGuard { .. } = &profile.protocol {
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password, "wg-quick", &["down", &profile.name]
                ).await;
            } else if let ProtocolConfig::OpenVpn { .. } = &profile.protocol {
                let filter = format!("openvpn.*{}", profile.path);
                let _ = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password, "pkill", &["-f", &filter]
                ).await;
            }
            
            self.active_profiles.retain(|p| p.name != profile.name);
            self.status_message = Some(format!("Disconnected {}", profile.name));
            self.log_lines.push(format!("[tuneli-tui] Disconnected {}", profile.name));
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

    pub fn trigger_update(&mut self) {
        if self.update_task.is_none() {
            self.status_message = Some("Checking for updates...".to_string());
            self.log_lines.push("[tuneli-tui] Launching update fetch task...".to_string());
            self.update_task = Some(tokio::task::spawn_blocking(|| {
                crate::engine::updater::update_binary()
            }));
        } else {
            self.status_message = Some("Update already in progress...".to_string());
        }
    }
}
