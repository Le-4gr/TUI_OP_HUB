//! Dashboard system monitor (US-PROC-01): a mini-btop style overview with
//! compact CPU/RAM/network/temperature/GPU panels and quick-launch detection.
//!
//! Virtual interfaces (docker bridges, veth pairs, loopback, VPN tunnels, \u2026)
//! are filtered out \u2014 many machines have dozens of docker subnetworks.

use serde::{Deserialize, Serialize};
use sysinfo::{Components, Networks, System};

/// One network interface worth showing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetIface {
    pub name: String,
    /// Bytes received since the previous refresh (rate basis).
    pub rx: u64,
    pub tx: u64,
    pub total_received: u64,
    pub total_transmitted: u64,
}

/// A temperature sensor reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempReading {
    pub label: String,
    pub celsius: f32,
}

/// A detected quick-launch TUI tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickLaunch {
    pub key: char,
    pub name: String,
    pub description: String,
}

/// One full monitor snapshot for the dashboard panels.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitorSnapshot {
    pub cpu_overall: f32,
    pub cpu_per_core: Vec<f32>,
    pub cpu_freq_mhz: u64,
    pub ram_used: u64,
    pub ram_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub interfaces: Vec<NetIface>,
    pub temps: Vec<TempReading>,
    /// Best-effort: (name, utilization %, temp \u00b0C) per GPU via nvidia-smi.
    pub gpus: Vec<(String, f32, f32)>,
    pub quick_launches: Vec<QuickLaunch>,
}

impl MonitorSnapshot {
    /// ASCII bar `\u{2588}\u{2588}\u{2588}\u{2591}\u{2591}` for the given percentage (0-100).
    pub fn bar(pct: f32, width: usize) -> String {
        let pct = pct.clamp(0.0, 100.0);
        let filled = ((pct / 100.0) * width as f32).round() as usize;
        let mut s = String::with_capacity(width * 3);
        for i in 0..width {
            s.push(if i < filled { '\u{2588}' } else { '\u{2591}' });
        }
        s
    }
}

/// Filter: is this a *physical* interface worth showing? Docker bridges,
/// veth pairs, loopback, VPN tunnels and other virtual devices are excluded \u2014
/// machines with many docker subnetworks would otherwise flood the panel.
pub fn is_physical_interface(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n == "lo" || n == "lo0" {
        return false;
    }
    const VIRTUAL_PREFIXES: [&str; 12] = [
        "docker",
        "veth",
        "br-",
        "virbr",
        "vnet",
        "tun",
        "tap",
        "wg",
        "tailscale",
        "zt",
        "vmnet",
        "ifb",
    ];
    for p in VIRTUAL_PREFIXES {
        if n.starts_with(p) {
            return false;
        }
    }
    // docker-style suffixes on bridges
    if n.starts_with("mon.") || n.contains(".ifb") {
        return false;
    }
    // Keep known physical naming schemes; unknown names are shown too
    // (better to show one extra iface than hide the real NIC).
    true
}

/// Detect installed quick-launch tools (US-PROC): lazygit, lazydocker, k9s, lazynpm.
pub fn detect_quick_launches(which: fn(&str) -> bool) -> Vec<QuickLaunch> {
    let candidates: [(char, &str, &str); 4] = [
        ('g', "lazygit", "Git TUI"),
        ('d', "lazydocker", "Docker TUI"),
        ('k', "k9s", "Kubernetes TUI"),
        ('n', "lazynpm", "npm TUI"),
    ];
    candidates
        .iter()
        .filter(|(_, prog, _)| which(prog))
        .map(|(key, prog, desc)| QuickLaunch {
            key: *key,
            name: prog.to_string(),
            description: desc.to_string(),
        })
        .collect()
}

