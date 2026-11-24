//! How audio is cut into pieces before decoding, without gating it.
//!
//! VAD gating loses quiet and distant speech: on the AMI room-mic meetings it
//! deleted a third of the words (`docs/research/room-mic-capture-2026-11.md`).
//! These pieces cover every sample instead:
//!
//! - [`energy_cuts`] cuts a whole file.
//! - [`PieceCutter`] cuts a live stream the same way.
//!
//! Where a VAD stays (live captions, and models that need short segments), it
//! judges a levelled copy of the audio from a [`Leveller`], while the decoder
//! keeps the original.

/// Length of the window searched for the quietest moment, in seconds.
const QUIET_WINDOW_S: f32 = 0.1;

/// Start of the quietest `window`-long run in `region`, or `None` if the
/// region is shorter than a window.
fn quietest(region: &[f32], window: usize) -> Option<usize> {
    if region.len() < window {
        return None;
    }
    // Energy of each `window`-long run, via prefix sums.
    let mut prefix = Vec::with_capacity(region.len() + 1);
    prefix.push(0f64);
    for &x in region {
        prefix.push(prefix.last().unwrap() + (x as f64) * (x as f64));
    }
    let mut best = (f64::INFINITY, 0usize);
    for k in 0..=region.len() - window {
        let e = prefix[k + window] - prefix[k];
        if e < best.0 {
            best = (e, k);
        }
    }
    Some(best.1)
}

/// Cut points (sample indices, ascending, excluding 0 and the end) for
/// voice2text's piece rule: while at least `piece + search` seconds remain
/// after the last cut, look in `[mark - search, mark + search)` for the 100 ms
/// window with the least energy and cut at its middle.
pub fn energy_cuts(samples: &[f32], sample_rate: u32, piece: f32, search: f32) -> Vec<usize> {
    let sr = sample_rate as f32;
    let piece = (piece * sr) as usize;
    let search = (search * sr) as usize;
    let window = ((QUIET_WINDOW_S * sr) as usize).max(1);
    let mut cuts = Vec::new();
    let mut start = 0usize;
    while piece > search && samples.len() - start >= piece + search {
        let lo = start + piece - search;
        let Some(k) = quietest(&samples[lo..lo + 2 * search], window) else { break };
        let cut = lo + k + window / 2;
        cuts.push(cut);
        start = cut;
    }
    cuts
}

/// [`energy_cuts`] for a stream: feed samples as they arrive, and get each
/// piece as `(start_sample, len)` once enough audio follows it to choose its
/// cut. Positions count samples from creation. Only audio not yet emitted is
/// held.
pub struct PieceCutter {
    piece: usize,
    search: usize,
    window: usize,
    buf: Vec<f32>,
    start: usize,
}

impl PieceCutter {
    pub fn new(sample_rate: u32, piece: f32, search: f32) -> Self {
        let sr = sample_rate as f32;
        PieceCutter {
            piece: (piece * sr) as usize,
            search: (search * sr) as usize,
            window: ((QUIET_WINDOW_S * sr) as usize).max(1),
            buf: Vec::new(),
            start: 0,
        }
    }

    /// Add samples; returns the pieces this completed.
    pub fn push(&mut self, samples: &[f32]) -> Vec<(usize, usize)> {
        self.buf.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.piece > self.search && self.buf.len() >= self.piece + self.search {
            let lo = self.piece - self.search;
            let Some(k) = quietest(&self.buf[lo..lo + 2 * self.search], self.window) else { break };
            let cut = lo + k + self.window / 2;
            out.push((self.start, cut));
            self.buf.drain(..cut);
            self.start += cut;
        }
        out
    }

    /// The rest of the stream as one last piece (end of input, or a pause).
    pub fn flush(&mut self) -> Option<(usize, usize)> {
        if self.buf.is_empty() {
            return None;
        }
        let piece = (self.start, self.buf.len());
        self.start += self.buf.len();
        self.buf.clear();
        Some(piece)
    }
}

/// Level aimed for, as RMS: −23 dBFS.
const TARGET_RMS: f64 = 0.070_794_6;
/// Most gain applied: +30 dB.
const MAX_GAIN: f64 = 31.622_78;
/// Peak ceiling after gain: −1 dBFS.
const CEILING: f32 = 0.891_250_9;
/// Gain is recomputed every 10 ms from the RMS of the last 3 s.
const BLOCK_S: f32 = 0.01;
const WINDOW_BLOCKS: usize = 300;

/// A digital automatic gain control for what a VAD hears, never for what the
/// decoder hears. It raises quiet audio towards −23 dBFS by at most +30 dB,
/// judged over the last 3 s; never lowers it; and limits peaks to −1 dBFS. It
/// never touches the operating system's microphone volume.
pub struct Leveller {
    block: usize,
    in_block: usize,
    block_energy: f64,
    energies: std::collections::VecDeque<f64>,
    window_energy: f64,
    gain: f64,
    step: f64,
}

