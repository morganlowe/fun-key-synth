use crate::model::{midi_to_hz, MotionLocks, Tb303Params, Tb303Wave};

use super::{
    one_pole_coeff, osc_analog_pulse, osc_analog_saw, soft_ceiling, soft_tanh, AnalogDrift,
    DiodeLadder303Filter, HalfBandDecim2x, HalfBandUp2x,
};

#[derive(Debug, Clone)]
pub struct Tb303Synth {
    active: bool,
    held: bool,
    note: u8,
    current_midi: f32,
    target_midi: f32,
    velocity: f32,
    accent_flag: bool,
    phase: f32,
    vca_env: f32,
    vca_attack_env: f32,
    vcf_env: f32,
    accent_env: f32,
    slide_active: bool,
    filter: DiodeLadder303Filter,
    sample_rate: f32,
    motion_locks: MotionLocks,
    slew_cutoff: f32,
    slew_res: f32,
    slew_drive: f32,
    sub_lp: f32,
    dc_state: f32,
    sq_lp: f32,
    post_lp: f32,
    od_up: HalfBandUp2x,
    od_decim: HalfBandDecim2x,
    thermal: AnalogDrift,
}

impl Tb303Synth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            active: false,
            held: false,
            note: 36,
            current_midi: 36.0,
            target_midi: 36.0,
            velocity: 0.9,
            accent_flag: false,
            phase: 0.0,
            vca_env: 0.0,
            vca_attack_env: 0.0,
            vcf_env: 0.0,
            accent_env: 0.0,
            slide_active: false,
            filter: DiodeLadder303Filter::new(sr),
            sample_rate: sr,
            motion_locks: MotionLocks::default(),
            slew_cutoff: 420.0,
            slew_res: 0.62,
            slew_drive: 0.28,
            sub_lp: 0.0,
            dc_state: 0.0,
            sq_lp: 0.0,
            post_lp: 0.0,
            od_up: HalfBandUp2x::new(),
            od_decim: HalfBandDecim2x::new(),
            thermal: AnalogDrift::new(0x0303_AC1D),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.filter.set_sample_rate(self.sample_rate);
    }

    pub fn set_motion_locks(&mut self, locks: MotionLocks) {
        self.motion_locks = locks;
    }

    pub fn clear_motion_locks(&mut self) {
        self.motion_locks = MotionLocks::default();
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, accent: bool, slide_from_prev: bool) {
        let was_active = self.active && self.vca_env > 0.01;
        self.note = note;
        self.target_midi = note as f32;
        self.velocity = velocity.clamp(0.1, 1.0);
        self.accent_flag = accent;
        self.held = true;
        self.active = true;

        if slide_from_prev && was_active {
            // Classic 303 slide: glides pitch smoothly and does NOT retrigger filter/amp envelopes!
            self.slide_active = true;
            self.vca_attack_env = 1.0;
        } else {
            self.current_midi = note as f32;
            self.slide_active = false;
            self.vca_env = 1.0;
            self.vca_attack_env = 0.0;
            self.vcf_env = 1.0;
            if accent {
                // Accent capacitor builds up slightly when consecutive accented notes hit
                self.accent_env = (self.accent_env * 0.35 + 1.0).min(1.35);
            } else {
                self.accent_env *= 0.25;
            }
        }
    }

    pub fn note_off(&mut self, note: u8) {
        if self.active && self.held && self.note == note {
            self.held = false;
        }
    }

    pub fn all_notes_off(&mut self) {
        self.held = false;
        self.vca_env *= 0.1;
    }

    pub fn is_active(&self) -> bool {
        self.active && self.vca_env > 0.001
    }

    #[inline]
    pub fn process_sample(&mut self, params: &Tb303Params) -> f32 {
        if !self.active {
            return 0.0;
        }
        let sr = self.sample_rate;

        let target_cutoff = self
            .motion_locks
            .cutoff
            .map(|n| 55.0 * (2200.0_f32 / 55.0).powf(n.clamp(0.0, 1.0)))
            .unwrap_or(params.cutoff);
        let target_res = self.motion_locks.resonance.unwrap_or(params.resonance);
        let target_drive = self.motion_locks.drive.unwrap_or(params.overdrive);
        let env_mod = self.motion_locks.shape.unwrap_or(params.env_mod);

        let slew_c = one_pole_coeff(60.0, sr);
        self.slew_cutoff += slew_c * (target_cutoff - self.slew_cutoff);
        self.slew_res += slew_c * (target_res - self.slew_res);
        self.slew_drive += slew_c * (target_drive - self.slew_drive);

        // 60ms 303 pitch slide exponential glide
        if self.slide_active {
            let slide_coeff = 1.0 - (-1.0 / (0.045 * sr)).exp();
            self.current_midi += slide_coeff * (self.target_midi - self.current_midi);
        } else {
            self.current_midi = self.target_midi;
        }

        // Smooth ~2ms analog VCA attack + decay
        self.vca_attack_env += one_pole_coeff(220.0, sr) * (1.0 - self.vca_attack_env);
        if self.held {
            let hold_decay = (-1.0 / (1.8 * sr)).exp();
            self.vca_env *= hold_decay;
        } else {
            let rel_decay = (-1.0 / (0.018 * sr)).exp();
            self.vca_env *= rel_decay;
        }

        // VCF exponential decay (accented steps shorten decay to ~180ms for punchy wow!)
        let vcf_decay_s = if self.accent_flag {
            params.decay.min(0.22).max(0.08)
        } else {
            params.decay.clamp(0.06, 2.0)
        };
        self.vcf_env *= (-1.0 / (vcf_decay_s * sr)).exp();
        self.accent_env *= (-1.0 / (0.14 * sr)).exp();

        if self.vca_env <= 0.0008 && !self.held {
            self.active = false;
            return 0.0;
        }

        let therm = self.thermal.next(sr);
        let freq = midi_to_hz(self.current_midi + params.tuning + therm * 0.013).clamp(20.0, sr * 0.42);
        let dt = freq / sr;
        self.phase = (self.phase + dt).fract();

        let raw_osc = match params.wave {
            Tb303Wave::Saw => osc_analog_saw(self.phase, dt),
            // 303 square has frequency-dependent asymmetric rounded pulse character
            Tb303Wave::Square => {
                let duty = 0.50 - 0.045 * ((freq - 55.0) / 300.0).clamp(0.0, 1.0) + therm * 0.012;
                let p = osc_analog_pulse(self.phase, dt, duty);
                let sq_alpha = one_pole_coeff(6200.0, sr);
                self.sq_lp += sq_alpha * (p - self.sq_lp);
                soft_tanh(self.sq_lp * 1.14 - 0.08 * (self.phase * std::f32::consts::TAU).sin())
            }
        };

        let acc_boost = self.accent_env * params.accent;
        let mod_oct = env_mod.clamp(0.0, 1.0) * self.vcf_env * 2.8 + acc_boost * 1.15;
        let max_cut = 7_500.0_f32.min(sr * 0.32);
        let cutoff = (self.slew_cutoff * (1.0 + therm * 0.015) * 2.0_f32.powf(mod_oct))
            .clamp(45.0, max_cut);
        let res = (self.slew_res.clamp(0.0, 0.72) + acc_boost * 0.04).clamp(0.0, 0.76);
        let filtered = self.filter.process(raw_osc * 0.95, cutoff, res);

        // VCA + Accent gain + half-band 2x-oversampled warm Boss/Roland overdrive + clean sub floor retention
        let vca_gain = self.vca_env * self.vca_attack_env * (self.velocity + acc_boost * 0.45);
        let clean_vca = filtered * vca_gain;
        let sub_alpha = one_pole_coeff(140.0, sr);
        self.sub_lp += sub_alpha * (clean_vca - self.sub_lp);

        let drive_gain = 1.0 + self.slew_drive.clamp(0.0, 1.0) * 3.2;
        let od_step = |x: f32| -> f32 {
            let r = x * drive_gain;
            let rc = r.clamp(-2.6, 2.6);
            soft_tanh(r + 0.06 * rc * rc)
        };
        let up = self.od_up.process(clean_vca);
        let od = self.od_decim.process(od_step(up[0]), od_step(up[1]));
        let dc_alpha = one_pole_coeff(20.0, sr);
        self.dc_state += dc_alpha * (od - self.dc_state);
        let od_clean = od - self.dc_state;

        // Warm 5.5 kHz post-drive smoothing prevents razor-like clipped top-end fizz
        let post_a = one_pole_coeff(5_500.0_f32.min(sr * 0.42), sr);
        self.post_lp += post_a * (od_clean - self.post_lp);

        let blend = self.post_lp * 0.78 + self.sub_lp * 0.46;
        soft_ceiling(blend * (0.86 / (1.0 + self.slew_drive * 0.25)), 1.0, 0.35)
    }
}
