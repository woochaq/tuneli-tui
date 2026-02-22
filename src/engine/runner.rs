use std::process::Stdio;
use std::sync::RwLock;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

lazy_static::lazy_static! {
    static ref SUDO_PASS: RwLock<Option<String>> = RwLock::new(None);
}

pub struct SudoRunner;

impl SudoRunner {
    pub fn set_password(password: String) {
        if let Ok(mut lock) = SUDO_PASS.write() {
            *lock = Some(password);
        }
    }

    pub fn clear_password() {
        if let Ok(mut lock) = SUDO_PASS.write() {
            *lock = None;
        }
    }

    pub fn get_password() -> Option<String> {
        if let Ok(lock) = SUDO_PASS.read() {
            lock.clone()
        } else {
            None
        }
    }

    pub async fn validate_password(password: &str) -> bool {
        let mut cmd = Command::new("sudo");
        cmd.arg("-k")
           .arg("-S")
           .arg("-p")
           .arg("")
           .arg("true")
           .stdin(Stdio::piped())
           .stdout(Stdio::null())
           .stderr(Stdio::null());
           
        if let Ok(mut child) = cmd.spawn() {
            if let Some(mut stdin) = child.stdin.take() {
                if stdin.write_all(format!("{}\n", password).as_bytes()).await.is_err() {
                    return false;
                }
            }
            if let Ok(status) = child.wait().await {
                return status.success();
            }
        }
        false
    }
    
    /// Returns the names of all currently active WireGuard interfaces.
    pub async fn get_active_wg_interfaces(password: &str) -> Vec<String> {
        match Self::run_with_sudo(password, "wg", &["show", "interfaces"]).await {
            Ok(output) => output
                .split_whitespace()
                .map(|s| s.to_string())
                .collect(),
            Err(_) => vec![],
        }
    }

    pub async fn run_with_sudo(password: &str, program: &str, args: &[&str]) -> anyhow::Result<String> {
        let mut cmd = Command::new("sudo");
        cmd.arg("-S")
            .arg("-p")
            .arg("") // prevent sudo from polluting stdout with a prompt
            .arg(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        let mut child = cmd.spawn()?;
        
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(format!("{}\n", password).as_bytes()).await?;
        }
        
        let output = child.wait_with_output().await?;
        
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let raw_err = String::from_utf8_lossy(&output.stderr).to_string();
            let safe_err = raw_err.replace(password, "***");
            Err(anyhow::anyhow!("Command failed: {}", safe_err.trim()))
        }
    }
}