/// Persistent monitor state: keeps sysinfo instances so rate/usage deltas are
/// meaningful between refreshes.
pub struct Monitor {
    system: System,
    networks: Networks,
    components: Components,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_cpu_usage();
        system.refresh_memory();
        Self {
            system,
            networks: Networks::new_with_refreshed_list(),
            components: Components::new_with_refreshed_list(),
        }
    }

    /// Refresh and build a dashboard snapshot.
    pub fn snapshot(&mut self) -> MonitorSnapshot {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh();
        self.components.refresh();

        let cpus = self.system.cpus();
        let cpu_per_core: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
        let cpu_overall = self.system.global_cpu_usage();
        let cpu_freq_mhz = cpus.first().map(|c| c.frequency()).unwrap_or(0);

        let mut interfaces: Vec<NetIface> = self
            .networks
            .iter()
            .filter(|(name, _)| is_physical_interface(name))
            .map(|(name, data)| NetIface {
                name: name.clone(),
                rx: data.received(),
                tx: data.transmitted(),
                total_received: data.total_received(),
                total_transmitted: data.total_transmitted(),
            })
            .collect();
        interfaces.sort_by(|a, b| (b.rx + b.tx).cmp(&(a.rx + a.tx)));

        let temps: Vec<TempReading> = self
            .components
            .iter()
            .filter(|c| c.temperature() > 0.0)
            .map(|c| TempReading {
                label: c.label().to_string(),
                celsius: c.temperature(),
            })
            .collect();

        MonitorSnapshot {
            cpu_overall,
            cpu_per_core,
            cpu_freq_mhz,
            ram_used: self.system.used_memory(),
            ram_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
            interfaces,
            temps,
            gpus: query_gpus(),
            quick_launches: detect_quick_launches(crate::keygen::which),
        }
    }
}

/// Best-effort GPU stats via nvidia-smi (skipped silently when not installed).
fn query_gpus() -> Vec<(String, f32, f32)> {
    if !crate::keygen::which("nvidia-smi") {
        return Vec::new();
    }
    let out = match std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(',').map(str::trim).collect();
            if parts.len() < 3 {
                return None;
            }
            Some((
                parts[0].to_string(),
                parts[1].parse::<f32>().unwrap_or(0.0),
                parts[2].parse::<f32>().unwrap_or(0.0),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_interfaces_are_filtered() {
        assert!(!is_physical_interface("lo"));
        assert!(!is_physical_interface("docker0"));
        assert!(!is_physical_interface("veth1a2b3c"));
        assert!(!is_physical_interface("br-abc123"));
        assert!(!is_physical_interface("virbr0"));
        assert!(!is_physical_interface("vnet0"));
        assert!(!is_physical_interface("tun0"));
        assert!(!is_physical_interface("wg0"));
        assert!(!is_physical_interface("tailscale0"));
        assert!(!is_physical_interface("zt0"));
        // physical ones stay
        assert!(is_physical_interface("eth0"));
        assert!(is_physical_interface("wlan0"));
        assert!(is_physical_interface("eno1"));
        assert!(is_physical_interface("ens33"));
        assert!(is_physical_interface("enp3s0"));
        assert!(is_physical_interface("wlp2s0"));
    }

    #[test]
    fn bar_renders_filled_and_empty_blocks() {
        assert_eq!(
            MonitorSnapshot::bar(0.0, 4),
            "\u{2591}\u{2591}\u{2591}\u{2591}"
        );
        assert_eq!(
            MonitorSnapshot::bar(100.0, 4),
            "\u{2588}\u{2588}\u{2588}\u{2588}"
        );
        let half = MonitorSnapshot::bar(50.0, 4);
        assert_eq!(half, "\u{2588}\u{2588}\u{2591}\u{2591}");
        // clamped
        assert_eq!(MonitorSnapshot::bar(150.0, 2), "\u{2588}\u{2588}");
    }

    #[tokio::test]
    async fn snapshot_builds_without_panic() {
        let mut m = Monitor::new();
        let snap = m.snapshot();
        assert!(!snap.cpu_per_core.is_empty(), "per-core list populated");
        assert!(snap.ram_total > 0);
        // quick launches: lazygit/lazydocker/k9s presence depends on the host,
        // but detection must not panic and keys must be lowercase letters.
        for q in &snap.quick_launches {
            assert!(q.key.is_ascii_lowercase());
        }
    }
}
