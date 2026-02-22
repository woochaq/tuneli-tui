use crate::models::{ProtocolConfig, VpnProfile};

async fn get_password() -> Option<String> {
    crate::engine::runner::SudoRunner::get_password()
}

pub async fn list_all_profiles() -> anyhow::Result<Vec<VpnProfile>> {
    let mut profiles = Vec::new();
    profiles.extend(list_wireguard_profiles().await?);
    profiles.extend(list_openvpn_profiles().await?);
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

pub async fn list_wireguard_profiles() -> anyhow::Result<Vec<VpnProfile>> {
    let mut profiles = Vec::new();
    let password = match get_password().await {
        Some(p) => p,
        None => return Ok(profiles),
    };

    let search_paths = vec![
        "/etc/wireguard",
        "/usr/local/etc/wireguard",
        "/opt/homebrew/etc/wireguard",
    ];

    for base_path in search_paths {
        let ls_cmd = format!("ls -1 {}/*.conf 2>/dev/null", base_path);
        if let Ok(ls_output) = crate::engine::runner::SudoRunner::run_with_sudo(
            &password, "sh", &["-c", &ls_cmd],
        ).await {
            for line in ls_output.lines() {
                let path = line.trim().to_string();
                if path.is_empty() { continue; }

                let file_name = std::path::Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let profile_name = file_name.trim_end_matches(".conf").to_string();

                let mut endpoint = None;
                if let Ok(content) = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password, "cat", &[&path],
                ).await {
                    for content_line in content.lines() {
                        if content_line.starts_with("Endpoint") {
                            if let Some(ep) = content_line.split('=').nth(1) {
                                endpoint = Some(ep.trim().to_string());
                            }
                        }
                    }
                }

                profiles.push(VpnProfile {
                    name: profile_name,
                    path,
                    protocol: ProtocolConfig::WireGuard {
                        pubkey: None,
                        endpoint,
                        allowed_ips: vec![],
                    },
                });
            }
        }
    }

    Ok(profiles)
}

pub async fn list_openvpn_profiles() -> anyhow::Result<Vec<VpnProfile>> {
    let mut profiles = Vec::new();
    let password = match get_password().await {
        Some(p) => p,
        None => return Ok(profiles),
    };

    let search_paths = vec![
        "/etc/openvpn",
        "/usr/local/etc/openvpn",
        "/opt/homebrew/etc/openvpn",
    ];

    for base_path in search_paths {
        let find_cmd = format!("find {} -maxdepth 2 -type f \\( -name '*.ovpn' -o -name '*.vpn' -o -name '*.conf' \\) 2>/dev/null", base_path);
        if let Ok(ls_output) = crate::engine::runner::SudoRunner::run_with_sudo(
            &password, "sh", &["-c", &find_cmd],
        ).await {
            for line in ls_output.lines() {
                let path = line.trim().to_string();
                if path.is_empty() { continue; }

                let file_name = std::path::Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let profile_name = file_name
                    .trim_end_matches(".ovpn")
                    .trim_end_matches(".vpn")
                    .trim_end_matches(".conf")
                    .to_string();

                let mut remote = None;
                let mut port = None;
                let mut proto = "udp".to_string();

                if let Ok(content) = crate::engine::runner::SudoRunner::run_with_sudo(
                    &password, "cat", &[&path],
                ).await {
                    for content_line in content.lines() {
                        let parts: Vec<&str> = content_line.split_whitespace().collect();
                        if parts.is_empty() { continue; }
                        match parts[0] {
                            "remote" if parts.len() >= 2 => {
                                remote = Some(parts[1].to_string());
                                if parts.len() >= 3 {
                                    port = parts[2].parse::<u16>().ok();
                                }
                            }
                            "proto" if parts.len() >= 2 => {
                                proto = parts[1].to_string();
                            }
                            _ => {}
                        }
                    }
                }

                profiles.push(VpnProfile {
                    name: profile_name,
                    path,
                    protocol: ProtocolConfig::OpenVpn { proto, remote, port },
                });
            }
        }
    }

    Ok(profiles)
}
