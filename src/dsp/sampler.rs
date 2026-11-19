use std::f32::consts::TAU;
use std::path::Path;
use std::sync::Arc;

use crate::model::{LoopMode, MotionLocks, SamplerMode, SamplerParams};

use super::{soft_tanh, AdsrEnv, Svf12Filter};

const MAX_SAMPLER_VOICES: usize = 4;
const MAX_SAMPLE_SECONDS: f32 = 15.0;

pub const BUILTIN_SAMPLE_NAMES: [&str; 5] = [
    "SK-1 Choir Ahh",
    "SK-1 Brass Stab",
    "SK-1 Toy Piano",
    "SK-1 Minor9 Rhodes",
    "Retro FM Bell",
];

#[derive(Debug, Clone)]
struct SamplerVoice {
    active: bool,
    held: bool,
    note: u8,
    pos: f64,
    dir: f64, // +1.0 or -1.0 for ping-pong
    rate: f64,
    velocity: f32,
    age: u64,
    sh_timer: f32,
    sh_held_sample: f32,
    amp_env: AdsrEnv,
    filter: Svf12Filter,
}

impl SamplerVoice {
    fn new(sr: f32) -> Self {
        Self {
            active: false,
            held: false,
            note: 60,
            pos: 0.0,
            dir: 1.0,
            rate: 1.0,
            velocity: 0.9,
            age: 0,
            sh_timer: 0.0,
            sh_held_sample: 0.0,
            amp_env: AdsrEnv::new(sr),
            filter: Svf12Filter::new(sr),
        }
    }

    fn set_sample_rate(&mut self, sr: f32) {
        self.amp_env.set_sample_rate(sr);
        self.filter.set_sample_rate(sr);
    }
}

#[derive(Debug, Clone)]
pub struct Sk1Sampler {
    buffer: Arc<Vec<f32>>,
    buffer_sr: f32,
    voices: [SamplerVoice; MAX_SAMPLER_VOICES],
    age_counter: u64,
    sample_rate: f32,
    motion_cutoff: Option<f32>,
    motion_res: Option<f32>,
    motion_crush: Option<f32>,
}

impl Sk1Sampler {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let init_buf = Arc::new(generate_builtin_sample(3, sr as u32));
        Self {
            buffer: init_buf,
            buffer_sr: sr,
            voices: std::array::from_fn(|_| SamplerVoice::new(sr)),
            age_counter: 0,
            sample_rate: sr,
            motion_cutoff: None,
            motion_res: None,
            motion_crush: None,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        for v in &mut self.voices {
            v.set_sample_rate(self.sample_rate);
        }
    }

    /// Replaces the playback buffer, returning the displaced Arc so the RT thread never drops heap memory.
    pub fn swap_buffer(&mut self, new_buf: Arc<Vec<f32>>, buf_sr: u32) -> Arc<Vec<f32>> {
        self.all_notes_off();
        for v in &mut self.voices {
            v.active = false;
            v.amp_env.reset();
        }
        self.buffer_sr = (buf_sr as f32).max(4000.0);
        std::mem::replace(&mut self.buffer, new_buf)
    }

    pub fn set_motion_locks(&mut self, locks: MotionLocks) {
        self.motion_cutoff = locks.cutoff;
        self.motion_res = locks.resonance;
        self.motion_crush = locks.drive;
    }

