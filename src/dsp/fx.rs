use crate::model::{FxParams, GlobalFilterMode};

/// Tempo-synchronised mono delay, three damped combs, and an analog-style
/// saturation stage. Kept allocation-free in the audio callback.
#[derive(Debug, Clone)]
pub struct MasterFx {
    sample_rate: f32,
    delay: Vec<f32>,
    delay_pos: usize,
    delay_tone: f32,
    combs: [Vec<f32>; 3],
    comb_pos: [usize; 3],
    comb_lp: [f32; 3],
    warmth: f32,
    filter: super::Svf12Filter,
    chop_phase: f32,
    bend_phase: f32,
    repeat_buffer: Vec<f32>,
    repeat_pos: usize,
    repeat_phase: usize,
    repeat_anchor: usize,
    repeat_was_active: bool,
    glitch_counter: usize,
    glitch_hold: f32,
}

impl Default for MasterFx {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl MasterFx {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8_000.0);
        let lengths = [
            (0.0297 * sr) as usize,
            (0.0371 * sr) as usize,
            (0.0411 * sr) as usize,
        ];
        Self {
            sample_rate: sr,
            delay: vec![0.0; (sr * 3.0) as usize],
            delay_pos: 0,
            delay_tone: 0.0,
            combs: lengths.map(|n| vec![0.0; n.max(1)]),
            comb_pos: [0; 3],
            comb_lp: [0.0; 3],
            warmth: 0.0,
            filter: super::Svf12Filter::new(sr),
            chop_phase: 0.0,
            bend_phase: 0.0,
            repeat_buffer: vec![0.0; (sr * 2.0) as usize],
            repeat_pos: 0,
            repeat_phase: 0,
            repeat_anchor: 0,
            repeat_was_active: false,
            glitch_counter: 0,
            glitch_hold: 0.0,
        }
    }

    pub fn process(&mut self, input: f32, params: &FxParams, bpm: f32) -> f32 {
        let delay_len = (60.0 / bpm.clamp(40.0, 240.0) * params.delay_div.sixteenth_multiplier()
            / 4.0
            * self.sample_rate)
            .round()
            .clamp(1.0, (self.delay.len() - 1) as f32) as usize;
        let tap = (self.delay_pos + self.delay.len() - delay_len) % self.delay.len();
        let delayed = self.delay[tap];
        let tone = params.delay_tone.clamp(0.0, 1.0);
        let lp_coeff = 0.015 + tone * 0.45;
        self.delay_tone += lp_coeff * (delayed - self.delay_tone);
        self.delay[self.delay_pos] =
            input + self.delay_tone * params.delay_feedback.clamp(0.0, 0.92);
        self.delay_pos = (self.delay_pos + 1) % self.delay.len();

        let size = params.reverb_size.clamp(0.0, 0.98);
        let damp = params.reverb_damp.clamp(0.0, 1.0);
        let mut wet = 0.0;
        for i in 0..3 {
            let pos = self.comb_pos[i];
            let out = self.combs[i][pos];
            self.comb_lp[i] += (0.04 + (1.0 - damp) * 0.35) * (out - self.comb_lp[i]);
            self.combs[i][pos] = input * 0.28 + self.comb_lp[i] * (0.35 + size * 0.58);
            self.comb_pos[i] = (pos + 1) % self.combs[i].len();
            wet += out;
        }
        wet /= 3.0;

        let delay_mix = params.delay_mix.clamp(0.0, 1.0);
        let reverb_mix = params.reverb_mix.clamp(0.0, 1.0);
        let effected = input * (1.0 - delay_mix) + self.delay_tone * delay_mix;
        let mut x = effected * (1.0 - reverb_mix) + wet * reverb_mix;
        let chop = params.chop.clamp(0.0, 1.0);
        self.chop_phase =
            (self.chop_phase + (bpm / 60.0 * (1.0 + chop * 7.0)) / self.sample_rate).fract();
        let gate = if self.chop_phase < 0.5 {
            1.0
        } else {
            1.0 - chop * 0.96
        };
        x *= gate;

        let repeat = params.repeat.clamp(0.0, 1.0);
        let repeat_len = (60.0 / bpm.clamp(40.0, 240.0) * 0.5 * self.sample_rate).round() as usize;
        if repeat > 0.001 && !self.repeat_was_active {
            self.repeat_anchor = self.repeat_pos;
            self.repeat_phase = 0;
        }
        let repeat_idx = (self.repeat_anchor + self.repeat_buffer.len()
            - repeat_len.min(self.repeat_buffer.len() - 1)
            + self.repeat_phase)
            % self.repeat_buffer.len();
        let repeated = self.repeat_buffer[repeat_idx];
        self.repeat_buffer[self.repeat_pos] = x;
        self.repeat_pos = (self.repeat_pos + 1) % self.repeat_buffer.len();
        if repeat > 0.001 {
            self.repeat_phase = (self.repeat_phase + 1) % repeat_len.max(1);
        } else {
            self.repeat_phase = 0;
        }
        self.repeat_was_active = repeat > 0.001;
        x = x * (1.0 - repeat) + repeated * repeat;

        let glitch = params.glitch.clamp(0.0, 1.0);
        let hold_n = 1 + (glitch * 48.0) as usize;
        if self.glitch_counter == 0 {
            let levels = (65535.0 / (1.0 + glitch * 255.0)).max(16.0);
            self.glitch_hold = (x * levels).round() / levels;
        }
        self.glitch_counter = (self.glitch_counter + 1) % hold_n;
        x = x * (1.0 - glitch) + self.glitch_hold * glitch;

        let bend = params.circuit_bend.clamp(0.0, 1.0);
        self.bend_phase = (self.bend_phase + (0.17 + bend * 8.0) / self.sample_rate).fract();
        let bend_wave = (self.bend_phase * std::f32::consts::TAU).sin();
        let cutoff = (params.global_filter_cutoff * (1.0 + bend_wave * bend * 0.85))
            .clamp(20.0, self.sample_rate * 0.42);
        let resonance = (params.global_filter_resonance + bend * (0.12 + bend_wave.abs() * 0.24))
            .clamp(0.0, 0.96);
        let (lp, bp, hp) = self.filter.process_all(x, cutoff, resonance);
        x = match params.global_filter_mode {
            GlobalFilterMode::LowPass => lp,
            GlobalFilterMode::HighPass => hp,
            GlobalFilterMode::BandPass => bp,
        };
        let folded = (x * (1.0 + bend * 11.0)).sin();
        x = x * (1.0 - bend * 0.48) + folded * bend * 0.48;

        self.warmth += 0.002 * (params.analog_warmth.clamp(0.0, 1.0) - self.warmth);
        x *= 1.0 + self.warmth * 1.6;
        x / (1.0 + x.abs() * self.warmth * 1.8)
    }
}
