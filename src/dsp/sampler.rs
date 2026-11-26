use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::model::{
    GlobalFilterMode, LoopMode, MotionLocks, SamplePart, SamplerMode, SamplerParams, SavedSample,
};

use super::{one_pole_coeff, soft_ceiling, soft_tanh, AdsrEnv, DelayAllpass, Svf12Filter};

const MAX_SAMPLER_VOICES: usize = 4;
const MAX_SAMPLE_SECONDS: f32 = 30.0;

pub const BUILTIN_SAMPLE_NAMES: [&str; 20] = [
    "SK-1 Choir Ahh",
    "SK-1 Brass Stab",
    "SK-1 Toy Piano",
    "SK-1 Minor9 Rhodes",
    "Retro FM Bell",
    "70s Solina String",
    "OB-X Power Chord",
    "Electric Guitar",
    "Cinematic Brass",
    "Vinyl Minor9 Chord",
    "90s Rave Hoover",
    "Fairlight Orch Hit",
    "Island Steel Drum",
    "Thumb Kalimba",
    "808 Cowbell Hit",
    "Mellotron Flute",
    "Dub Siren Hit",
    "Gospel Organ",
    "Chamber Pizzicato",
    "Cosmic Shimmer",
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
    sh_held_l: f32,
    sh_held_r: f32,
    amp_env: AdsrEnv,
    filter_l: Svf12Filter,
    filter_r: Svf12Filter,
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
            sh_held_l: 0.0,
            sh_held_r: 0.0,
            amp_env: AdsrEnv::new(sr),
            filter_l: Svf12Filter::new(sr),
            filter_r: Svf12Filter::new(sr),
        }
    }

    fn set_sample_rate(&mut self, sr: f32) {
        self.amp_env.set_sample_rate(sr);
        self.filter_l.set_sample_rate(sr);
        self.filter_r.set_sample_rate(sr);
    }
}

#[derive(Debug, Clone)]
pub struct Sk1Sampler {
    buffer_l: Arc<Vec<f32>>,
    buffer_r: Arc<Vec<f32>>,
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
        let (init_l, init_r) = generate_builtin_sample_stereo(3, sr as u32);
        Self {
            buffer_l: Arc::new(init_l),
            buffer_r: Arc::new(init_r),
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

    /// Replaces the stereo playback buffers, returning the displaced Arcs so the RT thread never drops heap memory.
    pub fn swap_stereo_buffer(
        &mut self,
        left: Arc<Vec<f32>>,
        right: Arc<Vec<f32>>,
        buf_sr: u32,
    ) -> (Arc<Vec<f32>>, Arc<Vec<f32>>) {
        self.all_notes_off();
        for v in &mut self.voices {
            v.active = false;
            v.amp_env.reset();
        }
        self.buffer_sr = (buf_sr as f32).max(4000.0);
        let eff_right = if right.len() == left.len() {
            right
        } else {
            Arc::clone(&left)
        };
        let old_l = std::mem::replace(&mut self.buffer_l, left);
        let old_r = std::mem::replace(&mut self.buffer_r, eff_right);
        (old_l, old_r)
    }

    /// Replaces the playback buffer with a mono buffer (duplicated L/R), returning the displaced Arc.
    pub fn swap_buffer(&mut self, new_buf: Arc<Vec<f32>>, buf_sr: u32) -> Arc<Vec<f32>> {
        self.swap_stereo_buffer(Arc::clone(&new_buf), new_buf, buf_sr).0
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
        let len = self.buffer_l.len().min(self.buffer_r.len());
        if len == 0 {
            return;
        }
        self.age_counter += 1;
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
        v.sh_held_l = 0.0;
        v.sh_held_r = 0.0;
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
        let len = self.buffer_l.len().max(1) as f64;
        self.voices
            .iter()
            .filter(|v| v.active && v.amp_env.is_active())
            .max_by_key(|v| v.age)
            .map(|v| (v.pos / len).clamp(0.0, 1.0) as f32)
    }

    #[inline]
    pub fn process_sample(&mut self, params: &SamplerParams) -> f32 {
        let (l, r) = self.process_stereo(params);
        (l + r) * 0.5
    }

    #[inline]
    pub fn process_stereo(&mut self, params: &SamplerParams) -> (f32, f32) {
        let len = self.buffer_l.len().min(self.buffer_r.len());
        if len == 0 {
            return (0.0, 0.0);
        }
        let (start_idx, end_idx) = trimmed_bounds(len, params.start, params.end);
        let start_f = start_idx as f64;
        let end_f = end_idx as f64;
        if end_f <= start_f + 1.0 {
            return (0.0, 0.0);
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
        let hifi = matches!(params.mode, SamplerMode::HiFi) && crush_extra <= 0.01;
        let max_i = end_idx.saturating_sub(1).max(start_idx);

        let mut mix_l = 0.0_f32;
        let mut mix_r = 0.0_f32;
        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let env = v.amp_env.next(params.attack, params.decay, params.sustain, params.release);
            if !v.amp_env.is_active() {
                v.active = false;
                continue;
            }

            // Stereo playback: 4-point Hermite in HiFi mode, linear + S&H in Vintage/SK1 modes
            let (crushed_l, crushed_r) = if hifi {
                let idx0 = (v.pos.floor() as isize).clamp(start_idx as isize, max_i as isize) as usize;
                let idx_m1 = if idx0 > start_idx { idx0 - 1 } else { start_idx };
                let idx1 = (idx0 + 1).min(max_i);
                let idx2 = (idx0 + 2).min(max_i);
                let frac = (v.pos - idx0 as f64) as f32;
                (
                    hermite4(self.buffer_l[idx_m1], self.buffer_l[idx0], self.buffer_l[idx1], self.buffer_l[idx2], frac),
                    hermite4(self.buffer_r[idx_m1], self.buffer_r[idx0], self.buffer_r[idx1], self.buffer_r[idx2], frac),
                )
            } else {
                let idx0 = (v.pos.floor() as isize).clamp(0, (len - 1) as isize) as usize;
                let idx1 = (idx0 + 1).min(len - 1);
                let frac = (v.pos - idx0 as f64) as f32;
                let raw_l = self.buffer_l[idx0] * (1.0 - frac) + self.buffer_l[idx1] * frac;
                let raw_r = self.buffer_r[idx0] * (1.0 - frac) + self.buffer_r[idx1] * frac;
                v.sh_timer += sh_step;
                if v.sh_timer >= 1.0 {
                    v.sh_timer -= 1.0;
                    v.sh_held_l = (raw_l * quant_levels).round() / quant_levels;
                    v.sh_held_r = (raw_r * quant_levels).round() / quant_levels;
                }
                (v.sh_held_l, v.sh_held_r)
            };

            let bypass_filter = !params.filter_enabled && self.motion_cutoff.is_none();
            let (filt_l, filt_r) = if bypass_filter {
                (crushed_l, crushed_r)
            } else {
                // Per-voice velocity brightness & gentle anti-chipmunk key-pitch taming
                let vel_bright = 0.55 + 0.55 * v.velocity;
                let pitch_tame = if params.key_pitch {
                    (1.0 / (v.rate as f32).powf(0.28)).clamp(0.45, 1.25)
                } else {
                    1.0
                };
                let eff_cutoff = (cutoff * vel_bright * pitch_tame).clamp(120.0, post_lp_cap);

                let (lp_l, bp_l, hp_l) = v.filter_l.process_all(crushed_l, eff_cutoff, res);
                let (lp_r, bp_r, hp_r) = v.filter_r.process_all(crushed_r, eff_cutoff, res);
                match params.filter_mode {
                    GlobalFilterMode::LowPass => (lp_l, lp_r),
                    GlobalFilterMode::BandPass => (bp_l, bp_r),
                    GlobalFilterMode::HighPass => (hp_l, hp_r),
                }
            };
            let voice_gain = env * v.velocity * 0.65;
            mix_l += filt_l * voice_gain;
            mix_r += filt_r * voice_gain;

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

        (soft_ceiling(mix_l, 1.35, 0.35), soft_ceiling(mix_r, 1.35, 0.35))
    }
}

#[inline]
fn hermite4(ym1: f32, y0: f32, y1: f32, y2: f32, frac: f32) -> f32 {
    let c0 = y0;
    let c1 = 0.5 * (y1 - ym1);
    let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
    let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
    ((c3 * frac + c2) * frac + c1) * frac + c0
}

fn trimmed_bounds(len: usize, start: f32, end: f32) -> (usize, usize) {
    let s_norm = start.clamp(0.0, 0.98);
    let e_norm = end.clamp(s_norm + 0.02, 1.0);
    let start_idx = ((s_norm * len as f32) as usize).min(len.saturating_sub(2));
    let end_idx = ((e_norm * len as f32) as usize).clamp(start_idx + 2, len);
    (start_idx, end_idx)
}

/// Generates one of 9 built-in studio starter samples in mono (downmixed from stereo).
pub fn generate_builtin_sample(id: u8, sample_rate: u32) -> Vec<f32> {
    let (l, r) = generate_builtin_sample_stereo(id, sample_rate);
    let mono: Vec<f32> = l.iter().zip(r.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
    normalize_peak(&mono)
}

static BUILTIN_SAMPLE_CACHE: OnceLock<Mutex<Vec<((u8, u32), (Arc<Vec<f32>>, Arc<Vec<f32>>))>>> =
    OnceLock::new();

/// Resolves the local `philharmonia-all-samples` studio sample library if present in the workspace/app dir.
pub fn resolve_philharmonia_dir() -> Option<PathBuf> {
    static PHIL_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    PHIL_DIR
        .get_or_init(|| {
            let mut candidates = vec![
                PathBuf::from("philharmonia-all-samples"),
                PathBuf::from(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/philharmonia-all-samples"
                )),
            ];
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    candidates.push(dir.join("philharmonia-all-samples"));
                    if let Some(p) = dir.parent() {
                        candidates.push(p.join("philharmonia-all-samples"));
                        if let Some(pp) = p.parent() {
                            candidates.push(pp.join("philharmonia-all-samples"));
                        }
                    }
                }
            }
            candidates.into_iter().find(|p| p.is_dir())
        })
        .clone()
}

