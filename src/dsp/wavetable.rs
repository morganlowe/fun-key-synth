use std::f32::consts::TAU;
use std::sync::OnceLock;

use crate::model::midi_to_hz;

use super::{
    one_pole_coeff, soft_ceiling, soft_tanh, AdsrEnv, AmpCabSim, AnalogDrift, MoogLadderFilter,
    StereoPhaser,
};

const MAX_WT_VOICES: usize = 8;
const WT_BANKS: usize = 6;
const WT_FRAMES: usize = 16;
const WT_MIPS: usize = 7;
const WT_SIZE: usize = 512;
const MIP_MAX_H: [usize; WT_MIPS] = [48, 32, 20, 12, 7, 4, 2];

struct WavetableBankSet {
    // Flattened [bank * WT_MIPS + mip][frame][sample]
    tables: Vec<[[f32; WT_SIZE]; WT_FRAMES]>,
}

fn lanczos_sigma(k: usize, max_h: usize) -> f32 {
    if k == 0 || max_h <= 1 {
        return 1.0;
    }
    let x = std::f32::consts::PI * k as f32 / (max_h as f32 + 1.0);
    if x.abs() < 1e-5 {
        1.0
    } else {
        x.sin() / x
    }
}

fn wavetable_banks() -> &'static WavetableBankSet {
    static BANKS: OnceLock<WavetableBankSet> = OnceLock::new();
    BANKS.get_or_init(|| {
        let mut tables = vec![[[0.0_f32; WT_SIZE]; WT_FRAMES]; WT_BANKS * WT_MIPS];
        for b in 0..WT_BANKS {
            for mip in 0..WT_MIPS {
                let max_h = MIP_MAX_H[mip];
                for f in 0..WT_FRAMES {
                    let m = f as f32 / (WT_FRAMES - 1) as f32;
                    let mut frame = [0.0_f32; WT_SIZE];
                    for k in 1..=max_h {
                        let kf = k as f32;
                        let amp = match b {
                            // 0: Virus 7x HyperSaw (warm saw -> fat resonant saw -> octave supersaw)
                            0 => {
                                let saw = 1.0 / kf;
                                let oct = if k % 2 == 0 { 0.55 * m / (kf * 0.5) } else { 0.0 };
                                let res_peak = (-(kf - (4.0 + m * 10.0)).powi(2) / 9.0).exp() * 0.45 * m;
                                saw * (1.0 + res_peak) + oct
                            }
                            // 1: PPG Digital Glass (sine -> odd metallic glass -> sweeping PPG wave)
                            1 => {
                                let odd = if k % 2 == 1 { 1.0 / kf.powf(0.85) } else { 0.32 * m / kf };
                                let glass = (-(kf - (3.0 + m * 14.0)).powi(2) / 7.5).exp() * 0.85 * m;
                                if k == 1 {
                                    1.0 - m * 0.22
                                } else {
                                    (odd * m + glass) * 0.72
                                }
                            }
                            // 2: Vocal Formant Choir (Ooh -> Ahh -> Eeh -> Airy Choir)
                            2 => {
                                let (f1, f2, f3) = if m < 0.5 {
                                    let t = m * 2.0;
                                    (
                                        3.2 * (1.0 - t) + 7.2 * t,
                                        8.4 * (1.0 - t) + 11.2 * t,
                                        18.0 * (1.0 - t) + 22.0 * t,
                                    )
                                } else {
                                    let t = (m - 0.5) * 2.0;
                                    (
                                        7.2 * (1.0 - t) + 3.0 * t,
                                        11.2 * (1.0 - t) + 20.5 * t,
                                        22.0 * (1.0 - t) + 26.0 * t,
                                    )
                                };
                                let g1 = (-(kf - f1).powi(2) / 3.8).exp() * 1.15;
                                let g2 = (-(kf - f2).powi(2) / 6.5).exp() * 0.78;
                                let g3 = (-(kf - f3).powi(2) / 10.0).exp() * 0.35;
                                (1.0 / kf.powf(0.7)) * (0.28 + g1 + g2 + g3)
                            }
                            // 3: Waldorf Metallic Bell / Pluck
                            3 => {
                                let ring = (kf * (1.1 + m * 1.8)).cos().abs();
                                let shimmer = (-(kf - (5.0 + m * 15.0)).powi(2) / 11.0).exp();
                                if k == 1 {
                                    0.92
                                } else {
                                    (1.0 / kf.powf(0.95)) * (0.25 + 0.75 * m) * (0.45 + 0.55 * ring + 0.65 * shimmer)
                                }
                            }
                            // 4: Juno / Solina PWM Strings (animated pulse-width harmonic comb + sub-saw weight)
                            4 => {
                                let duty = 0.50 - 0.34 * m;
                                let pulse = (std::f32::consts::PI * kf * duty).sin().abs() / kf;
                                let saw = 0.52 / kf;
                                let oct = if k % 2 == 0 { 0.25 * (1.0 - m * 0.4) / kf } else { 0.0 };
                                pulse * 0.85 + saw + oct
                            }
                            // 5: Acid / Bass Morph (thick sub+saw -> hollow square-formant -> gnarly Reese)
                            _ => {
                                let sq = if k % 2 == 1 { 1.0 / kf } else { (1.0 - m) * 0.85 / kf };
                                let f_peak = (-(kf - (2.0 + m * 9.0)).powi(2) / 5.5).exp() * (0.25 + 0.85 * m);
                                if k == 1 {
                                    1.15
                                } else {
                                    (sq + f_peak * 0.45) * 0.82
                                }
                            }
                        };
                        let w = amp * lanczos_sigma(k, max_h);
                        let phase_off = if b == 1 || b == 3 { (k as f32 * 0.61).sin() * 0.35 } else { 0.0 };
                        for s in 0..WT_SIZE {
                            let p = s as f32 / WT_SIZE as f32;
                            frame[s] += w * (p * kf * TAU + phase_off).sin();
                        }
                    }
                    let rms = (frame.iter().map(|x| x * x).sum::<f32>() / WT_SIZE as f32)
                        .sqrt()
                        .max(1e-5);
                    let norm = 0.55 / rms;
                    for s in &mut frame {
                        *s *= norm;
                    }
                    tables[b * WT_MIPS + mip][f] = frame;
                }
            }
        }
        WavetableBankSet { tables }
    })
}

