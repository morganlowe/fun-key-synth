use std::f32::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::midi_to_hz;

use super::{
    soft_ceiling, soft_tanh, AdsrEnv, AmpCabSim, StereoPhaser, Svf12Filter,
};

const MAX_SF2_VOICES: usize = 16;

#[derive(Debug, Clone)]
pub struct Sf2Zone {
    pub key_lo: u8,
    pub key_hi: u8,
    pub vel_lo: u8,
    pub vel_hi: u8,
    pub start: usize,
    pub end: usize,
    pub loop_start: usize,
    pub loop_end: usize,
    pub loop_mode: u8,
    pub sample_rate: f32,
    pub root_key: u8,
    pub coarse_tune: i16,
    pub fine_tune: i16,
    pub pan: f32,
    pub gain: f32,
    pub fc_hz: f32,
    pub q_norm: f32,
    pub attack_s: f32,
    pub decay_s: f32,
    pub sustain_lvl: f32,
    pub release_s: f32,
}

#[derive(Debug, Clone)]
pub struct Sf2Preset {
    pub name: String,
    pub bank: u16,
    pub program: u16,
    pub zones: Vec<Sf2Zone>,
}

/// Zero-dependency pure-Rust SoundFont 2.0 (`.sf2`) RIFF `sfbk` bank.
#[derive(Debug, Clone)]
pub struct SoundFontBank {
    pub name: String,
    pub sample_data: Arc<Vec<f32>>,
    pub presets: Vec<Sf2Preset>,
}

