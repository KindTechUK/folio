//! "This computer": which device tier the machine falls in, and how long a
//! transcript takes here. Shown on Models & settings, Record and Import.
//!
//! Tiers (docs/research/windows-device-tiers-2026-11.md):
//! - Essentials: under 12 GB of memory. Most donated and refurbished laptops.
//! - Standard: 12 GB or more.
//! - Copilot+: 16 GB or more with a 40+ TOPS NPU.
//!
//! Everything is local and network-free. [`classify`], [`parse_pnp`] and
//! [`minutes_per_hour`] are the unit-tested cores; the command wrapper gathers
//! facts and fails SOFT to `Unknown`, never an app error.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Essentials,
    Standard,
    CopilotPlus,
    Unknown,
}

/// What the machine reports about itself. Any fact may be missing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    /// Memory Windows can use, in GiB (an 8 GB laptop reports about 7.7).
    pub ram_gb: Option<f64>,
    pub cpu: Option<String>,
    pub cores: Option<usize>,
    /// Friendly name of the NPU, when Windows lists one.
    pub npu: Option<String>,
}

/// Usable memory sits below the installed size (graphics and firmware reserve
/// some), so the thresholds sit below 12 and 16.
const STANDARD_MIN_GB: f64 = 11.0;
const COPILOT_MIN_GB: f64 = 14.0;

/// The tier, and why, in words for the technical details.
pub fn classify(facts: &Facts) -> (Tier, String) {
    let Some(ram) = facts.ram_gb.filter(|gb| *gb > 0.0) else {
        return (
            Tier::Unknown,
            "Couldn't read how much memory this computer has.".into(),
        );
    };
    let copilot_npu = facts.npu.is_some() && facts.cpu.as_deref().is_some_and(cpu_has_40_tops_npu);
    if ram >= COPILOT_MIN_GB && copilot_npu {
        (
            Tier::CopilotPlus,
            format!("{ram:.1} GB of memory and a 40+ TOPS AI accelerator."),
        )
    } else if ram >= STANDARD_MIN_GB {
        (Tier::Standard, format!("{ram:.1} GB of memory."))
    } else {
        (
            Tier::Essentials,
            format!("{ram:.1} GB of memory, under the 12 GB of the next group."),
        )
    }
}

/// No API reports NPU TOPS, so the 40+ TOPS processor families are listed by
/// name: Intel Core Ultra 200V (Lunar Lake) and series 3 (Panther Lake), AMD
/// Ryzen AI 300/400, Qualcomm Snapdragon X. Arrow Lake (e.g. Core Ultra 5 225U,
/// about 13 TOPS) and Ryzen 8040 (16 TOPS) fall short.
fn cpu_has_40_tops_npu(cpu: &str) -> bool {
    let lower = cpu.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let after = |word: &str| {
        tokens
            .iter()
            .position(|t| *t == word)
            .map(|i| &tokens[i + 1..])
    };
    let model_number = |rest: &[&str]| {
        rest.iter()
            .find(|t| t.len() >= 3 && t.as_bytes()[..3].iter().all(u8::is_ascii_digit))
            .map(|t| t.to_string())
    };

    if let Some(rest) = after("ultra") {
        if let Some(model) = model_number(rest) {
            return (model.starts_with('2') && model.ends_with('v')) || model.starts_with('3');
        }
    }
    if let Some(rest) = after("ryzen") {
        if rest.first() == Some(&"ai") {
            return model_number(rest).is_some_and(|m| m.starts_with('3') || m.starts_with('4'));
        }
    }
    if let Some(rest) = after("snapdragon") {
        return rest.iter().take(2).any(|t| matches!(*t, "x" | "x1" | "x2"));
    }
    false
}

/// The first device name in `Get-PnpDevice -Class ComputeAccelerator` output
/// (one friendly name per line), or `None` when it lists nothing.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn parse_pnp(output: &str) -> Option<String> {
    output
        .lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}').trim())
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// One finished import: how long the audio was and how long Folio took.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub audio_s: f64,
    pub elapsed_s: f64,
    pub diarize: bool,
}

/// Minutes Folio needs per hour of audio, before any import has been timed
/// here. Placeholders until the Windows laptop bench measures them
/// (docs/spikes/windows-bench-runbook.md).
fn placeholder_minutes_per_hour(tier: Tier) -> Option<f64> {
    match tier {
        Tier::Essentials => Some(15.0),
        Tier::Standard => Some(6.0),
        Tier::CopilotPlus => Some(4.0),
        Tier::Unknown => None,
    }
}

const RUNS_USED: usize = 5;
const MIN_RUN_AUDIO_S: f64 = 60.0;
const MEDIAN_FROM_RUNS: usize = 3;

