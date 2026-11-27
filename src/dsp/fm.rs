use std::f32::consts::TAU;

use crate::model::midi_to_hz;

use super::{
    one_pole_coeff, soft_ceiling, soft_tanh, AdsrEnv, AmpCabSim, AnalogDrift, StereoPhaser,
    Svf12Filter,
};

const MAX_FM_VOICES: usize = 8;

#[derive(Debug, Clone)]
struct FmVoice {
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
    phases: [f32; 4],
    mod_envs: [AdsrEnv; 3],
    amp_env: AdsrEnv,
    fb_prev: f32,
    sub_phase: f32,
    filter_l: Svf12Filter,
    filter_r: Svf12Filter,
    dc_l: f32,
    dc_r: f32,
    side_hp: f32,
    thermal: AnalogDrift,
}

impl FmVoice {
    fn new(sr: f32, idx: usize) -> Self {
        let pan_table = [-0.28_f32, 0.28, -0.18, 0.18, -0.34, 0.34, -0.10, 0.10];
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
            phases: [0.0; 4],
            mod_envs: [AdsrEnv::new(sr), AdsrEnv::new(sr), AdsrEnv::new(sr)],
            amp_env: AdsrEnv::new(sr),
            fb_prev: 0.0,
            sub_phase: 0.0,
            filter_l: Svf12Filter::new(sr),
            filter_r: Svf12Filter::new(sr),
            dc_l: 0.0,
            dc_r: 0.0,
            side_hp: 0.0,
            thermal: AnalogDrift::new(0x464D_3737_u32.wrapping_add(idx as u32 * 0x1993)),
        }
    }

    fn set_sample_rate(&mut self, sr: f32) {
        for e in &mut self.mod_envs {
            e.set_sample_rate(sr);
        }
        self.amp_env.set_sample_rate(sr);
        self.filter_l.set_sample_rate(sr);
        self.filter_r.set_sample_rate(sr);
    }
}

/// 4-Operator Polyphonic FM Synthesizer inspired by Yamaha DX7, TX81Z & Elektron Digitone.
#[derive(Debug, Clone)]
pub struct FmSynth {
    voices: [FmVoice; MAX_FM_VOICES],
    age_counter: u64,
    sample_rate: f32,
    trem_phase: f32,
    last_mono_midi: Option<f32>,
    phaser: StereoPhaser,
    amp_cab: AmpCabSim,
}

