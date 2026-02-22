use std::time::Duration;
use tokio::process::Command;

pub async fn measure_latency(ip: &str) -> Option<Duration> {
    let output = Command::new("ping")
        .arg("-c")
        .arg("1")
        .arg("-W")
        .arg("1")
        .arg(ip)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("time=") {
            let parts: Vec<&str> = line.split("time=").collect();
            if parts.len() > 1 {
                let time_str = parts[1].split_whitespace().next().unwrap_or("");
                if let Ok(ms) = time_str.parse::<f64>() {
                    return Some(Duration::from_secs_f64(ms / 1000.0));
                }
            }
        }
    }
    None
}