/// Minutes per hour of audio, and whether it was measured on this computer.
/// Measured: from the last 5 imports of at least a minute without speaker
/// labels (labelling adds time that depends on the conversation). Speed varies
/// between recordings, and an overrun disappoints more than an early finish, so
/// until 3 imports are timed the estimate is the slowest of those imports and
/// the tier's placeholder; from then on the median. With no imports, the
/// placeholder alone. "Measured" means an import set the number.
pub fn minutes_per_hour(tier: Tier, runs: &[Run]) -> (Option<f64>, bool) {
    let mut recent: Vec<f64> = runs
        .iter()
        .rev()
        .filter(|r| !r.diarize && r.audio_s >= MIN_RUN_AUDIO_S && r.elapsed_s > 0.0)
        .take(RUNS_USED)
        .map(|r| r.elapsed_s / r.audio_s * 60.0)
        .collect();
    if recent.is_empty() {
        return (placeholder_minutes_per_hour(tier), false);
    }
    recent.sort_by(f64::total_cmp);
    if recent.len() < MEDIAN_FROM_RUNS {
        let slowest_run = recent[recent.len() - 1];
        return match placeholder_minutes_per_hour(tier) {
            Some(placeholder) if placeholder > slowest_run => (Some(placeholder), false),
            _ => (Some(slowest_run), true),
        };
    }
    let mid = recent.len() / 2;
    let median = if recent.len() % 2 == 1 {
        recent[mid]
    } else {
        (recent[mid - 1] + recent[mid]) / 2.0
    };
    (Some(median), true)
}

/// What the frontend shows. Mirrored by `DeviceProfile` in `app/src/Device.tsx`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProfile {
    pub tier: Tier,
    pub reason: String,
    pub ram_gb: Option<f64>,
    pub cpu: Option<String>,
    pub cores: Option<usize>,
    pub npu: Option<String>,
    pub minutes_per_hour: Option<f64>,
    pub measured: bool,
}

const RUNS_FILE: &str = "import-speed.json";
const RUNS_KEPT: usize = 20;

fn read_runs(dir: &Path) -> Vec<Run> {
    std::fs::read(dir.join(RUNS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Remember one import's timing (best-effort: a failed write only means the
/// estimate learns nothing from this run).
pub(crate) fn record_run(app: &tauri::AppHandle, run: Run) {
    use tauri::Manager;
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let mut runs = read_runs(&dir);
    runs.push(run);
    let start = runs.len().saturating_sub(RUNS_KEPT);
    if std::fs::create_dir_all(&dir).is_ok() {
        if let Ok(bytes) = serde_json::to_vec(&runs[start..]) {
            let tmp = dir.join(format!("{RUNS_FILE}.tmp"));
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, dir.join(RUNS_FILE));
            }
        }
    }
}

/// Facts don't change while the app runs, and the NPU check starts PowerShell,
/// so gather them once.
fn facts() -> &'static Facts {
    static FACTS: OnceLock<Facts> = OnceLock::new();
    FACTS.get_or_init(|| {
        use sysinfo::System;
        let mut sys = System::new();
        sys.refresh_memory();
        sys.refresh_cpu_all();
        let total = sys.total_memory();
        Facts {
            ram_gb: (total > 0).then(|| (total as f64 / (1u64 << 30) as f64 * 10.0).round() / 10.0),
            cpu: sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .filter(|s| !s.is_empty()),
            cores: sys.physical_core_count(),
            npu: npu_name(),
        }
    })
}

#[cfg(windows)]
fn npu_name() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-PnpDevice -Class ComputeAccelerator -PresentOnly -ErrorAction SilentlyContinue \
             | Select-Object -ExpandProperty FriendlyName",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    parse_pnp(&String::from_utf8_lossy(&out.stdout))
}

/// Only Windows lists NPUs this way; elsewhere (dev on macOS) report none.
#[cfg(not(windows))]
fn npu_name() -> Option<String> {
    None
}

