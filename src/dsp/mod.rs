pub mod drums;
pub mod fx;
pub mod monologue;
pub mod moog;
pub mod sampler;
pub mod tb303;

use std::f32::consts::PI;

pub use drums::DrumMachine;
pub use fx::MasterFx;
pub use monologue::MonologueSynth;
pub use moog::MoogSynth;
pub use sampler::{generate_builtin_sample, load_wav_to_mono_f32, Sk1Sampler};
pub use tb303::Tb303Synth;

#[derive(Debug, Clone)]
pub struct Xorshift32 {
    state: u32,
}

impl Default for Xorshift32 {
    fn default() -> Self {
        Self { state: 0x1234_5678 }
    }
}

impl Xorshift32 {
    pub fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() & 0x00FF_FFFF) as f32 / 16_777_216.0
    }

    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        self.next_f32() * 2.0 - 1.0
    }
}

#[inline]
pub fn soft_tanh(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }
}

#[inline]
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 || dt >= 0.5 {
        return 0.0;
    }
    if t < dt {
        let n = t / dt;
        n + n - n * n - 1.0
    } else if t > 1.0 - dt {
        let n = (t - 1.0) / dt;
        n * n + n + n + 1.0
    } else {
        0.0
    }
}

#[inline]
pub fn osc_saw(phase: f32, dt: f32) -> f32 {
    let raw = 2.0 * phase - 1.0;
    raw - poly_blep(phase, dt)
}

#[inline]
pub fn osc_pulse(phase: f32, dt: f32, duty: f32) -> f32 {
    let duty = duty.clamp(0.05, 0.95);
    let mut s = if phase < duty { 1.0 } else { -1.0 };
    s += poly_blep(phase, dt);
    let p2 = (phase + 1.0 - duty).fract();
    s -= poly_blep(p2, dt);
    s
}

#[inline]
pub fn osc_tri(phase: f32) -> f32 {
    1.0 - 4.0 * (phase - 0.5).abs()
}

#[inline]
pub fn osc_trisaw(phase: f32, dt: f32) -> f32 {
    0.55 * osc_tri(phase) + 0.45 * osc_saw(phase, dt)
}

/// 4-pole 24dB/oct nonlinear Huovilainen Moog Ladder filter (2x oversampled).
#[derive(Debug, Clone)]
pub struct MoogLadderFilter {
    stage: [f32; 4],
    stage_tanh: [f32; 3],
    delay: [f32; 4],
    sample_rate: f32,
}

impl MoogLadderFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            stage: [0.0; 4],
            stage_tanh: [0.0; 3],
            delay: [0.0; 4],
            sample_rate: sample_rate.max(8000.0),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.stage = [0.0; 4];
        self.stage_tanh = [0.0; 3];
        self.delay = [0.0; 4];
    }

    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32) -> f32 {
        let cutoff = cutoff_hz.clamp(20.0, self.sample_rate * 0.42);
        let res = resonance.clamp(0.0, 0.97) * 4.0;
        let fc = cutoff / (self.sample_rate * 2.0); // 2x oversampled
        let f = fc * 0.5;
        let p = f * (1.8 - 0.8 * f);
        let k = 2.0 * (fc * PI).sin() - 1.0;
        let _ = k;
        // Huovilainen tuning
        let fc_rad = (PI * cutoff / (self.sample_rate * 2.0)).min(1.2);
        let tune = (1.0 - (-fc_rad * 2.0).exp()).clamp(0.001, 0.92);
        let res_comp = 0.5;
        let _ = p;

        let mut out = 0.0;
        for _ in 0..2 {
            let feedback = res * (self.delay[3] - res_comp * input);
            let x = soft_tanh((input - feedback) * 0.85);

            self.stage[0] = self.delay[0] + tune * (x - self.stage_tanh[0]);
            self.stage_tanh[0] = soft_tanh(self.stage[0]);
            self.delay[0] = self.stage[0];

            self.stage[1] = self.delay[1] + tune * (self.stage_tanh[0] - self.stage_tanh[1]);
            self.stage_tanh[1] = soft_tanh(self.stage[1]);
            self.delay[1] = self.stage[1];

            self.stage[2] = self.delay[2] + tune * (self.stage_tanh[1] - self.stage_tanh[2]);
            self.stage_tanh[2] = soft_tanh(self.stage[2]);
            self.delay[2] = self.stage[2];

            self.stage[3] = self.delay[3] + tune * (self.stage_tanh[2] - soft_tanh(self.delay[3]));
            self.delay[3] = self.stage[3];

            out = self.stage[3];
        }

        if !out.is_finite() {
            self.reset();
            return 0.0;
        }
        out
    }
}

/// TB-303 style 18dB/24dB nonlinear diode ladder filter (2x oversampled).
#[derive(Debug, Clone)]
pub struct DiodeLadder303Filter {
    s1: f32,
    s2: f32,
    s3: f32,
    s4: f32,
    hp_state: f32,
    sample_rate: f32,
}

