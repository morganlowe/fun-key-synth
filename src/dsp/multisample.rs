use std::f32::consts::TAU;
use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::model::midi_to_hz;

use super::sampler::{load_philharmonia_trimmed_stereo, resolve_philharmonia_dir};
use super::{
    soft_ceiling, soft_tanh, AdsrEnv, AmpCabSim, AnalogDrift, StereoPhaser, Svf12Filter,
};

const MAX_MS_VOICES: usize = 8;
const MS_SR: u32 = 44_100;
pub const NUM_PHILHARMONIA_BANKS: usize = 26;

pub const PHILHARMONIA_PERC_LABELS: [&str; 24] = [
    "C3 Bass Drum",
    "C#3 Snare Drum",
    "D3 Tom-Toms",
    "D#3 Surdo",
    "E3 Djembe",
    "F3 Djundjun",
    "F#3 Tenor Drum",
    "G3 Tambourine",
    "G#3 Clash Cymbals",
    "A3 Suspended Cymbal",
    "A#3 Chinese Cymbal",
    "B3 Sizzle Cymbal",
    "C4 Tam-Tam",
    "C#4 Thai Gong",
    "D4 Triangle",
    "D#4 Agogo Bells",
    "E4 Cowbell",
    "F4 Woodblock",
    "F#4 Castanets",
    "G4 Cabasa",
    "G#4 Guiro",
    "A4 Shaker",
    "A#4 Vibraslap",
    "B4 Bell Tree",
];

