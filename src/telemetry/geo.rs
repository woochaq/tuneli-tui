#[derive(Debug, Clone)]
pub struct GeoInfo {
    pub public_ip: String,
}

/// Fetch public IP from ipify.org (3s timeout for fast response).
pub async fn fetch_public_ip() -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;
    let ip = client
        .get("https://api.ipify.org")
        .send()
        .await?
        .text()
        .await?
        .trim()
        .to_string();
    Ok(ip)
}

/// Fetch public IP — fast, no ping.
pub async fn fetch_geo_info() -> Option<GeoInfo> {
    let ip = fetch_public_ip().await.ok()?;
    Some(GeoInfo { public_ip: ip })
}
