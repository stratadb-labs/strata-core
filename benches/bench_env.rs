//! Benchmark Environment Capture
//!
//! This module captures and reports system information relevant to benchmark reproducibility.
//! All performance gates and Redis comparisons are only valid on the reference platform.

#![allow(dead_code)] // Some functions are for future use (JSON export, perf parsing)

#[cfg(target_os = "linux")]
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// Complete environment information for benchmark runs
#[derive(Debug, Clone)]
pub struct BenchEnvironment {
    pub os: OsInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub numa: NumaInfo,
    pub governor: GovernorInfo,
    pub rust: RustInfo,
    pub git: GitInfo,
    pub timestamp: String,
    pub is_reference_platform: bool,
}

#[derive(Debug, Clone)]
pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub kernel: String,
}

#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub model: String,
    pub cores: usize,
    pub threads: usize,
    pub cache_l1d: Option<String>,
    pub cache_l1i: Option<String>,
    pub cache_l2: Option<String>,
    pub cache_l3: Option<String>,
    pub frequency_mhz: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct MemoryInfo {
    pub total_gb: f64,
    pub available_gb: f64,
}

#[derive(Debug, Clone)]
pub struct NumaInfo {
    pub nodes: usize,
    pub layout: Vec<NumaNode>,
}

#[derive(Debug, Clone)]
pub struct NumaNode {
    pub id: usize,
    pub cpus: Vec<usize>,
    pub memory_gb: f64,
}

#[derive(Debug, Clone)]
pub struct GovernorInfo {
    pub current: String,
    pub available: Vec<String>,
    pub is_performance: bool,
}

#[derive(Debug, Clone)]
pub struct RustInfo {
    pub version: String,
    pub channel: String,
    pub target: String,
    pub profile: String,
    pub lto: bool,
    pub opt_level: String,
}

#[derive(Debug, Clone)]
pub struct GitInfo {
    pub commit: String,
    pub branch: String,
    pub dirty: bool,
}

impl BenchEnvironment {
    /// Capture the current environment
    pub fn capture() -> Self {
        let os = capture_os_info();
        let cpu = capture_cpu_info();
        let memory = capture_memory_info();
        let numa = capture_numa_info();
        let governor = capture_governor_info();
        let rust = capture_rust_info();
        let git = capture_git_info();
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let is_reference_platform = Self::check_reference_platform(&os, &cpu, &governor);

        Self {
            os,
            cpu,
            memory,
            numa,
            governor,
            rust,
            git,
            timestamp,
            is_reference_platform,
        }
    }

    /// Check if running on reference platform
    fn check_reference_platform(_os: &OsInfo, cpu: &CpuInfo, governor: &GovernorInfo) -> bool {
        // Check if running on Linux - the OS name may be the distro name (e.g., "Ubuntu")
        // rather than containing "Linux", so we use cfg!(target_os) instead
        let is_linux = cfg!(target_os = "linux");
        let is_performance = governor.is_performance;
        let has_enough_cores = cpu.cores >= 8;

        is_linux && is_performance && has_enough_cores
    }

    /// Print full environment report to stderr
    pub fn print_report(&self) {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("BENCHMARK ENVIRONMENT REPORT");
        eprintln!("{}", "=".repeat(60));

        // Platform validation warning
        if !self.is_reference_platform {
            eprintln!("\n⚠️  WARNING: NOT RUNNING ON REFERENCE PLATFORM");
            eprintln!("   Results are for development only. Do not use for:");
            eprintln!("   - Performance gate decisions");
            eprintln!("   - Redis competitiveness claims");
            eprintln!("   - PR performance reports");
            if self.os.name.to_lowercase().contains("darwin") {
                eprintln!("\n   macOS detected. Use Linux for official benchmarks.");
            }
        } else {
            eprintln!("\n✓ Running on reference platform");
        }

        // OS
        eprintln!("\n[Operating System]");
        eprintln!("  Name:     {}", self.os.name);
        eprintln!("  Version:  {}", self.os.version);
        eprintln!("  Kernel:   {}", self.os.kernel);

        // CPU
        eprintln!("\n[CPU]");
        eprintln!("  Model:    {}", self.cpu.model);
        eprintln!("  Cores:    {} physical", self.cpu.cores);
        eprintln!("  Threads:  {} logical", self.cpu.threads);
        if let Some(ref freq) = self.cpu.frequency_mhz {
            eprintln!("  Frequency: {} MHz", freq);
        }

        // Cache
        eprintln!("\n[Cache Hierarchy]");
        if let Some(ref l1d) = self.cpu.cache_l1d {
            eprintln!("  L1d:      {}", l1d);
        }
        if let Some(ref l1i) = self.cpu.cache_l1i {
            eprintln!("  L1i:      {}", l1i);
        }
        if let Some(ref l2) = self.cpu.cache_l2 {
            eprintln!("  L2:       {}", l2);
        }
        if let Some(ref l3) = self.cpu.cache_l3 {
            eprintln!("  L3:       {}", l3);
        }

        // Memory
        eprintln!("\n[Memory]");
        eprintln!("  Total:     {:.1} GB", self.memory.total_gb);
        eprintln!("  Available: {:.1} GB", self.memory.available_gb);

        // NUMA
        eprintln!("\n[NUMA Topology]");
        eprintln!("  Nodes: {}", self.numa.nodes);
        for node in &self.numa.layout {
            eprintln!(
                "  Node {}: CPUs {:?}, {:.1} GB",
                node.id, node.cpus, node.memory_gb
            );
        }

        // Governor
        eprintln!("\n[CPU Governor]");
        eprintln!("  Current:   {}", self.governor.current);
        eprintln!("  Available: {}", self.governor.available.join(", "));
        if !self.governor.is_performance {
            eprintln!("  ⚠️  WARNING: Not in 'performance' mode!");
            eprintln!("     Run: sudo cpupower frequency-set -g performance");
        }

        // Rust
        eprintln!("\n[Rust Toolchain]");
        eprintln!("  Version:   {}", self.rust.version);
        eprintln!("  Channel:   {}", self.rust.channel);
        eprintln!("  Target:    {}", self.rust.target);
        eprintln!("  Profile:   {}", self.rust.profile);
        eprintln!(
            "  LTO:       {}",
            if self.rust.lto { "enabled" } else { "disabled" }
        );
        eprintln!("  Opt-level: {}", self.rust.opt_level);

        // Git
        eprintln!("\n[Git]");
        eprintln!("  Commit:  {}", self.git.commit);
        eprintln!("  Branch:  {}", self.git.branch);
        if self.git.dirty {
            eprintln!("  ⚠️  Working tree is dirty!");
        }

        // Timestamp
        eprintln!("\n[Timestamp]");
        eprintln!("  {}", self.timestamp);

        eprintln!("\n{}", "=".repeat(60));
    }