const PERC_KIT_FOLDERS: [&str; 24] = [
    "bass drum",
    "snare drum",
    "tom-toms",
    "surdo",
    "djembe",
    "djundjun",
    "tenor drum",
    "tambourine",
    "clash cymbals",
    "suspended cymbal",
    "Chinese cymbal",
    "sizzle cymbal",
    "tam-tam",
    "Thai gong",
    "triangle",
    "agogo bells",
    "cowbell",
    "woodblock",
    "castanets",
    "cabasa",
    "guiro",
    "banana shaker",
    "vibraslap",
    "bell tree",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Articulation {
    Normal,
    Pizzicato,
}

#[derive(Debug, Clone)]
struct SampleZone {
    root_midi: u8,
    left: Arc<Vec<f32>>,
    right: Arc<Vec<f32>>,
    sample_rate: f32,
    looped: bool,
    unpitched: bool,
    loop_start: usize,
    loop_end: usize,
}

#[derive(Debug, Clone)]
struct MultiSampleBank {
    zones: Vec<SampleZone>,
    has_d50_pad: bool,
    pad_gain: f32,
    loaded_from_disk: bool,
}

fn parse_phil_note_midi(filename: &str) -> Option<u8> {
    let mut parts = filename.split('_');
    let _inst = parts.next()?;
    let note_tok = parts.next()?;
    if note_tok.len() < 2 {
        return None;
    }
    let bytes = note_tok.as_bytes();
    let letter_pc = match bytes[0] {
        b'C' | b'c' => 0i32,
        b'D' | b'd' => 2,
        b'E' | b'e' => 4,
        b'F' | b'f' => 5,
        b'G' | b'g' => 7,
        b'A' | b'a' => 9,
        b'B' | b'b' => 11,
        _ => return None,
    };
    let (acc, oct_slice) = match bytes.get(1) {
        Some(b's') | Some(b'S') | Some(b'#') => (1i32, &note_tok[2..]),
        Some(b'b') if note_tok.len() >= 3 => (-1i32, &note_tok[2..]),
        _ => (0i32, &note_tok[1..]),
    };
    let oct: i32 = oct_slice.parse().ok()?;
    let midi = (oct + 1) * 12 + letter_pc + acc;
    if (21..=108).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

fn collect_mp3s_recursive(
    root: &Path,
    dir: &Path,
    articulation: Articulation,
    out: &mut Vec<(String, u8, i32)>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        if p.is_dir() {
            collect_mp3s_recursive(root, &p, articulation, out);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("mp3"))
            .unwrap_or(false)
        {
            let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if let Some(midi) = parse_phil_note_midi(fname) {
                let lower = fname.to_ascii_lowercase();
                if lower.contains("col-legno")
                    || lower.contains("tremolo")
                    || lower.contains("trill")
                    || lower.contains("harmonics")
                    || lower.contains("gliss")
                    || lower.contains("flutter")
                    || lower.contains("sul-pont")
                    || lower.contains("snap")
                    || lower.contains("bartok")
                    || lower.contains("staccato")
                {
                    continue;
                }
                match articulation {
                    Articulation::Normal => {
                        if lower.contains("pizz") {
                            continue;
                        }
                    }
                    Articulation::Pizzicato => {
                        if !lower.contains("pizz") {
                            continue;
                        }
                    }
                }
                let mut score = 0i32;
                if lower.contains("very-long") || lower.contains("_15_") || lower.contains("_long_") {
                    score += 50;
                } else if lower.contains("_1_") || lower.contains("_05_") {
                    score += 25;
                } else if lower.contains("_025_") {
                    score -= 20;
                }
                if lower.contains("arco-normal") || lower.contains("_normal.") {
                    score += 30;
                }
                if lower.contains("_forte_") {
                    score += 20;
                } else if lower.contains("_mezzo-forte_") {
                    score += 16;
                } else if lower.contains("_mezzo-piano_") {
                    score += 10;
                } else if lower.contains("_fortissimo_") {
                    score += 8;
                }
                if let Ok(rel) = p.strip_prefix(root) {
                    if let Some(rel_str) = rel.to_str() {
                        out.push((rel_str.replace('\\', "/"), midi, score));
                    }
                }
            }
        }
    }
}

fn collect_best_perc_hit(root: &Path, sub_dir: &Path) -> Option<String> {
    let entries = fs::read_dir(sub_dir).ok()?;
    let mut best: Option<(String, i32)> = None;
    for ent in entries.flatten() {
        let p = ent.path();
        if !p.is_file() {
            continue;
        }
        let is_mp3 = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("mp3"))
            .unwrap_or(false);
        if !is_mp3 {
            continue;
        }
        let fname = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let lower = fname.to_ascii_lowercase();
        let mut score = 0i32;
        if lower.contains("phrase") || lower.contains("rhythm") {
            score -= 80;
        }
        if lower.contains("roll") {
            score -= 25;
        }
        if lower.contains("_025_") || lower.contains("_05_") {
            score += 35;
        } else if lower.contains("_1_") || lower.contains("_15_") {
            score += 25;
        } else if lower.contains("_long_") {
            score += 15;
        }
        if lower.contains("struck-singly")
            || lower.contains("struck-together")
            || lower.contains("bass-drum-mallet")
            || lower.contains("with-snares")
            || lower.contains("_hand")
            || lower.contains("undamped")
            || lower.contains("clean")
        {
            score += 25;
        }
        if lower.contains("_forte_") {
            score += 18;
        } else if lower.contains("_mezzo-forte_") {
            score += 14;
        }
        if let Ok(rel) = p.strip_prefix(root) {
            if let Some(rel_str) = rel.to_str() {
                if best.as_ref().map(|&(_, bs)| score > bs).unwrap_or(true) {
                    best = Some((rel_str.replace('\\', "/"), score));
                }
            }
        }
    }
    best.map(|(s, _)| s)
}

