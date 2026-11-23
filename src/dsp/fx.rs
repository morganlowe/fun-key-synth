use crate::model::{FxParams, GlobalFilterMode};

/// Simple Schroeder allpass filter for smooth reverb diffusion.
#[derive(Debug, Clone)]
struct Allpass {
    buf: Vec<f32>,
    pos: usize,
    gain: f32,
}

impl Allpass {
    fn new(len: usize, gain: f32) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            gain,
        }
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buf[self.pos];
        let y = -input * self.gain + delayed;
        self.buf[self.pos] = input + delayed * self.gain;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

/// True-stereo tempo-synced ping-pong delay, lush 4-comb + 2-allpass stereo
/// Schroeder reverb, stereo performance FX, global SVF filter, warm tape
/// saturation, and transparent soft-clipping master limiter.
/// Kept allocation-free in the audio callback.
#[derive(Debug, Clone)]
pub struct MasterFx {
    sample_rate: f32,
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    delay_pos: usize,
    delay_tone_l: f32,
    delay_tone_r: f32,
    combs_l: [Vec<f32>; 4],
    combs_r: [Vec<f32>; 4],
    comb_pos_l: [usize; 4],
    comb_pos_r: [usize; 4],
    comb_lp_l: [f32; 4],
    comb_lp_r: [f32; 4],
    allpass_l: [Allpass; 2],
    allpass_r: [Allpass; 2],
    warmth: f32,
    crisp: f32,
    del_hp_l: f32,
    del_hp_r: f32,
    rev_hp_l: f32,
    rev_hp_r: f32,
    sub_lp_l: f32,
    sub_lp_r: f32,
    mud_lp_l: f32,
    mud_lp_r: f32,
    pres_lp_l: f32,
    pres_lp_r: f32,
    air_lp_l: f32,
    air_lp_r: f32,
    side_sub_hp: super::Svf12Filter,
    filter_l: super::Svf12Filter,
    filter_r: super::Svf12Filter,
    chop_phase: f32,
    bend_phase: f32,
    repeat_buf_l: Vec<f32>,
    repeat_buf_r: Vec<f32>,
    repeat_pos: usize,
    repeat_phase: usize,
    repeat_anchor: usize,
    repeat_was_active: bool,
    glitch_counter: usize,
    glitch_hold_l: f32,
    glitch_hold_r: f32,
    delay_active: bool,
    reverb_active: bool,
}

impl Default for MasterFx {
    fn default() -> Self {
        Self::new(48_000.0)
    }
}

