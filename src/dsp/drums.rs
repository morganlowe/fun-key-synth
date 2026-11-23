use std::f32::consts::TAU;

use crate::model::{DrumKitParams, DrumModel, DrumVoiceId, DrumVoiceParams};

use super::{soft_tanh, Svf12Filter, Xorshift32};

const METAL_FREQS_808: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
const METAL_FREQS_909: [f32; 6] = [245.0, 342.0, 468.0, 610.0, 745.0, 980.0];

#[derive(Debug, Clone)]
pub struct DrumMachine {
    sample_rate: f32,
    rng: Xorshift32,

    // Kick
    kick_active: bool,
    kick_phase: f32,
    kick_amp: f32,
    kick_pitch_env: f32,
    kick_click_env: f32,
    kick_vel: f32,

    // Snare
    snare_active: bool,
    snare_p1: f32,
    snare_p2: f32,
    snare_body_env: f32,
    snare_noise_env: f32,
    snare_vel: f32,
    snare_filter: Svf12Filter,

    // Clap
    clap_active: bool,
    clap_samples: u32,
    clap_tail_env: f32,
    clap_vel: f32,
    clap_filter: Svf12Filter,

    // Cowbell / Rim
    cb_active: bool,
    cb_p1: f32,
    cb_p2: f32,
    cb_env: f32,
    cb_fast_env: f32,
    cb_vel: f32,
    cb_filter: Svf12Filter,

    // Metallic bank shared phase for CH / OH / Cymbal
    metal_phases: [f32; 6],

    // Closed Hat
    ch_active: bool,
    ch_env: f32,
    ch_vel: f32,
    ch_bp: Svf12Filter,
    ch_hp: Svf12Filter,

    // Open Hat
    oh_active: bool,
    oh_env: f32,
    oh_vel: f32,
    oh_bp: Svf12Filter,
    oh_hp: Svf12Filter,

    // Tom
    tom_active: bool,
    tom_phase: f32,
    tom_amp: f32,
    tom_pitch_env: f32,
    tom_vel: f32,

    // Cymbal
    cym_active: bool,
    cym_env: f32,
    cym_vel: f32,
    cym_bp: Svf12Filter,
    cym_hp: Svf12Filter,

    // Flam queue (one pending per voice)
    flam_countdown: [u32; 8],
    flam_vel: [f32; 8],
}

