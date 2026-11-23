//! The machine a run happened on, and the process's peak memory.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Machine {
    pub cpu: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub ram_gb: f64,
    pub os: String,
    pub arch: String,
}

pub fn machine() -> Machine {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let cpu = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    Machine {
        cpu,
        physical_cores: sys.physical_core_count(),
        logical_cores: sys.cpus().len(),
        ram_gb: (sys.total_memory() as f64 / 1e9 * 10.0).round() / 10.0,
        os: System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string()),
        arch: std::env::consts::ARCH.to_string(),
    }
}

/// Peak resident memory of this process so far, in MB (working set on Windows).
pub fn peak_mb() -> Option<f64> {
    #[cfg(unix)]
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return None;
        }
        let raw = u.ru_maxrss as f64;
        // macOS reports bytes, Linux kilobytes.
        let bytes = if cfg!(target_os = "macos") { raw } else { raw * 1024.0 };
        Some(bytes / 1e6)
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) == 0 {
            return None;
        }
        Some(c.PeakWorkingSetSize as f64 / 1e6)
    }
    #[cfg(not(any(unix, windows)))]
    None
}