#[inline]
fn read_frame_cubic(frame: &[f32; WT_SIZE], phase: f32) -> f32 {
    let p = phase.fract() * WT_SIZE as f32;
    let i0 = (p as usize) & (WT_SIZE - 1);
    let frac = p - i0 as f32;
    let im1 = (i0 + WT_SIZE - 1) & (WT_SIZE - 1);
    let i1 = (i0 + 1) & (WT_SIZE - 1);
    let i2 = (i0 + 2) & (WT_SIZE - 1);

    let ym1 = frame[im1];
    let y0 = frame[i0];
    let y1 = frame[i1];
    let y2 = frame[i2];

    let c0 = y0;
    let c1 = 0.5 * (y1 - ym1);
    let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
    let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
    ((c3 * frac + c2) * frac + c1) * frac + c0
}

#[inline]
fn mip_position_for_dt(dt: f32) -> f32 {
    const H_FLOAT: [f32; WT_MIPS] = [48.0, 32.0, 20.0, 12.0, 7.0, 4.0, 2.0];
    let max_h_nyq = 0.44 / dt.clamp(1e-5, 0.48);
    if max_h_nyq >= H_FLOAT[0] {
        return 0.0;
    }
    if max_h_nyq <= H_FLOAT[WT_MIPS - 1] {
        return (WT_MIPS - 1) as f32;
    }
    for m in 0..(WT_MIPS - 1) {
        let hi = H_FLOAT[m];
        let lo = H_FLOAT[m + 1];
        if max_h_nyq >= lo {
            let t = ((hi - max_h_nyq) / (hi - lo)).clamp(0.0, 1.0);
            return m as f32 + t;
        }
    }
    (WT_MIPS - 1) as f32
}

