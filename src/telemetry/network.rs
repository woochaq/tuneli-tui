use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct NetStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy)]
pub struct Throughput {
    pub rx_bps: f64, // bytes per second
    pub tx_bps: f64,
}

#[cfg(target_os = "linux")]
pub fn get_net_stats() -> Option<NetStats> {
    use std::fs;
    let content = fs::read_to_string("/proc/net/dev").ok()?;
    let mut total_rx = 0;
    let mut total_tx = 0;

    for line in content.lines().skip(2) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 { continue; }
        
        let iface = parts[0].trim_end_matches(':');
        if iface == "lo" { continue; }

        let rx: u64 = parts[1].parse().unwrap_or(0);
        let tx: u64 = parts[9].parse().unwrap_or(0);

        total_rx += rx;
        total_tx += tx;
    }

    Some(NetStats {
        rx_bytes: total_rx,
        tx_bytes: total_tx,
        timestamp: Instant::now(),
    })
}

#[cfg(target_os = "macos")]
pub fn get_net_stats() -> Option<NetStats> {
    use std::process::Command;
    
    // netstat -ibn gives byte counts per interface
    let output = Command::new("netstat")
        .args(&["-i", "-b", "-n"])
        .output()
        .ok()?;
    
    if !output.status.success() { return None; }
    
    let content = String::from_utf8_lossy(&output.stdout);
    let mut total_rx = 0;
    let mut total_tx = 0;

    // Skip header line
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // Typically Link layer lines have enough columns
        // Column indices for -ibn:
        // 0: Name, 1: Mtu, 2: Network, 3: Address, 4: Ipkts, 5: Ierrs, 6: Ibytes, 7: Opkts, 8: Oerrs, 9: Obytes
        if parts.len() < 10 { continue; }
        
        // Only count Link layer entries to avoid double counting same interface
        if !parts[2].starts_with("<Link") { continue; }
        if parts[0].starts_with("lo") { continue; }

        let rx: u64 = parts[6].parse().unwrap_or(0);
        let tx: u64 = parts[9].parse().unwrap_or(0);

        total_rx += rx;
        total_tx += tx;
    }

    Some(NetStats {
        rx_bytes: total_rx,
        tx_bytes: total_tx,
        timestamp: Instant::now(),
    })
}

// Fallback for other systems
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn get_net_stats() -> Option<NetStats> {
    None
}

pub fn calculate_throughput(prev: &NetStats, curr: &NetStats) -> Throughput {
    let duration = curr.timestamp.duration_since(prev.timestamp).as_secs_f64();
    if duration == 0.0 {
        return Throughput { rx_bps: 0.0, tx_bps: 0.0 };
    }

    let rx_diff = if curr.rx_bytes >= prev.rx_bytes { curr.rx_bytes - prev.rx_bytes } else { 0 };
    let tx_diff = if curr.tx_bytes >= prev.tx_bytes { curr.tx_bytes - prev.tx_bytes } else { 0 };

    Throughput {
        rx_bps: rx_diff as f64 / duration,
        tx_bps: tx_diff as f64 / duration,
    }
}