    pub fn clear_motion_locks(&mut self) {
        self.motion_cutoff = None;
        self.motion_res = None;
        self.motion_crush = None;
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, params: &SamplerParams) {
        if self.buffer.is_empty() {
            return;
        }
        self.age_counter += 1;
        let len = self.buffer.len();
        let (start_idx, end_idx) = trimmed_bounds(len, params.start, params.end);

        let idx = self
            .voices
            .iter()
            .position(|v| !v.active || !v.amp_env.is_active())
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| (v.held as u64 * 1_000_000_000) + v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        let semitones = (note as f32 - params.root_note as f32) + (params.tune_cents / 100.0);
        let pitch_ratio = 2.0_f64.powf(semitones as f64 / 12.0);
        let sr_ratio = self.buffer_sr as f64 / self.sample_rate as f64;

        let v = &mut self.voices[idx];
        v.active = true;
        v.held = true;
        v.note = note;
        v.rate = (pitch_ratio * sr_ratio).clamp(0.05, 16.0);
        v.dir = if params.reverse { -1.0 } else { 1.0 };
        v.pos = if params.reverse {
            end_idx.saturating_sub(1) as f64
        } else {
            start_idx as f64
        };
        v.velocity = velocity.clamp(0.05, 1.0);
        v.age = self.age_counter;
        v.sh_timer = 1.0;
        v.sh_held_sample = 0.0;
        v.amp_env.trigger();
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.held && v.note == note {
                v.held = false;
                v.amp_env.release();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.held = false;
            v.amp_env.release();
        }
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.active && v.amp_env.is_active()).count()
    }

    pub fn latest_playhead_norm(&self) -> Option<f32> {
        let len = self.buffer.len().max(1) as f64;
        self.voices
            .iter()
            .filter(|v| v.active && v.amp_env.is_active())
            .max_by_key(|v| v.age)
            .map(|v| (v.pos / len).clamp(0.0, 1.0) as f32)
    }

    #[inline]
    pub fn process_sample(&mut self, params: &SamplerParams) -> f32 {
        if self.buffer.is_empty() {
            return 0.0;
        }
        let len = self.buffer.len();
        let (start_idx, end_idx) = trimmed_bounds(len, params.start, params.end);
        let start_f = start_idx as f64;
        let end_f = end_idx as f64;
        if end_f <= start_f + 1.0 {
            return 0.0;
        }

        let crush_extra = self.motion_crush.unwrap_or(params.bit_crush).clamp(0.0, 1.0);
        let cutoff = self
            .motion_cutoff
            .map(|n| 180.0 * (16000.0_f32 / 180.0).powf(n.clamp(0.0, 1.0)))
            .unwrap_or(params.cutoff);
        let res = self.motion_res.unwrap_or(params.resonance);

        let (hold_hz, quant_levels, post_lp_cap) = match params.mode {
            SamplerMode::Sk1LoFi => (
                (9380.0 - crush_extra * 5400.0).max(2800.0),
                if crush_extra > 0.55 { 64.0 } else { 128.0 }, // ~7-8 bit signed levels
                4400.0_f32,
            ),
            SamplerMode::Vintage12Bit => (
                (26000.0 - crush_extra * 14000.0).max(6000.0),
                2048.0,
                12000.0_f32,
            ),
            SamplerMode::HiFi => (self.sample_rate, 65536.0, self.sample_rate * 0.44),
        };
        let sh_step = hold_hz / self.sample_rate;

        let mut mix = 0.0_f32;
        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let env = v.amp_env.next(params.attack, params.decay, params.sustain, params.release);
            if !v.amp_env.is_active() {
                v.active = false;
                continue;
            }

            // Sample playback with linear interpolation
            let idx0 = (v.pos.floor() as isize).clamp(0, (len - 1) as isize) as usize;
            let idx1 = (idx0 + 1).min(len - 1);
            let frac = (v.pos - idx0 as f64) as f32;
            let raw = self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac;

            // Casio SK-1 sample-and-hold aliasing & bit quantization
            let crushed = if matches!(params.mode, SamplerMode::HiFi) && crush_extra <= 0.01 {
                raw
            } else {
                v.sh_timer += sh_step;
                if v.sh_timer >= 1.0 {
                    v.sh_timer -= 1.0;
                    v.sh_held_sample = (raw * quant_levels).round() / quant_levels;
                }
                v.sh_held_sample
            };

            let filtered = v.filter.process_lp(crushed, cutoff.min(post_lp_cap), res);
            mix += filtered * env * v.velocity * 0.65;

            // Advance position
            v.pos += v.rate * v.dir;
            match params.loop_mode {
                LoopMode::OneShot => {
                    if v.pos >= end_f || v.pos < start_f {
                        v.active = false;
                        v.amp_env.reset();
                    }
                }
                LoopMode::Loop => {
                    let span = end_f - start_f;
                    if v.pos >= end_f {
                        v.pos = start_f + (v.pos - start_f) % span;
                    } else if v.pos < start_f {
                        v.pos = end_f - (start_f - v.pos) % span;
                    }
                }
                LoopMode::PingPong => {
                    if v.pos >= end_f {
                        v.pos = end_f - (v.pos - end_f).min(end_f - start_f);
                        v.dir = -1.0;
                    } else if v.pos < start_f {
                        v.pos = start_f + (start_f - v.pos).min(end_f - start_f);
                        v.dir = 1.0;
                    }
                }
            }
        }

        soft_tanh(mix)
    }
}