impl SoundFontBank {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        Self::from_bytes(&bytes)
    }

    pub fn preset_names(&self) -> Vec<String> {
        self.presets
            .iter()
            .map(|p| format!("{:03}:{:03} {}", p.bank, p.program, p.name))
            .collect()
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"sfbk" {
            return Err("Not a valid RIFF SoundFont 2.0 (.sf2) file".into());
        }

        let mut info_name = String::from("SoundFont Bank");
        let mut smpl_bytes: Option<&[u8]> = None;
        let mut phdr_b: Option<&[u8]> = None;
        let mut pbag_b: Option<&[u8]> = None;
        let mut pgen_b: Option<&[u8]> = None;
        let mut inst_b: Option<&[u8]> = None;
        let mut ibag_b: Option<&[u8]> = None;
        let mut igen_b: Option<&[u8]> = None;
        let mut shdr_b: Option<&[u8]> = None;

        let mut pos = 12usize;
        while pos + 8 <= data.len() {
            let cid = &data[pos..pos + 4];
            let csz = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            let end = (pos + csz).min(data.len());
            let body = &data[pos..end];
            pos = end + (csz & 1);

            if cid == b"LIST" && body.len() >= 4 {
                let ltype = &body[0..4];
                let mut sp = 4usize;
                while sp + 8 <= body.len() {
                    let scid = &body[sp..sp + 4];
                    let scsz = u32::from_le_bytes(body[sp + 4..sp + 8].try_into().unwrap()) as usize;
                    sp += 8;
                    let send = (sp + scsz).min(body.len());
                    let sbody = &body[sp..send];
                    sp = send + (scsz & 1);

                    if ltype == b"INFO" && scid == b"INAM" {
                        let s = String::from_utf8_lossy(sbody).trim_end_matches('\0').trim().to_string();
                        if !s.is_empty() {
                            info_name = s;
                        }
                    } else if ltype == b"sdta" && scid == b"smpl" {
                        smpl_bytes = Some(sbody);
                    } else if ltype == b"pdta" {
                        match scid {
                            b"phdr" => phdr_b = Some(sbody),
                            b"pbag" => pbag_b = Some(sbody),
                            b"pgen" => pgen_b = Some(sbody),
                            b"inst" => inst_b = Some(sbody),
                            b"ibag" => ibag_b = Some(sbody),
                            b"igen" => igen_b = Some(sbody),
                            b"shdr" => shdr_b = Some(sbody),
                            _ => {}
                        }
                    }
                }
            }
        }

        let smpl = smpl_bytes.ok_or_else(|| "SF2 missing sdta/smpl chunk".to_string())?;
        let phdr = phdr_b.ok_or_else(|| "SF2 missing pdta/phdr chunk".to_string())?;
        let pbag = pbag_b.ok_or_else(|| "SF2 missing pdta/pbag chunk".to_string())?;
        let pgen = pgen_b.ok_or_else(|| "SF2 missing pdta/pgen chunk".to_string())?;
        let inst = inst_b.ok_or_else(|| "SF2 missing pdta/inst chunk".to_string())?;
        let ibag = ibag_b.ok_or_else(|| "SF2 missing pdta/ibag chunk".to_string())?;
        let igen = igen_b.ok_or_else(|| "SF2 missing pdta/igen chunk".to_string())?;
        let shdr = shdr_b.ok_or_else(|| "SF2 missing pdta/shdr chunk".to_string())?;

        let n_smpl = smpl.len() / 2;
        let mut sample_data = Vec::with_capacity(n_smpl);
        for chunk in smpl.chunks_exact(2) {
            let v = i16::from_le_bytes([chunk[0], chunk[1]]);
            sample_data.push(v as f32 / 32768.0);
        }
        let sample_arc = Arc::new(sample_data);

        #[derive(Clone, Copy)]
        struct SampleHdr {
            start: usize,
            end: usize,
            loop_start: usize,
            loop_end: usize,
            sample_rate: f32,
            orig_pitch: u8,
            pitch_corr: i8,
        }

        let mut samples = Vec::new();
        for rec in shdr.chunks_exact(46) {
            let name = String::from_utf8_lossy(&rec[0..20]).trim_end_matches('\0').to_string();
            if name == "EOS" {
                break;
            }
            let start = u32::from_le_bytes(rec[20..24].try_into().unwrap()) as usize;
            let end = u32::from_le_bytes(rec[24..28].try_into().unwrap()) as usize;
            let loop_start = u32::from_le_bytes(rec[28..32].try_into().unwrap()) as usize;
            let loop_end = u32::from_le_bytes(rec[32..36].try_into().unwrap()) as usize;
            let sample_rate = u32::from_le_bytes(rec[36..40].try_into().unwrap()).max(4000) as f32;
            let orig_pitch = rec[40].min(127);
            let pitch_corr = rec[41] as i8;
            samples.push(SampleHdr {
                start: start.min(n_smpl),
                end: end.min(n_smpl),
                loop_start: loop_start.min(n_smpl),
                loop_end: loop_end.min(n_smpl),
                sample_rate,
                orig_pitch,
                pitch_corr,
            });
        }

        let read_bags = |b: &[u8]| -> Vec<usize> {
            b.chunks_exact(4)
                .map(|c| u16::from_le_bytes([c[0], c[1]]) as usize)
                .collect()
        };
        let read_gens = |b: &[u8]| -> Vec<(u16, u16)> {
            b.chunks_exact(4)
                .map(|c| {
                    (
                        u16::from_le_bytes([c[0], c[1]]),
                        u16::from_le_bytes([c[2], c[3]]),
                    )
                })
                .collect()
        };

        let pbags = read_bags(pbag);
        let pgens = read_gens(pgen);
        let ibags = read_bags(ibag);
        let igens = read_gens(igen);

        let tc_to_sec = |tc: i16| -> f32 {
            if tc <= -12000 {
                0.001
            } else {
                2.0_f32.powf(tc as f32 / 1200.0).clamp(0.001, 20.0)
            }
        };

        #[derive(Clone, Default)]
        struct RawGenAcc {
            key_lo: Option<u8>,
            key_hi: Option<u8>,
            vel_lo: Option<u8>,
            vel_hi: Option<u8>,
            start_off: i32,
            end_off: i32,
            ls_off: i32,
            le_off: i32,
            fc_cents: Option<i16>,
            q_cb: Option<i16>,
            pan: Option<i16>,
            att_cb: Option<i16>,
            atk_tc: Option<i16>,
            dec_tc: Option<i16>,
            sus_cb: Option<i16>,
            rel_tc: Option<i16>,
            coarse_tune: i16,
            fine_tune: i16,
            sample_modes: Option<u8>,
            override_root: Option<u8>,
            sample_id: Option<usize>,
            inst_id: Option<usize>,
        }

        let apply_gen = |acc: &mut RawGenAcc, op: u16, raw: u16, relative: bool| {
            let signed = raw as i16;
            match op {
                0 => acc.start_off += signed as i32,
                1 => acc.end_off += signed as i32,
                2 => acc.ls_off += signed as i32,
                3 => acc.le_off += signed as i32,
                4 => acc.start_off += (signed as i32) * 32768,
                8 => {
                    acc.fc_cents = Some(if relative {
                        acc.fc_cents.unwrap_or(13500).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                9 => {
                    acc.q_cb = Some(if relative {
                        acc.q_cb.unwrap_or(0).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                12 => acc.end_off += (signed as i32) * 32768,
                17 => {
                    acc.pan = Some(if relative {
                        acc.pan.unwrap_or(0).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                34 => {
                    acc.atk_tc = Some(if relative {
                        acc.atk_tc.unwrap_or(-12000).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                36 => {
                    acc.dec_tc = Some(if relative {
                        acc.dec_tc.unwrap_or(-12000).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                37 => {
                    acc.sus_cb = Some(if relative {
                        acc.sus_cb.unwrap_or(0).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                38 => {
                    acc.rel_tc = Some(if relative {
                        acc.rel_tc.unwrap_or(-12000).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                41 => acc.inst_id = Some(raw as usize),
                43 => {
                    let lo = (raw & 0xFF) as u8;
                    let hi = ((raw >> 8) & 0xFF) as u8;
                    acc.key_lo = Some(acc.key_lo.map_or(lo, |p| p.max(lo)));
                    acc.key_hi = Some(acc.key_hi.map_or(hi, |p| p.min(hi)));
                }
                44 => {
                    let lo = (raw & 0xFF) as u8;
                    let hi = ((raw >> 8) & 0xFF) as u8;
                    acc.vel_lo = Some(acc.vel_lo.map_or(lo, |p| p.max(lo)));
                    acc.vel_hi = Some(acc.vel_hi.map_or(hi, |p| p.min(hi)));
                }
                45 => acc.ls_off += (signed as i32) * 32768,
                48 => {
                    acc.att_cb = Some(if relative {
                        acc.att_cb.unwrap_or(0).saturating_add(signed)
                    } else {
                        signed
                    })
                }
                50 => acc.le_off += (signed as i32) * 32768,
                51 => acc.coarse_tune = acc.coarse_tune.saturating_add(signed),
                52 => acc.fine_tune = acc.fine_tune.saturating_add(signed),
                53 => acc.sample_id = Some(raw as usize),
                54 => acc.sample_modes = Some((raw & 3) as u8),
                58 if signed >= 0 && signed <= 127 => acc.override_root = Some(signed as u8),
                _ => {}
            }
        };

        // Parse instruments into raw instrument zones
        let mut inst_starts = Vec::new();
        for rec in inst.chunks_exact(22) {
            let name = String::from_utf8_lossy(&rec[0..20]).trim_end_matches('\0').to_string();
            let bag_idx = u16::from_le_bytes([rec[20], rec[21]]) as usize;
            inst_starts.push((name, bag_idx));
        }

        let mut inst_zones: Vec<Vec<RawGenAcc>> = Vec::new();
        for i in 0..inst_starts.len().saturating_sub(1) {
            if inst_starts[i].0 == "EOI" {
                break;
            }
            let b0 = inst_starts[i].1.min(ibags.len().saturating_sub(1));
            let b1 = inst_starts[i + 1].1.min(ibags.len().saturating_sub(1));
            let mut global = RawGenAcc::default();
            let mut list = Vec::new();
            for bi in b0..b1 {
                let g0 = ibags[bi].min(igens.len());
                let g1 = ibags.get(bi + 1).copied().unwrap_or(igens.len()).min(igens.len());
                let mut acc = global.clone();
                let mut has_sample = false;
                for &(op, raw) in &igens[g0..g1] {
                    if op == 53 {
                        has_sample = true;
                    }
                    apply_gen(&mut acc, op, raw, false);
                }
                if bi == b0 && !has_sample {
                    global = acc;
                } else if has_sample {
                    list.push(acc);
                }
            }
            inst_zones.push(list);
        }

        // Parse presets and resolve preset + instrument zones into Sf2Preset
        let mut phdr_recs = Vec::new();
        for rec in phdr.chunks_exact(38) {
            let name = String::from_utf8_lossy(&rec[0..20]).trim_end_matches('\0').trim().to_string();
            let prog = u16::from_le_bytes([rec[20], rec[21]]);
            let bank = u16::from_le_bytes([rec[22], rec[23]]);
            let bag = u16::from_le_bytes([rec[24], rec[25]]) as usize;
            phdr_recs.push((name, prog, bank, bag));
        }

        let mut presets = Vec::new();
        for i in 0..phdr_recs.len().saturating_sub(1) {
            let (ref name, program, bank, b0_raw) = phdr_recs[i];
            if name == "EOP" {
                break;
            }
            let b0 = b0_raw.min(pbags.len().saturating_sub(1));
            let b1 = phdr_recs[i + 1].3.min(pbags.len().saturating_sub(1));
            let mut p_global = RawGenAcc::default();
            let mut p_zones = Vec::new();

            for bi in b0..b1 {
                let g0 = pbags[bi].min(pgens.len());
                let g1 = pbags.get(bi + 1).copied().unwrap_or(pgens.len()).min(pgens.len());
                let mut p_acc = p_global.clone();
                let mut has_inst = false;
                for &(op, raw) in &pgens[g0..g1] {
                    if op == 41 {
                        has_inst = true;
                    }
                    apply_gen(&mut p_acc, op, raw, true);
                }
                if bi == b0 && !has_inst {
                    p_global = p_acc;
                    continue;
                }
                let Some(inst_idx) = p_acc.inst_id else {
                    continue;
                };
                let Some(izones) = inst_zones.get(inst_idx) else {
                    continue;
                };
                for iz in izones {
                    let Some(sid) = iz.sample_id else { continue };
                    let Some(sh) = samples.get(sid) else { continue };

                    let key_lo = p_acc.key_lo.unwrap_or(0).max(iz.key_lo.unwrap_or(0));
                    let key_hi = p_acc.key_hi.unwrap_or(127).min(iz.key_hi.unwrap_or(127));
                    let vel_lo = p_acc.vel_lo.unwrap_or(0).max(iz.vel_lo.unwrap_or(0));
                    let vel_hi = p_acc.vel_hi.unwrap_or(127).min(iz.vel_hi.unwrap_or(127));
                    if key_lo > key_hi || vel_lo > vel_hi {
                        continue;
                    }

                    let start = (sh.start as i32 + iz.start_off).clamp(0, n_smpl as i32) as usize;
                    let end = (sh.end as i32 + iz.end_off).clamp(start as i32, n_smpl as i32) as usize;
                    if end <= start + 2 {
                        continue;
                    }
                    let loop_start = (sh.loop_start as i32 + iz.ls_off)
                        .clamp(start as i32, end as i32) as usize;
                    let loop_end = (sh.loop_end as i32 + iz.le_off)
                        .clamp(loop_start as i32, end as i32) as usize;

                    let fc_cents = iz.fc_cents.unwrap_or(13500).saturating_add(p_acc.fc_cents.unwrap_or(0) - 13500);
                    let fc_hz = (8.176 * 2.0_f32.powf(fc_cents as f32 / 1200.0)).clamp(30.0, 20_000.0);
                    let q_cb = iz.q_cb.unwrap_or(0).saturating_add(p_acc.q_cb.unwrap_or(0)).clamp(0, 400);
                    let q_norm = (q_cb as f32 / 500.0).clamp(0.0, 0.75);
                    let pan = ((iz.pan.unwrap_or(0).saturating_add(p_acc.pan.unwrap_or(0))) as f32 / 500.0)
                        .clamp(-1.0, 1.0);
                    let att_cb = iz.att_cb.unwrap_or(0).saturating_add(p_acc.att_cb.unwrap_or(0)).clamp(0, 1440);
                    let gain = 10.0_f32.powf(-(att_cb as f32) / 200.0).clamp(0.02, 1.0);

                    let sus_cb = iz.sus_cb.unwrap_or(0).saturating_add(p_acc.sus_cb.unwrap_or(0)).clamp(0, 1000);
                    let sustain_lvl = 10.0_f32.powf(-(sus_cb as f32) / 200.0).clamp(0.0, 1.0);
                    let attack_s = tc_to_sec(iz.atk_tc.unwrap_or(-12000).saturating_add(p_acc.atk_tc.unwrap_or(-12000) + 12000));
                    let decay_s = tc_to_sec(iz.dec_tc.unwrap_or(-600).saturating_add(p_acc.dec_tc.unwrap_or(-12000) + 12000));
                    let release_s = tc_to_sec(iz.rel_tc.unwrap_or(-800).saturating_add(p_acc.rel_tc.unwrap_or(-12000) + 12000));

                    p_zones.push(Sf2Zone {
                        key_lo,
                        key_hi,
                        vel_lo,
                        vel_hi,
                        start,
                        end,
                        loop_start,
                        loop_end,
                        loop_mode: iz.sample_modes.unwrap_or(0),
                        sample_rate: sh.sample_rate,
                        root_key: iz.override_root.unwrap_or(sh.orig_pitch),
                        coarse_tune: iz.coarse_tune.saturating_add(p_acc.coarse_tune),
                        fine_tune: iz.fine_tune.saturating_add(p_acc.fine_tune).saturating_add(sh.pitch_corr as i16),
                        pan,
                        gain,
                        fc_hz,
                        q_norm,
                        attack_s,
                        decay_s,
                        sustain_lvl,
                        release_s,
                    });
                }
            }

            if !p_zones.is_empty() {
                presets.push(Sf2Preset {
                    name: name.clone(),
                    bank,
                    program,
                    zones: p_zones,
                });
            }
        }

        if presets.is_empty() {
            return Err("No playable presets found in SF2 file".into());
        }
        presets.sort_by_key(|p| (p.bank, p.program));
        Ok(Self {
            name: info_name,
            sample_data: sample_arc,
            presets,
        })
    }
}

pub fn discover_system_soundfonts() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("soundfonts"), PathBuf::from(".")];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/soundfonts"));
    }
    dirs.push(PathBuf::from("/usr/share/sounds/sf2"));
    dirs.push(PathBuf::from("/usr/share/soundfonts"));
    dirs.push(PathBuf::from("/usr/local/share/soundfonts"));

    let mut out = Vec::new();
    for dir in dirs {
        if let Ok(entries) = fs::read_dir(&dir) {
            for ent in entries.flatten() {
                let p = ent.path();
                if p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("sf2"))
                    .unwrap_or(false)
                {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[derive(Debug, Clone)]
struct Sf2Voice {
    active: bool,
    held: bool,
    note: u8,
    preset_idx: usize,
    zone_idx: usize,
    pos: f64,
    rate: f64,
    fb_phase: f32,
    velocity: f32,
    age: u64,
    amp_env: AdsrEnv,
    filter_l: Svf12Filter,
    filter_r: Svf12Filter,
}

impl Sf2Voice {
    fn new(sr: f32) -> Self {
        Self {
            active: false,
            held: false,
            note: 60,
            preset_idx: 0,
            zone_idx: 0,
            pos: 0.0,
            rate: 1.0,
            fb_phase: 0.0,
            velocity: 0.9,
            age: 0,
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

/// Pure-Rust SoundFont 2.0 Player Synthesizer (with warm built-in fallback when no `.sf2` is loaded).
#[derive(Debug, Clone)]
pub struct SoundFontSynth {
    bank: Option<Arc<SoundFontBank>>,
    voices: [Sf2Voice; MAX_SF2_VOICES],
    age_counter: u64,
    sample_rate: f32,
    phaser: StereoPhaser,
    amp_cab: AmpCabSim,
}

impl SoundFontSynth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            bank: None,
            voices: std::array::from_fn(|_| Sf2Voice::new(sr)),
            age_counter: 0,
            sample_rate: sr,
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

    pub fn set_bank(&mut self, bank: Option<Arc<SoundFontBank>>) -> Option<Arc<SoundFontBank>> {
        self.all_notes_off();
        for v in &mut self.voices {
            v.active = false;
            v.held = false;
            v.amp_env.reset();
        }
        std::mem::replace(&mut self.bank, bank)
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, poly_mode: bool, preset_idx: usize) {
        self.age_counter = self.age_counter.wrapping_add(1);
        let vel = velocity.clamp(0.05, 1.0);
        let vel_u8 = (vel * 127.0).round().clamp(1.0, 127.0) as u8;

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

        let mut p_idx = 0usize;
        let mut z_idx = 0usize;
        let mut pos = 0.0_f64;
        let mut rate = 1.0_f64;
        if let Some(bank) = &self.bank {
            if !bank.presets.is_empty() {
                p_idx = preset_idx % bank.presets.len();
                let preset = &bank.presets[p_idx];
                if let Some((zi, z)) = preset
                    .zones
                    .iter()
                    .enumerate()
                    .find(|(_, z)| {
                        note >= z.key_lo && note <= z.key_hi && vel_u8 >= z.vel_lo && vel_u8 <= z.vel_hi
                    })
                    .or_else(|| {
                        preset
                            .zones
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, z)| (z.root_key as i32 - note as i32).abs())
                    })
                {
                    z_idx = zi;
                    pos = z.start as f64;
                    let semi = note as f64 - z.root_key as f64 + z.coarse_tune as f64
                        + z.fine_tune as f64 * 0.01;
                    rate = 2.0_f64.powf(semi / 12.0) * (z.sample_rate as f64 / self.sample_rate as f64);
                }
            }
        }

        let v = &mut self.voices[idx];
        v.note = note;
        v.preset_idx = p_idx;
        v.zone_idx = z_idx;
        v.pos = pos;
        v.rate = rate.clamp(0.125, 8.0);
        v.fb_phase = 0.0;
        v.velocity = vel;
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
        cutoff: f32,
        resonance: f32,
        drive: f32,
        phaser_mix: f32,
        amp_cab: f32,
    ) -> (f32, f32) {
        let sr = self.sample_rate;
        let user_cut = cutoff.clamp(80.0, 18_000.0);
        let user_res = (resonance * 0.5).clamp(0.0, 0.6);

        let mut mix_l = 0.0_f32;
        let mut mix_r = 0.0_f32;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            if let Some(bank) = &self.bank {
                let Some(preset) = bank.presets.get(v.preset_idx) else {
                    v.active = false;
                    continue;
                };
                let Some(z) = preset.zones.get(v.zone_idx) else {
                    v.active = false;
                    continue;
                };

                let a_env = v.amp_env.next(z.attack_s, z.decay_s, z.sustain_lvl, z.release_s);
                if !v.amp_env.is_active() {
                    v.active = false;
                    continue;
                }

                let smpl = &bank.sample_data;
                let loop_active =
                    (z.loop_mode == 1 || (z.loop_mode == 3 && v.held)) && z.loop_end > z.loop_start + 4;
                if loop_active && v.pos >= z.loop_end as f64 {
                    let loop_len = (z.loop_end - z.loop_start) as f64;
                    v.pos = z.loop_start as f64 + ((v.pos - z.loop_start as f64) % loop_len);
                }
                if v.pos as usize >= z.end.min(smpl.len()).saturating_sub(1) {
                    v.active = false;
                    continue;
                }

                let i0 = (v.pos as usize).min(smpl.len() - 1);
                let i1 = (i0 + 1).min(z.end.min(smpl.len() - 1));
                let frac = (v.pos - i0 as f64) as f32;
                let sample = smpl[i0] + (smpl[i1] - smpl[i0]) * frac;
                v.pos += v.rate;

                let eff_cut = z.fc_hz.min(user_cut.max(2400.0)).clamp(60.0, sr * 0.43);
                let eff_res = z.q_norm.max(user_res * 0.4);
                let filt = v.filter_l.process_lp_analog(sample, eff_cut, eff_res, drive * 0.4);
                let p = (z.pan * 0.5 + 0.5).clamp(0.0, 1.0);
                let amp = filt * a_env * v.velocity * z.gain * 0.95;
                mix_l += amp * (1.0 - p).sqrt() * 1.4142;
                mix_r += amp * p.sqrt() * 1.4142;
            } else {
                // Built-in fallback Suitcase EP when no .sf2 bank is loaded yet
                let a_env = v.amp_env.next(0.003, 0.75, 0.35, 0.32);
                if !v.amp_env.is_active() {
                    v.active = false;
                    continue;
                }
                let f0 = midi_to_hz(v.note as f32).clamp(20.0, sr * 0.40);
                v.fb_phase = (v.fb_phase + f0 / sr).fract();
                let fm = (v.fb_phase * 7.0 * TAU).sin() * 0.45 * a_env;
                let s = (v.fb_phase * TAU + fm).sin() * 0.72
                    + (v.fb_phase * 2.0 * TAU).sin() * 0.22 * a_env;
                let amp = a_env * v.velocity * 0.52;
                mix_l += s * amp;
                mix_r += s * amp;
            }
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
