use std::time::Duration;

pub async fn measure_latency(ip: &str) -> Option<Duration> {
    let url = format!("http://{}", ip);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;

    let start = std::time::Instant::now();
    match client.get(&url).send().await {
        Ok(_) => Some(start.elapsed()),
        Err(_) => None,
    }
}