#[tauri::command]
pub async fn device_profile(app: tauri::AppHandle) -> DeviceProfile {
    use tauri::Manager;
    let runs = app
        .path()
        .app_data_dir()
        .map(|dir| read_runs(&dir))
        .unwrap_or_default();
    let facts = tauri::async_runtime::spawn_blocking(|| facts().clone())
        .await
        .unwrap_or_default();
    let (tier, reason) = classify(&facts);
    let (minutes_per_hour, measured) = minutes_per_hour(tier, &runs);
    DeviceProfile {
        tier,
        reason,
        ram_gb: facts.ram_gb,
        cpu: facts.cpu,
        cores: facts.cores,
        npu: facts.npu,
        minutes_per_hour,
        measured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(ram_gb: Option<f64>, cpu: &str, npu: Option<&str>) -> Facts {
        Facts {
            ram_gb,
            cpu: Some(cpu.to_string()),
            cores: Some(4),
            npu: npu.map(str::to_string),
        }
    }

    #[test]
    fn classify_sorts_machines_into_tiers() {
        let cases = [
            (
                facts(Some(7.7), "Intel(R) Core(TM) i5-8265U CPU @ 1.60GHz", None),
                Tier::Essentials,
            ),
            (
                facts(
                    Some(15.6),
                    "Intel(R) Core(TM) Ultra 5 225U",
                    Some("Intel(R) AI Boost"),
                ),
                Tier::Standard,
            ),
            (
                facts(
                    Some(15.6),
                    "Intel(R) Core(TM) Ultra 7 258V",
                    Some("Intel(R) AI Boost"),
                ),
                Tier::CopilotPlus,
            ),
            (
                facts(
                    Some(31.6),
                    "AMD Ryzen AI 9 HX 370 w/ Radeon 890M",
                    Some("NPU Compute Accelerator Device"),
                ),
                Tier::CopilotPlus,
            ),
            (
                facts(
                    Some(15.6),
                    "Snapdragon(R) X Elite - X1E80100 - Qualcomm(R) Oryon(TM) CPU",
                    Some("Qualcomm(R) Hexagon(TM) NPU"),
                ),
                Tier::CopilotPlus,
            ),
            (
                facts(
                    Some(15.6),
                    "AMD Ryzen 7 8840U w/ Radeon 780M Graphics",
                    Some("NPU Compute Accelerator Device"),
                ),
                Tier::Standard,
            ),
            // A Copilot+ processor without the memory stays below Copilot+.
            (
                facts(
                    Some(7.7),
                    "Intel(R) Core(TM) Ultra 5 226V",
                    Some("Intel(R) AI Boost"),
                ),
                Tier::Essentials,
            ),
            // The NPU must actually be listed.
            (
                facts(Some(15.6), "Intel(R) Core(TM) Ultra 7 258V", None),
                Tier::Standard,
            ),
            (
                facts(None, "Intel(R) Core(TM) i5-8265U CPU @ 1.60GHz", None),
                Tier::Unknown,
            ),
        ];
        for (facts, tier) in cases {
            assert_eq!(classify(&facts).0, tier, "{facts:?}");
        }
    }

    #[test]
    fn parse_pnp_reads_the_first_listed_accelerator() {
        assert_eq!(
            parse_pnp("\u{feff}\r\nIntel(R) AI Boost\r\n\r\n").as_deref(),
            Some("Intel(R) AI Boost")
        );
        assert_eq!(parse_pnp("").as_deref(), None);
        assert_eq!(parse_pnp("  \r\n\r\n").as_deref(), None);
    }

    fn run(audio_s: f64, elapsed_s: f64, diarize: bool) -> Run {
        Run {
            audio_s,
            elapsed_s,
            diarize,
        }
    }

    #[test]
    fn minutes_per_hour_uses_placeholders_until_imports_are_timed() {
        assert_eq!(minutes_per_hour(Tier::Essentials, &[]), (Some(15.0), false));
        assert_eq!(minutes_per_hour(Tier::Standard, &[]), (Some(6.0), false));
        assert_eq!(minutes_per_hour(Tier::CopilotPlus, &[]), (Some(4.0), false));
        assert_eq!(minutes_per_hour(Tier::Unknown, &[]), (None, false));
    }

    #[test]
    fn minutes_per_hour_is_the_median_of_the_last_five_plain_imports_once_three_are_timed() {
        let runs = [
            run(3600.0, 3600.0, false), // oldest: dropped, only the last 5 count
            run(3600.0, 600.0, false),  // 10 min per hour
            run(1800.0, 360.0, false),  // 12
            run(600.0, 50.0, false),    // 5
            run(3600.0, 480.0, false),  // 8
            run(30.0, 300.0, false),    // under a minute: ignored
            run(3600.0, 7200.0, true),  // speaker labels: ignored
            run(1200.0, 180.0, false),  // 9
        ];
        assert_eq!(minutes_per_hour(Tier::Essentials, &runs), (Some(9.0), true));
        assert_eq!(
            minutes_per_hour(
                Tier::Standard,
                &[
                    run(600.0, 60.0, false),
                    run(600.0, 120.0, false),
                    run(600.0, 30.0, false)
                ]
            ),
            (Some(6.0), true)
        );
    }

    #[test]
    fn minutes_per_hour_errs_long_until_three_imports_are_timed() {
        // Faster than the placeholder: the placeholder still stands.
        assert_eq!(
            minutes_per_hour(Tier::Standard, &[run(600.0, 30.0, false)]),
            (Some(6.0), false)
        );
        // Slower than the placeholder: the import counts.
        assert_eq!(
            minutes_per_hour(Tier::Standard, &[run(600.0, 120.0, false)]),
            (Some(12.0), true)
        );
        // No placeholder when the computer wasn't checked: the slowest import.
        assert_eq!(
            minutes_per_hour(
                Tier::Unknown,
                &[run(600.0, 120.0, false), run(600.0, 60.0, false)]
            ),
            (Some(12.0), true)
        );
    }
}