/// Loads a Philharmonia sample (`rel_path` inside `philharmonia-all-samples`), trims leading MP3/room
/// silence down to sub-millisecond onset accuracy, applies click-free fade-in/out, and synthesizes
/// phase-clean Mid/Side stereo studio width when the source recording is mono.
pub fn load_philharmonia_trimmed_stereo(
    rel_path: &str,
    target_sr: u32,
    max_sec: f32,
) -> Option<(Vec<f32>, Vec<f32>)> {
    let dir = resolve_philharmonia_dir()?;
    let full = dir.join(rel_path);
    if !full.is_file() {
        return None;
    }
    let (raw_l, raw_r) = load_audio_to_stereo_f32(&full, target_sr).ok()?;
    let frames = raw_l.len().min(raw_r.len());
    if frames < 64 {
        return None;
    }
    let peak = (0..frames)
        .map(|i| raw_l[i].abs().max(raw_r[i].abs()))
        .fold(0.0_f32, f32::max);
    if peak < 1e-4 {
        return None;
    }
    let onset_thresh = (peak * 0.032).max(0.008);
    let first = (0..frames)
        .position(|i| raw_l[i].abs().max(raw_r[i].abs()) >= onset_thresh)
        .unwrap_or(0);
    let pre = ((0.0012 * target_sr as f32).round() as usize).min(first);
    let start = first - pre;
    let cap_frames = ((max_sec.max(0.05) * target_sr as f32).round() as usize).max(64);
    let end = (start + cap_frames).min(frames);
    let len = end - start;
    if len < 32 {
        return None;
    }

    let is_mono = (start..end)
        .take(2048)
        .all(|i| (raw_l[i] - raw_r[i]).abs() < 1e-4);
    let fade_in = ((0.0012 * target_sr as f32).round() as usize).max(4);
    let fade_out = ((0.035 * target_sr as f32).round() as usize)
        .min(len / 3)
        .max(8);

    let mut out_l = Vec::with_capacity(len);
    let mut out_r = Vec::with_capacity(len);
    let mut ap1 = DelayAllpass::new(((0.00179 * target_sr as f32).round() as usize).max(2));
    let mut ap2 = DelayAllpass::new(((0.00311 * target_sr as f32).round() as usize).max(3));

    for k in 0..len {
        let env_in = if k < fade_in {
            k as f32 / fade_in as f32
        } else {
            1.0
        };
        let env_out = if k + fade_out > len {
            let rem = (len - k) as f32 / fade_out as f32;
            0.5 - 0.5 * (rem * std::f32::consts::PI).cos()
        } else {
            1.0
        };
        let g = env_in * env_out;
        let l = raw_l[start + k] * g;
        let r = raw_r[start + k] * g;
        if is_mono {
            // Orthogonal Mid/Side allpass room spread: 100% mono-compatible (0.5*(L+R) == l)!
            let side = ap2.process(ap1.process(l, 0.54), 0.48) * 0.25;
            out_l.push(l + side);
            out_r.push(l - side);
        } else {
            out_l.push(l);
            out_r.push(r);
        }
    }
    Some(normalize_stereo_peak(&out_l, &out_r))
}

pub const VOLCA_PHIL_PERC_ITEMS: &[(&str, &str)] = &[
    ("Bass Drum", "percussion/bass drum"),
    ("Snare Drum", "percussion/snare drum"),
    ("Tom-Toms", "percussion/tom-toms"),
    ("Djembe", "percussion/djembe"),
    ("Tambourine", "percussion/tambourine"),
    ("Susp. Cymbal", "percussion/suspended cymbal"),
    ("Triangle", "percussion/triangle"),
    ("Shaker", "percussion/banana shaker"),
    ("Cowbell", "percussion/cowbell"),
    ("Woodblock", "percussion/woodblock"),
    ("Agogo Bells", "percussion/agogo bells"),
    ("Castanets", "percussion/castanets"),
    ("Cabasa", "percussion/cabasa"),
    ("Surdo", "percussion/surdo"),
    ("Clash Cym", "percussion/clash cymbals"),
    ("Thai Gong", "percussion/Thai gong"),
];

pub const VOLCA_PHIL_CHOP_ITEMS: &[(&str, &str)] = &[
    ("Guitar Pluck", "guitar"),
    ("Violin Pizz", "violin"),
    ("Cello Pluck", "cello"),
    ("Banjo", "banjo"),
    ("Mandolin", "mandolin"),
    ("Flute Note", "flute"),
    ("Sax Stab", "saxophone"),
    ("Trumpet Stab", "trumpet"),
    ("French Horn", "french horn"),
    ("Trombone", "trombone"),
    ("Clarinet", "clarinet"),
    ("Oboe", "oboe"),
];

pub const VOLCA_KIT_NAMES: [&str; 4] = [
    "Philharmonia Percussion Kit",
    "Hybrid Beat + Chops",
    "Philharmonia Acoustic Chops",
    "SK-1 Vintage Bank",
];

fn collect_mp3_shallow(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if p.extension()
                .and_then(|s| s.to_str())
                .map(|e| e.eq_ignore_ascii_case("mp3"))
                .unwrap_or(false)
            {
                out.push(p);
            }
        } else if p.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&p) {
                for se in sub.flatten() {
                    let sp = se.path();
                    if sp.is_file()
                        && sp
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|e| e.eq_ignore_ascii_case("mp3"))
                            .unwrap_or(false)
                    {
                        out.push(sp);
                    }
                }
            }
        }
    }
}

/// Scores and loads the cleanest one-shot recording inside `philharmonia-all-samples/<folder_rel>`.
pub fn load_philharmonia_folder_sample(
    folder_rel: &str,
    prefer_pizz: bool,
    max_sec: f32,
    target_sr: u32,
) -> Option<(Vec<f32>, Vec<f32>)> {
    let root = resolve_philharmonia_dir()?;
    let dir = root.join(folder_rel);
    if !dir.is_dir() {
        return None;
    }
    let mut files = Vec::new();
    collect_mp3_shallow(&dir, &mut files);
    if files.is_empty() {
        return None;
    }
    let is_perc = folder_rel.starts_with("percussion");
    let mut best: Option<(String, i32)> = None;
    for path in files {
        let fname = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        let mut score = 0i32;
        if fname.contains("_phrase_")
            || fname.contains("_tremolo_")
            || fname.contains("_trill_")
            || fname.contains("_glissando_")
            || fname.contains("_roll_")
        {
            score -= 50;
        }
        if prefer_pizz {
            if fname.contains("pizz") {
                score += 40;
            }
        } else if fname.contains("_normal") {
            score += 20;
        }
        if fname.contains("_025_") || fname.contains("_05_") {
            score += 18;
        } else if fname.contains("_1_") {
            score += 12;
        }
        if fname.contains("_forte_") {
            score += 14;
        } else if fname.contains("_mezzo-forte_") {
            score += 12;
        }
        if !is_perc {
            if fname.contains("_c4_") {
                score += 20;
            } else if fname.contains("_c3_") {
                score += 16;
            } else if fname.contains("_g3_") || fname.contains("_f3_") {
                score += 10;
            }
        }
        if let Ok(rel) = path.strip_prefix(&root) {
            if let Some(rel_str) = rel.to_str() {
                let rel_norm = rel_str.replace('\\', "/");
                if best
                    .as_ref()
                    .map(|(bn, bs)| score > *bs || (score == *bs && rel_norm < *bn))
                    .unwrap_or(true)
                {
                    best = Some((rel_norm, score));
                }
            }
        }
    }
    let (rel, _) = best?;
    load_philharmonia_trimmed_stereo(&rel, target_sr, max_sec)
}