#[inline]
fn sample_wavetable(bank: usize, frame_pos: f32, phase: f32, dt: f32) -> f32 {
    let set = wavetable_banks();
    let b = bank.min(WT_BANKS - 1);
    let f = frame_pos.clamp(0.0, (WT_FRAMES - 1) as f32);
    let f0 = (f as usize).min(WT_FRAMES - 2);
    let frac = f - f0 as f32;

    let mip_f = mip_position_for_dt(dt);
    let m0 = (mip_f as usize).min(WT_MIPS - 1);
    let m1 = (m0 + 1).min(WT_MIPS - 1);
    let m_frac = mip_f - m0 as f32;

    let bank_m0 = &set.tables[b * WT_MIPS + m0];
    let s0 = read_frame_cubic(&bank_m0[f0], phase);
    let s1 = read_frame_cubic(&bank_m0[f0 + 1], phase);
    let out0 = s0 + (s1 - s0) * frac;
    if m0 == m1 || m_frac < 1e-4 {
        return out0;
    }
    let bank_m1 = &set.tables[b * WT_MIPS + m1];
    let t0 = read_frame_cubic(&bank_m1[f0], phase);
    let t1 = read_frame_cubic(&bank_m1[f0 + 1], phase);
    let out1 = t0 + (t1 - t0) * frac;
    out0 + (out1 - out0) * m_frac
}

#[derive(Debug, Clone)]
struct WtVoice {
    active: bool,
    held: bool,
    note: u8,
    current_midi: f32,
    target_midi: f32,
    velocity: f32,
    accent: f32,
    age: u64,
    note_age_s: f32,
    pan: f32,
    phases: [f32; 7],
    sub_phase: f32,
    filt_env: AdsrEnv,
    amp_env: AdsrEnv,
    filter_l: MoogLadderFilter,
    filter_r: MoogLadderFilter,
    dc_l: f32,
    dc_r: f32,
    side_hp: f32,
    thermal: AnalogDrift,
}

impl WtVoice {
    fn new(sr: f32, idx: usize) -> Self {
        let pan_table = [-0.25_f32, 0.25, -0.15, 0.15, -0.32, 0.32, -0.08, 0.08];
        let mut phases = [0.0_f32; 7];
        for (k, p) in phases.iter_mut().enumerate() {
            *p = ((idx as f32 * 0.21 + k as f32 * 0.143) % 1.0).abs();
        }
        Self {
            active: false,
            held: false,
            note: 60,
            current_midi: 60.0,
            target_midi: 60.0,
            velocity: 0.9,
            accent: 0.0,
            age: 0,
            note_age_s: 0.0,
            pan: pan_table[idx % pan_table.len()],
            phases,
            sub_phase: 0.0,
            filt_env: AdsrEnv::new(sr),
            amp_env: AdsrEnv::new(sr),
            filter_l: MoogLadderFilter::new(sr),
            filter_r: MoogLadderFilter::new(sr),
            dc_l: 0.0,
            dc_r: 0.0,
            side_hp: 0.0,
            thermal: AnalogDrift::new(0x5741_5645_u32.wrapping_add(idx as u32 * 0x2468)),
        }
    }

    fn set_sample_rate(&mut self, sr: f32) {
        self.filt_env.set_sample_rate(sr);
        self.amp_env.set_sample_rate(sr);
        self.filter_l.set_sample_rate(sr);
        self.filter_r.set_sample_rate(sr);
    }
}

/// Morphing 2D Wavetable & 7-Voice Stereo HyperSaw Synthesizer (PPG Wave / Waldorf / Access Virus TI2).
#[derive(Debug, Clone)]
pub struct WavetableSynth {
    voices: [WtVoice; MAX_WT_VOICES],
    age_counter: u64,
    sample_rate: f32,
    lfo_phase: f32,
    last_mono_midi: Option<f32>,
    phaser: StereoPhaser,
    amp_cab: AmpCabSim,
}

