use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProtocolConfig {
    WireGuard {
        pubkey: Option<String>,
        endpoint: Option<String>,
        allowed_ips: Vec<String>,
    },
    OpenVpn {
        proto: String,
        remote: Option<String>,
        port: Option<u16>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnProfile {
    pub name: String,
    pub path: String,
    pub protocol: ProtocolConfig,
}