    /// Generate JSON output for CI/CD
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "os": {
                "name": self.os.name,
                "version": self.os.version,
                "kernel": self.os.kernel,
            },
            "cpu": {
                "model": self.cpu.model,
                "cores": self.cpu.cores,
                "threads": self.cpu.threads,
                "cache_l1d": self.cpu.cache_l1d,
                "cache_l1i": self.cpu.cache_l1i,
                "cache_l2": self.cpu.cache_l2,
                "cache_l3": self.cpu.cache_l3,
                "frequency_mhz": self.cpu.frequency_mhz,
            },
            "memory": {
                "total_gb": self.memory.total_gb,
                "available_gb": self.memory.available_gb,
            },
            "numa": {
                "nodes": self.numa.nodes,
                "layout": self.numa.layout.iter().map(|n| {
                    serde_json::json!({
                        "id": n.id,
                        "cpus": n.cpus,
                        "memory_gb": n.memory_gb,
                    })
                }).collect::<Vec<_>>(),
            },
            "governor": {
                "current": self.governor.current,
                "available": self.governor.available,
                "is_performance": self.governor.is_performance,
            },
            "rust": {
                "version": self.rust.version,
                "channel": self.rust.channel,
                "target": self.rust.target,
                "profile": self.rust.profile,
                "lto": self.rust.lto,
                "opt_level": self.rust.opt_level,
            },
            "git": {
                "commit": self.git.commit,
                "branch": self.git.branch,
                "dirty": self.git.dirty,
            },
            "timestamp": self.timestamp,
            "is_reference_platform": self.is_reference_platform,
        })
        .to_string()
    }

    /// Print compact one-line summary
    pub fn print_summary(&self) {
        let platform_marker = if self.is_reference_platform {
            "✓"
        } else {
            "⚠"
        };
        eprintln!(
            "{} {} | {} ({} cores) | {} | {} | {} | {}",
            platform_marker,
            self.os.name,
            self.cpu.model,
            self.cpu.cores,
            format!("{:.0}GB", self.memory.total_gb),
            self.governor.current,
            self.rust.version,
            self.git.commit,
        );
    }

    /// Generate markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# Benchmark Environment Report\n\n");
        md.push_str(&format!("**Generated:** {}\n\n", self.timestamp));

        // Platform validation
        if !self.is_reference_platform {
            md.push_str("> **WARNING: NOT RUNNING ON REFERENCE PLATFORM**\n");
            md.push_str(">\n");
            md.push_str("> Results are for development only. Do not use for:\n");
            md.push_str("> - Performance gate decisions\n");
            md.push_str("> - Redis competitiveness claims\n");
            md.push_str("> - PR performance reports\n");
            if self.os.name.to_lowercase().contains("darwin") {
                md.push_str(">\n> *macOS detected. Use Linux for official benchmarks.*\n");
            }
            md.push_str("\n");
        } else {
            md.push_str("**Status:** Running on reference platform\n\n");
        }

        // OS
        md.push_str("## Operating System\n\n");
        md.push_str("| Property | Value |\n");
        md.push_str("|----------|-------|\n");
        md.push_str(&format!("| Name | {} |\n", self.os.name));
        md.push_str(&format!("| Version | {} |\n", self.os.version));
        md.push_str(&format!("| Kernel | {} |\n", self.os.kernel));
        md.push_str("\n");

        // CPU
        md.push_str("## CPU\n\n");
        md.push_str("| Property | Value |\n");
        md.push_str("|----------|-------|\n");
        md.push_str(&format!("| Model | {} |\n", self.cpu.model));
        md.push_str(&format!("| Physical Cores | {} |\n", self.cpu.cores));
        md.push_str(&format!("| Logical Threads | {} |\n", self.cpu.threads));
        if let Some(ref freq) = self.cpu.frequency_mhz {
            md.push_str(&format!("| Frequency | {} MHz |\n", freq));
        }
        md.push_str("\n");

        // Cache
        md.push_str("## Cache Hierarchy\n\n");
        md.push_str("| Level | Size |\n");
        md.push_str("|-------|------|\n");
        if let Some(ref l1d) = self.cpu.cache_l1d {
            md.push_str(&format!("| L1d | {} |\n", l1d));
        }
        if let Some(ref l1i) = self.cpu.cache_l1i {
            md.push_str(&format!("| L1i | {} |\n", l1i));
        }
        if let Some(ref l2) = self.cpu.cache_l2 {
            md.push_str(&format!("| L2 | {} |\n", l2));
        }
        if let Some(ref l3) = self.cpu.cache_l3 {
            md.push_str(&format!("| L3 | {} |\n", l3));
        }
        md.push_str("\n");

        // Memory
        md.push_str("## Memory\n\n");
        md.push_str("| Property | Value |\n");
        md.push_str("|----------|-------|\n");
        md.push_str(&format!("| Total | {:.1} GB |\n", self.memory.total_gb));
        md.push_str(&format!(
            "| Available | {:.1} GB |\n",
            self.memory.available_gb
        ));
        md.push_str("\n");

        // NUMA
        md.push_str("## NUMA Topology\n\n");
        md.push_str(&format!("**Nodes:** {}\n\n", self.numa.nodes));
        if !self.numa.layout.is_empty() {
            md.push_str("| Node | CPUs | Memory |\n");
            md.push_str("|------|------|--------|\n");
            for node in &self.numa.layout {
                md.push_str(&format!(
                    "| {} | {:?} | {:.1} GB |\n",
                    node.id, node.cpus, node.memory_gb
                ));
            }
            md.push_str("\n");
        }

        // Governor
        md.push_str("## CPU Governor\n\n");
        md.push_str("| Property | Value |\n");
        md.push_str("|----------|-------|\n");
        md.push_str(&format!("| Current | {} |\n", self.governor.current));
        md.push_str(&format!(
            "| Available | {} |\n",
            self.governor.available.join(", ")
        ));
        if !self.governor.is_performance {
            md.push_str("\n> **WARNING:** Not in 'performance' mode!\n");
            md.push_str("> Run: `sudo cpupower frequency-set -g performance`\n");
        }
        md.push_str("\n");

        // Rust
        md.push_str("## Rust Toolchain\n\n");
        md.push_str("| Property | Value |\n");
        md.push_str("|----------|-------|\n");
        md.push_str(&format!("| Version | {} |\n", self.rust.version));
        md.push_str(&format!("| Channel | {} |\n", self.rust.channel));
        md.push_str(&format!("| Target | {} |\n", self.rust.target));
        md.push_str(&format!("| Profile | {} |\n", self.rust.profile));
        md.push_str(&format!(
            "| LTO | {} |\n",
            if self.rust.lto { "enabled" } else { "disabled" }
        ));
        md.push_str(&format!("| Opt-level | {} |\n", self.rust.opt_level));
        md.push_str("\n");

        // Git
        md.push_str("## Git\n\n");
        md.push_str("| Property | Value |\n");
        md.push_str("|----------|-------|\n");
        md.push_str(&format!("| Commit | `{}` |\n", self.git.commit));
        md.push_str(&format!("| Branch | {} |\n", self.git.branch));
        if self.git.dirty {
            md.push_str("| Status | **DIRTY** |\n");
        } else {
            md.push_str("| Status | Clean |\n");
        }
        md.push_str("\n");

        md
    }

    /// Write markdown report to file
    pub fn write_report(&self, output_dir: &Path) -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(output_dir)?;

        let timestamp = self
            .timestamp
            .replace(":", "-")
            .replace("T", "_")
            .replace("Z", "");
        let filename = format!("environment_{}.md", timestamp);
        let filepath = output_dir.join(&filename);

        let mut file = std::fs::File::create(&filepath)?;
        file.write_all(self.to_markdown().as_bytes())?;

        eprintln!("Environment report written to: {}", filepath.display());
        Ok(filepath)
    }

    /// Write JSON report to file
    pub fn write_json(&self, output_dir: &Path) -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(output_dir)?;

        let timestamp = self
            .timestamp
            .replace(":", "-")
            .replace("T", "_")
            .replace("Z", "");
        let filename = format!("environment_{}.json", timestamp);
        let filepath = output_dir.join(&filename);

        let mut file = std::fs::File::create(&filepath)?;
        file.write_all(self.to_json().as_bytes())?;

        eprintln!("Environment JSON written to: {}", filepath.display());
        Ok(filepath)
    }
}