impl WavetableSynth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        // Ensure global wavetable bank is initialized at construction (away from audio callback)
        let _ = wavetable_banks();
        Self {
            voices: std::array::from_fn(|i| WtVoice::new(sr, i)),
            age_counter: 0,
            sample_rate: sr,
            lfo_phase: 0.21,
            last_mono_midi: None,
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

    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        accent: bool,
        slide: bool,
        poly_mode: bool,
        glide: f32,
    ) {
        self.age_counter = self.age_counter.wrapping_add(1);
        let vel = velocity.clamp(0.05, 1.0);
        let acc = if accent { 0.30 } else { 0.0 };

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

        let v = &mut self.voices[idx];
        let was_active = v.active && v.amp_env.is_active();
        v.note = note;
        v.target_midi = note as f32;
        if !poly_mode {
            if let Some(prev) = self.last_mono_midi {
                if glide > 0.005 || (slide && was_active) {
                    v.current_midi = prev;
                } else {
                    v.current_midi = note as f32;
                }
            } else {
                v.current_midi = note as f32;
            }
            self.last_mono_midi = Some(note as f32);
        } else {
            v.current_midi = note as f32;
        }
        v.velocity = vel;
        v.accent = acc;
        v.age = self.age_counter;
        v.held = true;
        v.active = true;

        if !(slide && was_active && !poly_mode) {
            v.note_age_s = 0.0;
            v.filt_env.trigger();
            v.amp_env.trigger();
        }
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.held && v.note == note {
                v.held = false;
                v.filt_env.release();
                v.amp_env.release();
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            if v.active {
                v.held = false;
                v.filt_env.release();
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
        bank: u8,
        morph: f32,
        cutoff: f32,
        resonance: f32,
        drive: f32,
        env_amount: f32,
        filt_attack: f32,
        filt_decay: f32,
        filt_sustain: f32,
        filt_release: f32,
        amp_attack: f32,
        amp_decay: f32,
        amp_sustain: f32,
        amp_release: f32,
        glide: f32,
        poly_mode: bool,
        phaser_mix: f32,
        amp_cab: f32,
    ) -> (f32, f32) {
        const UNISON_SPREAD: [f32; 7] = [-1.0, -0.62, -0.25, 0.0, 0.25, 0.62, 1.0];
        const UNISON_GAIN: [f32; 7] = [0.62, 0.78, 0.90, 1.0, 0.90, 0.78, 0.62];

        let sr = self.sample_rate;
        let bank = (bank as usize) % WT_BANKS;
        let morph = morph.clamp(0.0, 1.0);
        let drive = drive.clamp(0.0, 1.0);

        self.lfo_phase = (self.lfo_phase + 0.62 / sr).fract();
        let lfo = (self.lfo_phase * TAU).sin();
        let dc_alpha = one_pole_coeff(20.0, sr);
        let side_hp_alpha = one_pole_coeff(150.0, sr);

        let mut mix_l = 0.0_f32;
        let mut mix_r = 0.0_f32;
        let mut poly_env_sum = 0.0_f32;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            if !poly_mode && glide > 0.005 {
                let c = 1.0 - (-1.0 / (glide * 0.25 * sr)).exp();
                v.current_midi += c * (v.target_midi - v.current_midi);
            } else {
                v.current_midi = v.target_midi;
            }
            v.note_age_s += 1.0 / sr;

            let f_env = v.filt_env.next(
                filt_attack.max(0.002),
                filt_decay.max(0.04),
                filt_sustain,
                filt_release.max(0.04),
            );
            let a_env = v.amp_env.next(
                amp_attack.max(0.002),
                amp_decay.max(0.05),
                amp_sustain,
                amp_release.max(0.06),
            );
            if !v.amp_env.is_active() {
                v.active = false;
                continue;
            }

            let therm = v.thermal.next(sr);
            let vib = if matches!(bank, 2 | 4) {
                ((v.note_age_s - 0.14) / 0.35).clamp(0.0, 1.0)
                    * ((v.note_age_s * 5.2 * TAU).sin())
                    * 0.06
            } else {
                0.0
            };
            let base_midi = v.current_midi + therm * 0.02 + vib;
            let f0 = midi_to_hz(base_midi).clamp(16.0, sr * 0.42);
            v.sub_phase = (v.sub_phase + (f0 * 0.5 / sr).min(0.42)).fract();
            let sub = (v.sub_phase * TAU).sin() * if poly_mode { 0.16 } else { 0.36 };

            let frame_pos =
                (morph * 0.62 + f_env * 0.32 + lfo * 0.12).clamp(0.0, 1.0) * (WT_FRAMES - 1) as f32;

            let (mut in_l, mut in_r) = (0.0_f32, 0.0_f32);
            if bank == 0 {
                // Virus TI2 7-Voice Stereo HyperSaw
                let max_detune_cents = 10.0 + morph * 26.0;
                for u in 0..7 {
                    let s = UNISON_SPREAD[u];
                    let cents = s * max_detune_cents + therm * (s * 1.4);
                    let fu = midi_to_hz(base_midi + cents / 100.0).clamp(16.0, sr * 0.43);
                    let dt = fu / sr;
                    v.phases[u] = (v.phases[u] + dt).fract();
                    let samp = sample_wavetable(bank, frame_pos, v.phases[u], dt) * UNISON_GAIN[u];
                    let w_l = 0.5 - s * 0.42;
                    let w_r = 0.5 + s * 0.42;
                    in_l += samp * w_l;
                    in_r += samp * w_r;
                }
                in_l = in_l * 0.36 + sub;
                in_r = in_r * 0.36 + sub;
            } else if matches!(bank, 2 | 4) {
                // 5-Voice Stereo Ensemble (Vocal Choir / Juno PWM Strings)
                let max_detune_cents = 7.0 + morph * 15.0;
                for u in 1..6 {
                    let s = UNISON_SPREAD[u];
                    let cents = s * max_detune_cents + therm * (s * 1.2);
                    let fu = midi_to_hz(base_midi + cents / 100.0).clamp(16.0, sr * 0.43);
                    let dt = fu / sr;
                    v.phases[u] = (v.phases[u] + dt).fract();
                    let samp = sample_wavetable(bank, frame_pos, v.phases[u], dt) * UNISON_GAIN[u];
                    let w_l = 0.5 - s * 0.45;
                    let w_r = 0.5 + s * 0.45;
                    in_l += samp * w_l;
                    in_r += samp * w_r;
                }
                in_l = in_l * 0.44 + sub * 0.85;
                in_r = in_r * 0.44 + sub * 0.85;
            } else {
                // 3-Voice Rich Stereo Wavetable (PPG Digital Glass / Waldorf Bell / Acid Morph)
                let detune_cents = if !poly_mode && bank == 5 {
                    8.0 + morph * 16.0
                } else {
                    6.0 + morph * 11.0
                };
                let spreads = [-1.0_f32, 0.0, 1.0];
                for (i, &s) in spreads.iter().enumerate() {
                    let fu = midi_to_hz(base_midi + s * detune_cents / 100.0).clamp(16.0, sr * 0.43);
                    let dt = fu / sr;
                    v.phases[i] = (v.phases[i] + dt).fract();
                    let samp = sample_wavetable(bank, frame_pos, v.phases[i], dt);
                    if s < 0.0 {
                        in_l += samp * 0.46;
                        in_r += samp * 0.24;
                    } else if s > 0.0 {
                        in_l += samp * 0.24;
                        in_r += samp * 0.46;
                    } else {
                        in_l += samp * 0.45;
                        in_r += samp * 0.45;
                    }
                }
                in_l += sub;
                in_r += sub;
            }

            let key_oct = ((v.current_midi - 54.0) / 12.0) * 0.45;
            let env_oct = env_amount * f_env * 3.2;
            let vel_oct = (v.velocity - 0.6) * 1.1;
            let eff_cut = (cutoff * 2.0_f32.powf(key_oct + env_oct + vel_oct)).clamp(45.0, sr * 0.43);

            let pre = 0.85 + drive * 1.35;
            let sl = 1.8 * soft_tanh(in_l * pre / 1.8);
            let sr_in = 1.8 * soft_tanh(in_r * pre / 1.8);
            let ob_blend = if bank == 0 || bank == 4 { 0.35 } else { 0.15 };
            let fl = v.filter_l.process_mode(sl, eff_cut, resonance, ob_blend);
            let fr = v.filter_r.process_mode(sr_in, eff_cut, resonance, ob_blend);

            v.dc_l += dc_alpha * (fl - v.dc_l);
            v.dc_r += dc_alpha * (fr - v.dc_r);
            let cl = fl - v.dc_l;
            let cr = fr - v.dc_r;

            let amp = a_env * (v.velocity + v.accent * 0.22) * if poly_mode { 0.56 } else { 0.72 };
            if !poly_mode {
                let mid = (cl + cr) * 0.5;
                let side = (cl - cr) * 0.5;
                v.side_hp += side_hp_alpha * (side - v.side_hp);
                let sc = side - v.side_hp;
                mix_l += (mid + sc) * amp;
                mix_r += (mid - sc) * amp;
            } else {
                let p = (v.pan * 0.4 + 0.5).clamp(0.0, 1.0);
                let pl = (1.0 - p).sqrt() * 1.4142;
                let pr = p.sqrt() * 1.4142;
                poly_env_sum += a_env;
                mix_l += cl * amp * pl;
                mix_r += cr * amp * pr;
            }
        }

        if poly_mode && poly_env_sum > 1.0 {
            let norm = 1.0 / poly_env_sum.powf(0.20);
            mix_l *= norm;
            mix_r *= norm;
        }

        let (mut out_l, mut out_r) = (mix_l, mix_r);
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
