use std::f32::consts::TAU;
use std::path::Path;
use std::sync::Arc;

use crate::model::{GlobalFilterMode, LoopMode, MotionLocks, SamplePart, SamplerMode, SamplerParams};

use super::{soft_tanh, AdsrEnv, Svf12Filter};

const MAX_SAMPLER_VOICES: usize = 4;
const MAX_SAMPLE_SECONDS: f32 = 30.0;

pub const BUILTIN_SAMPLE_NAMES: [&str; 9] = [
    "SK-1 Choir Ahh",
    "SK-1 Brass Stab",
    "SK-1 Toy Piano",
    "SK-1 Minor9 Rhodes",
    "Retro FM Bell",
    "70s Solina String",
    "OB-X Power Chord",
    "Electric Guitar",
    "Cinematic Brass",
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

        let note_semi = if params.key_pitch {
            note as f32 - params.root_note as f32
        } else {
            0.0
        };
        let semitones = note_semi + params.transpose as f32 + (params.tune_cents / 100.0);
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

            let (lp, bp, hp) = v.filter.process_all(crushed, cutoff.min(post_lp_cap), res);
            let filtered = match params.filter_mode {
                GlobalFilterMode::LowPass => lp,
                GlobalFilterMode::BandPass => bp,
                GlobalFilterMode::HighPass => hp,
            };
            mix += filtered * env * v.velocity * 0.65;

            // Advance position (live speed multiplier so you can pitch/varispeed in real time!)
            let live_speed = params.speed.clamp(0.15, 3.0) as f64;
            v.pos += v.rate * live_speed * v.dir;
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
        4 => {
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
        5 => {
            // 70s Solina / ARP String Ensemble (alias-free harmonic string + 3-tap BBD chorus!)
            let len = (2.4 * sr) as usize;
            let mut dry = Vec::with_capacity(len);
            // Warm cello + viola + violin harmonic profile (no aliasing, rich midrange!)
            let harmonics: [(f32, f32); 10] = [
                (0.5, 0.28), // Sub-cello octave warmth
                (1.0, 0.90),
                (2.0, 0.62),
                (3.0, 0.44),
                (4.0, 0.30),
                (5.0, 0.20),
                (6.0, 0.14),
                (7.0, 0.09),
                (8.0, 0.06),
                (9.0, 0.035),
            ];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum = 0.0_f32;
                for &(mult, amp) in &harmonics {
                    let ph = t * base_hz * mult * TAU;
                    sum += ph.sin() * amp;
                }
                dry.push(sum * 0.45);
            }

            // Classic 3-phase BBD Ensemble Chorus (0.58 Hz slow + 5.8 Hz fast, ±1.4 ms)
            let max_delay = (0.025 * sr) as usize;
            let mut out = Vec::with_capacity(len);
            let mut svf = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let mut wet = dry[i] * 0.35;
                for k in 0..3 {
                    let ph_off = (k as f32) * (TAU / 3.0);
                    let delay_s = 0.0085
                        + 0.0014 * (t * 0.58 * TAU + ph_off).sin()
                        + 0.00045 * (t * 5.8 * TAU + ph_off).sin();
                    let d_samp = delay_s * sr;
                    let idx_f = (i as f32 - d_samp).max(0.0);
                    let i0 = (idx_f as usize).min(len - 1);
                    let i1 = (i0 + 1).min(len - 1);
                    let frac = idx_f - i0 as f32;
                    let sample = dry[i0] * (1.0 - frac) + dry[i1] * frac;
                    wet += sample * 0.36;
                }
                let _ = max_delay;
                let warm = svf.process_lp(wet, 2200.0, 0.14);
                let attack = (t / 0.24).min(1.0);
                let release = (1.0 - (t / 2.4)).max(0.0).powf(0.45);
                out.push(soft_tanh(warm * attack * release * 1.15).clamp(-1.0, 1.0));
            }
            out
        }
        6 => {
            // 80s Oberheim OB-X Power Synth Brass (alias-free detuned unison + resonant brass sweep)
            let len = (1.6 * sr) as usize;
            let mut out = Vec::with_capacity(len);
            let mut svf = Svf12Filter::new(sr);
            let detunes = [0.9935_f32, 0.9972, 1.0028, 1.0068, 0.5];
            let weights = [0.30_f32, 0.36, 0.36, 0.30, 0.34];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum = 0.0_f32;
                for (v, &d) in detunes.iter().enumerate() {
                    let f = base_hz * d;
                    // Band-limited 12-harmonic sawtooth (zero harsh aliasing!)
                    let mut saw = 0.0_f32;
                    for h in 1..=12 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            saw += (t * hf * TAU).sin() / (h as f32);
                        }
                    }
                    sum += saw * 0.62 * weights[v];
                }
                let cutoff = 680.0 + 3400.0 * (-t * 3.8).exp();
                let filt = svf.process_lp(sum, cutoff, 0.24);
                let attack = (t / 0.016).min(1.0);
                let env = attack * (-t * 1.05).exp();
                out.push(soft_tanh(filt * 1.55 * env).clamp(-1.0, 1.0));
            }
            out
        }
        7 => {
            // Warm Double-Tracked Humbucker Electric Guitar (Karplus-Strong + 4x12 amp cab!)
            let len = (1.8 * sr) as usize;
            let make_string = |freq: f32, seed_init: u32| -> Vec<f32> {
                let delay_len = (sr / freq).round().max(16.0) as usize;
                let pickup_tap = ((delay_len as f32) / 3.6).round().max(3.0) as usize;
                let mut ring = vec![0.0_f32; delay_len];
                // Warm low-passed pick exciter (woody & musical, no harsh digital noise!)
                let mut seed = seed_init;
                let mut exc_lp = 0.0_f32;
                for j in 0..delay_len {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    let n = ((seed & 0xFFFF) as f32 / 32768.0) - 1.0;
                    exc_lp += 0.22 * (n - exc_lp);
                    let body = ((j as f32 / delay_len as f32) * TAU).sin() * 0.42;
                    ring[j] = exc_lp * 0.78 + body;
                }
                // Warm humbucker pickup comb blend
                let orig = ring.clone();
                for j in 0..delay_len {
                    ring[j] = orig[j] - 0.34 * orig[(j + delay_len - pickup_tap) % delay_len];
                }
                let mut s_out = Vec::with_capacity(len);
                let mut idx = 0usize;
                let mut prev = 0.0_f32;
                for _ in 0..len {
                    let cur = ring[idx];
                    let damped = 0.9968 * (0.54 * cur + 0.46 * prev);
                    prev = cur;
                    ring[idx] = damped;
                    idx = (idx + 1) % delay_len;
                    s_out.push(cur);
                }
                s_out
            };

            let str_a = make_string(base_hz, 0x51A7_E1EC);
            let str_b = make_string(base_hz * 1.0022, 0x9E37_79B9);
            let mut out = Vec::with_capacity(len);
            let mut cab_svf = Svf12Filter::new(sr);
            let mut body_svf = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let raw = str_a[i] * 0.62 + str_b[i] * 0.48;
                // Warm 2-stage tube preamp overdrive
                let pre = soft_tanh(raw * 2.8);
                let driven = soft_tanh(pre * 1.65 + 0.12 * pre * pre);
                // 4x12 speaker cabinet roll-off (~2.2 kHz) + warm 240 Hz body resonance
                let cab = cab_svf.process_lp(driven, 2150.0, 0.22);
                let body = body_svf.process_bp(driven, 240.0, 0.30) * 0.32;
                let attack = (t / 0.004).min(1.0);
                let tail = if t > 1.4 { (1.0 - (t - 1.4) / 0.4).max(0.0) } else { 1.0 };
                out.push(soft_tanh((cab + body) * 1.15 * attack * tail).clamp(-1.0, 1.0));
            }
            out
        }
        _ => {
            // Cinematic Orchestral Low Brass Horn Ensemble (stacked horn blast!)
            let len = (1.8 * sr) as usize;
            let mut out = Vec::with_capacity(len);
            let mut lp_svf = Svf12Filter::new(sr);
            let mut bp_svf = Svf12Filter::new(sr);
            let detunes = [0.5_f32, 0.9945, 0.9985, 1.0018, 1.0060];
            let weights = [0.48_f32, 0.34, 0.38, 0.38, 0.32];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum = 0.0_f32;
                for (v, &d) in detunes.iter().enumerate() {
                    let f = base_hz * d;
                    let mut saw = 0.0_f32;
                    for h in 1..=12 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            saw += (t * hf * TAU).sin() / (h as f32);
                        }
                    }
                    sum += saw * 0.56 * weights[v];
                }
                // Subtle octave brilliance during initial horn blare transient
                let br_env = (t / 0.022).min(1.0) * (-t * 5.2).exp();
                for h in 1..=6 {
                    let hf = (base_hz * 2.0) * h as f32;
                    if hf < sr * 0.42 {
                        sum += ((t * hf * TAU).sin() / (h as f32)) * 0.14 * br_env;
                    }
                }
                let swell_env = (t / 0.028).min(1.0) * (0.38 + 0.62 * (-t * 2.4).exp());
                let cutoff = (520.0 + 3800.0 * swell_env).clamp(120.0, sr * 0.40);
                let lp = lp_svf.process_lp(sum, cutoff, 0.24);
                let formant = bp_svf.process_bp(sum, 920.0, 0.22) * 0.28;
                let driven = soft_tanh((lp + formant) * 1.65);
                let attack = (t / 0.018).min(1.0);
                let decay = (-t * 0.95).exp();
                let tail = if t > 1.55 { (1.0 - (t - 1.55) / 0.25).max(0.0) } else { 1.0 };
                out.push((driven * attack * decay * tail).clamp(-1.0, 1.0));
            }
            normalize_peak(&out)
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

