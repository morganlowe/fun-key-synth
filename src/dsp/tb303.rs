use crate::model::{midi_to_hz, MotionLocks, Tb303Params, Tb303Wave};

use super::{osc_pulse, osc_saw, soft_tanh, DiodeLadder303Filter};

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
    vcf_env: f32,
    accent_env: f32,
    slide_active: bool,
    filter: DiodeLadder303Filter,
    sample_rate: f32,
    motion_locks: MotionLocks,
    slew_cutoff: f32,
    slew_res: f32,
    slew_drive: f32,
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
            vcf_env: 0.0,
            accent_env: 0.0,
            slide_active: false,
            filter: DiodeLadder303Filter::new(sr),
            sample_rate: sr,
            motion_locks: MotionLocks::default(),
            slew_cutoff: 420.0,
            slew_res: 0.72,
            slew_drive: 0.38,
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
        } else {
            self.current_midi = note as f32;
            self.slide_active = false;
            self.vca_env = 1.0;
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
            .map(|n| 40.0 * (8000.0_f32 / 40.0).powf(n.clamp(0.0, 1.0)))
            .unwrap_or(params.cutoff);
        let target_res = self.motion_locks.resonance.unwrap_or(params.resonance);
        let target_drive = self.motion_locks.drive.unwrap_or(params.overdrive);
        let env_mod = self.motion_locks.shape.unwrap_or(params.env_mod);

        self.slew_cutoff += 0.01 * (target_cutoff - self.slew_cutoff);
        self.slew_res += 0.01 * (target_res - self.slew_res);
        self.slew_drive += 0.01 * (target_drive - self.slew_drive);

        // 60ms 303 pitch slide exponential glide
        if self.slide_active {
            let slide_coeff = 1.0 - (-1.0 / (0.045 * sr)).exp();
            self.current_midi += slide_coeff * (self.target_midi - self.current_midi);
        } else {
            self.current_midi = self.target_midi;
        }

        // VCA decay (holds steady during gate, fast 14ms release after gate off, or long ring on accent)
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

        let freq = midi_to_hz(self.current_midi + params.tuning).clamp(20.0, sr * 0.42);
        let dt = freq / sr;
        self.phase = (self.phase + dt).fract();

        let raw_osc = match params.wave {
            Tb303Wave::Saw => osc_saw(self.phase, dt),
            // 303 square has asymmetric rounded pulse character
            Tb303Wave::Square => {
                let p = osc_pulse(self.phase, dt, 0.52);
                soft_tanh(p * 1.15 - 0.12 * (self.phase * std::f32::consts::TAU).sin())
            }
        };

        let acc_boost = self.accent_env * params.accent;
        let mod_oct = env_mod * self.vcf_env * 4.4 + acc_boost * 2.1;
        let cutoff = self.slew_cutoff * 2.0_f32.powf(mod_oct);
        let filtered = self.filter.process(raw_osc * 0.9, cutoff, self.slew_res);

        // VCA + Accent gain + Overdrive pedal saturation
        let vca_gain = self.vca_env * (self.velocity + acc_boost * 0.55);
        let drive_gain = 1.0 + self.slew_drive * 6.5;
        let driven = soft_tanh(filtered * vca_gain * drive_gain);

        driven * (0.68 / (1.0 + self.slew_drive * 0.45))
    }
}