impl DrumMachine {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            sample_rate: sr,
            rng: Xorshift32::new(0x808_909),
            kick_active: false,
            kick_phase: 0.0,
            kick_amp: 0.0,
            kick_pitch_env: 0.0,
            kick_click_env: 0.0,
            kick_vel: 1.0,
            snare_active: false,
            snare_p1: 0.0,
            snare_p2: 0.0,
            snare_body_env: 0.0,
            snare_noise_env: 0.0,
            snare_vel: 1.0,
            snare_filter: Svf12Filter::new(sr),
            clap_active: false,
            clap_samples: 0,
            clap_tail_env: 0.0,
            clap_vel: 1.0,
            clap_filter: Svf12Filter::new(sr),
            cb_active: false,
            cb_p1: 0.0,
            cb_p2: 0.0,
            cb_env: 0.0,
            cb_fast_env: 0.0,
            cb_vel: 1.0,
            cb_filter: Svf12Filter::new(sr),
            metal_phases: [0.0; 6],
            ch_active: false,
            ch_env: 0.0,
            ch_vel: 1.0,
            ch_bp: Svf12Filter::new(sr),
            ch_hp: Svf12Filter::new(sr),
            oh_active: false,
            oh_env: 0.0,
            oh_vel: 1.0,
            oh_bp: Svf12Filter::new(sr),
            oh_hp: Svf12Filter::new(sr),
            tom_active: false,
            tom_phase: 0.0,
            tom_amp: 0.0,
            tom_pitch_env: 0.0,
            tom_vel: 1.0,
            cym_active: false,
            cym_env: 0.0,
            cym_vel: 1.0,
            cym_bp: Svf12Filter::new(sr),
            cym_hp: Svf12Filter::new(sr),
            flam_countdown: [0; 8],
            flam_vel: [0.0; 8],
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.snare_filter.set_sample_rate(self.sample_rate);
        self.clap_filter.set_sample_rate(self.sample_rate);
        self.cb_filter.set_sample_rate(self.sample_rate);
        self.ch_bp.set_sample_rate(self.sample_rate);
        self.ch_hp.set_sample_rate(self.sample_rate);
        self.oh_bp.set_sample_rate(self.sample_rate);
        self.oh_hp.set_sample_rate(self.sample_rate);
        self.cym_bp.set_sample_rate(self.sample_rate);
        self.cym_hp.set_sample_rate(self.sample_rate);
    }

    pub fn trigger(&mut self, voice: DrumVoiceId, velocity: f32, accent: bool, flam: bool, kit: &DrumKitParams) {
        let vel = (velocity * (1.0 + if accent { kit.accent_level * 0.45 } else { 0.0 })).clamp(0.05, 1.4);
        self.trigger_immediate(voice, vel);
        if flam {
            let idx = voice.index();
            self.flam_countdown[idx] = (0.022 * self.sample_rate) as u32;
            self.flam_vel[idx] = vel * 0.72;
        }
    }

    fn trigger_immediate(&mut self, voice: DrumVoiceId, vel: f32) {
        match voice {
            DrumVoiceId::Kick => {
                self.kick_active = true;
                self.kick_phase = 0.0;
                self.kick_amp = 1.0;
                self.kick_pitch_env = 1.0;
                self.kick_click_env = 1.0;
                self.kick_vel = vel;
            }
            DrumVoiceId::Snare => {
                self.snare_active = true;
                self.snare_p1 = 0.0;
                self.snare_p2 = 0.0;
                self.snare_body_env = 1.0;
                self.snare_noise_env = 1.0;
                self.snare_vel = vel;
            }
            DrumVoiceId::Clap => {
                self.clap_active = true;
                self.clap_samples = 0;
                self.clap_tail_env = 1.0;
                self.clap_vel = vel;
            }
            DrumVoiceId::CowbellRim => {
                self.cb_active = true;
                self.cb_p1 = 0.0;
                self.cb_p2 = 0.0;
                self.cb_env = 1.0;
                self.cb_fast_env = 1.0;
                self.cb_vel = vel;
            }
            DrumVoiceId::ClosedHat => {
                self.ch_active = true;
                self.ch_env = 1.0;
                self.ch_vel = vel;
                // Classic hardware choke: Closed Hat chokes Open Hat!
                self.oh_env *= 0.08;
            }
            DrumVoiceId::OpenHat => {
                self.oh_active = true;
                self.oh_env = 1.0;
                self.oh_vel = vel;
            }
            DrumVoiceId::Tom => {
                self.tom_active = true;
                self.tom_phase = 0.0;
                self.tom_amp = 1.0;
                self.tom_pitch_env = 1.0;
                self.tom_vel = vel;
            }
            DrumVoiceId::Cymbal => {
                self.cym_active = true;
                self.cym_env = 1.0;
                self.cym_vel = vel;
            }
        }
    }

    pub fn active_voice_count(&self) -> usize {
        [
            self.kick_active,
            self.snare_active,
            self.clap_active,
            self.cb_active,
            self.ch_active,
            self.oh_active,
            self.tom_active,
            self.cym_active,
        ]
        .into_iter()
        .filter(|&x| x)
        .count()
    }

    #[inline]
    pub fn process_sample(&mut self, kit: &DrumKitParams) -> (f32, f32) {
        let sr = self.sample_rate;

        // Process flam timers
        for i in 0..8 {
            if self.flam_countdown[i] > 0 {
                self.flam_countdown[i] -= 1;
                if self.flam_countdown[i] == 0 {
                    if let Some(vid) = DrumVoiceId::ALL.get(i).copied() {
                        self.trigger_immediate(vid, self.flam_vel[i]);
                    }
                }
            }
        }

        let noise = self.rng.next_bipolar();

        // Advance metallic oscillator bank only if needed
        let metal_active = self.ch_active || self.oh_active || self.cym_active;
        let mut metal_808 = 0.0_f32;
        let mut metal_909 = 0.0_f32;
        if metal_active {
            for i in 0..6 {
                let p = self.metal_phases[i];
                let sq = if p < 0.5 { 1.0 } else { -1.0 };
                metal_808 += sq;
                // Slightly detuned ratio for 909 shimmer
                let sq2 = if ((p * (METAL_FREQS_909[i] / METAL_FREQS_808[i])).fract()) < 0.5 {
                    1.0
                } else {
                    -1.0
                };
                metal_909 += sq2;
                self.metal_phases[i] = (p + METAL_FREQS_808[i] / sr).fract();
            }
            metal_808 *= 0.25;
            metal_909 = metal_909 * 0.22 + noise * 0.22;
        }

        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;

        // 0: Kick
        if self.kick_active {
            let vp = &kit.voices[0];
            let s = self.process_kick(vp, noise);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.kick_vel, vp.pan);
            }
        }
        // 1: Snare
        if self.snare_active {
            let vp = &kit.voices[1];
            let s = self.process_snare(vp, noise);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.snare_vel, vp.pan);
            }
        }
        // 2: Clap
        if self.clap_active {
            let vp = &kit.voices[2];
            let s = self.process_clap(vp, noise);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.clap_vel, vp.pan);
            }
        }
        // 3: Cowbell / Rim
        if self.cb_active {
            let vp = &kit.voices[3];
            let s = self.process_cowbell_rim(vp);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.cb_vel, vp.pan);
            }
        }
        // 4: Closed Hat
        if self.ch_active {
            let vp = &kit.voices[4];
            let raw = if vp.model == DrumModel::Tr808 { metal_808 } else { metal_909 };
            let s = self.process_ch(vp, raw);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.ch_vel, vp.pan);
            }
        }
        // 5: Open Hat
        if self.oh_active {
            let vp = &kit.voices[5];
            let raw = if vp.model == DrumModel::Tr808 { metal_808 } else { metal_909 };
            let s = self.process_oh(vp, raw);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.oh_vel, vp.pan);
            }
        }
        // 6: Tom
        if self.tom_active {
            let vp = &kit.voices[6];
            let s = self.process_tom(vp, noise);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.tom_vel, vp.pan);
            }
        }
        // 7: Cymbal
        if self.cym_active {
            let vp = &kit.voices[7];
            let raw = if vp.model == DrumModel::Tr808 { metal_808 } else { metal_909 };
            let s = self.process_cymbal(vp, raw);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.cym_vel, vp.pan);
            }
        }

        (soft_tanh(out_l * 0.85), soft_tanh(out_r * 0.85))
    }

    #[inline]
    fn process_kick(&mut self, vp: &DrumVoiceParams, noise: f32) -> f32 {
        let sr = self.sample_rate;
        let tune_mult = 2.0_f32.powf(vp.tune * 0.7);
        let (base_hz, sweep_hz, decay_s, pitch_decay_s, drive) = match vp.model {
            DrumModel::Tr808 => (
                46.0 * tune_mult,
                140.0 * tune_mult,
                0.14 + vp.decay * 1.1,
                0.028 + vp.tone * 0.025,
                1.2 + vp.tone * 0.8,
            ),
            DrumModel::Tr909 => (
                52.0 * tune_mult,
                220.0 * tune_mult,
                0.09 + vp.decay * 0.55,
                0.036 + vp.tone * 0.02,
                1.5 + vp.tone * 1.3,
            ),
        };

        let freq = base_hz + sweep_hz * self.kick_pitch_env;
        self.kick_phase = (self.kick_phase + freq / sr).fract();
        let body = (self.kick_phase * TAU).sin();
        let click = (noise * 0.35 + (self.kick_phase * 4.0 * TAU).sin() * 0.65)
            * self.kick_click_env
            * (0.15 + vp.snappy * 0.55);

        self.kick_amp *= (-1.0 / (decay_s * sr)).exp();
        self.kick_pitch_env *= (-1.0 / (pitch_decay_s * sr)).exp();
        self.kick_click_env *= (-1.0 / (0.0032 * sr)).exp();

        if self.kick_amp < 0.0005 {
            self.kick_active = false;
        }
        soft_tanh((body * self.kick_amp * drive) + click) * 0.92
    }

    #[inline]
    fn process_snare(&mut self, vp: &DrumVoiceParams, noise: f32) -> f32 {
        let sr = self.sample_rate;
        let tune_mult = 2.0_f32.powf(vp.tune * 0.65);
        let (f1, f2, body_dec, noise_dec, hp_freq) = match vp.model {
            DrumModel::Tr808 => (
                182.0 * tune_mult,
                332.0 * tune_mult,
                0.065 + vp.decay * 0.08,
                0.08 + vp.decay * 0.25,
                1900.0 + vp.tone * 2400.0,
            ),
            DrumModel::Tr909 => (
                195.0 * tune_mult,
                318.0 * tune_mult,
                0.075 + vp.decay * 0.11,
                0.10 + vp.decay * 0.32,
                1300.0 + vp.tone * 2200.0,
            ),
        };
        let pitch_drop = 1.0 + 0.35 * self.snare_body_env * self.snare_body_env;
        self.snare_p1 = (self.snare_p1 + (f1 * pitch_drop) / sr).fract();
        self.snare_p2 = (self.snare_p2 + (f2 * pitch_drop) / sr).fract();
        let body = ((self.snare_p1 * TAU).sin() * 0.6 + (self.snare_p2 * TAU).sin() * 0.4) * self.snare_body_env;

        let filtered_noise = if vp.model == DrumModel::Tr808 {
            self.snare_filter.process_hp(noise, hp_freq, 0.25)
        } else {
            self.snare_filter.process_bp(noise, hp_freq, 0.32) * 1.45
        };
        let snappy = filtered_noise * self.snare_noise_env * (0.25 + vp.snappy * 0.95);

        self.snare_body_env *= (-1.0 / (body_dec * sr)).exp();
        self.snare_noise_env *= (-1.0 / (noise_dec * sr)).exp();
        if self.snare_body_env < 0.0005 && self.snare_noise_env < 0.0005 {
            self.snare_active = false;
        }
        soft_tanh((body * 0.65 + snappy) * 1.1)
    }

    #[inline]
    fn process_clap(&mut self, vp: &DrumVoiceParams, noise: f32) -> f32 {
        let sr = self.sample_rate;
        let t_ms = (self.clap_samples as f32 / sr) * 1000.0;
        self.clap_samples = self.clap_samples.saturating_add(1);

        // Four short hand-clap bursts. The tail is a voice envelope, not a reverb effect.
        let burst_env = if t_ms < 42.0 {
            let phase_ms = t_ms % 10.5;
            (-phase_ms / 4.2).exp()
        } else {
            0.0
        };

        let tail_dec = match vp.model {
            DrumModel::Tr808 => 0.035 + vp.decay * 0.14,
            DrumModel::Tr909 => 0.030 + vp.decay * 0.12,
        };
        if t_ms >= 30.0 {
            self.clap_tail_env *= (-1.0 / (tail_dec * sr)).exp();
        }
        let env = (burst_env * 0.9 + self.clap_tail_env * 0.52).min(1.0);
        let bp_freq = match vp.model {
            DrumModel::Tr808 => 1050.0 + vp.tone * 900.0,
            DrumModel::Tr909 => 1350.0 + vp.tone * 1100.0,
        };
        let filt = self.clap_filter.process_bp(noise, bp_freq, 0.45) * 2.0;

        if t_ms > 42.0 && self.clap_tail_env < 0.0008 {
            self.clap_active = false;
        }
        soft_tanh(filt * env * (0.75 + vp.snappy * 0.4))
    }

    #[inline]
    fn process_cowbell_rim(&mut self, vp: &DrumVoiceParams) -> f32 {
        let sr = self.sample_rate;
        let tune_mult = 2.0_f32.powf(vp.tune * 0.5);
        match vp.model {
            DrumModel::Tr808 => {
                // Iconic TR-808 540Hz + 800Hz dual square cowbell
                self.cb_p1 = (self.cb_p1 + (540.0 * tune_mult) / sr).fract();
                self.cb_p2 = (self.cb_p2 + (800.0 * tune_mult) / sr).fract();
                let sq = (if self.cb_p1 < 0.5 { 1.0 } else { -1.0 })
                    + (if self.cb_p2 < 0.5 { 1.0 } else { -1.0 });
                let bp = self.cb_filter.process_bp(sq * 0.5, 720.0 + vp.tone * 380.0, 0.38);
                self.cb_fast_env *= (-1.0 / (0.025 * sr)).exp();
                self.cb_env *= (-1.0 / ((0.045 + vp.decay * 0.20) * sr)).exp();
                if self.cb_env < 0.0005 {
                    self.cb_active = false;
                }
                soft_tanh(bp * (self.cb_fast_env * 0.7 + self.cb_env * 0.45) * 0.58)
            }
            DrumModel::Tr909 => {
                // TR-909 crisp wooden rimshot
                self.cb_p1 = (self.cb_p1 + (470.0 * tune_mult) / sr).fract();
                self.cb_p2 = (self.cb_p2 + (1650.0 * tune_mult) / sr).fract();
                let tri = (self.cb_p1 * TAU).sin() * 0.7 + (self.cb_p2 * TAU).sin() * 0.5;
                let bp = self.cb_filter.process_bp(tri, 950.0 + vp.tone * 800.0, 0.35);
                self.cb_env *= (-1.0 / ((0.014 + vp.decay * 0.05) * sr)).exp();
                if self.cb_env < 0.0005 {
                    self.cb_active = false;
                }
                soft_tanh(bp * self.cb_env * 1.25)
            }
        }
    }

    #[inline]
    fn process_ch(&mut self, vp: &DrumVoiceParams, metal_raw: f32) -> f32 {
        let sr = self.sample_rate;
        let dec = 0.018 + vp.decay * 0.095;
        let bp = self.ch_bp.process_bp(metal_raw, 6500.0 + vp.tone * 3500.0, 0.30);
        let hp = self.ch_hp.process_hp(bp, 5200.0 + vp.tone * 2500.0, 0.15);
        self.ch_env *= (-1.0 / (dec * sr)).exp();
        if self.ch_env < 0.0005 {
            self.ch_active = false;
        }
        soft_tanh(hp * self.ch_env * 1.5)
    }

    #[inline]
    fn process_oh(&mut self, vp: &DrumVoiceParams, metal_raw: f32) -> f32 {
        let sr = self.sample_rate;
        let dec = 0.10 + vp.decay * 0.75;
        let bp = self.oh_bp.process_bp(metal_raw, 6200.0 + vp.tone * 3200.0, 0.28);
        let hp = self.oh_hp.process_hp(bp, 4800.0 + vp.tone * 2200.0, 0.15);
        self.oh_env *= (-1.0 / (dec * sr)).exp();
        if self.oh_env < 0.0005 {
            self.oh_active = false;
        }
        soft_tanh(hp * self.oh_env * 1.35)
    }

    #[inline]
    fn process_tom(&mut self, vp: &DrumVoiceParams, noise: f32) -> f32 {
        let sr = self.sample_rate;
        let base_hz = 110.0 * 2.0_f32.powf(vp.tune * 0.8);
        let sweep = match vp.model {
            DrumModel::Tr808 => 90.0,
            DrumModel::Tr909 => 155.0,
        };
        let freq = base_hz + sweep * self.tom_pitch_env;
        self.tom_phase = (self.tom_phase + freq / sr).fract();
        let body = (self.tom_phase * TAU).sin();
        let stick = noise * self.tom_pitch_env * self.tom_pitch_env * vp.snappy * 0.35;

        let dec = 0.10 + vp.decay * 0.45;
        self.tom_amp *= (-1.0 / (dec * sr)).exp();
        self.tom_pitch_env *= (-1.0 / (0.045 * sr)).exp();
        if self.tom_amp < 0.0005 {
            self.tom_active = false;
        }
        soft_tanh((body * self.tom_amp + stick) * 0.95)
    }

    #[inline]
    fn process_cymbal(&mut self, vp: &DrumVoiceParams, metal_raw: f32) -> f32 {
        let sr = self.sample_rate;
        let dec = 0.25 + vp.decay * 1.45;
        let bp = self.cym_bp.process_bp(metal_raw, 5400.0 + vp.tone * 3800.0, 0.22);
        let hp = self.cym_hp.process_hp(bp, 3800.0 + vp.tone * 2200.0, 0.15);
        self.cym_env *= (-1.0 / (dec * sr)).exp();
        if self.cym_env < 0.0005 {
            self.cym_active = false;
        }
        soft_tanh(hp * self.cym_env * 1.15)
    }
}

#[inline]
fn add_panned(out_l: &mut f32, out_r: &mut f32, sig: f32, pan: f32) {
    let p = pan.clamp(-1.0, 1.0) * 0.5 + 0.5;
    let gl = (1.0 - p).sqrt() * 1.414;
    let gr = p.sqrt() * 1.414;
    *out_l += sig * gl;
    *out_r += sig * gr;
}
