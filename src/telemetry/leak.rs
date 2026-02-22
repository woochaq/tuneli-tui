#[cfg(target_os = "linux")]
pub async fn check_default_routes() -> anyhow::Result<Vec<String>> {
    use rtnetlink::new_connection;
    use futures::stream::TryStreamExt;
    use netlink_packet_route::route::RouteMessage;

    let (connection, handle, _) = new_connection()?;
    tokio::spawn(connection);

    let mut routes = handle.route().get(RouteMessage::default()).execute();
    let mut interfaces = vec![];
    
    while let Some(route) = routes.try_next().await? {
        interfaces.push(format!("{:?}", route));
    }
    
    Ok(interfaces)
}

#[cfg(target_os = "macos")]
pub async fn check_default_routes() -> anyhow::Result<Vec<String>> {
    use std::process::Command;
    
    // On macOS, netstat -rn shows the routing table
    let output = Command::new("netstat")
        .args(&["-rn", "-f", "inet"])
        .output()?;
    
    if !output.status.success() {
        return anyhow::bail!("Failed to run netstat");
    }
    
    let content = String::from_utf8_lossy(&output.stdout);
    let mut interfaces = vec![];
    
    for line in content.lines() {
        if line.starts_with("default") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                // index 3 is Netif
                interfaces.push(parts[3].to_string());
            }
        }
    }
    
    Ok(interfaces)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub async fn check_default_routes() -> anyhow::Result<Vec<String>> {
    Ok(vec![])
}