fn trimmed_bounds(len: usize, start: f32, end: f32) -> (usize, usize) {
    let s_norm = start.clamp(0.0, 0.98);
    let e_norm = end.clamp(s_norm + 0.02, 1.0);
    let start_idx = ((s_norm * len as f32) as usize).min(len.saturating_sub(2));
    let end_idx = ((e_norm * len as f32) as usize).clamp(start_idx + 2, len);
    (start_idx, end_idx)
}

/// Generates one of 5 nostalgic built-in SK-1 starter samples at `sample_rate`.
pub fn generate_builtin_sample(id: u8, sample_rate: u32) -> Vec<f32> {
    let sr = sample_rate.max(8000) as f32;
    let base_hz = 261.6256_f32; // C4 (root_note = 60)
    match id {
        0 => {
            // SK-1 Choir "Ahh" (formant-like harmonic stack with subtle vibrato)
            let len = (1.6 * sr) as usize;
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let vib = 1.0 + 0.004 * (t * 5.2 * TAU).sin();
                let f = base_hz * vib;
                let h1 = (t * f * TAU).sin() * 0.45;
                let h2 = (t * f * 2.0 * TAU).sin() * 0.35;
                let h3 = (t * f * 3.0 * TAU).sin() * 0.30; // ~785Hz formant
                let h4 = (t * f * 4.0 * TAU).sin() * 0.22; // ~1050Hz formant
                let h5 = (t * f * 5.0 * TAU).sin() * 0.12;
                let env = (t / 0.04).min(1.0) * (1.0 - (t / 1.6)).powf(0.45);
                out.push(((h1 + h2 + h3 + h4 + h5) * 0.72 * env).clamp(-1.0, 1.0));
            }
            out
        }
        1 => {
            // SK-1 80s Brass Stab
            let len = (1.1 * sr) as usize;
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let f1 = base_hz;
                let f2 = base_hz * 1.006;
                let f3 = base_hz * 0.994;
                let saw1 = ((t * f1).fract() * 2.0 - 1.0) + ((t * f2).fract() * 2.0 - 1.0) * 0.8 + ((t * f3).fract() * 2.0 - 1.0) * 0.8;
                let bright = (-t * 5.5).exp();
                let env = (t / 0.015).min(1.0) * (-t * 1.8).exp();
                let shaped = (saw1 * (0.35 + bright * 0.7)).tanh();
                out.push((shaped * env * 0.85).clamp(-1.0, 1.0));
            }
            out
        }
        2 => {
            // SK-1 Toy Piano Pluck
            let len = (1.2 * sr) as usize;
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let body = (t * base_hz * TAU).sin() * (-t * 3.2).exp() * 0.65;
                let tine = (t * base_hz * 2.756 * TAU).sin() * (-t * 14.0).exp() * 0.40;
                let chime = (t * base_hz * 5.404 * TAU).sin() * (-t * 24.0).exp() * 0.20;
                let attack = (t / 0.002).min(1.0);
                out.push(((body + tine + chime) * attack * 0.9).clamp(-1.0, 1.0));
            }
            out
        }
        3 => {
            // Lo-Fi Minor 9th Rhodes Chord (C4-Eb4-G4-Bb4-D5 -> playable transposed!)
            let len = (1.4 * sr) as usize;
            let freqs = [261.63, 311.13, 392.00, 466.16, 587.33_f32];
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum = 0.0;
                for (k, &f) in freqs.iter().enumerate() {
                    let bark = (t * f * 3.0 * TAU).sin() * (-t * 18.0).exp() * 0.25;
                    let tone = (t * f * TAU).sin() + (t * f * 2.0 * TAU).sin() * 0.18;
                    let w = 1.0 - (k as f32 * 0.08);
                    sum += (tone + bark) * w;
                }
                let env = (t / 0.004).min(1.0) * (-t * 1.9).exp();
                out.push(((sum * 0.28).tanh() * env * 0.95).clamp(-1.0, 1.0));
            }
            out
        }
        _ => {
            // Retro FM Bell / Laser
            let len = (1.1 * sr) as usize;
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let mod_idx = 3.2 * (-t * 5.0).exp();
                let mod_sig = (t * base_hz * 1.414 * TAU).sin() * mod_idx;
                let car = (t * base_hz * TAU + mod_sig).sin();
                let env = (t / 0.002).min(1.0) * (-t * 2.6).exp();
                out.push((car * env * 0.85).clamp(-1.0, 1.0));
            }
            out
        }
    }
}

