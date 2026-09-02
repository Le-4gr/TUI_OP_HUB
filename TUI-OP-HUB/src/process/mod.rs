//! Process & Resource Management (US-PROC-01..07).
//!
//! Provides system resource monitoring and process management.

use serde::{Deserialize, Serialize};
use sysinfo::System;

use crate::error::{AppError, AppResult};

/// System resource overview (US-PROC-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemOverview {
    pub cpu_usage: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub uptime: u64,
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
}

/// Process info (US-PROC-02).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory: u64,
    pub command: String,
    pub status: String,
    pub start_time: u64,
}

/// Process manager.
pub struct ProcessManager {
    system: System,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
        }
    }

    /// Get system overview (US-PROC-01).
    pub fn overview(&mut self) -> SystemOverview {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();

        SystemOverview {
            cpu_usage: self.system.global_cpu_usage(),
            memory_used: self.system.used_memory(),
            memory_total: self.system.total_memory(),
            swap_used: self.system.used_swap(),
            swap_total: self.system.total_swap(),
            uptime: System::uptime(),
            hostname: System::host_name().unwrap_or_default(),
            os_name: System::name().unwrap_or_default(),
            os_version: System::os_version().unwrap_or_default(),
            kernel_version: System::kernel_version().unwrap_or_default(),
        }
    }

    /// List running processes (US-PROC-02).
    pub fn list_processes(&mut self) -> Vec<ProcessInfo> {
        use sysinfo::ProcessesToUpdate;
        self.system.refresh_processes(ProcessesToUpdate::All, true);

        let mut processes: Vec<ProcessInfo> = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| ProcessInfo {
                pid: pid.as_u32(),
                name: process.name().to_string_lossy().to_string(),
                cpu_usage: process.cpu_usage(),
                memory: process.memory(),
                command: process
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                status: format!("{:?}", process.status()),
                start_time: process.start_time(),
            })
            .collect();

        // Sort by CPU usage descending (US-PROC-06)
        processes.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        processes
    }

    /// Filter processes by name (US-PROC-06).
    pub fn filter_processes(&mut self, query: &str) -> Vec<ProcessInfo> {
        let processes = self.list_processes();
        let query_lower = query.to_lowercase();
        processes
            .into_iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.command.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Start a process (US-PROC-03).
    pub async fn start_process(command: &str) -> AppResult<u32> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(AppError::Validation("Empty command".to_string()));
        }

        let mut cmd = tokio::process::Command::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }

        let child = cmd
            .spawn()
            .map_err(|e| AppError::Other(format!("Failed to start process: {}", e)))?;

        let pid = child
            .id()
            .ok_or_else(|| AppError::Other("Failed to get process ID".to_string()))?;

        tracing::info!(pid, command, "process started");
        Ok(pid)
    }

    /// Stop/kill a process (US-PROC-04).
    pub fn stop_process(&mut self, pid: u32) -> AppResult<()> {
        use sysinfo::ProcessesToUpdate;
        let pid_obj = sysinfo::Pid::from_u32(pid);

        // Refresh to get the latest process info
        self.system.refresh_processes(ProcessesToUpdate::All, true);

        // Get the process and kill it
        if let Some(process) = self.system.process(pid_obj) {
            if process.kill() {
                tracing::info!(pid = pid, "process killed");
                Ok(())
            } else {
                Err(AppError::Other(format!("Failed to kill process {}", pid)))
            }
        } else {
            Err(AppError::NotFound {
                entity: "process",
                id: pid.to_string(),
            })
        }
    }

    /// Restart a managed process (US-PROC-07).
    pub async fn restart_process(&mut self, pid: u32, command: &str) -> AppResult<u32> {
        self.stop_process(pid)?;
        Self::start_process(command).await
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_overview() {
        let mut pm = ProcessManager::new();
        let overview = pm.overview();
        assert!(overview.memory_total > 0);
    }

    #[test]
    fn test_list_processes() {
        let mut pm = ProcessManager::new();
        let processes = pm.list_processes();
        assert!(!processes.is_empty());
    }

    #[test]
    fn test_filter_processes() {
        let mut pm = ProcessManager::new();
        let filtered = pm.filter_processes("init");
        // Filtering must not crash and must never return more than available
        assert!(filtered.len() <= pm.list_processes().len());
    }
}