impl FmSynth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            voices: std::array::from_fn(|i| FmVoice::new(sr, i)),
            age_counter: 0,
            sample_rate: sr,
            trem_phase: 0.12,
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
        let acc = if accent { 0.32 } else { 0.0 };

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
            v.fb_prev = 0.0;
            // Reset phases on non-legato trigger for consistent percussive DX7 attack punch
            v.phases = [0.0; 4];
            v.sub_phase = 0.0;
            for e in &mut v.mod_envs {
                e.trigger();
            }
            v.amp_env.trigger();
        }
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.held && v.note == note {
                v.held = false;
                v.amp_env.release();
                for e in &mut v.mod_envs {
                    e.release();
                }
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            if v.active {
                v.held = false;
                v.amp_env.release();
                for e in &mut v.mod_envs {
                    e.release();
                }
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
        amp_attack: f32,
        amp_decay: f32,
        amp_sustain: f32,
        amp_release: f32,
        glide: f32,
        poly_mode: bool,
        phaser_mix: f32,
        amp_cab: f32,
    ) -> (f32, f32) {
        let sr = self.sample_rate;
        let bank = bank % 6;
        let morph = morph.clamp(0.0, 1.0);
        let drive = drive.clamp(0.0, 1.0);
        let cutoff = cutoff.clamp(40.0, 18_000.0);
        let res = (resonance * 0.65).clamp(0.0, 0.72);

        self.trem_phase = (self.trem_phase + 4.15 / sr).fract();
        let trem_l = 0.88 + 0.12 * (self.trem_phase * TAU).sin();
        let trem_r = 0.88 - 0.12 * (self.trem_phase * TAU).sin();

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

            let a_atk = match bank {
                5 => amp_attack.max(0.09),
                4 => amp_attack.max(0.014),
                1 | 2 => amp_attack.clamp(0.002, 0.025),
                _ => amp_attack.max(0.002),
            };
            let a_dec = match bank {
                0 => amp_decay.max(0.55),
                1 => amp_decay.max(0.95),
                2 => amp_decay.clamp(0.18, 0.65),
                3 => amp_decay.max(0.22),
                _ => amp_decay.max(0.25),
            };
            let a_sus = match bank {
                1 | 2 => (amp_sustain * 0.35).min(0.28),
                0 => amp_sustain.clamp(0.28, 0.68),
                _ => amp_sustain,
            };
            let a_rel = match bank {
                0 => amp_release.max(0.26),
                1 => amp_release.max(0.55),
                5 => amp_release.max(0.70),
                _ => amp_release.max(0.12),
            };
            let a_env = v.amp_env.next(a_atk, a_dec, a_sus, a_rel);
            if !v.amp_env.is_active() {
                v.active = false;
                continue;
            }

            // 3 independent operator modulator envelopes
            let e_tine = v.mod_envs[0].next(0.001, 0.09 + morph * 0.14, 0.04, 0.14);
            let e_body = v.mod_envs[1].next(
                if bank == 4 { 0.032 } else if bank == 5 { 0.18 } else { 0.002 },
                0.35 + morph * 0.55,
                if bank == 4 || bank == 5 { 0.48 } else { 0.14 },
                0.28,
            );
            let e_spark = v.mod_envs[2].next(0.001, 0.035 + morph * 0.08, 0.02, 0.08);

            let therm = v.thermal.next(sr);
            let f0 = midi_to_hz(v.current_midi + therm * 0.018).clamp(18.0, sr * 0.40);
            let vel_bright = (0.35 + v.velocity * 0.78 + v.accent * 0.35) * (0.55 + morph * 0.85);

            // Band-limit high-ratio modulators cleanly so high notes remain glassy without aliasing
            let ratio_atten = |ratio: f32| -> f32 {
                let nyq_ratio = (f0 * ratio) / (sr * 0.40);
                if nyq_ratio >= 1.0 {
                    0.0
                } else if nyq_ratio > 0.72 {
                    (1.0 - nyq_ratio) / 0.28
                } else {
                    1.0
                }
            };

            let (r0, r1, r2, r3): (f32, f32, f32, f32) = match bank {
                // 0: DX7 Suitcase Rhodes EP ([Op1(7)->Op0(1)] warm tine + [Op3(1,fb)->Op2(1.003)] mellow body)
                0 => (1.0, 7.0, 1.003, 1.0),
                // 1: Warm Celesta / Atmos Bell ([Op1(2.0)->Op0(1)] + [Op3(3.0)->Op2(1.002)])
                1 => (1.0, 2.0, 1.002, 3.0),
                // 2: Woody Marimba / Funk Clav ([Op1(4.0)->Op0(1)] + [Op3(2.76)->Op2(1.0)])
                2 => (1.0, 4.0, 1.0, 2.76),
                // 3: FM Slap / Lately Bass ([Op3(1,fb)->Op2(2)->Op1(1)->Op0(1)])
                3 => (1.0, 1.0, 2.0, 1.0),
                // 4: Digitone Brass / Organ ([Op1(1)->Op0(1)] + [Op3(2,fb)->Op2(1.005)])
                4 => (1.0, 1.0, 1.005, 2.0),
                // 5: Crystal Evolving Pad ([Op1(2.002)->Op0(1)] + [Op3(3,fb)->Op2(0.998)])
                _ => (1.0, 2.002, 0.998, 3.0),
            };

            let ratios = [r0, r1, r2, r3];
            for i in 0..4 {
                let dt = (f0 * ratios[i] / sr).min(0.44);
                v.phases[i] = (v.phases[i] + dt).fract();
            }

            let p = v.phases;
            let (sig_l, sig_r) = match bank {
                0 => {
                    // Warm Suitcase Rhodes EP: gentle tine sparkle (tamed on upper keys) + singing tube bark body
                    let hi_tame = (1.0 - ((v.current_midi - 62.0) / 24.0).clamp(0.0, 0.65)).max(0.25);
                    let op1 = (p[1] * TAU).sin() * e_tine * (0.32 + 0.42 * morph) * hi_tame * ratio_atten(r1);
                    let car_tine = (p[0] * TAU + op1 * vel_bright * 0.42).sin();

                    let op3_in = p[3] * TAU + v.fb_prev * 0.24;
                    let op3 = op3_in.sin();
                    v.fb_prev = 0.5 * (v.fb_prev + op3);
                    let idx_body = (0.78 + morph * 0.92) * e_body * vel_bright * ratio_atten(r3);
                    let car_body = (p[2] * TAU + op3 * idx_body).sin();
                    let warm_body = car_body + 0.12 * car_body * car_body;

                    let s = warm_body * 0.84 + car_tine * (0.14 + 0.12 * e_tine);
                    (s * trem_l, s * trem_r)
                }
                1 => {
                    // Warm Atmos Celesta / Bell: pure grounded fundamental + soft stereo harmonic shimmer
                    let op1 = (p[1] * TAU).sin() * (0.82 * e_tine * vel_bright) * ratio_atten(r1);
                    let c1 = (p[0] * TAU + op1).sin();
                    let op3 = (p[3] * TAU).sin() * (0.62 * e_spark * vel_bright) * ratio_atten(r3);
                    let c2 = (p[2] * TAU + op3).sin() * (0.48 + 0.20 * e_tine);
                    let fund = (p[0] * TAU).sin() * 0.34;
                    (c1 * 0.56 + c2 * 0.30 + fund, c1 * 0.38 + c2 * 0.48 + fund)
                }
                2 => {
                    // Woody Marimba / Funk Clav
                    let op1 = (p[1] * TAU).sin() * (1.9 * e_spark * vel_bright) * ratio_atten(r1);
                    let c1 = (p[0] * TAU + op1).sin() * 0.55;
                    let op3_raw = (p[3] * TAU + v.fb_prev * 0.22).sin();
                    v.fb_prev = op3_raw;
                    let op3 = op3_raw * (1.45 * e_body * vel_bright) * ratio_atten(r3);
                    let c2 = (p[2] * TAU + op3).sin() * 0.65;
                    let s = c1 + c2;
                    (s * 0.88 + (c1 - c2) * 0.14, s * 0.88 - (c1 - c2) * 0.14)
                }
                3 => {
                    // FM Slap / Lately Bass: 4-op cascade + clean center sub
                    let op3_raw = (p[3] * TAU + v.fb_prev * (0.45 + morph * 0.35)).sin();
                    v.fb_prev = 0.5 * (v.fb_prev + op3_raw);
                    let op2 = (p[2] * TAU + op3_raw * (1.6 * e_body * vel_bright) * ratio_atten(r3)).sin();
                    let op1 = (p[1] * TAU + op2 * (1.75 * e_tine * vel_bright) * ratio_atten(r2)).sin();
                    let car = (p[0] * TAU + op1 * (1.45 * e_tine * vel_bright) * ratio_atten(r1)).sin();
                    v.sub_phase = (v.sub_phase + (f0 * 0.5 / sr).min(0.42)).fract();
                    let sub = (v.sub_phase * TAU).sin() * 0.42;
                    let s = car * 0.72 + sub;
                    (s, s)
                }
                4 => {
                    // Digitone Brass / Organ
                    let op1 = (p[1] * TAU + v.fb_prev * 0.25).sin() * (1.8 * e_body * vel_bright) * ratio_atten(r1);
                    let c1 = (p[0] * TAU + op1).sin();
                    let op3_raw = (p[3] * TAU + v.fb_prev * 0.42).sin();
                    v.fb_prev = 0.5 * (v.fb_prev + op3_raw);
                    let c2 = (p[2] * TAU + op3_raw * (1.45 * e_body * vel_bright) * ratio_atten(r3)).sin();
                    (c1 * 0.62 + c2 * 0.48, c1 * 0.48 + c2 * 0.62)
                }
                _ => {
                    // Crystal Evolving Pad
                    let swell = (0.35 + 0.65 * (v.note_age_s * 0.9).min(1.0)) * vel_bright;
                    let op1 = (p[1] * TAU).sin() * (1.35 * swell * e_body) * ratio_atten(r1);
                    let c1 = (p[0] * TAU + op1).sin();
                    let op3_raw = (p[3] * TAU + v.fb_prev * 0.28).sin();
                    v.fb_prev = 0.5 * (v.fb_prev + op3_raw);
                    let c2 = (p[2] * TAU + op3_raw * (1.15 * swell) * ratio_atten(r3)).sin();
                    (c1 * 0.64 + c2 * 0.46, c1 * 0.46 + c2 * 0.64)
                }
            };

            let filt_oct = env_amount * e_tine * 1.4 + (v.velocity - 0.55) * 0.55;
            // Respect warm/dark cutoff presets while letting bright bells shimmer cleanly
            let eff_cut = (cutoff.max(380.0) * 2.0_f32.powf(filt_oct)).clamp(180.0, sr * 0.43);
            let fl = v.filter_l.process_lp_analog(sig_l, eff_cut, res, drive);
            let fr = v.filter_r.process_lp_analog(sig_r, eff_cut, res, drive);

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
                let p_norm = (v.pan * 0.45 + 0.5).clamp(0.0, 1.0);
                let pan_l = (1.0 - p_norm).sqrt() * 1.4142;
                let pan_r = p_norm.sqrt() * 1.4142;
                poly_env_sum += a_env;
                mix_l += cl * amp * pan_l;
                mix_r += cr * amp * pan_r;
            }
        }

        if poly_mode && poly_env_sum > 1.0 {
            let norm = 1.0 / poly_env_sum.powf(0.28);
            mix_l *= norm;
            mix_r *= norm;
        }

        let (mut out_l, mut out_r) = (
            soft_tanh(mix_l * (1.0 + drive * 0.35)),
            soft_tanh(mix_r * (1.0 + drive * 0.35)),
        );
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