pub fn apply_philharmonia_sample_to_part(
    part: &mut SamplePart,
    label: &str,
    folder_rel: &str,
    prefer_pizz: bool,
    max_sec: f32,
    sample_rate: u32,
) -> Result<(), String> {
    let (l, r) = load_philharmonia_folder_sample(folder_rel, prefer_pizz, max_sec, sample_rate)
        .ok_or_else(|| format!("Sample folder '{}' not found", folder_rel))?;
    part.name = label.chars().take(16).collect();
    part.sample = SavedSample::from_stereo_pcm(label.to_string(), sample_rate, &l, &r, None);
    part.start = 0.0;
    part.end = 1.0;
    part.reverse = false;
    part.transpose = 0;
    part.tune_cents = 0.0;
    part.speed = 1.0;
    part.loop_mode = LoopMode::OneShot;
    part.mode = SamplerMode::HiFi;
    part.mute = false;
    part.set_filter_passthrough();
    part.generation = part.generation.wrapping_add(1).max(1);
    Ok(())
}

fn assign_builtin_to_part(part: &mut SamplePart, name: &str, bid: u8, pan: f32, choke: u8) {
    part.name = name.to_string();
    part.sample = SavedSample {
        name: name.to_string(),
        sample_rate: 48_000,
        pcm_i16_b64: String::new(),
        builtin_id: Some(bid),
        channels: 1,
    };
    part.start = 0.0;
    part.end = 1.0;
    part.reverse = false;
    part.transpose = 0;
    part.tune_cents = 0.0;
    part.speed = 1.0;
    part.pan = pan;
    part.choke_group = choke;
    part.loop_mode = LoopMode::OneShot;
    part.mode = SamplerMode::HiFi;
    part.mute = false;
    part.set_filter_passthrough();
    part.generation = part.generation.wrapping_add(1).max(1);
}

pub fn load_volca_kit(parts: &mut [SamplePart], kit_id: usize, sample_rate: u32) -> String {
    let sr = sample_rate.max(8_000);
    match kit_id {
        0 => {
            let pans = [0.0, -0.05, -0.25, 0.25, -0.35, 0.35, 0.45, -0.45];
            let chokes = [0u8, 0, 0, 0, 1, 1, 0, 0];
            let mut loaded_disk = 0usize;
            for (i, part) in parts.iter_mut().take(8).enumerate() {
                let (label, folder) = VOLCA_PHIL_PERC_ITEMS[i];
                if apply_philharmonia_sample_to_part(part, label, folder, false, 1.35, sr).is_ok() {
                    loaded_disk += 1;
                } else {
                    assign_builtin_to_part(part, label, (i as u8) % 9, pans[i], chokes[i]);
                }
                part.pan = pans[i];
                part.choke_group = chokes[i];
                part.reverb_send = if i == 0 { 0.08 } else { 0.18 };
            }
            if loaded_disk > 0 {
                format!("Loaded Philharmonia Percussion Kit ({loaded_disk}/8 pads)")
            } else {
                "Loaded Percussion Starter Kit (builtin fallback)".to_string()
            }
        }
        1 => {
            let specs: [(&str, &str, bool, f32, u8, u8); 8] = [
                ("Bass Drum", "percussion/bass drum", false, 0.0, 0, 1),
                ("Snare Drum", "percussion/snare drum", false, -0.05, 0, 6),
                ("Tambourine", "percussion/tambourine", false, -0.32, 1, 4),
                ("Shaker", "percussion/banana shaker", false, 0.32, 1, 2),
                ("Guitar Pluck", "guitar", false, -0.22, 2, 7),
                ("Violin Pizz", "violin", true, 0.22, 2, 5),
                ("Sax Stab", "saxophone", false, -0.28, 2, 1),
                ("Trumpet Stab", "trumpet", false, 0.28, 2, 8),
            ];
            let mut loaded_disk = 0usize;
            for (i, part) in parts.iter_mut().take(8).enumerate() {
                let (label, folder, pizz, pan, choke, fb_bid) = specs[i];
                if apply_philharmonia_sample_to_part(part, label, folder, pizz, 1.45, sr).is_ok() {
                    loaded_disk += 1;
                } else {
                    assign_builtin_to_part(part, label, fb_bid, pan, choke);
                }
                part.pan = pan;
                part.choke_group = choke;
                part.reverb_send = if i < 2 { 0.10 } else { 0.24 };
            }
            if loaded_disk > 0 {
                format!("Loaded Hybrid Beat + Chops Kit ({loaded_disk}/8 pads)")
            } else {
                "Loaded Hybrid Starter Kit (builtin fallback)".to_string()
            }
        }
        2 => {
            let pans = [-0.42, 0.42, -0.25, 0.25, -0.50, 0.50, -0.18, 0.18];
            let mut loaded_disk = 0usize;
            for (i, part) in parts.iter_mut().take(8).enumerate() {
                let (label, folder) = VOLCA_PHIL_CHOP_ITEMS[i];
                let pizz = folder == "violin" || folder == "cello";
                if apply_philharmonia_sample_to_part(part, label, folder, pizz, 1.65, sr).is_ok() {
                    loaded_disk += 1;
                } else {
                    assign_builtin_to_part(part, label, (i as u8) % 9, pans[i], 1);
                }
                part.pan = pans[i];
                part.choke_group = 1;
                part.reverb_send = 0.26;
            }
            if loaded_disk > 0 {
                format!("Loaded Philharmonia Acoustic Chops ({loaded_disk}/8 pads)")
            } else {
                "Loaded Acoustic Chops Kit (builtin fallback)".to_string()
            }
        }
        _ => {
            let defaults = SamplePart::default_bank();
            for (i, part) in parts.iter_mut().take(8).enumerate() {
                let prev_gen = part.generation;
                *part = defaults[i].clone();
                part.set_filter_passthrough();
                part.mute = false;
                part.generation = prev_gen.wrapping_add(1).max(1);
            }
            "Loaded SK-1 Vintage Bank (passthrough)".to_string()
        }
    }
}

/// Generates one of 9 built-in studio starter samples in true stereo `(left, right)`,
/// cached per `(id, sample_rate)` and hybridized with real Philharmonia orchestral/acoustic layers when present.
pub fn generate_builtin_sample_stereo(id: u8, sample_rate: u32) -> (Vec<f32>, Vec<f32>) {
    let sr_key = sample_rate.max(8000);
    let cache = BUILTIN_SAMPLE_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((_, (l, r))) = guard.iter().find(|((cid, csr), _)| *cid == id && *csr == sr_key) {
            return (l.as_ref().clone(), r.as_ref().clone());
        }
    }
    let (mut l, mut r) = build_builtin_sample_stereo_uncached(id, sr_key);
    blend_philharmonia_into_patch(id, sr_key, &mut l, &mut r);
    let (nl, nr) = normalize_stereo_peak(&l, &r);
    let arc_l = Arc::new(nl.clone());
    let arc_r = Arc::new(nr.clone());
    if let Ok(mut guard) = cache.lock() {
        if !guard.iter().any(|((cid, csr), _)| *cid == id && *csr == sr_key) {
            guard.push(((id, sr_key), (arc_l, arc_r)));
        }
    }
    (nl, nr)
}

fn mix_stereo_layer(
    dst_l: &mut [f32],
    dst_r: &mut [f32],
    src_l: &[f32],
    src_r: &[f32],
    gain_l: f32,
    gain_r: f32,
) {
    let n = dst_l.len().min(dst_r.len()).min(src_l.len()).min(src_r.len());
    for i in 0..n {
        dst_l[i] += src_l[i] * gain_l;
        dst_r[i] += src_r[i] * gain_r;
    }
}