// =============================================================================
// Platform-specific capture functions
// =============================================================================

fn capture_os_info() -> OsInfo {
    #[cfg(target_os = "linux")]
    {
        let name = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .map(|l| {
                        l.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "Linux".to_string());

        let version = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("VERSION_ID="))
                    .map(|l| {
                        l.trim_start_matches("VERSION_ID=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "unknown".to_string());

        let kernel = Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        OsInfo {
            name,
            version,
            kernel,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let name = "macOS".to_string();

        let version = Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let kernel = Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        OsInfo {
            name,
            version,
            kernel,
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        OsInfo {
            name: std::env::consts::OS.to_string(),
            version: "unknown".to_string(),
            kernel: "unknown".to_string(),
        }
    }
}

fn capture_cpu_info() -> CpuInfo {
    #[cfg(target_os = "linux")]
    {
        let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();

        let model = cpuinfo
            .lines()
            .find(|l| l.starts_with("model name"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let threads = cpuinfo
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count();

        // Physical cores (unique core ids)
        let core_ids: std::collections::HashSet<_> = cpuinfo
            .lines()
            .filter(|l| l.starts_with("core id"))
            .filter_map(|l| l.split(':').nth(1))
            .map(|s| s.trim())
            .collect();
        let cores = if core_ids.is_empty() {
            threads
        } else {
            core_ids.len()
        };

        // Cache info from sysfs
        let cache_l1d = read_cache_size("/sys/devices/system/cpu/cpu0/cache/index0/size");
        let cache_l1i = read_cache_size("/sys/devices/system/cpu/cpu0/cache/index1/size");
        let cache_l2 = read_cache_size("/sys/devices/system/cpu/cpu0/cache/index2/size");
        let cache_l3 = read_cache_size("/sys/devices/system/cpu/cpu0/cache/index3/size");

        // Frequency
        let frequency_mhz =
            fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|khz| khz / 1000);

        CpuInfo {
            model,
            cores,
            threads,
            cache_l1d,
            cache_l1i,
            cache_l2,
            cache_l3,
            frequency_mhz,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let model = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let cores = Command::new("sysctl")
            .args(["-n", "hw.physicalcpu"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(1);

        let threads = Command::new("sysctl")
            .args(["-n", "hw.logicalcpu"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(cores);

        let cache_l1d = Command::new("sysctl")
            .args(["-n", "hw.l1dcachesize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .map(|b| format_cache_size(b));

        let cache_l1i = Command::new("sysctl")
            .args(["-n", "hw.l1icachesize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .map(|b| format_cache_size(b));

        let cache_l2 = Command::new("sysctl")
            .args(["-n", "hw.l2cachesize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .map(|b| format_cache_size(b));

        let cache_l3 = Command::new("sysctl")
            .args(["-n", "hw.l3cachesize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .map(|b| format_cache_size(b));

        CpuInfo {
            model,
            cores,
            threads,
            cache_l1d,
            cache_l1i,
            cache_l2,
            cache_l3,
            frequency_mhz: None,
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        CpuInfo {
            model: "unknown".to_string(),
            cores: 1,
            threads: std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1),
            cache_l1d: None,
            cache_l1i: None,
            cache_l2: None,
            cache_l3: None,
            frequency_mhz: None,
        }
    }
}

fn capture_memory_info() -> MemoryInfo {
    #[cfg(target_os = "linux")]
    {
        let meminfo = fs::read_to_string("/proc/meminfo").unwrap_or_default();

        let parse_kb = |prefix: &str| -> Option<u64> {
            meminfo
                .lines()
                .find(|l| l.starts_with(prefix))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|s| s.parse().ok()))
        };

        let total_kb = parse_kb("MemTotal:").unwrap_or(0);
        let available_kb = parse_kb("MemAvailable:").unwrap_or(0);

        MemoryInfo {
            total_gb: total_kb as f64 / 1024.0 / 1024.0,
            available_gb: available_kb as f64 / 1024.0 / 1024.0,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let total_bytes = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .unwrap_or(0);

        // Available memory is harder to get on macOS, approximate from vm_stat
        let available_bytes = Command::new("vm_stat")
            .output()
            .ok()
            .map(|o| {
                let output = String::from_utf8_lossy(&o.stdout);
                let page_size: u64 = 4096; // Default page size
                let free_pages: u64 = output
                    .lines()
                    .find(|l| l.contains("Pages free"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|s| s.trim().trim_end_matches('.').parse().ok())
                    .unwrap_or(0);
                free_pages * page_size
            })
            .unwrap_or(0);

        MemoryInfo {
            total_gb: total_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
            available_gb: available_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        MemoryInfo {
            total_gb: 0.0,
            available_gb: 0.0,
        }
    }
}

fn capture_numa_info() -> NumaInfo {
    #[cfg(target_os = "linux")]
    {
        // Check if NUMA is available
        let numa_nodes = fs::read_dir("/sys/devices/system/node")
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("node"))
                    .count()
            })
            .unwrap_or(1);

        let mut layout = Vec::new();

        for node_id in 0..numa_nodes {
            let node_path = format!("/sys/devices/system/node/node{}", node_id);

            // Get CPUs for this node
            let cpus = fs::read_to_string(format!("{}/cpulist", node_path))
                .ok()
                .map(|s| parse_cpu_list(s.trim()))
                .unwrap_or_default();

            // Get memory for this node (from meminfo)
            let memory_kb = fs::read_to_string(format!("{}/meminfo", node_path))
                .ok()
                .and_then(|content| {
                    content
                        .lines()
                        .find(|l| l.contains("MemTotal"))
                        .and_then(|l| {
                            l.split_whitespace()
                                .nth(3)
                                .and_then(|s| s.parse::<u64>().ok())
                        })
                })
                .unwrap_or(0);

            layout.push(NumaNode {
                id: node_id,
                cpus,
                memory_gb: memory_kb as f64 / 1024.0 / 1024.0,
            });
        }

        NumaInfo {
            nodes: numa_nodes,
            layout,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // macOS and others don't expose NUMA in the same way
        let threads = std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(1);

        NumaInfo {
            nodes: 1,
            layout: vec![NumaNode {
                id: 0,
                cpus: (0..threads).collect(),
                memory_gb: capture_memory_info().total_gb,
            }],
        }
    }
}

fn capture_governor_info() -> GovernorInfo {
    #[cfg(target_os = "linux")]
    {
        let current = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let available =
            fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors")
                .map(|s| s.split_whitespace().map(|s| s.to_string()).collect())
                .unwrap_or_else(|_| vec!["unknown".to_string()]);

        let is_performance = current == "performance";

        GovernorInfo {
            current,
            available,
            is_performance,
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        GovernorInfo {
            current: "N/A (not Linux)".to_string(),
            available: vec![],
            is_performance: false,
        }
    }
}

fn capture_rust_info() -> RustInfo {
    let version_output = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let version = version_output
        .split_whitespace()
        .nth(1)
        .unwrap_or("unknown")
        .to_string();

    let channel = if version.contains("nightly") {
        "nightly"
    } else if version.contains("beta") {
        "beta"
    } else {
        "stable"
    }
    .to_string();

    let target = Command::new("rustc")
        .args(["--print", "target-triple"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // These are compile-time values, best effort
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
    .to_string();

    // LTO detection is tricky at runtime, assume based on profile
    let lto = !cfg!(debug_assertions);

    let opt_level = if cfg!(debug_assertions) { "0" } else { "3" }.to_string();

    RustInfo {
        version,
        channel,
        target,
        profile,
        lto,
        opt_level,
    }
}

fn capture_git_info() -> GitInfo {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    GitInfo {
        commit,
        branch,
        dirty,
    }
}

// =============================================================================
// Helper functions
// =============================================================================

#[cfg(target_os = "linux")]
fn read_cache_size(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn format_cache_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MB", bytes / 1024 / 1024)
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(target_os = "linux")]
fn parse_cpu_list(s: &str) -> Vec<usize> {
    let mut cpus = Vec::new();
    for part in s.split(',') {
        if part.contains('-') {
            let mut range = part.split('-');
            if let (Some(start), Some(end)) = (range.next(), range.next()) {
                if let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) {
                    cpus.extend(start..=end);
                }
            }
        } else if let Ok(cpu) = part.parse() {
            cpus.push(cpu);
        }
    }
    cpus
}

// =============================================================================
// Perf Integration
// =============================================================================

/// Perf counter configuration for detailed performance analysis
#[derive(Debug, Clone)]
pub struct PerfConfig {
    /// Events to monitor
    pub events: Vec<String>,
    /// Output file for perf data
    pub output_file: Option<String>,
}

impl Default for PerfConfig {
    fn default() -> Self {
        Self {
            events: vec![
                "cache-misses".to_string(),
                "cache-references".to_string(),
                "branch-misses".to_string(),
                "branch-instructions".to_string(),
                "LLC-loads".to_string(),
                "LLC-load-misses".to_string(),
                "cycles".to_string(),
                "instructions".to_string(),
            ],
            output_file: None,
        }
    }
}

impl PerfConfig {
    /// Generate perf stat command arguments
    pub fn perf_stat_args(&self) -> Vec<String> {
        let mut args = vec!["stat".to_string()];

        // Add events
        for event in &self.events {
            args.push("-e".to_string());
            args.push(event.clone());
        }

        // Detailed output
        args.push("-d".to_string());
        args.push("-d".to_string());
        args.push("-d".to_string());

        args
    }

    /// Generate perf record command arguments
    pub fn perf_record_args(&self) -> Vec<String> {
        let mut args = vec!["record".to_string()];

        // Add events
        for event in &self.events {
            args.push("-e".to_string());
            args.push(event.clone());
        }

        // Call graph
        args.push("-g".to_string());

        // Output file
        if let Some(ref output) = self.output_file {
            args.push("-o".to_string());
            args.push(output.clone());
        }

        args
    }

    /// Check if perf is available
    pub fn is_available() -> bool {
        Command::new("perf")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Print perf availability and configuration
    pub fn print_status(&self) {
        eprintln!("\n[Perf Configuration]");
        if Self::is_available() {
            eprintln!("  Status: Available");
            eprintln!("  Events: {}", self.events.join(", "));
        } else {
            eprintln!("  Status: NOT AVAILABLE");
            eprintln!("  Install: sudo apt install linux-tools-generic");
        }
    }
}

/// Results from a perf stat run
#[derive(Debug, Clone, Default)]
pub struct PerfResults {
    pub cache_misses: Option<u64>,
    pub cache_references: Option<u64>,
    pub branch_misses: Option<u64>,
    pub branch_instructions: Option<u64>,
    pub llc_loads: Option<u64>,
    pub llc_load_misses: Option<u64>,
    pub cycles: Option<u64>,
    pub instructions: Option<u64>,
}

impl PerfResults {
    /// Calculate cache miss rate
    pub fn cache_miss_rate(&self) -> Option<f64> {
        match (self.cache_misses, self.cache_references) {
            (Some(misses), Some(refs)) if refs > 0 => Some(misses as f64 / refs as f64 * 100.0),
            _ => None,
        }
    }

    /// Calculate branch miss rate
    pub fn branch_miss_rate(&self) -> Option<f64> {
        match (self.branch_misses, self.branch_instructions) {
            (Some(misses), Some(total)) if total > 0 => Some(misses as f64 / total as f64 * 100.0),
            _ => None,
        }
    }

    /// Calculate LLC miss rate
    pub fn llc_miss_rate(&self) -> Option<f64> {
        match (self.llc_load_misses, self.llc_loads) {
            (Some(misses), Some(loads)) if loads > 0 => Some(misses as f64 / loads as f64 * 100.0),
            _ => None,
        }
    }

    /// Calculate IPC (instructions per cycle)
    pub fn ipc(&self) -> Option<f64> {
        match (self.instructions, self.cycles) {
            (Some(instr), Some(cyc)) if cyc > 0 => Some(instr as f64 / cyc as f64),
            _ => None,
        }
    }

    /// Print formatted results
    pub fn print_report(&self) {
        eprintln!("\n[Perf Results]");

        if let Some(rate) = self.cache_miss_rate() {
            eprintln!("  Cache miss rate:  {:.2}%", rate);
        }
        if let Some(rate) = self.branch_miss_rate() {
            eprintln!("  Branch miss rate: {:.2}%", rate);
        }
        if let Some(rate) = self.llc_miss_rate() {
            eprintln!("  LLC miss rate:    {:.2}%", rate);
        }
        if let Some(ipc) = self.ipc() {
            eprintln!("  IPC:              {:.2}", ipc);
        }

        eprintln!("\n  Raw counters:");
        if let Some(v) = self.cache_misses {
            eprintln!("    cache-misses:       {:>15}", format_count(v));
        }
        if let Some(v) = self.cache_references {
            eprintln!("    cache-references:   {:>15}", format_count(v));
        }
        if let Some(v) = self.branch_misses {
            eprintln!("    branch-misses:      {:>15}", format_count(v));
        }
        if let Some(v) = self.branch_instructions {
            eprintln!("    branch-instructions:{:>15}", format_count(v));
        }
        if let Some(v) = self.llc_loads {
            eprintln!("    LLC-loads:          {:>15}", format_count(v));
        }
        if let Some(v) = self.llc_load_misses {
            eprintln!("    LLC-load-misses:    {:>15}", format_count(v));
        }
        if let Some(v) = self.cycles {
            eprintln!("    cycles:             {:>15}", format_count(v));
        }
        if let Some(v) = self.instructions {
            eprintln!("    instructions:       {:>15}", format_count(v));
        }
    }

    /// Generate markdown report section
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("## Perf Counter Results\n\n");

        md.push_str("### Derived Metrics\n\n");
        md.push_str("| Metric | Value |\n");
        md.push_str("|--------|-------|\n");
        if let Some(rate) = self.cache_miss_rate() {
            md.push_str(&format!("| Cache miss rate | {:.2}% |\n", rate));
        }
        if let Some(rate) = self.branch_miss_rate() {
            md.push_str(&format!("| Branch miss rate | {:.2}% |\n", rate));
        }
        if let Some(rate) = self.llc_miss_rate() {
            md.push_str(&format!("| LLC miss rate | {:.2}% |\n", rate));
        }
        if let Some(ipc) = self.ipc() {
            md.push_str(&format!("| IPC (instructions/cycle) | {:.2} |\n", ipc));
        }
        md.push_str("\n");

        md.push_str("### Raw Counters\n\n");
        md.push_str("| Counter | Value |\n");
        md.push_str("|---------|-------|\n");
        if let Some(v) = self.cache_misses {
            md.push_str(&format!("| cache-misses | {} |\n", format_count(v)));
        }
        if let Some(v) = self.cache_references {
            md.push_str(&format!("| cache-references | {} |\n", format_count(v)));
        }
        if let Some(v) = self.branch_misses {
            md.push_str(&format!("| branch-misses | {} |\n", format_count(v)));
        }
        if let Some(v) = self.branch_instructions {
            md.push_str(&format!("| branch-instructions | {} |\n", format_count(v)));
        }
        if let Some(v) = self.llc_loads {
            md.push_str(&format!("| LLC-loads | {} |\n", format_count(v)));
        }
        if let Some(v) = self.llc_load_misses {
            md.push_str(&format!("| LLC-load-misses | {} |\n", format_count(v)));
        }
        if let Some(v) = self.cycles {
            md.push_str(&format!("| cycles | {} |\n", format_count(v)));
        }
        if let Some(v) = self.instructions {
            md.push_str(&format!("| instructions | {} |\n", format_count(v)));
        }
        md.push_str("\n");

        md
    }
}

fn format_count(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

// =============================================================================
// Latency Percentile Tracking
// =============================================================================

/// Latency percentile statistics for a benchmark
#[derive(Debug, Clone, Default)]
pub struct LatencyStats {
    /// Benchmark name
    pub name: String,
    /// Minimum latency in nanoseconds
    pub min_ns: f64,
    /// Maximum latency in nanoseconds
    pub max_ns: f64,
    /// Mean latency in nanoseconds
    pub mean_ns: f64,
    /// Median (p50) latency in nanoseconds
    pub p50_ns: f64,
    /// 90th percentile latency in nanoseconds
    pub p90_ns: f64,
    /// 95th percentile latency in nanoseconds
    pub p95_ns: f64,
    /// 99th percentile latency in nanoseconds
    pub p99_ns: f64,
    /// 99.9th percentile latency in nanoseconds
    pub p999_ns: f64,
    /// Standard deviation in nanoseconds
    pub std_dev_ns: f64,
    /// Number of samples
    pub sample_count: usize,
}

impl LatencyStats {
    /// Create from a sorted list of latency samples (in nanoseconds)
    pub fn from_samples(name: &str, samples: &mut [f64]) -> Self {
        if samples.is_empty() {
            return Self {
                name: name.to_string(),
                ..Default::default()
            };
        }

        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let n = samples.len();
        let min_ns = samples[0];
        let max_ns = samples[n - 1];

        // Calculate mean
        let sum: f64 = samples.iter().sum();
        let mean_ns = sum / n as f64;

        // Calculate percentiles
        let p50_ns = percentile(samples, 50.0);
        let p90_ns = percentile(samples, 90.0);
        let p95_ns = percentile(samples, 95.0);
        let p99_ns = percentile(samples, 99.0);
        let p999_ns = percentile(samples, 99.9);

        // Calculate standard deviation
        let variance: f64 = samples.iter().map(|x| (x - mean_ns).powi(2)).sum::<f64>() / n as f64;
        let std_dev_ns = variance.sqrt();

        Self {
            name: name.to_string(),
            min_ns,
            max_ns,
            mean_ns,
            p50_ns,
            p90_ns,
            p95_ns,
            p99_ns,
            p999_ns,
            std_dev_ns,
            sample_count: n,
        }
    }

    /// Format latency value with appropriate unit
    pub fn format_latency(ns: f64) -> String {
        if ns >= 1_000_000_000.0 {
            format!("{:.2} s", ns / 1_000_000_000.0)
        } else if ns >= 1_000_000.0 {
            format!("{:.2} ms", ns / 1_000_000.0)
        } else if ns >= 1_000.0 {
            format!("{:.2} µs", ns / 1_000.0)
        } else {
            format!("{:.0} ns", ns)
        }
    }

    /// Print formatted stats
    pub fn print_report(&self) {
        eprintln!("\n[Latency Stats: {}]", self.name);
        eprintln!("  Samples:  {}", self.sample_count);
        eprintln!("  Min:      {}", Self::format_latency(self.min_ns));
        eprintln!("  Max:      {}", Self::format_latency(self.max_ns));
        eprintln!("  Mean:     {}", Self::format_latency(self.mean_ns));
        eprintln!("  Std Dev:  {}", Self::format_latency(self.std_dev_ns));
        eprintln!("  Percentiles:");
        eprintln!("    p50:    {}", Self::format_latency(self.p50_ns));
        eprintln!("    p90:    {}", Self::format_latency(self.p90_ns));
        eprintln!("    p95:    {}", Self::format_latency(self.p95_ns));
        eprintln!("    p99:    {}", Self::format_latency(self.p99_ns));
        eprintln!("    p99.9:  {}", Self::format_latency(self.p999_ns));
    }

    /// Generate markdown table row
    pub fn to_markdown_row(&self) -> String {
        format!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            self.name,
            Self::format_latency(self.mean_ns),
            Self::format_latency(self.p50_ns),
            Self::format_latency(self.p90_ns),
            Self::format_latency(self.p95_ns),
            Self::format_latency(self.p99_ns),
            Self::format_latency(self.p999_ns),
        )
    }
}

/// Calculate percentile from sorted samples
fn percentile(sorted_samples: &[f64], p: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    let n = sorted_samples.len();
    let idx = (p / 100.0 * (n - 1) as f64).round() as usize;
    let idx = idx.min(n - 1);
    sorted_samples[idx]
}

/// Collection of latency stats for multiple benchmarks
#[derive(Debug, Clone, Default)]
pub struct LatencyReport {
    pub stats: Vec<LatencyStats>,
}

impl LatencyReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, stats: LatencyStats) {
        self.stats.push(stats);
    }

    /// Generate markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("## Latency Percentile Report\n\n");
        md.push_str("| Benchmark | Mean | p50 | p90 | p95 | p99 | p99.9 |\n");
        md.push_str("|-----------|------|-----|-----|-----|-----|-------|\n");

        for stat in &self.stats {
            md.push_str(&stat.to_markdown_row());
            md.push('\n');
        }

        md.push_str("\n### Key Observations\n\n");

        // Find benchmarks with high tail latency (p99 > 2x p50)
        let high_tail: Vec<_> = self
            .stats
            .iter()
            .filter(|s| s.p99_ns > s.p50_ns * 2.0)
            .collect();

        if !high_tail.is_empty() {
            md.push_str("**Benchmarks with high tail latency (p99 > 2x median):**\n\n");
            for stat in high_tail {
                md.push_str(&format!(
                    "- `{}`: p99 ({}) is {:.1}x median ({})\n",
                    stat.name,
                    LatencyStats::format_latency(stat.p99_ns),
                    stat.p99_ns / stat.p50_ns,
                    LatencyStats::format_latency(stat.p50_ns),
                ));
            }
            md.push('\n');
        }

        // Find benchmarks with high variance (std_dev > 50% of mean)
        let high_variance: Vec<_> = self
            .stats
            .iter()
            .filter(|s| s.std_dev_ns > s.mean_ns * 0.5)
            .collect();

        if !high_variance.is_empty() {
            md.push_str("**Benchmarks with high variance (std_dev > 50% mean):**\n\n");
            for stat in high_variance {
                md.push_str(&format!(
                    "- `{}`: std_dev ({}) is {:.0}% of mean ({})\n",
                    stat.name,
                    LatencyStats::format_latency(stat.std_dev_ns),
                    (stat.std_dev_ns / stat.mean_ns) * 100.0,
                    LatencyStats::format_latency(stat.mean_ns),
                ));
            }
            md.push('\n');
        }

        md
    }

    /// Print summary
    pub fn print_summary(&self) {
        eprintln!("\n{}", "=".repeat(60));
        eprintln!("LATENCY PERCENTILE SUMMARY");
        eprintln!("{}", "=".repeat(60));

        for stat in &self.stats {
            stat.print_report();
        }
    }
}

/// Manual latency collector for custom benchmarks
///
/// Use this when you need to collect raw latency samples outside of Criterion.
/// Criterion handles percentiles internally, but this is useful for:
/// - Custom measurement scenarios
/// - Combining multiple benchmark results
/// - Post-processing analysis
#[derive(Debug)]
pub struct LatencyCollector {
    name: String,
    samples: Vec<f64>,
    capacity: usize,
}

impl LatencyCollector {
    /// Create a new collector with expected sample count
    pub fn new(name: &str, expected_samples: usize) -> Self {
        Self {
            name: name.to_string(),
            samples: Vec::with_capacity(expected_samples),
            capacity: expected_samples,
        }
    }

    /// Record a latency sample in nanoseconds
    pub fn record_ns(&mut self, latency_ns: f64) {
        self.samples.push(latency_ns);
    }

    /// Record a latency sample from Duration
    pub fn record(&mut self, duration: std::time::Duration) {
        self.samples.push(duration.as_nanos() as f64);
    }

    /// Get the number of samples collected
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Check if collector is empty
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Finalize and compute statistics
    pub fn finalize(mut self) -> LatencyStats {
        LatencyStats::from_samples(&self.name, &mut self.samples)
    }

    /// Get current samples without consuming
    pub fn samples(&self) -> &[f64] {
        &self.samples
    }

    /// Reset collector for reuse
    pub fn reset(&mut self) {
        self.samples.clear();
    }
}

// =============================================================================
// Benchmark Report Writer
// =============================================================================

/// Full benchmark report with all results
pub struct BenchmarkReport {
    pub environment: BenchEnvironment,
    pub perf_results: Option<PerfResults>,
    pub facade_tax: FacadeTaxReport,
    pub contention_results: Option<ContentionResults>,
}

/// Facade tax analysis results
#[derive(Debug, Clone, Default)]
pub struct FacadeTaxReport {
    /// A0 tier results (raw data structure) - operation -> nanoseconds
    pub tier_a0: Vec<(String, f64)>,
    /// A1 tier results (+ snapshot/commit) - operation -> nanoseconds
    pub tier_a1: Vec<(String, f64)>,
    /// B tier results (+ facade) - operation -> nanoseconds
    pub tier_b: Vec<(String, f64)>,
}

impl FacadeTaxReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_a0(&mut self, name: &str, ns: f64) {
        self.tier_a0.push((name.to_string(), ns));
    }

    pub fn add_a1(&mut self, name: &str, ns: f64) {
        self.tier_a1.push((name.to_string(), ns));
    }

    pub fn add_b(&mut self, name: &str, ns: f64) {
        self.tier_b.push((name.to_string(), ns));
    }

    /// Generate markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# Facade Tax Report\n\n");
        md.push_str("This report analyzes the abstraction overhead across tiers.\n\n");

        md.push_str("## Target Ratios\n\n");
        md.push_str("| Ratio | Target | Description |\n");
        md.push_str("|-------|--------|-------------|\n");
        md.push_str("| A1/A0 | < 20x | Correctness overhead (snapshot + commit) |\n");
        md.push_str("| B/A1 | < 10x | Facade overhead |\n");
        md.push_str("| B/A0 | < 50x | Total abstraction cost |\n");
        md.push_str("\n");

        md.push_str("## Operation Analysis\n\n");
        md.push_str("### GET Operations\n\n");
        md.push_str("| Tier | Benchmark | Latency | Ratio vs A0 |\n");
        md.push_str("|------|-----------|---------|-------------|\n");

        // Find GET benchmarks
        let a0_get = self.tier_a0.iter().find(|(n, _)| n.contains("get"));
        let a1_get = self.tier_a1.iter().find(|(n, _)| n.contains("get"));
        let b_get = self.tier_b.iter().find(|(n, _)| n.contains("get"));

        if let Some((name, ns)) = a0_get {
            md.push_str(&format!("| A0 | {} | {:.0} ns | 1.0x |\n", name, ns));
            let a0_ns = *ns;

            if let Some((name, ns)) = a1_get {
                let ratio = ns / a0_ns;
                let status = if ratio < 20.0 { "" } else { " **EXCEEDS**" };
                md.push_str(&format!(
                    "| A1 | {} | {:.0} ns | {:.1}x{} |\n",
                    name, ns, ratio, status
                ));
            }

            if let Some((name, ns)) = b_get {
                let ratio = ns / a0_ns;
                let status = if ratio < 50.0 { "" } else { " **EXCEEDS**" };
                md.push_str(&format!(
                    "| B | {} | {:.0} ns | {:.1}x{} |\n",
                    name, ns, ratio, status
                ));
            }
        }

        md.push_str("\n### PUT Operations\n\n");
        md.push_str("| Tier | Benchmark | Latency | Ratio vs A0 |\n");
        md.push_str("|------|-----------|---------|-------------|\n");

        // Find PUT benchmarks
        let a0_put = self.tier_a0.iter().find(|(n, _)| n.contains("put"));
        let a1_put = self.tier_a1.iter().find(|(n, _)| n.contains("put"));
        let b_put = self.tier_b.iter().find(|(n, _)| n.contains("put"));

        if let Some((name, ns)) = a0_put {
            md.push_str(&format!("| A0 | {} | {:.0} ns | 1.0x |\n", name, ns));
            let a0_ns = *ns;

            if let Some((name, ns)) = a1_put {
                let ratio = ns / a0_ns;
                let status = if ratio < 20.0 { "" } else { " **EXCEEDS**" };
                md.push_str(&format!(
                    "| A1 | {} | {:.0} ns | {:.1}x{} |\n",
                    name, ns, ratio, status
                ));
            }

            if let Some((name, ns)) = b_put {
                let ratio = ns / a0_ns;
                let status = if ratio < 50.0 { "" } else { " **EXCEEDS**" };
                md.push_str(&format!(
                    "| B | {} | {:.0} ns | {:.1}x{} |\n",
                    name, ns, ratio, status
                ));
            }
        }

        md.push_str("\n## All Results by Tier\n\n");

        if !self.tier_a0.is_empty() {
            md.push_str("### Tier A0 (Core Data Structure)\n\n");
            md.push_str("| Benchmark | Latency |\n");
            md.push_str("|-----------|--------|\n");
            for (name, ns) in &self.tier_a0 {
                md.push_str(&format!("| {} | {:.0} ns |\n", name, ns));
            }
            md.push_str("\n");
        }

        if !self.tier_a1.is_empty() {
            md.push_str("### Tier A1 (Engine Microbenchmarks)\n\n");
            md.push_str("| Benchmark | Latency |\n");
            md.push_str("|-----------|--------|\n");
            for (name, ns) in &self.tier_a1 {
                md.push_str(&format!("| {} | {:.0} ns |\n", name, ns));
            }
            md.push_str("\n");
        }

        if !self.tier_b.is_empty() {
            md.push_str("### Tier B (Primitive Facades)\n\n");
            md.push_str("| Benchmark | Latency |\n");
            md.push_str("|-----------|--------|\n");
            for (name, ns) in &self.tier_b {
                md.push_str(&format!("| {} | {:.0} ns |\n", name, ns));
            }
            md.push_str("\n");
        }

        md
    }
}

/// Contention benchmark results
#[derive(Debug, Clone, Default)]
pub struct ContentionResults {
    /// (benchmark_name, thread_count, ops_per_sec)
    pub results: Vec<(String, usize, f64)>,
}

impl ContentionResults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, name: &str, threads: usize, ops_per_sec: f64) {
        self.results.push((name.to_string(), threads, ops_per_sec));
    }

    /// Generate markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# Contention Benchmark Report\n\n");
        md.push_str("## Scaling Targets\n\n");
        md.push_str("| Threads | Same-Key Target | Disjoint-Key Target |\n");
        md.push_str("|---------|-----------------|--------------------|\n");
        md.push_str("| 2 | >= 50% of 1-thread | >= 1.8x of 1-thread |\n");
        md.push_str("| 4 | >= 25% of 1-thread | >= 3.2x of 1-thread |\n");
        md.push_str("| 8 | >= 15% of 1-thread | >= 6.0x of 1-thread |\n");
        md.push_str("\n");

        // Group results by benchmark name
        let mut by_benchmark: std::collections::HashMap<String, Vec<(usize, f64)>> =
            std::collections::HashMap::new();

        for (name, threads, ops) in &self.results {
            by_benchmark
                .entry(name.clone())
                .or_default()
                .push((*threads, *ops));
        }

        md.push_str("## Results\n\n");

        for (name, results) in &by_benchmark {
            md.push_str(&format!("### {}\n\n", name));
            md.push_str("| Threads | Ops/sec | Scaling |\n");
            md.push_str("|---------|---------|--------|\n");

            // Sort by thread count
            let mut sorted = results.clone();
            sorted.sort_by_key(|(t, _)| *t);

            let baseline = sorted.iter().find(|(t, _)| *t == 1).map(|(_, ops)| *ops);

            for (threads, ops) in sorted {
                let scaling = baseline.map_or("N/A".to_string(), |b| format!("{:.2}x", ops / b));
                md.push_str(&format!("| {} | {:.0} | {} |\n", threads, ops, scaling));
            }
            md.push_str("\n");
        }

        md
    }
}

impl BenchmarkReport {
    pub fn new(environment: BenchEnvironment) -> Self {
        Self {
            environment,
            perf_results: None,
            facade_tax: FacadeTaxReport::new(),
            contention_results: None,
        }
    }

    /// Generate full markdown report
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str("# M3 Benchmark Report\n\n");
        md.push_str(&format!(
            "**Generated:** {}\n\n",
            self.environment.timestamp
        ));

        if !self.environment.is_reference_platform {
            md.push_str("> **WARNING: NOT RUNNING ON REFERENCE PLATFORM**\n");
            md.push_str("> Results are for development only.\n\n");
        }

        // Environment summary
        md.push_str("## Environment Summary\n\n");
        md.push_str(&format!("- **OS:** {}\n", self.environment.os.name));
        md.push_str(&format!("- **CPU:** {}\n", self.environment.cpu.model));
        md.push_str(&format!(
            "- **Cores:** {} physical, {} logical\n",
            self.environment.cpu.cores, self.environment.cpu.threads
        ));
        md.push_str(&format!(
            "- **Memory:** {:.1} GB\n",
            self.environment.memory.total_gb
        ));
        md.push_str(&format!(
            "- **Governor:** {}\n",
            self.environment.governor.current
        ));
        md.push_str(&format!("- **Rust:** {}\n", self.environment.rust.version));
        md.push_str(&format!(
            "- **Commit:** `{}`\n",
            self.environment.git.commit
        ));
        md.push_str("\n---\n\n");

        // Facade tax
        md.push_str(&self.facade_tax.to_markdown());
        md.push_str("\n---\n\n");

        // Contention results
        if let Some(ref contention) = self.contention_results {
            md.push_str(&contention.to_markdown());
            md.push_str("\n---\n\n");
        }

        // Perf results
        if let Some(ref perf) = self.perf_results {
            md.push_str(&perf.to_markdown());
        }

        // Full environment details
        md.push_str("---\n\n");
        md.push_str(&self.environment.to_markdown());

        md
    }

    /// Write full report to file
    pub fn write_report(&self, output_dir: &Path) -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(output_dir)?;

        let timestamp = self
            .environment
            .timestamp
            .replace(":", "-")
            .replace("T", "_")
            .replace("Z", "");
        let filename = format!("benchmark_report_{}.md", timestamp);
        let filepath = output_dir.join(&filename);

        let mut file = std::fs::File::create(&filepath)?;
        file.write_all(self.to_markdown().as_bytes())?;

        eprintln!("Full benchmark report written to: {}", filepath.display());
        Ok(filepath)
    }
}

/// Default output directory for benchmark reports
pub fn default_output_dir() -> std::path::PathBuf {
    std::path::PathBuf::from("target/benchmark-results")
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::{format_cache_size, BenchEnvironment, PerfConfig, PerfResults};

    #[test]
    fn test_capture_environment() {
        let env = BenchEnvironment::capture();
        assert!(!env.os.name.is_empty());
        assert!(!env.cpu.model.is_empty());
        assert!(env.cpu.cores >= 1);
        assert!(env.cpu.threads >= 1);
    }

    #[test]
    fn test_format_cache_size() {
        assert_eq!(format_cache_size(32 * 1024), "32 KB");
        assert_eq!(format_cache_size(8 * 1024 * 1024), "8 MB");
        assert_eq!(format_cache_size(512), "512 B");
    }

    #[test]
    fn test_perf_config_default() {
        let config = PerfConfig::default();
        assert!(config.events.contains(&"cache-misses".to_string()));
        assert!(config.events.contains(&"cycles".to_string()));
    }

    #[test]
    fn test_perf_results_rates() {
        let results = PerfResults {
            cache_misses: Some(100),
            cache_references: Some(1000),
            branch_misses: Some(50),
            branch_instructions: Some(10000),
            llc_loads: Some(500),
            llc_load_misses: Some(25),
            cycles: Some(1000000),
            instructions: Some(2000000),
        };

        assert!((results.cache_miss_rate().unwrap() - 10.0).abs() < 0.01);
        assert!((results.branch_miss_rate().unwrap() - 0.5).abs() < 0.01);
        assert!((results.llc_miss_rate().unwrap() - 5.0).abs() < 0.01);
        assert!((results.ipc().unwrap() - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_latency_stats_from_samples() {
        use super::{LatencyStats, LatencyCollector};

        // Create samples: 100 values from 1 to 100
        let mut samples: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let stats = LatencyStats::from_samples("test", &mut samples);

        assert_eq!(stats.sample_count, 100);
        assert!((stats.min_ns - 1.0).abs() < 0.01);
        assert!((stats.max_ns - 100.0).abs() < 0.01);
        assert!((stats.mean_ns - 50.5).abs() < 0.01);
        assert!((stats.p50_ns - 50.0).abs() < 1.0); // Approximate
        assert!((stats.p99_ns - 99.0).abs() < 1.0);
    }

    #[test]
    fn test_latency_collector() {
        use super::LatencyCollector;

        let mut collector = LatencyCollector::new("test_bench", 1000);
        assert!(collector.is_empty());

        for i in 1..=100 {
            collector.record_ns(i as f64);
        }
        assert_eq!(collector.len(), 100);

        let stats = collector.finalize();
        assert_eq!(stats.sample_count, 100);
        assert!((stats.mean_ns - 50.5).abs() < 0.01);
    }

    #[test]
    fn test_latency_format() {
        use super::LatencyStats;

        assert_eq!(LatencyStats::format_latency(50.0), "50 ns");
        assert_eq!(LatencyStats::format_latency(1500.0), "1.50 µs");
        assert_eq!(LatencyStats::format_latency(1500000.0), "1.50 ms");
        assert_eq!(LatencyStats::format_latency(1500000000.0), "1.50 s");
    }
}
