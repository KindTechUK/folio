//! A bench pack: `manifest.jsonl` plus 16 kHz mono 16-bit WAVs, built once by
//! `pack/build_pack.py` and copied to the machine under test.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub set: String,
    /// WAV path relative to the pack directory.
    pub audio: String,
    pub reference: String,
    /// Meeting or call the clip comes from (for long-form sets, the clip is the recording).
    #[serde(default)]
    pub recording: String,
}

pub struct Pack {
    pub dir: PathBuf,
    pub clips: Vec<Clip>,
}

impl Pack {
    pub fn load(dir: &Path) -> Result<Pack> {
        let path = dir.join("manifest.jsonl");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let clips = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .enumerate()
            .map(|(i, l)| {
                serde_json::from_str(l).with_context(|| format!("{} line {}", path.display(), i + 1))
            })
            .collect::<Result<Vec<Clip>>>()?;
        Ok(Pack { dir: dir.to_path_buf(), clips })
    }

    pub fn set(&self, name: &str) -> Vec<&Clip> {
        self.clips.iter().filter(|c| c.set == name).collect()
    }

    pub fn sets(&self) -> Vec<String> {
        let mut s: Vec<String> = self.clips.iter().map(|c| c.set.clone()).collect();
        s.sort();
        s.dedup();
        s
    }
}