/// Loads MP3, WAV, FLAC, or OGG audio via `symphonia` (with `hound` fallback for WAV),
/// downmixes to mono f32, caps at MAX_SAMPLE_SECONDS, resamples to `target_sr`, and normalizes.
pub fn load_audio_to_mono_f32(path: &Path, target_sr: u32) -> Result<Vec<f32>, String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error as SymphoniaError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    // Fast path for WAV via hound if it works cleanly
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("wav"))
        .unwrap_or(false)
    {
        if let Ok(pcm) = load_wav_to_mono_f32(path, target_sr) {
            return Ok(pcm);
        }
    }

    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open {}: {e}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Unsupported audio format: {e}"))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| "No audio track found in file".to_string())?;
    let track_id = track.id;
    let mut src_sr = track.codec_params.sample_rate.unwrap_or(44_100).max(4000);
    let mut max_src_frames = (src_sr as f32 * MAX_SAMPLE_SECONDS) as usize;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Failed to initialize audio decoder: {e}"))?;

    let mut mono = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        if mono.len() >= max_src_frames {
            break;
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(_)) => break,
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(format!("Decode read error: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                if spec.rate >= 4000 {
                    src_sr = spec.rate;
                    max_src_frames = (src_sr as f32 * MAX_SAMPLE_SECONDS) as usize;
                }
                let channels = spec.channels.count().max(1);
                let frames = decoded.frames();
                let sb = sample_buf.get_or_insert_with(|| {
                    SampleBuffer::<f32>::new(decoded.capacity() as u64, spec)
                });
                if sb.capacity() < frames * channels {
                    *sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                }
                sb.copy_interleaved_ref(decoded);
                let samples = sb.samples();
                for frame in samples.chunks_exact(channels) {
                    if mono.len() >= max_src_frames {
                        break;
                    }
                    let sum: f32 = frame.iter().copied().sum();
                    mono.push((sum / channels as f32).clamp(-1.0, 1.0));
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(format!("Decode error: {e}")),
        }
    }

    if mono.is_empty() {
        return Err("Audio file decoded to 0 samples".to_string());
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

#[derive(Debug, Clone, Copy)]
struct PartStepFx {
    reverse_xor: bool,
    pitch_offset: i8,
    speed_mult: f32,
    cutoff_lock: Option<f32>,
    pan_lock: Option<f32>,
}

impl Default for PartStepFx {
    fn default() -> Self {
        Self {
            reverse_xor: false,
            pitch_offset: 0,
            speed_mult: 1.0,
            cutoff_lock: None,
            pan_lock: None,
        }
    }
}

/// 8-part Korg Volca Sample 2-inspired multi-track performance sampler bank
/// with per-part buffers, choke groups, per-step FX (P-locks: reverse, pitch, speed, filter, pan),
/// stereo panning, reverb send, and a master 2-band Analog Isolator (Bass & Treble kill/boost EQ).
pub struct VolcaSamplerBank {
    sr: f32,
    parts: [Sk1Sampler; 8],
    loaded_gen: [u64; 8],
    step_fx: [PartStepFx; 8],
    // Stereo 2-band Isolator EQ state (low ~260 Hz, high ~3200 Hz)
    lp_l: f32,
    lp_r: f32,
    hp_lp_l: f32,
    hp_lp_r: f32,
    low_alpha: f32,
    high_alpha: f32,
}

impl VolcaSamplerBank {
    pub fn new(sr: f32) -> Self {
        let sr = sr.max(8_000.0);
        let low_alpha = (TAU * 260.0 / sr).clamp(0.001, 0.5);
        let high_alpha = (TAU * 3200.0 / sr).clamp(0.01, 0.9);
        Self {
            sr,
            parts: std::array::from_fn(|_| Sk1Sampler::new(sr)),
            loaded_gen: [u64::MAX; 8],
            step_fx: [PartStepFx::default(); 8],
            lp_l: 0.0,
            lp_r: 0.0,
            hp_lp_l: 0.0,
            hp_lp_r: 0.0,
            low_alpha,
            high_alpha,
        }
    }

    pub fn sync_parts(&mut self, parts: &[SamplePart]) {
        for (i, part) in parts.iter().take(8).enumerate() {
            if self.loaded_gen[i] == part.generation {
                continue;
            }
            self.loaded_gen[i] = part.generation;
            if let Some(samples) = part.sample.decode_f32_pcm() {
                if !samples.is_empty() {
                    let _ = self.parts[i].swap_buffer(
                        Arc::new(samples),
                        part.sample.sample_rate.max(8_000),
                    );
                    continue;
                }
            }
            let bid = part.sample.builtin_id.unwrap_or(3);
            let buf = generate_builtin_sample(bid, self.sr as u32);
            let _ = self.parts[i].swap_buffer(Arc::new(buf), self.sr as u32);
        }
    }

    pub fn trigger_part(&mut self, idx: usize, velocity: f32, parts: &[SamplePart]) {
        if idx >= 8 || idx >= parts.len() {
            return;
        }
        self.step_fx[idx] = PartStepFx::default();
        self.trigger_part_internal(idx, velocity, parts);
    }

    pub fn trigger_part_with_step(
        &mut self,
        idx: usize,
        velocity: f32,
        step: &crate::model::DrumStep,
        parts: &[SamplePart],
    ) {
        if idx >= 8 || idx >= parts.len() {
            return;
        }
        self.step_fx[idx] = PartStepFx {
            reverse_xor: step.reverse,
            pitch_offset: step.pitch_offset,
            speed_mult: step.speed_mult.clamp(0.25, 2.0),
            cutoff_lock: step.cutoff_lock,
            pan_lock: step.pan_lock,
        };
        self.trigger_part_internal(idx, velocity, parts);
    }

    fn effective_params(&self, idx: usize, part: &SamplePart) -> SamplerParams {
        let mut sp = part.to_sampler_params();
        let fx = self.step_fx[idx];
        sp.reverse = sp.reverse ^ fx.reverse_xor;
        sp.transpose = (sp.transpose as i16 + fx.pitch_offset as i16).clamp(-36, 36) as i8;
        sp.speed = (sp.speed * fx.speed_mult).clamp(0.20, 3.0);
        if let Some(c_norm) = fx.cutoff_lock {
            sp.cutoff = 120.0 * (18_000.0_f32 / 120.0).powf(c_norm.clamp(0.0, 1.0));
        }
        sp
    }

    fn trigger_part_internal(&mut self, idx: usize, velocity: f32, parts: &[SamplePart]) {
        let grp = parts[idx].choke_group;
        if grp > 0 {
            for (other_i, other) in parts.iter().take(8).enumerate() {
                if other_i != idx && other.choke_group == grp {
                    self.parts[other_i].all_notes_off();
                }
            }
        }
        let sp = self.effective_params(idx, &parts[idx]);
        self.parts[idx].note_on(60, velocity, &sp);
    }

    pub fn release_part(&mut self, idx: usize) {
        if idx < 8 {
            self.parts[idx].all_notes_off();
        }
    }

    pub fn all_notes_off(&mut self) {
        for p in &mut self.parts {
            p.all_notes_off();
        }
    }

    pub fn playhead_norm(&self, idx: usize) -> Option<f32> {
        self.parts.get(idx).and_then(|p| p.latest_playhead_norm())
    }

    /// Renders all 8 sample parts in stereo + Volca Analog Isolator (Bass & Treble).
    /// Returns `(dry_l, dry_r, rev_l, rev_r)`.
    #[inline]
    pub fn process_stereo(
        &mut self,
        parts: &[SamplePart],
        iso_bass: f32,
        iso_treble: f32,
    ) -> (f32, f32, f32, f32) {
        let mut sum_l = 0.0_f32;
        let mut sum_r = 0.0_f32;
        let mut rev_l = 0.0_f32;
        let mut rev_r = 0.0_f32;

        for (i, part) in parts.iter().take(8).enumerate() {
            let sp = self.effective_params(i, part);
            let s = self.parts[i].process_sample(&sp);
            if part.mute || s.abs() < 1e-7 {
                continue;
            }
            let pan_val = self.step_fx[i].pan_lock.unwrap_or(part.pan);
            let p = (pan_val.clamp(-1.0, 1.0) + 1.0) * 0.5;
            let angle = p * std::f32::consts::FRAC_PI_2;
            let sl = s * part.level * angle.cos() * 1.15;
            let sr = s * part.level * angle.sin() * 1.15;
            sum_l += sl;
            sum_r += sr;
            rev_l += sl * part.reverb_send;
            rev_r += sr * part.reverb_send;
        }

        // Volca Analog Isolator (3-band split: Low < 260 Hz, Mid, High > 3200 Hz)
        self.lp_l += self.low_alpha * (sum_l - self.lp_l);
        self.lp_r += self.low_alpha * (sum_r - self.lp_r);
        self.hp_lp_l += self.high_alpha * (sum_l - self.hp_lp_l);
        self.hp_lp_r += self.high_alpha * (sum_r - self.hp_lp_r);

        let low_l = self.lp_l;
        let low_r = self.lp_r;
        let high_l = sum_l - self.hp_lp_l;
        let high_r = sum_r - self.hp_lp_r;
        let mid_l = sum_l - low_l - high_l;
        let mid_r = sum_r - low_r - high_r;

        let b = iso_bass.clamp(0.0, 2.0);
        let t = iso_treble.clamp(0.0, 2.0);
        let out_l = (low_l * b + mid_l + high_l * t).tanh();
        let out_r = (low_r * b + mid_r + high_r * t).tanh();
        (out_l, out_r, rev_l, rev_r)
    }
}