impl MasterFx {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8_000.0);
        // Decorrelated Freeverb-inspired prime-ish comb lengths for L & R
        let lens_l = [
            (0.0253 * sr) as usize,
            (0.0297 * sr) as usize,
            (0.0341 * sr) as usize,
            (0.0389 * sr) as usize,
        ];
        let lens_r = [
            (0.0261 * sr) as usize,
            (0.0309 * sr) as usize,
            (0.0355 * sr) as usize,
            (0.0403 * sr) as usize,
        ];
        let delay_cap = (sr * 3.0) as usize;
        let repeat_cap = (sr * 2.0) as usize;
        Self {
            sample_rate: sr,
            delay_l: vec![0.0; delay_cap],
            delay_r: vec![0.0; delay_cap],
            delay_pos: 0,
            delay_tone_l: 0.0,
            delay_tone_r: 0.0,
            combs_l: lens_l.map(|n| vec![0.0; n.max(1)]),
            combs_r: lens_r.map(|n| vec![0.0; n.max(1)]),
            comb_pos_l: [0; 4],
            comb_pos_r: [0; 4],
            comb_lp_l: [0.0; 4],
            comb_lp_r: [0.0; 4],
            allpass_l: [
                Allpass::new((0.0051 * sr) as usize, 0.52),
                Allpass::new((0.0017 * sr) as usize, 0.48),
            ],
            allpass_r: [
                Allpass::new((0.0056 * sr) as usize, 0.52),
                Allpass::new((0.0019 * sr) as usize, 0.48),
            ],
            warmth: 0.0,
            crisp: 0.0,
            del_hp_l: 0.0,
            del_hp_r: 0.0,
            rev_hp_l: 0.0,
            rev_hp_r: 0.0,
            sub_lp_l: 0.0,
            sub_lp_r: 0.0,
            mud_lp_l: 0.0,
            mud_lp_r: 0.0,
            pres_lp_l: 0.0,
            pres_lp_r: 0.0,
            air_lp_l: 0.0,
            air_lp_r: 0.0,
            side_sub_hp: super::Svf12Filter::new(sr),
            filter_l: super::Svf12Filter::new(sr),
            filter_r: super::Svf12Filter::new(sr),
            chop_phase: 0.0,
            bend_phase: 0.0,
            repeat_buf_l: vec![0.0; repeat_cap],
            repeat_buf_r: vec![0.0; repeat_cap],
            repeat_pos: 0,
            repeat_phase: 0,
            repeat_anchor: 0,
            repeat_was_active: false,
            glitch_counter: 0,
            glitch_hold_l: 0.0,
            glitch_hold_r: 0.0,
            delay_active: true,
            reverb_active: true,
        }
    }

    pub fn process_stereo(
        &mut self,
        in_l: f32,
        in_r: f32,
        delay_in_l: f32,
        delay_in_r: f32,
        reverb_in_l: f32,
        reverb_in_r: f32,
        params: &FxParams,
        bpm: f32,
    ) -> (f32, f32) {
        if self.delay_active && !params.delay_enabled {
            self.delay_l.fill(0.0);
            self.delay_r.fill(0.0);
            self.delay_tone_l = 0.0;
            self.delay_tone_r = 0.0;
            self.del_hp_l = 0.0;
            self.del_hp_r = 0.0;
        }
        if self.reverb_active && !params.reverb_enabled {
            for c in &mut self.combs_l {
                c.fill(0.0);
            }
            for c in &mut self.combs_r {
                c.fill(0.0);
            }
            self.comb_lp_l = [0.0; 4];
            self.comb_lp_r = [0.0; 4];
            self.rev_hp_l = 0.0;
            self.rev_hp_r = 0.0;
            for ap in &mut self.allpass_l {
                ap.clear();
            }
            for ap in &mut self.allpass_r {
                ap.clear();
            }
        }
        self.delay_active = params.delay_enabled;
        self.reverb_active = params.reverb_enabled;

        // 1. Tempo-synced stereo ping-pong delay (high-passed ~170 Hz to keep echoes clean)
        let del_hp_alpha = (std::f32::consts::TAU * 170.0 / self.sample_rate).clamp(0.002, 0.3);
        self.del_hp_l += del_hp_alpha * (delay_in_l - self.del_hp_l);
        self.del_hp_r += del_hp_alpha * (delay_in_r - self.del_hp_r);
        let clean_del_in_l = delay_in_l - self.del_hp_l;
        let clean_del_in_r = delay_in_r - self.del_hp_r;

        let delay_len = (60.0 / bpm.clamp(40.0, 240.0) * params.delay_div.sixteenth_multiplier()
            / 4.0
            * self.sample_rate)
            .round()
            .clamp(1.0, (self.delay_l.len() - 1) as f32) as usize;
        // Slightly offset R tap (0.75x or exact ping-pong cross-feed)
        let tap = (self.delay_pos + self.delay_l.len() - delay_len) % self.delay_l.len();
        let tap_r = (self.delay_pos + self.delay_r.len() - ((delay_len * 3) / 4).max(1))
            % self.delay_r.len();
        let del_l = self.delay_l[tap];
        let del_r = self.delay_r[tap_r];

        let tone = params.delay_tone.clamp(0.0, 1.0);
        let lp_coeff = 0.02 + tone * 0.42;
        self.delay_tone_l += lp_coeff * (del_l - self.delay_tone_l);
        self.delay_tone_r += lp_coeff * (del_r - self.delay_tone_r);

        let fb = params.delay_feedback.clamp(0.0, 0.88);
        if params.delay_enabled {
            // Ping-pong cross-feedback: L feeds R and R feeds L
            self.delay_l[self.delay_pos] = (clean_del_in_l + self.delay_tone_r * fb).clamp(-2.5, 2.5);
            self.delay_r[self.delay_pos] = (clean_del_in_r + self.delay_tone_l * fb).clamp(-2.5, 2.5);
        } else {
            self.delay_l[self.delay_pos] = 0.0;
            self.delay_r[self.delay_pos] = 0.0;
        }
        self.delay_pos = (self.delay_pos + 1) % self.delay_l.len();

        // 2. Lush Stereo Schroeder Reverb (4 damped combs + 2 allpass diffusers per channel, high-passed ~220 Hz)
        let rev_hp_alpha = (std::f32::consts::TAU * 220.0 / self.sample_rate).clamp(0.002, 0.35);
        self.rev_hp_l += rev_hp_alpha * (reverb_in_l - self.rev_hp_l);
        self.rev_hp_r += rev_hp_alpha * (reverb_in_r - self.rev_hp_r);
        let clean_rev_in_l = reverb_in_l - self.rev_hp_l;
        let clean_rev_in_r = reverb_in_r - self.rev_hp_r;

        let size = params.reverb_size.clamp(0.0, 0.96);
        let damp = params.reverb_damp.clamp(0.0, 1.0);
        let damp_coeff = 0.05 + (1.0 - damp) * 0.40;
        let rev_fb = 0.42 + size * 0.48;
        let mut wet_l = 0.0;
        let mut wet_r = 0.0;
        if params.reverb_enabled {
            for i in 0..4 {
                let pl = self.comb_pos_l[i];
                let out_l = self.combs_l[i][pl];
                self.comb_lp_l[i] += damp_coeff * (out_l - self.comb_lp_l[i]);
                self.combs_l[i][pl] = (clean_rev_in_l * 0.22 + self.comb_lp_l[i] * rev_fb).clamp(-2.0, 2.0);
                self.comb_pos_l[i] = (pl + 1) % self.combs_l[i].len();
                wet_l += out_l;

                let pr = self.comb_pos_r[i];
                let out_r = self.combs_r[i][pr];
                self.comb_lp_r[i] += damp_coeff * (out_r - self.comb_lp_r[i]);
                self.combs_r[i][pr] = (clean_rev_in_r * 0.22 + self.comb_lp_r[i] * rev_fb).clamp(-2.0, 2.0);
                self.comb_pos_r[i] = (pr + 1) % self.combs_r[i].len();
                wet_r += out_r;
            }
            let ap_l0 = self.allpass_l[0].process(wet_l * 0.30);
            wet_l = self.allpass_l[1].process(ap_l0);
            let ap_r0 = self.allpass_r[0].process(wet_r * 0.30);
            wet_r = self.allpass_r[1].process(ap_r0);
        }

        let delay_mix = if params.delay_enabled {
            params.delay_mix.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let reverb_mix = if params.reverb_enabled {
            params.reverb_mix.clamp(0.0, 1.0)
        } else {
            0.0
        };

        let mut xl = in_l + self.delay_tone_l * delay_mix + wet_l * reverb_mix;
        let mut xr = in_r + self.delay_tone_r * delay_mix + wet_r * reverb_mix;

        // 3. Stereo Chop (rhythmic gate)
        let chop = params.chop.clamp(0.0, 1.0);
        if chop > 0.001 {
            self.chop_phase =
                (self.chop_phase + (bpm / 60.0 * (2.0 + chop * 6.0)) / self.sample_rate).fract();
            let gate = if self.chop_phase < 0.5 {
                1.0
            } else {
                1.0 - chop * 0.94
            };
            xl *= gate;
            xr *= gate;
        }

        // 4. Stereo Beat Repeat / Stutter
        let repeat = params.repeat.clamp(0.0, 1.0);
        let repeat_len = (60.0 / bpm.clamp(40.0, 240.0) * 0.5 * self.sample_rate).round() as usize;
        if repeat > 0.001 && !self.repeat_was_active {
            self.repeat_anchor = self.repeat_pos;
            self.repeat_phase = 0;
        }
        let repeat_idx = (self.repeat_anchor + self.repeat_buf_l.len()
            - repeat_len.min(self.repeat_buf_l.len() - 1)
            + self.repeat_phase)
            % self.repeat_buf_l.len();
        let rep_l = self.repeat_buf_l[repeat_idx];
        let rep_r = self.repeat_buf_r[repeat_idx];
        self.repeat_buf_l[self.repeat_pos] = xl;
        self.repeat_buf_r[self.repeat_pos] = xr;
        self.repeat_pos = (self.repeat_pos + 1) % self.repeat_buf_l.len();
        if repeat > 0.001 {
            self.repeat_phase = (self.repeat_phase + 1) % repeat_len.max(1);
            xl = xl * (1.0 - repeat) + rep_l * repeat;
            xr = xr * (1.0 - repeat) + rep_r * repeat;
        } else {
            self.repeat_phase = 0;
        }
        self.repeat_was_active = repeat > 0.001;

        // 5. Stereo Glitch (bitcrush + sample-rate hold)
        let glitch = params.glitch.clamp(0.0, 1.0);
        if glitch > 0.001 {
            let hold_n = 1 + (glitch * 36.0) as usize;
            if self.glitch_counter == 0 {
                let levels = (65535.0 / (1.0 + glitch * 220.0)).max(12.0);
                self.glitch_hold_l = (xl * levels).round() / levels;
                self.glitch_hold_r = (xr * levels).round() / levels;
            }
            self.glitch_counter = (self.glitch_counter + 1) % hold_n;
            xl = xl * (1.0 - glitch) + self.glitch_hold_l * glitch;
            xr = xr * (1.0 - glitch) + self.glitch_hold_r * glitch;
        }

        // 6. Global DJ Filter + Circuit Bend
        let bend = params.circuit_bend.clamp(0.0, 1.0);
        self.bend_phase = (self.bend_phase + (0.17 + bend * 8.0) / self.sample_rate).fract();
        let bend_wave = (self.bend_phase * std::f32::consts::TAU).sin();
        let cutoff = (params.global_filter_cutoff * (1.0 + bend_wave * bend * 0.85))
            .clamp(30.0, self.sample_rate * 0.42);
        let resonance = (params.global_filter_resonance + bend * (0.12 + bend_wave.abs() * 0.24))
            .clamp(0.0, 0.92);
        if params.global_filter_enabled && (cutoff < 17_500.0 || !matches!(params.global_filter_mode, GlobalFilterMode::LowPass) || bend > 0.001) {
            let (lp_l, bp_l, hp_l) = self.filter_l.process_all(xl, cutoff, resonance);
            let (lp_r, bp_r, hp_r) = self.filter_r.process_all(xr, cutoff, resonance);
            match params.global_filter_mode {
                GlobalFilterMode::LowPass => {
                    xl = lp_l;
                    xr = lp_r;
                }
                GlobalFilterMode::HighPass => {
                    xl = hp_l;
                    xr = hp_r;
                }
                GlobalFilterMode::BandPass => {
                    xl = bp_l;
                    xr = bp_r;
                }
            }
        }
        if bend > 0.001 {
            let fold_l = (xl * (1.0 + bend * 9.0)).sin();
            let fold_r = (xr * (1.0 + bend * 9.0)).sin();
            xl = xl * (1.0 - bend * 0.45) + fold_l * bend * 0.45;
            xr = xr * (1.0 - bend * 0.45) + fold_r * bend * 0.45;
        }

        // 7. Modern Crisp Mastering Polish + Analog Tape Warmth + Transparent Soft-Knee Limiter
        self.warmth += 0.002 * (params.analog_warmth.clamp(0.0, 1.0) - self.warmth);
        self.crisp += 0.002 * (params.master_crisp.clamp(0.0, 1.0) - self.crisp);

        let mid_in = (xl + xr) * 0.5;
        let side_in = (xl - xr) * 0.5;
        let side_hp = self.side_sub_hp.process_hp(side_in, 140.0, 0.013);
        let crisp = self.crisp;
        if crisp > 0.001 {
            // a) Elliptical M/S sub mono-maker: 12 dB/oct Butterworth HPF on Side (< 140 Hz)
            let mono_amt = (crisp * 1.05).clamp(0.0, 0.96);
            let tight_side = side_in * (1.0 - mono_amt) + side_hp * mono_amt;
            xl = mid_in + tight_side;
            xr = mid_in - tight_side;
        }

        let sr = self.sample_rate;
        let sub_a = (std::f32::consts::TAU * 125.0 / sr).clamp(0.001, 0.25);
        let mud_a = (std::f32::consts::TAU * 290.0 / sr).clamp(0.002, 0.35);
        let pres_a = (std::f32::consts::TAU * 3_600.0 / sr).clamp(0.05, 0.72);
        let air_a = (std::f32::consts::TAU * 9_000.0 / sr).clamp(0.10, 0.85);

        self.sub_lp_l += sub_a * (xl - self.sub_lp_l);
        self.sub_lp_r += sub_a * (xr - self.sub_lp_r);
        self.mud_lp_l += mud_a * (xl - self.mud_lp_l);
        self.mud_lp_r += mud_a * (xr - self.mud_lp_r);
        self.pres_lp_l += pres_a * (xl - self.pres_lp_l);
        self.pres_lp_r += pres_a * (xr - self.pres_lp_r);
        self.air_lp_l += air_a * (xl - self.air_lp_l);
        self.air_lp_r += air_a * (xr - self.air_lp_r);

        if crisp > 0.001 {
            // b) Gentle low-mid mud carve (~125..290 Hz)
            let mud_l = self.mud_lp_l - self.sub_lp_l;
            let mud_r = self.mud_lp_r - self.sub_lp_r;
            let mud_cut = -0.16 * crisp;

            // c) Crisp presence (> 3.6 kHz) & silky soft-saturated air (> 9 kHz) + stereo air sheen
            let pres_l = xl - self.pres_lp_l;
            let pres_r = xr - self.pres_lp_r;
            let air_l = super::soft_tanh((xl - self.air_lp_l) * 1.45) / 1.45;
            let air_r = super::soft_tanh((xr - self.air_lp_r) * 1.45) / 1.45;
            let air_mid = (air_l + air_r) * 0.5;
            let air_side = (air_l - air_r) * 0.5 * (1.0 + crisp * 0.28);
            let air_wide_l = air_mid + air_side;
            let air_wide_r = air_mid - air_side;

            xl += mud_l * mud_cut + pres_l * (0.18 * crisp) + air_wide_l * (0.38 * crisp);
            xr += mud_r * mud_cut + pres_r * (0.18 * crisp) + air_wide_r * (0.38 * crisp);
        }

        // Transparent soft-knee punch limiter blended cleanly with optional tape warmth
        let warm_amt = self.warmth;
        let tape_drive = 1.0 + warm_amt * 1.15;
        let tape_l = (xl * tape_drive).tanh() * 0.94;
        let tape_r = (xr * tape_drive).tanh() * 0.94;

        let punch_gain = 1.0 + crisp * 0.14;
        let clean_l = soft_knee_limit(xl * punch_gain);
        let clean_r = soft_knee_limit(xr * punch_gain);

        let tape_blend = (warm_amt * (1.0 - crisp * 0.45)).clamp(0.0, 1.0);
        let out_l = clean_l * (1.0 - tape_blend) + tape_l * tape_blend;
        let out_r = clean_r * (1.0 - tape_blend) + tape_r * tape_blend;
        (out_l, out_r)
    }

    /// Backwards-compatible mono wrapper if ever needed.
    pub fn process(
        &mut self,
        input: f32,
        delay_input: f32,
        reverb_input: f32,
        params: &FxParams,
        bpm: f32,
    ) -> f32 {
        let (l, r) = self.process_stereo(
            input,
            input,
            delay_input,
            delay_input,
            reverb_input,
            reverb_input,
            params,
            bpm,
        );
        (l + r) * 0.5
    }
}

#[inline]
fn soft_knee_limit(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let ax = x.abs();
    const KNEE: f32 = 0.68;
    const HEADROOM: f32 = 0.28; // max ceiling = 0.96
    if ax <= KNEE {
        x
    } else {
        let excess = (ax - KNEE) / HEADROOM;
        x.signum() * (KNEE + HEADROOM * excess.tanh())
    }
}