fn blend_philharmonia_into_patch(id: u8, sr: u32, out_l: &mut [f32], out_r: &mut [f32]) {
    let dur = out_l.len() as f32 / sr.max(8000) as f32;
    match id {
        0 => {
            // Choir Ahh + real Philharmonia Flute C4 breathy Mellotron air
            if let Some((fl, fr)) =
                load_philharmonia_trimmed_stereo("flute/flute_C4_15_forte_normal.mp3", sr, dur)
            {
                mix_stereo_layer(out_l, out_r, &fl, &fr, 0.38, 0.38);
            }
        }
        1 => {
            // Brass Stab + real Philharmonia Trumpet C4 + Saxophone C4 studio horn section!
            for s in out_l.iter_mut() {
                *s *= 0.62;
            }
            for s in out_r.iter_mut() {
                *s *= 0.62;
            }
            if let Some((tl, tr)) =
                load_philharmonia_trimmed_stereo("trumpet/trumpet_C4_05_forte_normal.mp3", sr, dur)
            {
                mix_stereo_layer(out_l, out_r, &tl, &tr, 0.56, 0.42);
            }
            if let Some((sl, sr_c)) = load_philharmonia_trimmed_stereo(
                "saxophone/saxophone_C4_05_forte_normal.mp3",
                sr,
                dur,
            ) {
                mix_stereo_layer(out_l, out_r, &sl, &sr_c, 0.38, 0.54);
            }
        }
        5 => {
            // 70s Solina + real Philharmonia Violin C4 + Viola C4 + Cello C3 orchestral strings!
            for s in out_l.iter_mut() {
                *s *= 0.58;
            }
            for s in out_r.iter_mut() {
                *s *= 0.58;
            }
            if let Some((vl, vr)) = load_philharmonia_trimmed_stereo(
                "violin/violin_C4_15_forte_arco-normal.mp3",
                sr,
                dur,
            ) {
                mix_stereo_layer(out_l, out_r, &vl, &vr, 0.52, 0.36);
            }
            if let Some((al, ar)) = load_philharmonia_trimmed_stereo(
                "viola/viola_C4_very-long_mezzo-forte_arco-normal.mp3",
                sr,
                dur,
            ) {
                mix_stereo_layer(out_l, out_r, &al, &ar, 0.36, 0.52);
            }
            if let Some((cl, cr)) = load_philharmonia_trimmed_stereo(
                "cello/cello_C3_15_forte_arco-normal.mp3",
                sr,
                dur,
            ) {
                mix_stereo_layer(out_l, out_r, &cl, &cr, 0.40, 0.40);
            }
        }
        6 => {
            // OB-X Power Chord + real Philharmonia Trumpet C4 + Trombone C4 bite
            if let Some((tl, tr)) =
                load_philharmonia_trimmed_stereo("trumpet/trumpet_C4_1_forte_normal.mp3", sr, dur)
            {
                mix_stereo_layer(out_l, out_r, &tl, &tr, 0.42, 0.34);
            }
            if let Some((bl, br)) = load_philharmonia_trimmed_stereo(
                "trombone/trombone_C4_05_forte_normal.mp3",
                sr,
                dur,
            ) {
                mix_stereo_layer(out_l, out_r, &bl, &br, 0.34, 0.44);
            }
        }
        7 => {
            // Electric Guitar + real Philharmonia Plucked Guitar C4 through warm tube overdrive
            if let Some((gl, gr)) = load_philharmonia_trimmed_stereo(
                "guitar/guitar_C4_very-long_forte_normal.mp3",
                sr,
                dur,
            ) {
                let mut cab_l = Svf12Filter::new(sr as f32);
                let mut cab_r = Svf12Filter::new(sr as f32);
                let n = out_l.len().min(gl.len());
                for i in 0..n {
                    let dl = cab_l.process_lp(soft_tanh(gl[i] * 2.1), 2_850.0, 0.20);
                    let dr = cab_r.process_lp(soft_tanh(gr[i] * 2.1), 2_850.0, 0.20);
                    out_l[i] = out_l[i] * 0.52 + dl * 0.68;
                    out_r[i] = out_r[i] * 0.52 + dr * 0.68;
                }
            }
        }
        8 => {
            // Cinematic Brass: 3-octave London Philharmonia Tuba C2 + Trombone C3 + French Horn C3!
            for s in out_l.iter_mut() {
                *s *= 0.52;
            }
            for s in out_r.iter_mut() {
                *s *= 0.52;
            }
            if let Some((hl, hr)) = load_philharmonia_trimmed_stereo(
                "french horn/french-horn_C3_15_mezzo-forte_normal.mp3",
                sr,
                dur,
            ) {
                mix_stereo_layer(out_l, out_r, &hl, &hr, 0.54, 0.42);
            }
            if let Some((bl, br)) = load_philharmonia_trimmed_stereo(
                "trombone/trombone_C3_15_mezzo-forte_normal.mp3",
                sr,
                dur,
            ) {
                mix_stereo_layer(out_l, out_r, &bl, &br, 0.42, 0.54);
            }
            if let Some((tl, tr)) =
                load_philharmonia_trimmed_stereo("tuba/tuba_C2_1_forte_normal.mp3", sr, dur)
            {
                mix_stereo_layer(out_l, out_r, &tl, &tr, 0.46, 0.46);
            }
        }
        _ => {}
    }
}