impl Leveller {
    pub fn new(sample_rate: u32) -> Self {
        Leveller {
            block: ((BLOCK_S * sample_rate as f32) as usize).max(1),
            in_block: 0,
            block_energy: 0.0,
            energies: std::collections::VecDeque::with_capacity(WINDOW_BLOCKS),
            window_energy: 0.0,
            gain: 1.0,
            step: 0.0,
        }
    }

    /// The levelled copy of `samples`.
    pub fn process(&mut self, samples: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(samples.len());
        for &x in samples {
            out.push(((x as f64 * self.gain) as f32).clamp(-CEILING, CEILING));
            self.gain += self.step;
            self.block_energy += (x as f64) * (x as f64);
            self.in_block += 1;
            if self.in_block == self.block {
                self.end_block();
            }
        }
        out
    }

    /// Fold the finished block into the 3 s window and ramp towards the new
    /// gain over the next block.
    fn end_block(&mut self) {
        if self.energies.len() == WINDOW_BLOCKS {
            self.window_energy -= self.energies.pop_front().unwrap();
        }
        self.energies.push_back(self.block_energy);
        self.window_energy += self.block_energy;
        let n = (self.energies.len() * self.block) as f64;
        let rms = (self.window_energy.max(0.0) / n).sqrt();
        let target = if rms > 0.0 { (TARGET_RMS / rms).clamp(1.0, MAX_GAIN) } else { MAX_GAIN };
        self.step = (target - self.gain) / self.block as f64;
        self.block_energy = 0.0;
        self.in_block = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 16_000;

    /// 40 s of loud noise with one silent 200 ms gap at 31.0–31.2 s.
    fn noise_with_gap() -> (Vec<f32>, std::ops::Range<usize>) {
        let mut x: Vec<f32> = (0..SR as usize * 40)
            .map(|i| if (i * 7919) % 13 < 6 { 0.5 } else { -0.5 })
            .collect();
        let gap = (SR as usize * 31)..(SR as usize * 31 + 3200);
        for s in &mut x[gap.clone()] {
            *s = 0.0;
        }
        (x, gap)
    }

    fn tone(dbfs: f64, seconds: f64) -> Vec<f32> {
        let amp = 10f64.powf(dbfs / 20.0) * std::f64::consts::SQRT_2;
        (0..(seconds * SR as f64) as usize)
            .map(|i| (amp * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / SR as f64).sin()) as f32)
            .collect()
    }

    fn rms_dbfs(x: &[f32]) -> f64 {
        let ms = x.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / x.len() as f64;
        10.0 * ms.log10()
    }

    /// Loud noise everywhere except one silent 200 ms gap: the cut must land in it.
    #[test]
    fn energy_cut_lands_in_the_quiet_gap() {
        let (x, gap) = noise_with_gap();
        let cuts = energy_cuts(&x, SR, 30.0, 5.0);
        assert_eq!(cuts.len(), 1, "40 s with 30 s pieces: one cut, got {cuts:?}");
        assert!(gap.contains(&cuts[0]), "cut {} outside the gap {gap:?}", cuts[0]);
    }

    #[test]
    fn audio_shorter_than_a_piece_is_not_cut() {
        assert!(energy_cuts(&vec![0.1; 16_000 * 20], 16_000, 30.0, 5.0).is_empty());
    }

    #[test]
    fn a_stream_is_cut_in_the_quiet_gap_and_every_sample_lands_in_a_piece() {
        let (x, gap) = noise_with_gap();
        let mut cutter = PieceCutter::new(SR, 30.0, 5.0);
        let mut pieces = Vec::new();
        let mut rest = &x[..];
        for n in [160usize, 4800, 333].into_iter().cycle() {
            if rest.is_empty() {
                break;
            }
            let (chunk, tail) = rest.split_at(n.min(rest.len()));
            pieces.extend(cutter.push(chunk));
            rest = tail;
        }
        assert_eq!(pieces.len(), 1, "one piece before the end, got {pieces:?}");
        let (start, len) = pieces[0];
        assert_eq!(start, 0);
        assert!(gap.contains(&len), "cut {len} outside the gap {gap:?}");
        assert_eq!(cutter.flush(), Some((len, 640_000 - len)));
        assert_eq!(cutter.flush(), None);
    }

    #[test]
    fn quiet_audio_is_raised_to_the_target_level() {
        let out = Leveller::new(SR).process(&tone(-49.0, 4.0));
        let last_second = &out[out.len() - SR as usize..];
        let level = rms_dbfs(last_second);
        assert!((level - -23.0).abs() <= 2.0, "levelled to {level:.1} dBFS, wanted −23 ±2");
    }

    #[test]
    fn digital_silence_stays_silent() {
        let out = Leveller::new(SR).process(&vec![0.0; SR as usize * 4]);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn peaks_never_exceed_minus_one_dbfs() {
        let out = Leveller::new(SR).process(&tone(-3.0, 4.0));
        let peak = out.iter().fold(0f32, |m, &s| m.max(s.abs()));
        assert!(peak <= 0.891_251, "peak {peak}");
    }
}
