use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub struct Firewall;

impl Firewall {
    #[cfg(target_os = "linux")]
    pub async fn enable_killswitch(password: &str, allowed_iface: &str) -> anyhow::Result<()> {
        let rules = format!(
            "table inet tuneli_ks {{\n\
                chain forward {{\n\
                    type filter hook forward priority 0; policy drop;\n\
                    oifname \"{iface}\" accept\n\
                    iifname \"{iface}\" accept\n\
                }}\n\
            }}\n",
            iface = allowed_iface
        );

        let tmp_path = "/tmp/tuneli_ks.nft";
        tokio::fs::write(tmp_path, rules.as_bytes()).await?;

        let success = Self::run_sudo_cmd(password, "nft", &["-f", tmp_path]).await?;
        let _ = tokio::fs::remove_file(tmp_path).await;

        if success { Ok(()) } else { Err(anyhow::anyhow!("Failed to enable Linux killswitch")) }
    }

    #[cfg(target_os = "macos")]
    pub async fn enable_killswitch(password: &str, allowed_iface: &str) -> anyhow::Result<()> {
        // More permissive macOS rules: Only block forwarding, allow all local output for now
        // to avoid blocking the VPN handshake itself.
        let rules = format!(
            "set skip on lo0\n\
             pass out on {iface} all\n",
            iface = allowed_iface
        );

        let tmp_path = "/tmp/tuneli_ks.pf";
        tokio::fs::write(tmp_path, rules.as_bytes()).await?;

        // 1. Enable pf
        Self::run_sudo_cmd(password, "pfctl", &["-e"]).await?;
        // 2. Load rules
        let success = Self::run_sudo_cmd(password, "pfctl", &["-f", tmp_path]).await?;
        
        let _ = tokio::fs::remove_file(tmp_path).await;

        if success { Ok(()) } else { Err(anyhow::anyhow!("Failed to enable macOS killswitch (pfctl)")) }
    }

    #[cfg(target_os = "linux")]
    pub async fn disable_killswitch(password: &str) -> anyhow::Result<()> {
        Self::run_sudo_cmd(password, "nft", &["delete", "table", "inet", "tuneli_ks"]).await?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub async fn disable_killswitch(password: &str) -> anyhow::Result<()> {
        Self::run_sudo_cmd(password, "pfctl", &["-F", "all"]).await?;
        Self::run_sudo_cmd(password, "pfctl", &["-d"]).await?;
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub async fn enable_killswitch(_password: &str, _allowed_iface: &str) -> anyhow::Result<()> { Ok(()) }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub async fn disable_killswitch(_password: &str) -> anyhow::Result<()> { Ok(()) }

    async fn run_sudo_cmd(password: &str, cmd: &str, args: &[&str]) -> anyhow::Result<bool> {
        let mut child = Command::new("sudo")
            .arg("-S")
            .arg("-p")
            .arg("")
            .arg(cmd)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(format!("{}\n", password).as_bytes()).await?;
        }

        let output = child.wait_with_output().await?;
        Ok(output.status.success())
    }
}