fn prepare_looped_zone(
    mut l: Vec<f32>,
    mut r: Vec<f32>,
    sr: u32,
    root_midi: u8,
    looped: bool,
    unpitched: bool,
) -> SampleZone {
    let n = l.len().min(r.len());
    l.truncate(n);
    r.truncate(n);

    if looped && n > (sr as usize / 2) {
        let loop_start = (n * 38) / 100;
        let loop_end = ((n * 82) / 100).max(loop_start + (sr as usize / 6)).min(n);
        let xfade = ((0.11 * sr as f32) as usize).min((loop_end - loop_start) / 3).max(64);
        let pre_start = loop_start.saturating_sub(xfade);
        if loop_end > loop_start + xfade && pre_start + xfade <= loop_start {
            for k in 0..xfade {
                let t = k as f32 / xfade as f32;
                let a = (t * std::f32::consts::FRAC_PI_2).cos();
                let b = (t * std::f32::consts::FRAC_PI_2).sin();
                let dst = loop_end - xfade + k;
                let src = pre_start + k;
                l[dst] = l[dst] * a + l[src] * b;
                r[dst] = r[dst] * a + r[src] * b;
            }
        }
        SampleZone {
            root_midi,
            left: Arc::new(l),
            right: Arc::new(r),
            sample_rate: sr as f32,
            looped: true,
            unpitched,
            loop_start,
            loop_end,
        }
    } else {
        SampleZone {
            root_midi,
            left: Arc::new(l),
            right: Arc::new(r),
            sample_rate: sr as f32,
            looped: false,
            unpitched,
            loop_start: 0,
            loop_end: n.saturating_sub(1),
        }
    }
}

fn synthetic_fallback_zone(bank_id: u8) -> SampleZone {
    let sr = MS_SR as f32;
    let dur = 1.6_f32;
    let n = (sr * dur) as usize;
    let f0 = midi_to_hz(60.0);
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr;
        let atk = (t / 0.012).min(1.0);
        let dec = if matches!(bank_id, 2 | 5 | 10 | 11 | 12 | 25) {
            (-t * 2.4).exp()
        } else {
            (-t * 0.35).exp()
        };
        let env = atk * dec;
        let sl = (t * f0 * TAU).sin() * 0.55
            + (t * f0 * 2.004 * TAU).sin() * 0.25
            + (t * f0 * 3.007 * TAU).sin() * 0.14
            + (t * f0 * 4.99 * TAU).sin() * 0.07;
        let sr_v = (t * f0 * 0.999 * TAU).sin() * 0.55
            + (t * f0 * 1.996 * TAU).sin() * 0.25
            + (t * f0 * 3.012 * TAU).sin() * 0.14
            + (t * f0 * 5.01 * TAU).sin() * 0.07;
        l.push(sl * env);
        r.push(sr_v * env);
    }
    prepare_looped_zone(
        l,
        r,
        MS_SR,
        60,
        !matches!(bank_id, 2 | 5 | 10 | 11 | 12 | 25),
        false,
    )
}

