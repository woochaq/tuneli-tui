use anyhow::{Context, Result};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::sleep;

pub enum OpenVpnEvent {
    Log(String),
    NeedAuth,
    NeedPrivateKeyAuth,
    Connected(String),
    Disconnected,
    AuthFailed(String, String),
}

/// Spawns a background task that connects to the OpenVPN management interface on the specified port.
/// Returns a channel receiver that streams events (e.g. SAML challenges, log lines).
pub async fn start_management_client(
    mgmt_socket_path: String,
    profile_name: String,
) -> Result<(mpsc::Receiver<OpenVpnEvent>, mpsc::Sender<String>)> {
    let (event_tx, event_rx) = mpsc::channel(100);
    let (command_tx, mut command_rx) = mpsc::channel::<String>(10);

    // Give OpenVPN a moment to spin up its management interface
    let mut stream = None;
    for _ in 0..10 {
        if let Ok(s) = tokio::net::UnixStream::connect(&mgmt_socket_path).await {
            stream = Some(s);
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    let stream = stream.context("Failed to connect to OpenVPN management interface (unix socket)")?;
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    tokio::spawn(async move {
        let mut line = String::new();

        loop {
            tokio::select! {
                // Read commands from the app and send them to OpenVPN
                cmd_opt = command_rx.recv() => {
                    if let Some(cmd) = cmd_opt {
                        if write_half.write_all(format!("{}\n", cmd).as_bytes()).await.is_err() {
                            break; // Interface closed
                        }
                    } else {
                        break; // Channel closed
                    }
                }
                
                // Read responses from OpenVPN
                read_res = reader.read_line(&mut line) => {
                    match read_res {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            let content = line.trim();
                            // Passwords prompts and states
                            if content.starts_with(">PASSWORD:Need 'Auth'") {
                                let _ = event_tx.send(OpenVpnEvent::NeedAuth).await;
                            } else if content.starts_with(">PASSWORD:Need 'Private Key'") {
                                let _ = event_tx.send(OpenVpnEvent::NeedPrivateKeyAuth).await;
                            } else if content.starts_with(">PASSWORD:Verification Failed") {
                                let _ = event_tx.send(OpenVpnEvent::AuthFailed("Verification failed".to_string(), profile_name.clone())).await;
                            } else if content.starts_with(">INFO:OpenVPN Management Interface Version") {
                                // Connected to management interface
                                let _ = write_half.write_all(b"state on\n").await;
                                let _ = write_half.write_all(b"log on\n").await;
                                let _ = write_half.write_all(b"hold release\n").await;
                            } else if content.starts_with(">HOLD:Waiting for hold release:") {
                                // OpenVPN expects a hold release during startup or auth-renegotiation
                                let _ = write_half.write_all(b"hold release\n").await;
                            } else if content.starts_with(">STATE") && content.contains("CONNECTED") {
                                // Extract the IP or just send a generic connect message. 
                                let _ = event_tx.send(OpenVpnEvent::Connected(profile_name.clone())).await;
                            } else {
                                // General log line (strip the prefix if desired)
                                let _ = event_tx.send(OpenVpnEvent::Log(format!("[OpenVPN] {}", content))).await;
                            }
                            
                            line.clear();
                        }
                        Err(_) => break, // Read error
                    }
                }
            }
        }
        
        let _ = event_tx.send(OpenVpnEvent::Disconnected).await;
    });

    Ok((event_rx, command_tx))
}