/// Loads a WAV file via `hound`, converts to mono f32, resamples to `target_sr`, and normalizes.
pub fn load_wav_to_mono_f32(path: &Path, target_sr: u32) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("Failed to open WAV {}: {e}", path.display()))?;
    let spec = reader.spec();
    let channels = (spec.channels as usize).max(1);
    let src_sr = spec.sample_rate.max(4000);
    let max_src_frames = (src_sr as f32 * MAX_SAMPLE_SECONDS) as usize;

    let interleaved: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => reader
            .samples::<f32>()
            .take(max_src_frames * channels)
            .map(|r| r.unwrap_or(0.0))
            .collect(),
        (hound::SampleFormat::Int, 8) => reader
            .samples::<i8>()
            .take(max_src_frames * channels)
            .map(|r| r.unwrap_or(0) as f32 / 128.0)
            .collect(),
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .take(max_src_frames * channels)
            .map(|r| r.unwrap_or(0) as f32 / 32768.0)
            .collect(),
        (hound::SampleFormat::Int, 24) => reader
            .samples::<i32>()
            .take(max_src_frames * channels)
            .map(|r| r.unwrap_or(0) as f32 / 8_388_608.0)
            .collect(),
        (hound::SampleFormat::Int, _) => reader
            .samples::<i32>()
            .take(max_src_frames * channels)
            .map(|r| r.unwrap_or(0) as f32 / 2_147_483_648.0)
            .collect(),
    };

    if interleaved.is_empty() {
        return Err("WAV file contains no audio samples".to_string());
    }

    let mut mono = Vec::with_capacity(interleaved.len() / channels + 1);
    for frame in interleaved.chunks(channels) {
        let sum: f32 = frame.iter().copied().sum();
        mono.push((sum / frame.len() as f32).clamp(-1.0, 1.0));
    }

    let resampled = resample_mono_linear(&mono, src_sr, target_sr);
    Ok(normalize_peak(&resampled))
}

pub fn resample_mono_linear(input: &[f32], src_sr: u32, dst_sr: u32) -> Vec<f32> {
    if input.is_empty() || src_sr == dst_sr {
        return input.to_vec();
    }
    let ratio = dst_sr as f64 / src_sr as f64;
    let out_len = ((input.len() as f64) * ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = (i as f64 / ratio).min((input.len() - 1) as f64);
        let idx0 = pos.floor() as usize;
        let idx1 = (idx0 + 1).min(input.len() - 1);
        let frac = (pos - idx0 as f64) as f32;
        out.push(input[idx0] * (1.0 - frac) + input[idx1] * frac);
    }
    out
}

pub fn trim_silence_and_normalize(input: &[f32], sample_rate: u32) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }
    let threshold = 0.015_f32;
    let pre_pad = (sample_rate as usize / 100).min(256);
    let first_loud = input.iter().position(|&s| s.abs() > threshold).unwrap_or(0);
    let start = first_loud.saturating_sub(pre_pad);
    let last_loud = input.iter().rposition(|&s| s.abs() > threshold).unwrap_or(input.len() - 1);
    let end = (last_loud + pre_pad * 4).min(input.len());
    if end <= start + 16 {
        return normalize_peak(input);
    }
    normalize_peak(&input[start..end])
}

pub fn normalize_peak(input: &[f32]) -> Vec<f32> {
    let peak = input.iter().fold(0.0_f32, |m, &s| m.max(s.abs()));
    if peak < 1e-5 {
        return input.to_vec();
    }
    let gain = (0.94 / peak).min(12.0);
    input.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect()
}