impl DiodeLadder303Filter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            s1: 0.0,
            s2: 0.0,
            s3: 0.0,
            s4: 0.0,
            hp_state: 0.0,
            sample_rate: sample_rate.max(8000.0),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
        self.s3 = 0.0;
        self.s4 = 0.0;
        self.hp_state = 0.0;
    }

    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32) -> f32 {
        let cutoff = cutoff_hz.clamp(25.0, self.sample_rate * 0.42);
        let g = (PI * cutoff / (self.sample_rate * 2.0)).tan().clamp(0.001, 1.8);
        let g_coeff = g / (1.0 + g);
        // Acid resonance curve: pushes into juicy resonant squelch near 1.0
        let k = resonance.clamp(0.0, 0.98) * 16.5;

        let mut out = 0.0;
        for _ in 0..2 {
            // Subtle high-pass in feedback path gives 303 its liquid low-end retention
            self.hp_state += 0.015 * (self.s4 - self.hp_state);
            let fb = soft_tanh((self.s4 - 0.35 * self.hp_state) * 1.15);
            let u = soft_tanh((input - k * fb * 0.25) * 1.1);

            self.s1 += g_coeff * (u - self.s1 + 0.35 * (self.s2 - self.s1));
            self.s2 += g_coeff * (self.s1 - self.s2 + 0.25 * (self.s3 - self.s2));
            self.s3 += g_coeff * (self.s2 - self.s3 + 0.15 * (self.s4 - self.s3));
            self.s4 += g_coeff * (soft_tanh(self.s3 * 1.1) - self.s4);
            out = self.s4;
        }
        if !out.is_finite() {
            self.reset();
            return 0.0;
        }
        out
    }
}

/// Cytomic / Simper Zero-Delay Feedback 12dB State Variable Filter.
#[derive(Debug, Clone)]
pub struct Svf12Filter {
    ic1eq: f32,
    ic2eq: f32,
    sample_rate: f32,
}

impl Svf12Filter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            ic1eq: 0.0,
            ic2eq: 0.0,
            sample_rate: sample_rate.max(8000.0),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ic1eq = 0.0;
        self.ic2eq = 0.0;
    }

    #[inline]
    pub fn process_all(&mut self, v0: f32, cutoff_hz: f32, resonance: f32) -> (f32, f32, f32) {
        let cutoff = cutoff_hz.clamp(20.0, self.sample_rate * 0.44);
        let q = (0.55 + resonance.clamp(0.0, 0.96) * 12.0).max(0.5);
        let g = (PI * cutoff / self.sample_rate).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;

        let v3 = v0 - self.ic2eq;
        let v1 = a1 * self.ic1eq + a2 * v3;
        let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;

        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;

        if !v2.is_finite() || !v1.is_finite() {
            self.reset();
            return (0.0, 0.0, 0.0);
        }

        let lp = v2;
        let bp = v1;
        let hp = v0 - k * v1 - v2;
        (lp, bp, hp)
    }

    #[inline]
    pub fn process_lp(&mut self, input: f32, cutoff_hz: f32, resonance: f32) -> f32 {
        self.process_all(input, cutoff_hz, resonance).0
    }

    #[inline]
    pub fn process_bp(&mut self, input: f32, cutoff_hz: f32, resonance: f32) -> f32 {
        self.process_all(input, cutoff_hz, resonance).1
    }

    #[inline]
    pub fn process_hp(&mut self, input: f32, cutoff_hz: f32, resonance: f32) -> f32 {
        self.process_all(input, cutoff_hz, resonance).2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Debug, Clone)]
pub struct AdsrEnv {
    stage: EnvStage,
    level: f32,
    sample_rate: f32,
}

impl AdsrEnv {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            stage: EnvStage::Idle,
            level: 0.0,
            sample_rate: sample_rate.max(8000.0),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
    }

    pub fn reset(&mut self) {
        self.stage = EnvStage::Idle;
        self.level = 0.0;
    }

    pub fn trigger(&mut self) {
        self.stage = EnvStage::Attack;
    }

    pub fn release(&mut self) {
        if self.stage != EnvStage::Idle {
            self.stage = EnvStage::Release;
        }
    }

    pub fn is_active(&self) -> bool {
        self.stage != EnvStage::Idle
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    #[inline]
    pub fn next(&mut self, attack_s: f32, decay_s: f32, sustain: f32, release_s: f32) -> f32 {
        let sustain = sustain.clamp(0.0, 1.0);
        match self.stage {
            EnvStage::Idle => 0.0,
            EnvStage::Attack => {
                let inc = 1.0 / (attack_s.max(0.001) * self.sample_rate);
                self.level += inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = EnvStage::Decay;
                }
                self.level
            }
            EnvStage::Decay => {
                let dec = 1.0 / (decay_s.max(0.004) * self.sample_rate);
                self.level -= (self.level - sustain).max(0.0002) * dec * 4.2 + dec * 0.15;
                if self.level <= sustain + 0.0005 {
                    self.level = sustain;
                    self.stage = EnvStage::Sustain;
                }
                self.level
            }
            EnvStage::Sustain => {
                self.level = sustain;
                self.level
            }
            EnvStage::Release => {
                let rel = 1.0 / (release_s.max(0.004) * self.sample_rate);
                self.level -= self.level * rel * 4.5 + rel * 0.05;
                if self.level <= 0.0002 {
                    self.level = 0.0;
                    self.stage = EnvStage::Idle;
                }
                self.level
            }
        }
    }
}
