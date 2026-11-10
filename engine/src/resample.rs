//! Streaming conversion of device audio to the engine's 16 kHz mono.
//!
//! Microphones deliver interleaved frames at the device rate (often 44.1 or
//! 48 kHz, one or two channels) in callback-sized buffers. [`StreamResampler`]
//! downmixes and resamples them chunk by chunk, so a recording can be written
//! and transcribed while it is still going.

use rubato::{FftFixedInOut, Resampler};

use crate::EngineError;

const TARGET_RATE: usize = 16_000;

/// Interleaved `f32` at any rate and channel count in, 16 kHz mono out.
pub struct StreamResampler {
    channels: usize,
    src_rate: usize,
    inner: Option<FftFixedInOut<f32>>,
    /// Downmixed input not yet resampled.
    pending: Vec<f32>,
    /// Output frames still to drop (the resampler's delay).
    skip: usize,
    frames_in: usize,
    frames_out: usize,
}

impl StreamResampler {
    pub fn new(src_rate: u32, channels: u16) -> anyhow::Result<Self> {
        if src_rate == 0 || channels == 0 {
            return Err(EngineError::Audio("sample rate and channels must be non-zero".into()).into());
        }
        let src_rate = src_rate as usize;
        let inner = if src_rate == TARGET_RATE {
            None
        } else {
            Some(
                FftFixedInOut::<f32>::new(src_rate, TARGET_RATE, 1024, 1)
                    .map_err(|e| EngineError::Audio(format!("building resampler: {e}")))?,
            )
        };
        let skip = inner.as_ref().map(|r| r.output_delay()).unwrap_or(0);
        Ok(Self {
            channels: channels as usize,
            src_rate,
            inner,
            pending: Vec::new(),
            skip,
            frames_in: 0,
            frames_out: 0,
        })
    }

    /// Convert the next buffer. Returns whatever 16 kHz output is ready; the
    /// rest is kept for the next call. A trailing partial frame is ignored.
    pub fn push(&mut self, interleaved: &[f32]) -> anyhow::Result<Vec<f32>> {
        let frames = interleaved.len() / self.channels;
        self.frames_in += frames;
        self.pending.extend(
            interleaved
                .chunks_exact(self.channels)
                .map(|f| f.iter().sum::<f32>() / self.channels as f32),
        );
        self.drain(false)
    }

    /// Flush the remaining input. Total output is `frames_in * 16000 / rate`.
    pub fn finish(&mut self) -> anyhow::Result<Vec<f32>> {
        self.drain(true)
    }

    fn drain(&mut self, finishing: bool) -> anyhow::Result<Vec<f32>> {
        let Some(r) = self.inner.as_mut() else {
            self.frames_out += self.pending.len();
            return Ok(std::mem::take(&mut self.pending));
        };
        let expected_total = self.frames_in * TARGET_RATE / self.src_rate;
        let mut out = Vec::new();
        loop {
            let need = r.input_frames_next();
            if self.pending.len() < need {
                if !finishing || self.frames_out + out.len() >= expected_total {
                    break;
                }
                // Zero-pad so the tail (and the resampler's delay) comes out.
                self.pending.resize(need, 0.0);
            }
            let chunk: Vec<f32> = self.pending.drain(..need).collect();
            let res = r
                .process(&[chunk], None)
                .map_err(|e| EngineError::Audio(format!("resampling: {e}")))?;
            let mut produced = &res[0][..];
            let drop = self.skip.min(produced.len());
            self.skip -= drop;
            produced = &produced[drop..];
            out.extend_from_slice(produced);
        }
        if finishing {
            let keep = expected_total.saturating_sub(self.frames_out);
            out.truncate(keep);
            self.pending.clear();
        }
        self.frames_out += out.len();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, channels: u16, secs: f32, hz: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        let mut v = Vec::with_capacity(n * channels as usize);
        for i in 0..n {
            let s = (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.5;
            for _ in 0..channels {
                v.push(s);
            }
        }
        v
    }

    /// Rising zero crossings per second over the steady middle of the signal.
    fn frequency(samples: &[f32], rate: f32) -> f32 {
        let mid = &samples[samples.len() / 4..samples.len() * 3 / 4];
        let ups = mid.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        ups as f32 / (mid.len() as f32 / rate)
    }

    fn run(rate: u32, channels: u16, chunk_sizes: &[usize]) -> Vec<f32> {
        let input = tone(rate, channels, 3.0, 440.0);
        let mut r = StreamResampler::new(rate, channels).unwrap();
        let mut out = Vec::new();
        let mut pos = 0;
        let mut k = 0;
        while pos < input.len() {
            let frames = chunk_sizes[k % chunk_sizes.len()];
            let end = (pos + frames * channels as usize).min(input.len());
            out.extend(r.push(&input[pos..end]).unwrap());
            pos = end;
            k += 1;
        }
        out.extend(r.finish().unwrap());
        out
    }

    #[test]
    fn stereo_48k_in_uneven_chunks_becomes_16k_mono_of_the_same_length_and_pitch() {
        let out = run(48_000, 2, &[480, 1000, 7, 4096]);
        assert_eq!(out.len(), 48_000); // 3 s at 16 kHz
        let hz = frequency(&out, 16_000.0);
        assert!((hz - 440.0).abs() < 5.0, "tone came out at {hz} Hz");
    }

    #[test]
    fn mono_44k1_in_uneven_chunks_becomes_16k_mono_of_the_same_length_and_pitch() {
        let out = run(44_100, 1, &[441, 2048, 13]);
        assert_eq!(out.len(), 48_000);
        let hz = frequency(&out, 16_000.0);
        assert!((hz - 440.0).abs() < 5.0, "tone came out at {hz} Hz");
    }
}