fn build_builtin_sample_stereo_uncached(id: u8, sample_rate: u32) -> (Vec<f32>, Vec<f32>) {
    let sr = sample_rate.max(8000) as f32;
    let base_hz = 261.6256_f32; // C4 (root_note = 60)
    match id {
        0 => {
            // Pro Analog/Formant Choir "Ahh" (3-voice stack -> stereo vowel formants + 3-tap stereo BBD chorus)
            let len = (1.8 * sr) as usize;
            let mut dry_l = Vec::with_capacity(len);
            let mut dry_r = Vec::with_capacity(len);
            let mut bp1 = Svf12Filter::new(sr);
            let mut bp2_l = Svf12Filter::new(sr);
            let mut bp2_r = Svf12Filter::new(sr);
            let mut bp3 = Svf12Filter::new(sr);
            let mut rng = super::Xorshift32::new(0xC01A_AA44);
            let detunes = [0.9965_f32, 1.0, 1.0038];
            for i in 0..len {
                let t = i as f32 / sr;
                let vib_bloom = ((t - 0.08) / 0.22).clamp(0.0, 1.0);
                let vib = 1.0 + 0.0038 * vib_bloom * (t * 4.8 * TAU).sin();
                let mut raw = 0.0_f32;
                for (vi, &d) in detunes.iter().enumerate() {
                    let scoop = 1.0 - 0.0045 * (-t * (28.0 + vi as f32 * 7.0)).exp();
                    let f = base_hz * vib * d * scoop;
                    for h in 1..=14 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            raw += (t * hf * TAU).sin() / (h as f32);
                        }
                    }
                }
                let onset_chiff = (-t / 0.038).exp() * 0.11;
                let breath = rng.next_bipolar() * (0.035 + onset_chiff);
                let src = raw * 0.24 + breath;
                let f1_shift = 32.0 * (t * 0.72 * TAU).sin();
                let f2_shift = 48.0 * (t * 0.53 * TAU).cos();
                let f1 = bp1.process_bp(src, 730.0 + f1_shift, 0.55) * 0.85;
                let f2_l = bp2_l.process_bp(src, 1055.0 + f2_shift, 0.58) * 0.72;
                let f2_r = bp2_r.process_bp(src, 1135.0 - f2_shift, 0.58) * 0.72;
                let f3 = bp3.process_bp(src, 2440.0, 0.52) * 0.34;
                dry_l.push(f1 + f2_l + f3);
                dry_r.push(f1 + f2_r + f3);
            }
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            const TAP_L: [f32; 3] = [0.48, 0.32, 0.16];
            const TAP_R: [f32; 3] = [0.16, 0.32, 0.48];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut wet_l = dry_l[i] * 0.42;
                let mut wet_r = dry_r[i] * 0.42;
                for k in 0..3 {
                    let ph_off = (k as f32) * (TAU / 3.0);
                    let delay_s = 0.0072 + 0.0018 * (t * 0.62 * TAU + ph_off).sin();
                    let idx_f = (i as f32 - delay_s * sr).max(0.0);
                    let i0 = (idx_f as usize).min(len - 1);
                    let i1 = (i0 + 1).min(len - 1);
                    let frac = idx_f - i0 as f32;
                    let s_l = dry_l[i0] * (1.0 - frac) + dry_l[i1] * frac;
                    let s_r = dry_r[i0] * (1.0 - frac) + dry_r[i1] * frac;
                    wet_l += s_l * TAP_L[k];
                    wet_r += s_r * TAP_R[k];
                }
                let env = (t / 0.065).min(1.0) * (1.0 - (t / 1.8)).max(0.0).powf(0.45);
                out_l.push(soft_tanh(wet_l * 1.45 * env));
                out_r.push(soft_tanh(wet_r * 1.45 * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        1 => {
            // Pro Analog Brass Stab (stereo-spread 5-voice saw + centered sub horn + L/R SVF sweep)
            let len = (1.25 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut svf_l = Svf12Filter::new(sr);
            let mut svf_r = Svf12Filter::new(sr);
            let detunes = [0.5_f32, 0.992, 0.997, 1.0, 1.003, 1.008];
            let weights = [0.42_f32, 0.28, 0.34, 0.38, 0.34, 0.28];
            let pans = [0.0_f32, -0.58, -0.22, 0.0, 0.24, 0.58];
            let mut rng = super::Xorshift32::new(0xB4A5_557A);
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum_l = 0.0_f32;
                let mut sum_r = 0.0_f32;
                for (v, &d) in detunes.iter().enumerate() {
                    let blip = 1.0 + ((v as f32 - 2.5) * 0.0032) * (-t * 52.0).exp();
                    let f = base_hz * d * blip;
                    let mut saw = 0.0_f32;
                    for h in 1..=14 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            saw += (t * hf * TAU).sin() / (h as f32);
                        }
                    }
                    let amp = saw * 0.56 * weights[v];
                    let ang = ((pans[v] + 1.0) * 0.5) * std::f32::consts::FRAC_PI_2;
                    sum_l += amp * ang.cos() * 1.4142;
                    sum_r += amp * ang.sin() * 1.4142;
                }
                let lip_buzz = rng.next_bipolar() * (-t / 0.012).exp() * 0.22;
                let swell = (t / 0.012).min(1.0) * (-t * 4.5).exp();
                let cutoff = (650.0 + 3550.0 * swell).clamp(120.0, sr * 0.42);
                let filt_l = svf_l.process_lp(sum_l + lip_buzz, cutoff, 0.14);
                let filt_r = svf_r.process_lp(sum_r - lip_buzz * 0.7, cutoff, 0.14);
                let driven_l = soft_tanh(filt_l * 1.45 + 0.06 * filt_l * filt_l);
                let driven_r = soft_tanh(filt_r * 1.45 + 0.06 * filt_r * filt_r);
                let env = (t / 0.010).min(1.0) * (-t * 1.65).exp();
                out_l.push(driven_l * env);
                out_r.push(driven_r * env);
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        2 => {
            // Pro Toy Piano / Celesta Pluck (centered woody body + stereo tine/chime sparkle)
            let len = (1.3 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut body_bp = Svf12Filter::new(sr);
            let mut rng = super::Xorshift32::new(0x7019_1A40);
            for i in 0..len {
                let t = i as f32 / sr;
                let fund = (t * base_hz * TAU).sin() * (-t * 2.8).exp() * 0.72;
                let h2 = (t * base_hz * 2.0 * TAU).sin() * (-t * 4.6).exp() * 0.34;
                let tine = (t * base_hz * 2.756 * TAU).sin() * (-t * 12.0).exp() * 0.36;
                let chime = (t * base_hz * 5.404 * TAU).sin() * (-t * 22.0).exp() * 0.18;
                let hammer = rng.next_bipolar() * (-t * 260.0).exp() * 0.32;
                let wood = body_bp.process_bp(fund + hammer, 580.0, 0.28) * 0.35;
                let core = fund + h2 + hammer + wood;
                let attack = (t / 0.0015).min(1.0);
                out_l.push(soft_tanh((core + tine * 1.18 + chime * 0.72) * attack * 1.15));
                out_r.push(soft_tanh((core + tine * 0.78 + chime * 1.24) * attack * 1.15));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        3 => {
            // Pro Suitcase 9th Rhodes Chord (open Root-5th-Oct-9th-12th voicing: pure & consonant over Major & Minor!
            // + felt hammer wood thump + human micro-rolled tine strikes + stereo suitcase vibrato)
            let len = (1.6 * sr) as usize;
            let notes: [(f32, f32); 5] = [
                (261.63, 1.00), // C4 Root
                (392.00, 0.88), // G4 5th
                (523.25, 0.78), // C5 Octave
                (587.33, 0.68), // D5 9th
                (783.99, 0.24), // G5 upper shimmer 5th (zero Major7/Minor7 clash)
            ];
            let mut rng = super::Xorshift32::new(0x730D_E599);
            let mut dry = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum = 0.0_f32;
                for (k, &(f, w)) in notes.iter().enumerate() {
                    let tk = (t - k as f32 * 0.0042).max(0.0);
                    if tk <= 0.0 {
                        continue;
                    }
                    let tine_env = (-tk * 15.5).exp();
                    let bell_env = (-tk * 6.2).exp();
                    let note_atk = (tk / 0.0025).min(1.0);
                    let fm_mod = (tk * f * 4.0 * TAU).sin() * (0.42 * tine_env)
                        + (tk * f * 7.02 * TAU).sin() * (0.11 * (-tk * 38.0).exp());
                    let tone = (tk * f * TAU + fm_mod).sin()
                        + (tk * f * 2.0 * TAU).sin() * (0.20 * bell_env)
                        + (tk * f * 3.0 * TAU).sin() * (0.11 * tine_env);
                    sum += tone * w * note_atk;
                }
                // Mechanical felt hammer thump + metallic tine strike click
                let hammer_wood = (t * 148.0 * TAU).sin() * (-t / 0.0055).exp() * 0.24;
                let tine_click = (rng.next_bipolar() * 0.5 + (t * 2340.0 * TAU).sin() * 0.5)
                    * (-t / 0.0022).exp()
                    * 0.16;
                let trem = 0.89 + 0.11 * (t * 4.2 * TAU).sin();
                let env = (t / 0.003).min(1.0) * (-t * 1.52).exp();
                dry.push((soft_tanh(sum * 0.35 * trem) + hammer_wood + tine_click) * env);
            }
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let auto_pan = 0.16 * (t * 2.1 * TAU).sin();
                let d_l = 0.0062 + 0.0015 * (t * 0.55 * TAU).sin();
                let d_r = 0.0076 + 0.0015 * (t * 0.55 * TAU).cos();
                let sample_at = |delay_s: f32| -> f32 {
                    let idx_f = (i as f32 - delay_s * sr).max(0.0);
                    let i0 = (idx_f as usize).min(len - 1);
                    let i1 = (i0 + 1).min(len - 1);
                    let frac = idx_f - i0 as f32;
                    dry[i0] * (1.0 - frac) + dry[i1] * frac
                };
                let c_l = sample_at(d_l);
                let c_r = sample_at(d_r);
                out_l.push(soft_tanh((dry[i] * (0.72 + auto_pan) + c_l * 0.38) * 1.22));
                out_r.push(soft_tanh((dry[i] * (0.72 - auto_pan) + c_r * 0.38) * 1.22));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        4 => {
            // Pro 4-Op Yamaha DX7 Detuned FM Bell / Pluck (stereo carrier spread + centered sub)
            let len = (1.35 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let mod_env1 = (-t * 6.2).exp();
                let mod_env2 = (-t * 11.5).exp();
                let f_a = base_hz * 0.9985;
                let f_b = base_hz * 1.0015;
                let op2 = (t * f_a * 1.0 * TAU).sin() * 1.15 * mod_env1;
                let op4 = (t * f_a * 3.5 * TAU).sin() * 0.95 * mod_env2;
                let car_a = (t * f_a * TAU + op2 + op4).sin();

                let op2_b = (t * f_b * 2.0 * TAU).sin() * 1.05 * mod_env1;
                let op4_b = (t * f_b * 5.0 * TAU).sin() * 0.72 * mod_env2;
                let car_b = (t * f_b * TAU + op2_b + op4_b).sin();

                let warm_sub = (t * base_hz * 0.5 * TAU).sin() * 0.24 * (-t * 2.2).exp();
                let env = (t / 0.0018).min(1.0) * (-t * 2.15).exp();
                let st_l = car_a * 0.68 + car_b * 0.36;
                let st_r = car_a * 0.36 + car_b * 0.68;
                out_l.push(soft_tanh((st_l + warm_sub) * env * 1.2));
                out_r.push(soft_tanh((st_r + warm_sub) * env * 1.2));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        5 => {
            // 70s Solina / ARP String Ensemble (alias-free harmonic string + 3-phase stereo BBD Ensemble)
            let len = (2.4 * sr) as usize;
            let mut dry = Vec::with_capacity(len);
            let harmonics: [(f32, f32); 10] = [
                (0.5, 0.28),
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
            let mut rng = super::Xorshift32::new(0x5011_4A70);
            let mut rosin_bp = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum = 0.0_f32;
                for &(mult, amp) in &harmonics {
                    let ph = t * base_hz * mult * TAU;
                    sum += ph.sin() * amp;
                }
                let bow_env = 0.35 + 0.65 * (-t / 0.18).exp();
                let rosin = rosin_bp.process_bp(rng.next_bipolar(), 2650.0, 0.42) * 0.075 * bow_env;
                dry.push(sum * 0.45 + rosin);
            }

            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut svf_l = Svf12Filter::new(sr);
            let mut svf_r = Svf12Filter::new(sr);
            const TAP_L: [f32; 3] = [0.50, 0.34, 0.14];
            const TAP_R: [f32; 3] = [0.14, 0.34, 0.50];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut wet_l = dry[i] * 0.35;
                let mut wet_r = dry[i] * 0.35;
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
                    wet_l += sample * TAP_L[k];
                    wet_r += sample * TAP_R[k];
                }
                let warm_l = svf_l.process_lp(wet_l, 2200.0, 0.14);
                let warm_r = svf_r.process_lp(wet_r, 2200.0, 0.14);
                let attack = (t / 0.24).min(1.0);
                let release = (1.0 - (t / 2.4)).max(0.0).powf(0.45);
                out_l.push(soft_tanh(warm_l * attack * release * 1.15).clamp(-1.0, 1.0));
                out_r.push(soft_tanh(warm_r * attack * release * 1.15).clamp(-1.0, 1.0));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        6 => {
            // 80s Oberheim OB-X Power Synth Brass (stereo voice-card panning + L/R resonant brass sweep)
            let len = (1.6 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut svf_l = Svf12Filter::new(sr);
            let mut svf_r = Svf12Filter::new(sr);
            let detunes = [0.9935_f32, 0.9972, 1.0028, 1.0068, 0.5];
            let weights = [0.30_f32, 0.36, 0.36, 0.30, 0.34];
            let pans = [-0.64_f32, -0.24, 0.24, 0.64, 0.0];
            let mut rng = super::Xorshift32::new(0x0B80_B4A5);
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum_l = 0.0_f32;
                let mut sum_r = 0.0_f32;
                for (v, &d) in detunes.iter().enumerate() {
                    let blip = 1.0 + ((v as f32 - 2.0) * 0.0030) * (-t * 46.0).exp();
                    let f = base_hz * d * blip;
                    let mut saw = 0.0_f32;
                    for h in 1..=12 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            saw += (t * hf * TAU).sin() / (h as f32);
                        }
                    }
                    let amp = saw * 0.62 * weights[v];
                    let ang = ((pans[v] + 1.0) * 0.5) * std::f32::consts::FRAC_PI_2;
                    sum_l += amp * ang.cos() * 1.4142;
                    sum_r += amp * ang.sin() * 1.4142;
                }
                let lip_chiff = rng.next_bipolar() * (-t / 0.015).exp() * 0.18;
                let cutoff = 680.0 + 3400.0 * (-t * 3.8).exp();
                let filt_l = svf_l.process_lp(sum_l + lip_chiff, cutoff, 0.24);
                let filt_r = svf_r.process_lp(sum_r - lip_chiff * 0.7, cutoff, 0.24);
                let attack = (t / 0.016).min(1.0);
                let env = attack * (-t * 1.05).exp();
                out_l.push(soft_tanh(filt_l * 1.55 * env).clamp(-1.0, 1.0));
                out_r.push(soft_tanh(filt_r * 1.55 * env).clamp(-1.0, 1.0));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        7 => {
            // Double-Tracked Stereo Humbucker Electric Guitar (Karplus-Strong L/R + dual 4x12 cab!)
            let len = (1.8 * sr) as usize;
            let make_string = |freq: f32, seed_init: u32| -> Vec<f32> {
                let delay_len = (sr / freq).round().max(16.0) as usize;
                let pickup_tap = ((delay_len as f32) / 3.6).round().max(3.0) as usize;
                let mut ring = vec![0.0_f32; delay_len];
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
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut cab_l = Svf12Filter::new(sr);
            let mut cab_r = Svf12Filter::new(sr);
            let mut body_l = Svf12Filter::new(sr);
            let mut body_r = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let raw_l = str_a[i] * 0.72 + str_b[i] * 0.32;
                let raw_r = str_a[i] * 0.32 + str_b[i] * 0.72;
                let pre_l = soft_tanh(raw_l * 2.8);
                let pre_r = soft_tanh(raw_r * 2.8);
                let drv_l = soft_tanh(pre_l * 1.65 + 0.12 * pre_l * pre_l);
                let drv_r = soft_tanh(pre_r * 1.65 + 0.12 * pre_r * pre_r);
                let cab_out_l = cab_l.process_lp(drv_l, 2150.0, 0.22);
                let cab_out_r = cab_r.process_lp(drv_r, 2150.0, 0.22);
                let b_out_l = body_l.process_bp(drv_l, 240.0, 0.30) * 0.32;
                let b_out_r = body_r.process_bp(drv_r, 240.0, 0.30) * 0.32;
                let attack = (t / 0.004).min(1.0);
                let tail = if t > 1.4 { (1.0 - (t - 1.4) / 0.4).max(0.0) } else { 1.0 };
                out_l.push(soft_tanh((cab_out_l + b_out_l) * 1.15 * attack * tail).clamp(-1.0, 1.0));
                out_r.push(soft_tanh((cab_out_r + b_out_r) * 1.15 * attack * tail).clamp(-1.0, 1.0));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        8 => {
            // Cinematic Orchestral Low Brass Horn Ensemble (wide stereo horn section!)
            let len = (1.8 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut lp_l = Svf12Filter::new(sr);
            let mut lp_r = Svf12Filter::new(sr);
            let mut bp_l = Svf12Filter::new(sr);
            let mut bp_r = Svf12Filter::new(sr);
            let detunes = [0.5_f32, 0.9945, 0.9985, 1.0018, 1.0060];
            let weights = [0.48_f32, 0.34, 0.38, 0.38, 0.32];
            let pans = [0.0_f32, -0.60, -0.22, 0.22, 0.60];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum_l = 0.0_f32;
                let mut sum_r = 0.0_f32;
                for (v, &d) in detunes.iter().enumerate() {
                    let f = base_hz * d;
                    let mut saw = 0.0_f32;
                    for h in 1..=12 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            saw += (t * hf * TAU).sin() / (h as f32);
                        }
                    }
                    let amp = saw * 0.56 * weights[v];
                    let ang = ((pans[v] + 1.0) * 0.5) * std::f32::consts::FRAC_PI_2;
                    sum_l += amp * ang.cos() * 1.4142;
                    sum_r += amp * ang.sin() * 1.4142;
                }
                let br_env = (t / 0.022).min(1.0) * (-t * 5.2).exp();
                let mut br = 0.0_f32;
                for h in 1..=6 {
                    let hf = (base_hz * 2.0) * h as f32;
                    if hf < sr * 0.42 {
                        br += ((t * hf * TAU).sin() / (h as f32)) * 0.14 * br_env;
                    }
                }
                sum_l += br * 1.05;
                sum_r += br * 0.95;
                let swell_env = (t / 0.028).min(1.0) * (0.38 + 0.62 * (-t * 2.4).exp());
                let cutoff = (520.0 + 3800.0 * swell_env).clamp(120.0, sr * 0.40);
                let l_lp = lp_l.process_lp(sum_l, cutoff, 0.24);
                let r_lp = lp_r.process_lp(sum_r, cutoff, 0.24);
                let l_fm = bp_l.process_bp(sum_l, 920.0, 0.22) * 0.28;
                let r_fm = bp_r.process_bp(sum_r, 920.0, 0.22) * 0.28;
                let drv_l = soft_tanh((l_lp + l_fm) * 1.65);
                let drv_r = soft_tanh((r_lp + r_fm) * 1.65);
                let attack = (t / 0.018).min(1.0);
                let decay = (-t * 0.95).exp();
                let tail = if t > 1.55 { (1.0 - (t - 1.55) / 0.25).max(0.0) } else { 1.0 };
                out_l.push((drv_l * attack * decay * tail).clamp(-1.0, 1.0));
                out_r.push((drv_r * attack * decay * tail).clamp(-1.0, 1.0));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        9 => {
            // Dusty Vinyl Minor9 Chord (warm Rhodes/EP chord + tape wow + subtle vinyl crackle)
            let len = (1.5 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let notes = [261.63_f32, 311.13, 392.00, 466.16, 587.33];
            let mut rng = super::Xorshift32::new(0x10F1_9909);
            let mut lp_l = Svf12Filter::new(sr);
            let mut lp_r = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let wow = 1.0 + 0.0045 * (t * 1.35 * TAU).sin();
                let mut sum_l = 0.0_f32;
                let mut sum_r = 0.0_f32;
                for (k, &f) in notes.iter().enumerate() {
                    let f_w = f * wow;
                    let tine = (t * f_w * 3.0 * TAU).sin() * 0.28 * (-t * 9.0).exp();
                    let s = (t * f_w * TAU + tine).sin() + 0.22 * (t * f_w * 2.0 * TAU).sin();
                    let pan = -0.35 + (k as f32 / 4.0) * 0.70;
                    sum_l += s * (0.65 - pan * 0.35);
                    sum_r += s * (0.65 + pan * 0.35);
                }
                let crackle = if rng.next_f32() > 0.996 { rng.next_bipolar() * 0.16 } else { 0.0 };
                let env = (t / 0.006).min(1.0) * (-t * 1.65).exp();
                let fl = lp_l.process_lp(sum_l * 0.22 + crackle, 2850.0, 0.14);
                let fr = lp_r.process_lp(sum_r * 0.22 + crackle * 0.7, 2850.0, 0.14);
                out_l.push(soft_tanh(fl * env * 1.4));
                out_r.push(soft_tanh(fr * env * 1.4));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        10 => {
            // 90s Rave Hoover Stab (alpha Juno PWM + detuned saw swarm with downward pitch bend)
            let len = (1.1 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut lp_l = Svf12Filter::new(sr);
            let mut lp_r = Svf12Filter::new(sr);
            let detunes = [0.5_f32, 0.986, 0.994, 1.0, 1.007, 1.015, 2.0];
            for i in 0..len {
                let t = i as f32 / sr;
                let bend = 1.0 + 0.14 * (-t * 14.0).exp();
                let pwm_lfo = (t * 6.5 * TAU).sin() * 0.18;
                let mut sum_l = 0.0_f32;
                let mut sum_r = 0.0_f32;
                for (idx, &d) in detunes.iter().enumerate() {
                    let f = base_hz * d * bend;
                    let mut w = 0.0_f32;
                    for h in 1..=10 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            w += ((t * hf * TAU + pwm_lfo * (h as f32)).sin()) / (h as f32);
                        }
                    }
                    if idx % 2 == 0 {
                        sum_l += w * 0.24;
                        sum_r += w * 0.16;
                    } else {
                        sum_l += w * 0.16;
                        sum_r += w * 0.24;
                    }
                }
                let cut = (900.0 + 5200.0 * (-t * 4.5).exp()).clamp(200.0, sr * 0.42);
                let env = (t / 0.004).min(1.0) * (-t * 2.1).exp();
                out_l.push(soft_tanh(lp_l.process_lp(sum_l, cut, 0.34) * 1.7 * env));
                out_r.push(soft_tanh(lp_r.process_lp(sum_r, cut, 0.34) * 1.7 * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        11 => {
            // Fairlight Orchestral Hit (5th power chord + brass transient + gated reverb punch)
            let len = (1.0 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut rng = super::Xorshift32::new(0xFA17_1167);
            let mut lp = Svf12Filter::new(sr);
            let chord = [0.5_f32, 1.0, 1.498, 2.0, 2.996];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut s = 0.0_f32;
                for &m in &chord {
                    let f = base_hz * m;
                    for h in 1..=7 {
                        let hf = f * h as f32;
                        if hf < sr * 0.42 {
                            s += (t * hf * TAU).sin() / (h as f32);
                        }
                    }
                }
                let timpani = (t * (base_hz * 0.5 * (1.0 + 0.18 * (-t * 25.0).exp())) * TAU).sin()
                    * (-t * 8.0).exp()
                    * 0.55;
                let snap = rng.next_bipolar() * (-t * 42.0).exp() * 0.32;
                let env = (t / 0.002).min(1.0) * (-t * 3.4).exp();
                let filt = lp.process_lp(s * 0.18 + timpani + snap, 4600.0 * (-t * 2.2).exp() + 700.0, 0.25);
                let v = soft_tanh(filt * 1.8 * env);
                let wid = (t * 17.0 * TAU).sin() * 0.08 * env;
                out_l.push((v + wid).clamp(-1.0, 1.0));
                out_r.push((v - wid).clamp(-1.0, 1.0));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        12 => {
            // Island Steel Drum (tuned octave + 5th pan partials + wooden mallet transient)
            let len = (1.1 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut rng = super::Xorshift32::new(0x57EE_1D20);
            for i in 0..len {
                let t = i as f32 / sr;
                let f1 = (t * base_hz * TAU).sin() * (-t * 3.2).exp() * 0.85;
                let f2 = (t * base_hz * 2.003 * TAU).sin() * (-t * 4.8).exp() * 0.52;
                let f3 = (t * base_hz * 2.995 * TAU).sin() * (-t * 7.2).exp() * 0.26;
                let shimmer = (t * base_hz * 4.04 * TAU).sin() * (-t * 14.0).exp() * 0.14;
                let mallet = rng.next_bipolar() * (-t * 180.0).exp() * 0.28;
                let env = (t / 0.002).min(1.0);
                out_l.push(soft_tanh((f1 + f2 * 1.12 + f3 * 0.85 + shimmer + mallet) * env));
                out_r.push(soft_tanh((f1 + f2 * 0.88 + f3 * 1.15 + shimmer + mallet) * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        13 => {
            // Thumb Kalimba Tine (woody box body resonance + warm pure thumb tine)
            let len = (0.95 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut rng = super::Xorshift32::new(0xCA11_3BA0);
            let mut box_bp = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let tine = (t * base_hz * TAU).sin() * (-t * 3.8).exp() * 0.88;
                let overtone = (t * base_hz * 6.24 * TAU).sin() * (-t * 28.0).exp() * 0.22;
                let thumb = rng.next_bipolar() * (-t * 240.0).exp() * 0.26;
                let box_res = box_bp.process_bp(tine + thumb, 340.0, 0.35) * 0.38;
                let env = (t / 0.0012).min(1.0);
                out_l.push(soft_tanh((tine + box_res + overtone * 1.1 + thumb) * env));
                out_r.push(soft_tanh((tine + box_res + overtone * 0.9 + thumb) * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        14 => {
            // 808 Tuned Cowbell Hit (classic 540 / 800 Hz square-ratio metallic resonator)
            let len = (0.65 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut bp = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let sq1 = if (t * base_hz * TAU).sin() >= 0.0 { 1.0 } else { -1.0 };
                let sq2 = if (t * base_hz * 1.4815 * TAU).sin() >= 0.0 { 1.0 } else { -1.0 };
                let env = (-t * 16.0).exp() * 0.65 + (-t * 5.2).exp() * 0.35;
                let body = bp.process_bp((sq1 + sq2) * 0.5, base_hz * 2.1, 0.18) * env;
                let v = soft_tanh(body * 1.45);
                out_l.push(v);
                out_r.push(v * 0.96);
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        15 => {
            // Tape Mellotron Flute (breathy chiffon chiff + warm tape vibrato flute)
            let len = (2.0 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut rng = super::Xorshift32::new(0xF107_E747);
            let mut breath_bp = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let wow = 1.0 + 0.005 * (t * 4.4 * TAU).sin() + 0.002 * (t * 0.8 * TAU).sin();
                let f = base_hz * wow;
                let flute = (t * f * TAU).sin() * 0.78
                    + (t * f * 2.0 * TAU).sin() * 0.24
                    + (t * f * 3.0 * TAU).sin() * 0.07;
                let chiff = breath_bp.process_bp(rng.next_bipolar(), base_hz * 3.1, 0.32)
                    * (0.14 + 0.34 * (-t / 0.055).exp());
                let env = (t / 0.055).min(1.0) * (1.0 - (t / 2.0)).max(0.0).powf(0.4);
                out_l.push(soft_tanh((flute + chiff * 1.15) * env));
                out_r.push(soft_tanh((flute + chiff * 0.85) * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        16 => {
            // Dub Delay Siren Hit (sweeping LFO dub siren + stereo ping-pong echo)
            let len = (1.6 * sr) as usize;
            let mut dry = Vec::with_capacity(len);
            let mut ph = 0.0_f32;
            for i in 0..len {
                let t = i as f32 / sr;
                let lfo = (t * 7.5 * TAU).sin();
                let f = base_hz * (1.25 + 0.45 * lfo) * (1.0 - 0.18 * (t / 1.6));
                ph = (ph + f / sr).fract();
                let s = (ph * TAU).sin() * 0.72 + (ph * 2.0 * TAU).sin() * 0.22;
                let gate = if t < 0.55 { 1.0 } else { (-(t - 0.55) * 6.0).exp() };
                dry.push(s * gate * (t / 0.005).min(1.0));
            }
            let d1 = (0.18 * sr) as usize;
            let d2 = (0.36 * sr) as usize;
            let mut out_l = vec![0.0_f32; len];
            let mut out_r = vec![0.0_f32; len];
            for i in 0..len {
                let e1 = if i >= d1 { dry[i - d1] * 0.52 } else { 0.0 };
                let e2 = if i >= d2 { dry[i - d2] * 0.30 } else { 0.0 };
                out_l[i] = soft_tanh(dry[i] + e2);
                out_r[i] = soft_tanh(dry[i] * 0.82 + e1);
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        17 => {
            // Gospel Organ Drawbars (9 Hammond drawbars + key click + stereo Leslie chorus)
            let len = (1.8 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut rng = super::Xorshift32::new(0x046A_9988);
            let drawbars: [(f32, f32); 7] = [
                (0.5, 0.55),
                (1.0, 0.85),
                (1.5, 0.48),
                (2.0, 0.72),
                (3.0, 0.36),
                (4.0, 0.42),
                (6.0, 0.22),
            ];
            for i in 0..len {
                let t = i as f32 / sr;
                let mut sum = 0.0_f32;
                for &(mult, amp) in &drawbars {
                    sum += (t * base_hz * mult * TAU).sin() * amp;
                }
                let key_click = rng.next_bipolar() * (-t * 280.0).exp() * 0.26;
                let leslie_l = 0.82 + 0.18 * (t * 6.4 * TAU).sin();
                let leslie_r = 0.82 + 0.18 * (t * 6.4 * TAU + 1.8).sin();
                let env = (t / 0.004).min(1.0) * (1.0 - (t / 1.8)).max(0.0).powf(0.3);
                out_l.push(soft_tanh((sum * 0.26 + key_click) * leslie_l * env));
                out_r.push(soft_tanh((sum * 0.26 + key_click) * leslie_r * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        18 => {
            // Chamber Pizzicato Pluck (short string section pizzicato + wooden resonance)
            let len = (0.75 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            let mut rng = super::Xorshift32::new(0xD122_1C47);
            let mut body_bp = Svf12Filter::new(sr);
            for i in 0..len {
                let t = i as f32 / sr;
                let p1 = (t * base_hz * TAU).sin() * (-t * 6.2).exp() * 0.85;
                let p2 = (t * base_hz * 2.002 * TAU).sin() * (-t * 9.8).exp() * 0.46;
                let p3 = (t * base_hz * 3.005 * TAU).sin() * (-t * 14.5).exp() * 0.24;
                let finger = rng.next_bipolar() * (-t * 160.0).exp() * 0.28;
                let body = body_bp.process_bp(p1 + finger, 390.0, 0.30) * 0.42;
                let env = (t / 0.002).min(1.0);
                out_l.push(soft_tanh((p1 + p2 * 1.1 + p3 * 0.8 + body + finger) * env));
                out_r.push(soft_tanh((p1 + p2 * 0.9 + p3 * 1.15 + body + finger) * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
        _ => {
            // Cosmic Shimmer Glass (octave FM crystal partials + stereo air)
            let len = (2.0 * sr) as usize;
            let mut out_l = Vec::with_capacity(len);
            let mut out_r = Vec::with_capacity(len);
            for i in 0..len {
                let t = i as f32 / sr;
                let m_env = (-t * 2.4).exp();
                let fm_l = (t * base_hz * 3.0 * TAU).sin() * 0.85 * m_env;
                let fm_r = (t * base_hz * 4.0 * TAU).sin() * 0.85 * m_env;
                let c_l = (t * base_hz * 0.998 * TAU + fm_l).sin() * 0.62
                    + (t * base_hz * 2.002 * TAU).sin() * 0.34;
                let c_r = (t * base_hz * 1.002 * TAU + fm_r).sin() * 0.62
                    + (t * base_hz * 1.998 * TAU).sin() * 0.34;
                let env = (t / 0.025).min(1.0) * (-t * 1.25).exp();
                out_l.push(soft_tanh(c_l * env));
                out_r.push(soft_tanh(c_r * env));
            }
            normalize_stereo_peak(&out_l, &out_r)
        }
    }
}

/// Loads a WAV file via `hound`, preserving true stereo L/R, resamples via 4-point Hermite, and normalizes.
pub fn load_wav_to_stereo_f32(path: &Path, target_sr: u32) -> Result<(Vec<f32>, Vec<f32>), String> {
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

    let frames = interleaved.len() / channels;
    let mut left = Vec::with_capacity(frames + 1);
    let mut right = Vec::with_capacity(frames + 1);
    for frame in interleaved.chunks(channels) {
        match frame.len() {
            0 => {}
            1 => {
                let m = frame[0].clamp(-1.0, 1.0);
                left.push(m);
                right.push(m);
            }
            2 => {
                left.push(frame[0].clamp(-1.0, 1.0));
                right.push(frame[1].clamp(-1.0, 1.0));
            }
            n => {
                // Fold multi-channel evenly: even indices left, odd indices right, plus center blend
                let mut l = 0.0_f32;
                let mut r = 0.0_f32;
                for (ci, &s) in frame.iter().enumerate() {
                    if ci % 2 == 0 {
                        l += s;
                    } else {
                        r += s;
                    }
                }
                let scale = 2.0 / n as f32;
                left.push((l * scale).clamp(-1.0, 1.0));
                right.push((r * scale).clamp(-1.0, 1.0));
            }
        }
    }

    let res_l = resample_channel_hermite(&left, src_sr, target_sr);
    let res_r = resample_channel_hermite(&right, src_sr, target_sr);
    Ok(normalize_stereo_peak(&res_l, &res_r))
}

/// Loads a WAV file via `hound`, converts to mono f32, resamples to `target_sr`, and normalizes.
pub fn load_wav_to_mono_f32(path: &Path, target_sr: u32) -> Result<Vec<f32>, String> {
    let (l, r) = load_wav_to_stereo_f32(path, target_sr)?;
    let mono: Vec<f32> = l.iter().zip(r.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
    Ok(normalize_peak(&mono))
}

/// Loads MP3, WAV, FLAC, or OGG audio via `symphonia` (with `hound` fast-path for WAV),
/// preserving stereo L/R, caps at MAX_SAMPLE_SECONDS, Hermite-resamples to `target_sr`, and normalizes.
pub fn load_audio_to_stereo_f32(path: &Path, target_sr: u32) -> Result<(Vec<f32>, Vec<f32>), String> {
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
        if let Ok(stereo) = load_wav_to_stereo_f32(path, target_sr) {
            return Ok(stereo);
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

    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        if left.len() >= max_src_frames {
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
                    if left.len() >= max_src_frames {
                        break;
                    }
                    if channels == 1 {
                        let m = frame[0].clamp(-1.0, 1.0);
                        left.push(m);
                        right.push(m);
                    } else if channels == 2 {
                        left.push(frame[0].clamp(-1.0, 1.0));
                        right.push(frame[1].clamp(-1.0, 1.0));
                    } else {
                        let mut l = 0.0_f32;
                        let mut r = 0.0_f32;
                        for (ci, &s) in frame.iter().enumerate() {
                            if ci % 2 == 0 {
                                l += s;
                            } else {
                                r += s;
                            }
                        }
                        let scale = 2.0 / channels as f32;
                        left.push((l * scale).clamp(-1.0, 1.0));
                        right.push((r * scale).clamp(-1.0, 1.0));
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(format!("Decode error: {e}")),
        }
    }

    if left.is_empty() {
        return Err("Audio file decoded to 0 samples".to_string());
    }
    let res_l = resample_channel_hermite(&left, src_sr, target_sr);
    let res_r = resample_channel_hermite(&right, src_sr, target_sr);
    Ok(normalize_stereo_peak(&res_l, &res_r))
}

/// Loads MP3, WAV, FLAC, or OGG audio via `symphonia` (with `hound` fallback for WAV),
/// downmixes to mono f32, caps at MAX_SAMPLE_SECONDS, resamples to `target_sr`, and normalizes.
pub fn load_audio_to_mono_f32(path: &Path, target_sr: u32) -> Result<Vec<f32>, String> {
    let (l, r) = load_audio_to_stereo_f32(path, target_sr)?;
    let mono: Vec<f32> = l.iter().zip(r.iter()).map(|(&a, &b)| (a + b) * 0.5).collect();
    Ok(normalize_peak(&mono))
}

pub fn resample_channel_hermite(input: &[f32], src_sr: u32, dst_sr: u32) -> Vec<f32> {
    if input.len() < 4 || src_sr == dst_sr {
        return resample_mono_linear(input, src_sr, dst_sr);
    }
    let ratio = dst_sr as f64 / src_sr as f64;
    let out_len = ((input.len() as f64) * ratio).round().max(1.0) as usize;
    let max_i = input.len() - 1;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = (i as f64 / ratio).min(max_i as f64);
        let idx0 = pos.floor() as usize;
        let idx_m1 = idx0.saturating_sub(1);
        let idx1 = (idx0 + 1).min(max_i);
        let idx2 = (idx0 + 2).min(max_i);
        let frac = (pos - idx0 as f64) as f32;
        out.push(hermite4(input[idx_m1], input[idx0], input[idx1], input[idx2], frac).clamp(-1.0, 1.0));
    }
    out
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

pub fn trim_silence_and_normalize_stereo(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
) -> (Vec<f32>, Vec<f32>) {
    let frames = left.len().min(right.len());
    if frames == 0 {
        return (Vec::new(), Vec::new());
    }
    let threshold = 0.015_f32;
    let pre_pad = (sample_rate as usize / 100).min(256);
    let first_loud = (0..frames)
        .position(|i| left[i].abs().max(right[i].abs()) > threshold)
        .unwrap_or(0);
    let start = first_loud.saturating_sub(pre_pad);
    let last_loud = (0..frames)
        .rposition(|i| left[i].abs().max(right[i].abs()) > threshold)
        .unwrap_or(frames - 1);
    let end = (last_loud + pre_pad * 4).min(frames);
    if end <= start + 16 {
        return normalize_stereo_peak(&left[..frames], &right[..frames]);
    }
    normalize_stereo_peak(&left[start..end], &right[start..end])
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

pub fn normalize_stereo_peak(left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0_f32, |m, &s| m.max(s.abs()));
    if peak < 1e-5 {
        return (left.to_vec(), right.to_vec());
    }
    let gain = (0.94 / peak).min(12.0);
    (
        left.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect(),
        right.iter().map(|&s| (s * gain).clamp(-1.0, 1.0)).collect(),
    )
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
        let low_alpha = one_pole_coeff(260.0, sr);
        let high_alpha = one_pole_coeff(3200.0, sr);
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
            if let Some((left, right)) = part.sample.decode_stereo_pcm() {
                if !left.is_empty() {
                    let _ = self.parts[i].swap_stereo_buffer(
                        Arc::new(left),
                        Arc::new(right),
                        part.sample.sample_rate.max(8_000),
                    );
                    continue;
                }
            }
            let bid = part.sample.builtin_id.unwrap_or(3);
            let (l, r) = generate_builtin_sample_stereo(bid, self.sr as u32);
            let _ = self.parts[i].swap_stereo_buffer(Arc::new(l), Arc::new(r), self.sr as u32);
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
            sp.filter_enabled = true;
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

    /// Returns a bitmask of pads (`bit i` set => Pad `i` currently has an active playing voice).
    pub fn active_parts_mask(&self) -> u8 {
        let mut mask = 0u8;
        for i in 0..8 {
            if self.parts[i].active_voice_count() > 0 {
                mask |= 1u8 << i;
            }
        }
        mask
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
            let (sl_in, sr_in) = self.parts[i].process_stereo(&sp);
            if part.mute || (sl_in.abs() < 1e-7 && sr_in.abs() < 1e-7) {
                continue;
            }
            let pan_val = self.step_fx[i].pan_lock.unwrap_or(part.pan);
            let p = (pan_val.clamp(-1.0, 1.0) + 1.0) * 0.5;
            let angle = p * std::f32::consts::FRAC_PI_2;
            let sl = sl_in * part.level * angle.cos() * 1.15;
            let sr = sr_in * part.level * angle.sin() * 1.15;
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
        let out_l = soft_ceiling(low_l * b + mid_l + high_l * t, 1.15, 0.35);
        let out_r = soft_ceiling(low_r * b + mid_r + high_r * t, 1.15, 0.35);
        (out_l, out_r, rev_l, rev_r)
    }
}
