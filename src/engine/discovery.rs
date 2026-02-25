use crate::models::{ProtocolConfig, VpnProfile};


pub async fn list_all_profiles() -> anyhow::Result<Vec<VpnProfile>> {
    let mut profiles = Vec::new();
    profiles.extend(list_wireguard_profiles().await?);
    profiles.extend(list_openvpn_profiles().await?);
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

pub async fn list_wireguard_profiles() -> anyhow::Result<Vec<VpnProfile>> {
    let mut profiles = Vec::new();

    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let search_paths = vec![
        format!("{}/.config/tuneli-tui/profiles", home),
    ];

    for base_path in search_paths {
        if let Ok(entries) = std::fs::read_dir(&base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("conf") {
                    let profile_name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let path_str = path.to_string_lossy().to_string();
                    let mut endpoint = None;

                    if let Ok(content) = std::fs::read_to_string(&path) {
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
                        path: path_str,
                        protocol: ProtocolConfig::WireGuard {
                            pubkey: None,
                            endpoint,
                            allowed_ips: vec![],
                        },
                    });
                }
            }
        }
    }

    Ok(profiles)
}

pub async fn list_openvpn_profiles() -> anyhow::Result<Vec<VpnProfile>> {
    let mut profiles = Vec::new();

    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let search_paths = vec![
        format!("{}/.config/tuneli-tui/profiles", home),
    ];

    for base_path in search_paths {
        if let Ok(entries) = std::fs::read_dir(&base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("ovpn") {
                    let profile_name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    let path_str = path.to_string_lossy().to_string();

                    let mut remote = None;
                    let mut port = None;
                    let mut proto = "udp".to_string();

                    if let Ok(content) = std::fs::read_to_string(&path) {
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
                        path: path_str,
                        protocol: ProtocolConfig::OpenVpn { proto, remote, port },
                    });
                }
            }
        }
    }

    Ok(profiles)
}