fn build_bank(bank_id: u8) -> Arc<MultiSampleBank> {
    let bank_id = (bank_id as usize % NUM_PHILHARMONIA_BANKS) as u8;
    if bank_id == 25 {
        let mut zones = Vec::new();
        if let Some(root) = resolve_philharmonia_dir() {
            let perc_root = root.join("percussion");
            for (i, &folder) in PERC_KIT_FOLDERS.iter().enumerate() {
                let sub = perc_root.join(folder);
                if !sub.is_dir() {
                    continue;
                }
                if let Some(rel) = collect_best_perc_hit(&root, &sub) {
                    if let Some((l, r)) = load_philharmonia_trimmed_stereo(&rel, MS_SR, 2.4) {
                        zones.push(prepare_looped_zone(l, r, MS_SR, 48 + i as u8, false, true));
                    }
                }
            }
        }
        let loaded_from_disk = !zones.is_empty();
        if zones.is_empty() {
            zones.push(synthetic_fallback_zone(bank_id));
        }
        return Arc::new(MultiSampleBank {
            zones,
            has_d50_pad: false,
            pad_gain: 0.0,
            loaded_from_disk,
        });
    }

    let (targets, art, looped, has_d50_pad, pad_gain): (
        &[(&str, u8, u8, &[u8])],
        Articulation,
        bool,
        bool,
        f32,
    ) = match bank_id {
        // 0: Philharmonia Strings Ensemble (Cello + Viola + Violin)
        0 => (
            &[
                ("cello", 36, 54, &[40, 48]),
                ("viola", 50, 65, &[55, 60]),
                ("violin", 62, 86, &[67, 74]),
            ],
            Articulation::Normal,
            true,
            true,
            0.38,
        ),
        // 1: Cinematic Brass Section (Trombone + French Horn + Trumpet)
        1 => (
            &[
                ("tuba", 28, 45, &[36]),
                ("trombone", 40, 58, &[46, 53]),
                ("french horn", 48, 65, &[58]),
                ("trumpet", 58, 82, &[65, 72]),
            ],
            Articulation::Normal,
            true,
            true,
            0.34,
        ),
        // 2: Real Guitar
        2 => (
            &[("guitar", 38, 80, &[43, 50, 57, 64, 71, 76])],
            Articulation::Normal,
            false,
            false,
            0.0,
        ),
        // 3: Roland D-50 LA Fantasia (Mandolin + Flute + Lush Synth Bed)
        3 => (
            &[
                ("mandolin", 52, 74, &[57, 64]),
                ("flute", 60, 88, &[67, 74, 81]),
            ],
            Articulation::Normal,
            true,
            true,
            0.62,
        ),
        // 4: Jazz Sax & Winds
        4 => (
            &[
                ("saxophone", 44, 70, &[50, 57, 64]),
                ("flute", 64, 90, &[71, 78]),
            ],
            Articulation::Normal,
            true,
            true,
            0.24,
        ),
        // 5: Plucked Chamber (Guitar + Banjo + Mandolin)
        5 => (
            &[
                ("guitar", 40, 62, &[45, 52]),
                ("banjo", 54, 72, &[59, 66]),
                ("mandolin", 62, 86, &[69, 76]),
            ],
            Articulation::Normal,
            false,
            true,
            0.18,
        ),
        // 6..=10: Individual Strings + Pizzicato Strings
        6 => (
            &[("violin", 55, 93, &[55, 59, 63, 67, 71, 75, 79, 83, 88])],
            Articulation::Normal,
            true,
            true,
            0.22,
        ),
        7 => (
            &[("viola", 48, 81, &[48, 52, 56, 60, 64, 68, 72, 76])],
            Articulation::Normal,
            true,
            true,
            0.22,
        ),
        8 => (
            &[("cello", 36, 72, &[36, 40, 44, 48, 52, 56, 60, 64, 68])],
            Articulation::Normal,
            true,
            true,
            0.22,
        ),
        9 => (
            &[("double bass", 28, 57, &[28, 32, 36, 40, 44, 48, 52, 56])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        10 => (
            &[
                ("double bass", 28, 50, &[33, 40, 46]),
                ("viola", 48, 66, &[52, 57, 62]),
                ("violin", 64, 86, &[67, 72, 77, 83]),
            ],
            Articulation::Pizzicato,
            false,
            true,
            0.16,
        ),
        // 11..=12: Plucked Folk / Acoustic
        11 => (
            &[("banjo", 48, 79, &[50, 55, 60, 64, 69, 74])],
            Articulation::Normal,
            false,
            true,
            0.16,
        ),
        12 => (
            &[("mandolin", 55, 88, &[55, 60, 64, 69, 74, 79, 84])],
            Articulation::Normal,
            false,
            true,
            0.16,
        ),
        // 13..=16: Individual Brass
        13 => (
            &[("trumpet", 52, 82, &[54, 58, 62, 66, 70, 74, 78, 81])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        14 => (
            &[("french horn", 41, 74, &[43, 47, 51, 55, 59, 63, 67, 72])],
            Articulation::Normal,
            true,
            true,
            0.22,
        ),
        15 => (
            &[("trombone", 38, 69, &[40, 44, 48, 52, 56, 60, 64, 68])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        16 => (
            &[("tuba", 29, 58, &[31, 35, 39, 43, 47, 51, 55])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        // 17..=24: Individual Woodwinds & Saxophone
        17 => (
            &[("flute", 60, 93, &[60, 64, 68, 72, 76, 80, 84, 88])],
            Articulation::Normal,
            true,
            true,
            0.22,
        ),
        18 => (
            &[("oboe", 58, 88, &[58, 62, 66, 70, 74, 78, 82, 86])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        19 => (
            &[("clarinet", 50, 86, &[50, 54, 58, 62, 66, 70, 74, 79, 83])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        20 => (
            &[("bass clarinet", 36, 69, &[38, 42, 46, 50, 54, 58, 62, 66])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        21 => (
            &[("cor anglais", 52, 81, &[52, 56, 60, 64, 68, 72, 76, 80])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        22 => (
            &[("bassoon", 34, 69, &[36, 40, 44, 48, 52, 56, 60, 64])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
        23 => (
            &[("contrabassoon", 24, 55, &[26, 30, 34, 38, 42, 46, 50, 54])],
            Articulation::Normal,
            true,
            true,
            0.18,
        ),
        _ => (
            &[("saxophone", 49, 81, &[50, 54, 58, 62, 66, 70, 74, 78])],
            Articulation::Normal,
            true,
            true,
            0.20,
        ),
    };

    let mut zones = Vec::new();
    if let Some(root) = resolve_philharmonia_dir() {
        for &(sub, lo, hi, anchors) in targets {
            let sub_dir = root.join(sub);
            if !sub_dir.is_dir() {
                continue;
            }
            let mut cands = Vec::new();
            collect_mp3s_recursive(&root, &sub_dir, art, &mut cands);
            cands.retain(|&(_, midi, _)| midi >= lo && midi <= hi);
            if cands.is_empty() {
                continue;
            }
            let mut picked_paths: Vec<String> = Vec::new();
            for &target_midi in anchors {
                if let Some((rel, midi, _)) = cands
                    .iter()
                    .filter(|(rel, _, _)| !picked_paths.contains(rel))
                    .min_by_key(|(_, m, score)| {
                        let dist = (*m as i32 - target_midi as i32).abs();
                        (dist * 20) - *score
                    })
                    .cloned()
                {
                    let max_sec = if looped { 2.2 } else { 2.4 };
                    if let Some((l, r)) = load_philharmonia_trimmed_stereo(&rel, MS_SR, max_sec) {
                        picked_paths.push(rel);
                        zones.push(prepare_looped_zone(l, r, MS_SR, midi, looped, false));
                    }
                }
            }
        }
    }

    let loaded_from_disk = !zones.is_empty();
    if zones.is_empty() {
        zones.push(synthetic_fallback_zone(bank_id));
    } else {
        zones.sort_by_key(|z| z.root_midi);
    }

    Arc::new(MultiSampleBank {
        zones,
        has_d50_pad,
        pad_gain,
        loaded_from_disk,
    })
}

fn bank_slot(bank_id: u8) -> &'static OnceLock<Arc<MultiSampleBank>> {
    const INIT: OnceLock<Arc<MultiSampleBank>> = OnceLock::new();
    static BANKS: [OnceLock<Arc<MultiSampleBank>>; NUM_PHILHARMONIA_BANKS] =
        [INIT; NUM_PHILHARMONIA_BANKS];
    &BANKS[(bank_id as usize) % NUM_PHILHARMONIA_BANKS]
}

fn get_bank(bank_id: u8) -> Arc<MultiSampleBank> {
    let id = (bank_id as usize % NUM_PHILHARMONIA_BANKS) as u8;
    bank_slot(id).get_or_init(|| build_bank(id)).clone()
}

/// Triggers asynchronous background MP3 decoding for `bank_id` so UI switching stays glitch-free.
pub fn preload_philharmonia_bank_async(bank_id: u8) {
    let id = (bank_id as usize % NUM_PHILHARMONIA_BANKS) as u8;
    if bank_slot(id).get().is_some() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name(format!("phil-load-{id}"))
        .spawn(move || {
            let _ = get_bank(id);
        });
}

/// Non-blocking status check for the UI: `(library_on_disk, cached_zones, is_cached)`.
pub fn philharmonia_bank_status(bank_id: u8) -> (bool, usize, bool) {
    let on_disk = resolve_philharmonia_dir().is_some();
    if let Some(bank) = bank_slot(bank_id).get() {
        (on_disk && bank.loaded_from_disk, bank.zones.len(), true)
    } else {
        (on_disk, 0, false)
    }
}

/// Checks synchronously whether `bank_id` loaded real recordings from disk (used in tests).
pub fn philharmonia_bank_loaded_count(bank_id: u8) -> (bool, usize) {
    let bank = get_bank(bank_id);
    (bank.loaded_from_disk, bank.zones.len())
}

#[derive(Debug, Clone)]
struct MsVoice {
    active: bool,
    held: bool,
    note: u8,
    current_midi: f32,
    target_midi: f32,
    zone_idx: usize,
    pos: f64,
    rate: f64,
    velocity: f32,
    accent: f32,
    age: u64,
    pan: f32,
    pad_phase_a: f32,
    pad_phase_b: f32,
    amp_env: AdsrEnv,
    filter_l: Svf12Filter,
    filter_r: Svf12Filter,
    thermal: AnalogDrift,
}

impl MsVoice {
    fn new(sr: f32, idx: usize) -> Self {
        let pan_table = [-0.24_f32, 0.24, -0.14, 0.14, -0.30, 0.30, -0.08, 0.08];
        Self {
            active: false,
            held: false,
            note: 60,
            current_midi: 60.0,
            target_midi: 60.0,
            zone_idx: 0,
            pos: 0.0,
            rate: 1.0,
            velocity: 0.9,
            accent: 0.0,
            age: 0,
            pan: pan_table[idx % pan_table.len()],
            pad_phase_a: (idx as f32 * 0.17).fract(),
            pad_phase_b: (idx as f32 * 0.31 + 0.4).fract(),
            amp_env: AdsrEnv::new(sr),
            filter_l: Svf12Filter::new(sr),
            filter_r: Svf12Filter::new(sr),
            thermal: AnalogDrift::new(0x5048_494C_u32.wrapping_add(idx as u32 * 0x1013)),
        }
    }

    fn set_sample_rate(&mut self, sr: f32) {
        self.amp_env.set_sample_rate(sr);
        self.filter_l.set_sample_rate(sr);
        self.filter_r.set_sample_rate(sr);
    }
}

/// Chromatic Multi-Sampled Philharmonia & Roland D-50 LA Hybrid Engine.
#[derive(Debug, Clone)]
pub struct MultiSampleSynth {
    voices: [MsVoice; MAX_MS_VOICES],
    age_counter: u64,
    sample_rate: f32,
    active_bank_id: u8,
    phaser: StereoPhaser,
    amp_cab: AmpCabSim,
}

impl MultiSampleSynth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            voices: std::array::from_fn(|i| MsVoice::new(sr, i)),
            age_counter: 0,
            sample_rate: sr,
            active_bank_id: 0,
            phaser: StereoPhaser::new(sr),
            amp_cab: AmpCabSim::new(sr),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        let sr = sr.max(8000.0);
        self.sample_rate = sr;
        for v in &mut self.voices {
            v.set_sample_rate(sr);
        }
        self.phaser.set_sample_rate(sr);
        self.amp_cab.set_sample_rate(sr);
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, accent: bool, poly_mode: bool, bank_id: u8) {
        self.age_counter = self.age_counter.wrapping_add(1);
        let vel = velocity.clamp(0.05, 1.0);
        let acc = if accent { 0.25 } else { 0.0 };
        self.active_bank_id = (bank_id as usize % NUM_PHILHARMONIA_BANKS) as u8;
        let bank = get_bank(self.active_bank_id);

        let idx = if !poly_mode {
            for v in &mut self.voices[1..] {
                v.active = false;
                v.held = false;
                v.amp_env.reset();
            }
            0
        } else {
            self.voices
                .iter()
                .position(|v| !v.active)
                .or_else(|| {
                    self.voices
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| !v.held)
                        .min_by_key(|(_, v)| v.age)
                        .map(|(i, _)| i)
                })
                .unwrap_or_else(|| {
                    self.voices
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, v)| v.age)
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                })
        };

        let mut best_zi = 0usize;
        let mut best_dist = i32::MAX;
        for (zi, z) in bank.zones.iter().enumerate() {
            let d = (z.root_midi as i32 - note as i32).abs();
            if d < best_dist {
                best_dist = d;
                best_zi = zi;
            }
        }
        let z = &bank.zones[best_zi];
        let rate = if z.unpitched {
            z.sample_rate as f64 / self.sample_rate as f64
        } else {
            let semi_diff = note as f64 - z.root_midi as f64;
            2.0_f64.powf(semi_diff / 12.0) * (z.sample_rate as f64 / self.sample_rate as f64)
        };

        let v = &mut self.voices[idx];
        v.note = note;
        v.current_midi = note as f32;
        v.target_midi = note as f32;
        v.zone_idx = best_zi;
        v.pos = 0.0;
        v.rate = rate.clamp(0.18, 4.5);
        v.velocity = vel;
        v.accent = acc;
        v.age = self.age_counter;
        v.held = true;
        v.active = true;
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
            if v.active {
                v.held = false;
                v.amp_env.release();
            }
        }
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.active && v.amp_env.is_active())
            .count()
    }

    #[inline]
    pub fn process_stereo(
        &mut self,
        bank_id: u8,
        morph: f32,
        cutoff: f32,
        resonance: f32,
        drive: f32,
        amp_attack: f32,
        amp_decay: f32,
        amp_sustain: f32,
        amp_release: f32,
        poly_mode: bool,
        phaser_mix: f32,
        amp_cab: f32,
    ) -> (f32, f32) {
        let bank_id = (bank_id as usize % NUM_PHILHARMONIA_BANKS) as u8;
        if bank_id != self.active_bank_id {
            self.active_bank_id = bank_id;
        }
        let bank = get_bank(bank_id);
        let sr = self.sample_rate;
        let morph = morph.clamp(0.0, 1.0);
        let cutoff = cutoff.clamp(80.0, 18_000.0);
        let res = (resonance * 0.55).clamp(0.0, 0.65);

        let mut mix_l = 0.0_f32;
        let mut mix_r = 0.0_f32;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let z = &bank.zones[v.zone_idx.min(bank.zones.len() - 1)];
            let a_atk = if bank_id == 25 {
                0.001
            } else if bank_id == 0 || bank_id == 1 || (6..=9).contains(&bank_id) || (13..=24).contains(&bank_id) {
                amp_attack.max(0.012)
            } else {
                amp_attack.clamp(0.002, 0.06)
            };
            let a_sus = if bank_id == 25 {
                0.0
            } else if z.looped {
                amp_sustain.max(0.65)
            } else {
                0.0
            };
            let a_dec = if bank_id == 25 {
                2.4
            } else if z.looped {
                amp_decay.max(0.45)
            } else {
                amp_decay.max(1.2)
            };
            let a_rel = if bank_id == 25 {
                0.35
            } else if bank_id == 0 || bank_id == 3 {
                amp_release.max(0.42)
            } else {
                amp_release.max(0.16)
            };
            let a_env = v.amp_env.next(a_atk, a_dec, a_sus, a_rel);
            if !v.amp_env.is_active() {
                v.active = false;
                continue;
            }

            let len = z.left.len();
            if len < 4 {
                v.active = false;
                continue;
            }
            if v.pos as usize >= len - 1 {
                if z.looped && z.loop_end > z.loop_start + 8 {
                    let loop_len = (z.loop_end - z.loop_start) as f64;
                    v.pos = z.loop_start as f64 + ((v.pos - z.loop_start as f64) % loop_len);
                } else {
                    v.active = false;
                    continue;
                }
            }
            if z.looped && v.pos >= z.loop_end as f64 && z.loop_end > z.loop_start + 8 {
                let loop_len = (z.loop_end - z.loop_start) as f64;
                v.pos = z.loop_start as f64 + ((v.pos - z.loop_start as f64) % loop_len);
            }

            let i0 = (v.pos as usize).min(len - 1);
            let frac = (v.pos - i0 as f64) as f32;
            let i1 = (i0 + 1).min(len - 1);
            let s_l = z.left[i0] + (z.left[i1] - z.left[i0]) * frac;
            let s_r = z.right[i0] + (z.right[i1] - z.right[i0]) * frac;
            v.pos += v.rate;

            // Optional Roland D-50 LA / Analog Hybrid sustain bed scaled by morph
            let (mut sig_l, mut sig_r) = (s_l, s_r);
            if bank.has_d50_pad && morph > 0.02 {
                let therm = v.thermal.next(sr);
                let f_a = midi_to_hz(v.current_midi + 0.09 + therm * 0.02).clamp(18.0, sr * 0.42);
                let f_b = midi_to_hz(v.current_midi - 0.09 - therm * 0.02).clamp(18.0, sr * 0.42);
                v.pad_phase_a = (v.pad_phase_a + f_a / sr).fract();
                v.pad_phase_b = (v.pad_phase_b + f_b / sr).fract();
                let saw_a = 2.0 * v.pad_phase_a - 1.0;
                let saw_b = 2.0 * v.pad_phase_b - 1.0;
                let sub = ((v.pad_phase_a * 0.5) * TAU).sin() * 0.35;
                let pad_l = saw_a * 0.46 + saw_b * 0.24 + sub * 0.35;
                let pad_r = saw_b * 0.46 + saw_a * 0.24 + sub * 0.35;
                let w = bank.pad_gain * morph;
                let sample_w = if bank_id == 3 { 0.85 } else { 1.0 - w * 0.25 };
                sig_l = sig_l * sample_w + pad_l * w;
                sig_r = sig_r * sample_w + pad_r * w;
            }

            let eff_cut = cutoff.clamp(120.0, sr * 0.43);
            let fl = v.filter_l.process_lp_analog(sig_l, eff_cut, res, drive * 0.65);
            let fr = v.filter_r.process_lp_analog(sig_r, eff_cut, res, drive * 0.65);

            let amp = a_env * (v.velocity + v.accent * 0.2) * if poly_mode { 0.62 } else { 0.78 };
            let p = (v.pan * 0.35 + 0.5).clamp(0.0, 1.0);
            let pl = (1.0 - p).sqrt() * 1.4142;
            let pr = p.sqrt() * 1.4142;
            mix_l += fl * amp * pl;
            mix_r += fr * amp * pr;
        }

        let (mut out_l, mut out_r) = (soft_tanh(mix_l), soft_tanh(mix_r));
        if amp_cab > 0.002 {
            (out_l, out_r) = self.amp_cab.process_stereo(out_l, out_r, amp_cab);
        }
        if phaser_mix > 0.002 {
            (out_l, out_r) = self.phaser.process_stereo(out_l, out_r, phaser_mix);
        }
        (
            soft_ceiling(out_l, 1.5, 0.35),
            soft_ceiling(out_r, 1.5, 0.35),
        )
    }
}
