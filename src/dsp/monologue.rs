use crate::model::{
    midi_to_hz, EgTarget, LfoMode, LfoTarget, LfoWave, MonologueOscWave, MonologueParams,
    MotionLocks, Osc2ModMode,
};

use super::{osc_pulse, osc_saw, osc_tri, soft_tanh, AdsrEnv, Svf12Filter};

#[derive(Debug, Clone)]
pub struct MonologueSynth {
    active: bool,
    held: bool,
    note: u8,
    current_midi: f32,
    target_midi: f32,
    velocity: f32,
    accent: f32,
    phase1: f32,
    phase2: f32,
    lfo_phase: f32,
    lfo_oneshot_done: bool,
    filter: Svf12Filter,
    eg: AdsrEnv,
    amp_env: AdsrEnv,
    sample_rate: f32,
    // Motion lock overrides + smoothers
    motion_locks: MotionLocks,
    motion_smooth: bool,
    slew_cutoff: f32,
    slew_res: f32,
    slew_drive: f32,
    slew_shape: f32,
}

impl MonologueSynth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            active: false,
            held: false,
            note: 60,
            current_midi: 60.0,
            target_midi: 60.0,
            velocity: 0.85,
            accent: 0.0,
            phase1: 0.0,
            phase2: 0.0,
            lfo_phase: 0.0,
            lfo_oneshot_done: false,
            filter: Svf12Filter::new(sr),
            eg: AdsrEnv::new(sr),
            amp_env: AdsrEnv::new(sr),
            sample_rate: sr,
            motion_locks: MotionLocks::default(),
            motion_smooth: true,
            slew_cutoff: 1850.0,
            slew_res: 0.42,
            slew_drive: 0.42,
            slew_shape: 0.35,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.filter.set_sample_rate(self.sample_rate);
        self.eg.set_sample_rate(self.sample_rate);
        self.amp_env.set_sample_rate(self.sample_rate);
    }

    pub fn set_motion_locks(&mut self, locks: MotionLocks, smooth: bool) {
        self.motion_locks = locks;
        self.motion_smooth = smooth;
    }

    pub fn clear_motion_locks(&mut self) {
        self.motion_locks = MotionLocks::default();
    }

    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        accent: bool,
        slide: bool,
        params: &MonologueParams,
    ) {
        let was_active = self.active && self.amp_env.is_active();
        self.note = note;
        self.target_midi = note as f32;
        if !was_active && !slide && params.glide < 0.01 {
            self.current_midi = note as f32;
        } else if !was_active {
            self.current_midi = note as f32;
        }
        self.velocity = velocity.clamp(0.05, 1.0);
        self.accent = if accent { 0.35 } else { 0.0 };
        self.held = true;
        self.active = true;

        if !(slide && was_active) {
            self.eg.trigger();
            self.amp_env.trigger();
            if params.lfo_mode == LfoMode::OneShot {
                self.lfo_phase = 0.0;
                self.lfo_oneshot_done = false;
            }
        }
    }

    pub fn note_off(&mut self, note: u8) {
        if self.active && self.held && self.note == note {
            self.held = false;
            self.eg.release();
            self.amp_env.release();
        }
    }

    pub fn all_notes_off(&mut self) {
        self.held = false;
        self.eg.release();
        self.amp_env.release();
    }

    pub fn is_active(&self) -> bool {
        self.active && self.amp_env.is_active()
    }

    #[inline]
    pub fn process_sample(&mut self, params: &MonologueParams) -> f32 {
        if !self.active {
            return 0.0;
        }

        let sr = self.sample_rate;
        let target_cutoff = self
            .motion_locks
            .cutoff
            .map(|n| 35.0 * (16000.0_f32 / 35.0).powf(n.clamp(0.0, 1.0)))
            .unwrap_or(params.cutoff);
        let target_res = self.motion_locks.resonance.unwrap_or(params.resonance);
        let target_drive = self.motion_locks.drive.unwrap_or(params.drive);
        let target_shape = self.motion_locks.shape.unwrap_or(params.vco1_shape);

        let slew = if self.motion_smooth { 0.0055 } else { 0.25 };
        self.slew_cutoff += slew * (target_cutoff - self.slew_cutoff);
        self.slew_res += slew * (target_res - self.slew_res);
        self.slew_drive += slew * (target_drive - self.slew_drive);
        self.slew_shape += slew * (target_shape - self.slew_shape);

        // Monologue AD-type EG (or sustain when amp_sustain is enabled)
        let eg_sustain = 0.0;
        let eg_val = self.eg.next(params.eg_attack, params.eg_decay, eg_sustain, 0.08);
        let amp_val = if params.amp_sustain {
            self.amp_env.next(0.003, 0.05, 1.0, 0.045)
        } else {
            self.amp_env
                .next(params.eg_attack.max(0.002), params.eg_decay.max(0.04), 0.0, 0.05)
        };

        if !self.amp_env.is_active() {
            self.active = false;
            return 0.0;
        }

        // LFO rate mapping by Slow / Fast / OneShot
        let lfo_hz = match params.lfo_mode {
            LfoMode::Slow => 0.1 + params.lfo_rate.powf(2.0) * 14.0,
            LfoMode::Fast => 4.0 + params.lfo_rate.powf(2.0) * 180.0,
            LfoMode::OneShot => 0.4 + params.lfo_rate * 8.0,
        };
        if !self.lfo_oneshot_done {
            self.lfo_phase += lfo_hz / sr;
            if self.lfo_phase >= 1.0 {
                if params.lfo_mode == LfoMode::OneShot {
                    self.lfo_phase = 1.0;
                    self.lfo_oneshot_done = true;
                } else {
                    self.lfo_phase = self.lfo_phase.fract();
                }
            }
        }

        let lfo_raw = if self.lfo_oneshot_done {
            0.0
        } else {
            match params.lfo_wave {
                LfoWave::Triangle => osc_tri(self.lfo_phase),
                LfoWave::Saw => 1.0 - 2.0 * self.lfo_phase,
                LfoWave::Square => if self.lfo_phase < 0.5 { 1.0 } else { -1.0 },
            }
        };
        let lfo_mod = lfo_raw * params.lfo_int;

        // Glide
        let glide_coeff = if params.glide <= 0.005 {
            1.0
        } else {
            1.0 - (-1.0 / (params.glide * 0.25 * sr)).exp()
        };
        self.current_midi += glide_coeff * (self.target_midi - self.current_midi);

        let eg_pitch1 = if params.eg_target == EgTarget::Pitch {
            eg_val * params.eg_int * 24.0
        } else {
            0.0
        };
        let eg_pitch2 = if params.eg_target == EgTarget::Pitch2 {
            eg_val * params.eg_int * 24.0
        } else {
            0.0
        };
        let lfo_pitch = if params.lfo_target == LfoTarget::Pitch {
            lfo_mod * 7.0
        } else {
            0.0
        };

        let f1 = midi_to_hz(self.current_midi + eg_pitch1 + lfo_pitch).clamp(16.0, sr * 0.42);
        let f2 = midi_to_hz(
            self.current_midi
                + params.vco2_octave as f32 * 12.0
                + params.vco2_pitch
                + eg_pitch1
                + eg_pitch2
                + lfo_pitch,
        )
        .clamp(16.0, sr * 0.42);

        let dt1 = f1 / sr;
        let dt2 = f2 / sr;

        let prev_p1 = self.phase1;
        self.phase1 = (self.phase1 + dt1).fract();
        self.phase2 = (self.phase2 + dt2).fract();
        if params.mod_mode == Osc2ModMode::Sync && self.phase1 < prev_p1 {
            // Hard sync VCO2 to VCO1 phase wrap
            self.phase2 = (self.phase1 / dt1.max(1e-6)) * dt2;
            self.phase2 = self.phase2.fract();
        }

        let eg_shape = if params.eg_target == EgTarget::Shape {
            eg_val * params.eg_int * 0.65
        } else {
            0.0
        };
        let lfo_shape = if params.lfo_target == LfoTarget::Shape {
            lfo_mod * 0.45
        } else {
            0.0
        };

        let shape1 = (self.slew_shape + eg_shape + lfo_shape).clamp(0.0, 1.0);
        let shape2 = (params.vco2_shape + eg_shape * 0.6 + lfo_shape * 0.6).clamp(0.0, 1.0);

        let vco1 = monologue_wave(params.vco1_wave, self.phase1, dt1, shape1);
        let vco2 = monologue_wave(params.vco2_wave, self.phase2, dt2, shape2);

        let osc_mix = match params.mod_mode {
            Osc2ModMode::Ring => vco1 * params.vco1_level * 0.5 + (vco1 * vco2) * params.vco2_level * 0.9,
            Osc2ModMode::Off | Osc2ModMode::Sync => {
                vco1 * params.vco1_level * 0.6 + vco2 * params.vco2_level * 0.6
            }
        };

        let eg_cutoff = if params.eg_target == EgTarget::Cutoff {
            (params.eg_int + self.accent * 0.5) * eg_val * 4.2
        } else {
            self.accent * eg_val * 1.5
        };
        let lfo_cutoff = if params.lfo_target == LfoTarget::Cutoff {
            lfo_mod * 2.5
        } else {
            0.0
        };

        let cutoff = self.slew_cutoff * 2.0_f32.powf(eg_cutoff + lfo_cutoff);
        let filtered = self.filter.process_lp(osc_mix, cutoff, self.slew_res);

        // Korg Monologue asymmetric analog Drive stage
        let drive_gain = 1.0 + self.slew_drive * 5.2;
        let driven = soft_tanh(filtered * drive_gain + 0.08 * filtered * filtered) / (1.0 + self.slew_drive * 0.5);

        soft_tanh(driven * amp_val * (self.velocity + self.accent * 0.25) * 0.72)
    }
}

#[inline]
fn monologue_wave(wave: MonologueOscWave, phase: f32, dt: f32, shape: f32) -> f32 {
    match wave {
        MonologueOscWave::Saw => {
            let s = osc_saw(phase, dt);
            // Monologue saw wave-shaper adds second harmonic fold / clipper bite
            let folded = (s * (1.0 + shape * 1.6)).sin();
            s * (1.0 - shape * 0.5) + folded * shape * 0.65
        }
        MonologueOscWave::Triangle => {
            let t = osc_tri(phase);
            // Classic Korg wavefolder on triangle
            (t * (1.0 + shape * 3.0)).sin()
        }
        MonologueOscWave::Square => {
            let duty = 0.5 - shape * 0.42;
            osc_pulse(phase, dt, duty)
        }
    }
}
