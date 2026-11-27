use std::f32::consts::TAU;

use crate::model::{FxParams, GlobalFilterMode};

use super::{
    one_pole_coeff, read_ring_hermite, soft_ceiling, soft_tanh, Biquad, DelayAllpass,
    HalfBandDecim2x, HalfBandUp2x,
};

const FDN_MS: [f32; 8] = [29.7, 37.1, 43.9, 53.3, 61.7, 73.1, 83.9, 97.3];
const FDN_LFO_HZ: [f32; 8] = [0.29, 0.34, 0.41, 0.46, 0.52, 0.58, 0.63, 0.69];

/// Unitary 8x8 Fast Walsh-Hadamard Transform (orthogonal energy-preserving FDN matrix).
#[inline]
fn fwht8(x: &mut [f32; 8]) {
    for i in (0..8).step_by(2) {
        let a = x[i];
        let b = x[i + 1];
        x[i] = a + b;
        x[i + 1] = a - b;
    }
    for i in (0..8).step_by(4) {
        for j in 0..2 {
            let a = x[i + j];
            let b = x[i + j + 2];
            x[i + j] = a + b;
            x[i + j + 2] = a - b;
        }
    }
    for j in 0..4 {
        let a = x[j];
        let b = x[j + 4];
        x[j] = (a + b) * 0.35355338;
        x[j + 4] = (a - b) * 0.35355338;
    }
}

/// True-stereo tempo-synced ping-pong delay, 4-stage allpass-diffused 8-channel
/// modulated Hadamard FDN studio reverb with pre-delay, stereo performance FX,
/// global SVF filter, Biquad mastering EQ, band-limited bass harmonic exciter,
/// SSL-style soft-knee bus glue, 2x-oversampled tape warmth, and 2.5ms lookahead limiter.
/// Kept allocation-free in the audio callback.
#[derive(Debug, Clone)]
pub struct MasterFx {
    sample_rate: f32,
    delay_l: Vec<f32>,
    delay_r: Vec<f32>,
    delay_pos: usize,
    delay_len_smooth: f32,
    delay_tone_l: f32,
    delay_tone_r: f32,
    del_hp_l: f32,
    del_hp_r: f32,
    delay_wow_phase: f32,
    pre_delay_l: Vec<f32>,
    pre_delay_r: Vec<f32>,
    pre_delay_pos: usize,
    rev_er_lp_l: f32,
    rev_er_lp_r: f32,
    rev_in_hp_l: Biquad,
    rev_in_hp_r: Biquad,
    rev_in_lp_l: f32,
    rev_in_lp_r: f32,
    rev_ap_l: [DelayAllpass; 4],
    rev_ap_r: [DelayAllpass; 4],
    fdn_lines: [Vec<f32>; 8],
    fdn_pos: [usize; 8],
    fdn_base: [f32; 8],
    fdn_lp: [f32; 8],
    fdn_hp: [f32; 8],
    fdn_lfo_phase: [f32; 8],
    warmth: f32,
    crisp: f32,
    punch: f32,
    subsonic_l: Biquad,
    subsonic_r: Biquad,
    side_sub_hp: Biquad,
    side_width_hp: f32,
    club_sub_shelf: Biquad,
    club_exc_in_bp: Biquad,
    club_exc_out_bp: Biquad,
    eq_mud_l: Biquad,
    eq_mud_r: Biquad,
    eq_pres_l: Biquad,
    eq_pres_r: Biquad,
    eq_air_l: Biquad,
    eq_air_r: Biquad,
    ultra_lp_l: Biquad,
    ultra_lp_r: Biquad,
    last_eq_punch: f32,
    last_eq_crisp: f32,
    comp_sc_hp: f32,
    comp_sc_hp_r: f32,
    comp_env: f32,
    wet_duck: f32,
    tape_up_l: HalfBandUp2x,
    tape_up_r: HalfBandUp2x,
    tape_decim_l: HalfBandDecim2x,
    tape_decim_r: HalfBandDecim2x,
    tape_dc_l: f32,
    tape_dc_r: f32,
    tape_lp_l: f32,
    tape_lp_r: f32,
    lim_buf_l: Vec<f32>,
    lim_buf_r: Vec<f32>,
    lim_peak_ring: Vec<f32>,
    lim_pos: usize,
    lim_window_max: f32,
    lim_gain_smooth: f32,
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
    shimmer_buf_l: Vec<f32>,
    shimmer_buf_r: Vec<f32>,
    shimmer_pos: usize,
    shimmer_grain: f32,
    shimmer_hp_l: f32,
    shimmer_hp_r: f32,
    shimmer_lp_l: f32,
    shimmer_lp_r: f32,
    shimmer_fb_l: f32,
    shimmer_fb_r: f32,
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
        let mod_pad = ((0.0020 * sr).ceil() as usize).max(16);
        let fdn_base = FDN_MS.map(|ms| (ms * 0.001 * sr).max(8.0));
        let fdn_lines = fdn_base.map(|b| vec![0.0; (b.ceil() as usize) + mod_pad]);

        let pre_cap = ((0.048 * sr).ceil() as usize).max(32);
        let delay_cap = (sr * 3.0) as usize;
        let repeat_cap = (sr * 2.0) as usize;
        let la_len = ((0.0025 * sr).round() as usize).max(8);
        let shim_len = ((0.08 * sr).ceil() as usize).max(1024);

        let mut rev_in_hp_l = Biquad::new();
        let mut rev_in_hp_r = Biquad::new();
        rev_in_hp_l.set_highpass(sr, 180.0, 0.7071);
        rev_in_hp_r.set_highpass(sr, 180.0, 0.7071);

        let mut subsonic_l = Biquad::new();
        let mut subsonic_r = Biquad::new();
        subsonic_l.set_highpass(sr, 28.0, 0.7071);
        subsonic_r.set_highpass(sr, 28.0, 0.7071);

        let mut side_sub_hp = Biquad::new();
        side_sub_hp.set_highpass(sr, 115.0, 0.7071);

        let mut club_sub_shelf = Biquad::new();
        club_sub_shelf.set_low_shelf(sr, 58.0, 0.72, 0.0);

        let mut club_exc_in_bp = Biquad::new();
        club_exc_in_bp.set_bandpass(sr, 85.0, 0.9);
        let mut club_exc_out_bp = Biquad::new();
        club_exc_out_bp.set_bandpass(sr, 195.0, 0.85);

        let mut ultra_lp_l = Biquad::new();
        let mut ultra_lp_r = Biquad::new();
        ultra_lp_l.set_lowpass(sr, 17_500.0, 0.65);
        ultra_lp_r.set_lowpass(sr, 17_500.0, 0.65);

        Self {
            sample_rate: sr,
            delay_l: vec![0.0; delay_cap],
            delay_r: vec![0.0; delay_cap],
            delay_pos: 0,
            delay_len_smooth: (0.375 * sr).round().max(2.0),
            delay_tone_l: 0.0,
            delay_tone_r: 0.0,
            del_hp_l: 0.0,
            del_hp_r: 0.0,
            delay_wow_phase: 0.0,
            pre_delay_l: vec![0.0; pre_cap],
            pre_delay_r: vec![0.0; pre_cap],
            pre_delay_pos: 0,
            rev_er_lp_l: 0.0,
            rev_er_lp_r: 0.0,
            rev_in_hp_l,
            rev_in_hp_r,
            rev_in_lp_l: 0.0,
            rev_in_lp_r: 0.0,
            rev_ap_l: [
                DelayAllpass::new((0.0029 * sr).round() as usize),
                DelayAllpass::new((0.0043 * sr).round() as usize),
                DelayAllpass::new((0.0067 * sr).round() as usize),
                DelayAllpass::new((0.0097 * sr).round() as usize),
            ],
            rev_ap_r: [
                DelayAllpass::new((0.0033 * sr).round() as usize),
                DelayAllpass::new((0.0049 * sr).round() as usize),
                DelayAllpass::new((0.0073 * sr).round() as usize),
                DelayAllpass::new((0.0103 * sr).round() as usize),
            ],
            fdn_lines,
            fdn_pos: [0; 8],
            fdn_base,
            fdn_lp: [0.0; 8],
            fdn_hp: [0.0; 8],
            fdn_lfo_phase: [0.05, 0.17, 0.29, 0.43, 0.57, 0.69, 0.81, 0.93],
            warmth: 0.0,
            crisp: 0.0,
            punch: 0.52,
            subsonic_l,
            subsonic_r,
            side_sub_hp,
            side_width_hp: 0.0,
            club_sub_shelf,
            club_exc_in_bp,
            club_exc_out_bp,
            eq_mud_l: Biquad::new(),
            eq_mud_r: Biquad::new(),
            eq_pres_l: Biquad::new(),
            eq_pres_r: Biquad::new(),
            eq_air_l: Biquad::new(),
            eq_air_r: Biquad::new(),
            ultra_lp_l,
            ultra_lp_r,
            last_eq_punch: -1.0,
            last_eq_crisp: -1.0,
            comp_sc_hp: 0.0,
            comp_sc_hp_r: 0.0,
            comp_env: 0.0,
            wet_duck: 1.0,
            tape_up_l: HalfBandUp2x::new(),
            tape_up_r: HalfBandUp2x::new(),
            tape_decim_l: HalfBandDecim2x::new(),
            tape_decim_r: HalfBandDecim2x::new(),
            tape_dc_l: 0.0,
            tape_dc_r: 0.0,
            tape_lp_l: 0.0,
            tape_lp_r: 0.0,
            lim_buf_l: vec![0.0; la_len],
            lim_buf_r: vec![0.0; la_len],
            lim_peak_ring: vec![0.0; la_len],
            lim_pos: 0,
            lim_window_max: 0.0,
            lim_gain_smooth: 1.0,
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
            shimmer_buf_l: vec![0.0; shim_len],
            shimmer_buf_r: vec![0.0; shim_len],
            shimmer_pos: 0,
            shimmer_grain: 0.0,
            shimmer_hp_l: 0.0,
            shimmer_hp_r: 0.0,
            shimmer_lp_l: 0.0,
            shimmer_lp_r: 0.0,
            shimmer_fb_l: 0.0,
            shimmer_fb_r: 0.0,
            delay_active: true,
            reverb_active: true,
        }
    }

    #[inline]
    pub fn set_wet_duck(&mut self, duck: f32) {
        self.wet_duck = duck.clamp(0.4, 1.0);
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
            self.pre_delay_l.fill(0.0);
            self.pre_delay_r.fill(0.0);
            for line in &mut self.fdn_lines {
                line.fill(0.0);
            }
            for ap in &mut self.rev_ap_l {
                ap.clear();
            }
            for ap in &mut self.rev_ap_r {
                ap.clear();
            }
            self.fdn_lp = [0.0; 8];
            self.fdn_hp = [0.0; 8];
            self.rev_in_hp_l.reset();
            self.rev_in_hp_r.reset();
            self.rev_in_lp_l = 0.0;
            self.rev_in_lp_r = 0.0;
            self.rev_er_lp_l = 0.0;
            self.rev_er_lp_r = 0.0;
            self.shimmer_buf_l.fill(0.0);
            self.shimmer_buf_r.fill(0.0);
            self.shimmer_pos = 0;
            self.shimmer_hp_l = 0.0;
            self.shimmer_hp_r = 0.0;
            self.shimmer_lp_l = 0.0;
            self.shimmer_lp_r = 0.0;
            self.shimmer_fb_l = 0.0;
            self.shimmer_fb_r = 0.0;
        }
        self.delay_active = params.delay_enabled;
        self.reverb_active = params.reverb_enabled;

        // 1. Tempo-synced stereo ping-pong tape echo (quadrature wow/flutter + Hermite tap + warm feedback)
        let del_hp_alpha = one_pole_coeff(170.0, self.sample_rate);
        self.del_hp_l += del_hp_alpha * (delay_in_l - self.del_hp_l);
        self.del_hp_r += del_hp_alpha * (delay_in_r - self.del_hp_r);
        let clean_del_in_l = delay_in_l - self.del_hp_l;
        let clean_del_in_r = delay_in_r - self.del_hp_r;

        let target_delay_len = (60.0 / bpm.clamp(40.0, 240.0)
            * params.delay_div.sixteenth_multiplier()
            / 4.0
            * self.sample_rate)
            .clamp(2.0, (self.delay_l.len() - 4) as f32);
        self.delay_len_smooth +=
            one_pole_coeff(12.0, self.sample_rate) * (target_delay_len - self.delay_len_smooth);
        self.delay_wow_phase = (self.delay_wow_phase + 0.38 / self.sample_rate).fract();
        let wow_rad = self.delay_wow_phase * TAU;
        let wow_amp = 0.00026 * self.sample_rate;
        let wow_l = wow_rad.sin() * wow_amp + (wow_rad * 7.0).sin() * (wow_amp * 0.18);
        let wow_r = wow_rad.cos() * wow_amp - (wow_rad * 7.0).cos() * (wow_amp * 0.18);
        let tap_l_d = (self.delay_len_smooth + wow_l).clamp(2.0, (self.delay_l.len() - 4) as f32);
        let tap_r_d = (self.delay_len_smooth * 0.75 + wow_r).clamp(2.0, (self.delay_r.len() - 4) as f32);
        let del_l = read_ring_hermite(&self.delay_l, self.delay_pos, tap_l_d);
        let del_r = read_ring_hermite(&self.delay_r, self.delay_pos, tap_r_d);

        let tone = params.delay_tone.clamp(0.0, 1.0);
        let lp_hz = 900.0 * (10_500.0_f32 / 900.0).powf(tone);
        let lp_coeff = one_pole_coeff(lp_hz, self.sample_rate);
        self.delay_tone_l += lp_coeff * (del_l - self.delay_tone_l);
        self.delay_tone_r += lp_coeff * (del_r - self.delay_tone_r);

        let fb = params.delay_feedback.clamp(0.0, 0.88);
        if params.delay_enabled {
            let fb_l = 1.8 * soft_tanh((clean_del_in_l + self.delay_tone_r * fb) / 1.8);
            let fb_r = 1.8 * soft_tanh((clean_del_in_r + self.delay_tone_l * fb) / 1.8);
            self.delay_l[self.delay_pos] = (fb_l * 0.99).clamp(-2.5, 2.5);
            self.delay_r[self.delay_pos] = (fb_r * 0.99).clamp(-2.5, 2.5);
        } else {
            self.delay_l[self.delay_pos] = 0.0;
            self.delay_r[self.delay_pos] = 0.0;
        }
        self.delay_pos = (self.delay_pos + 1) % self.delay_l.len();

        // 2. Stereo Early Reflections + 4-Stage Allpass Diffused 8-Channel Hadamard FDN Studio Reverb
        let rev_in_l_hp = self.rev_in_hp_l.process(reverb_in_l);
        let rev_in_r_hp = self.rev_in_hp_r.process(reverb_in_r);
        let rev_lp_a = one_pole_coeff(11_500.0, self.sample_rate);
        self.rev_in_lp_l += rev_lp_a * (rev_in_l_hp - self.rev_in_lp_l);
        self.rev_in_lp_r += rev_lp_a * (rev_in_r_hp - self.rev_in_lp_r);

        let pd_cap = self.pre_delay_l.len();
        self.pre_delay_l[self.pre_delay_pos] = self.rev_in_lp_l;
        self.pre_delay_r[self.pre_delay_pos] = self.rev_in_lp_r;
        let main_pd_samp = ((0.022 * self.sample_rate).round() as usize).clamp(1, pd_cap - 1);
        let pd_idx = (self.pre_delay_pos + pd_cap - main_pd_samp) % pd_cap;
        let pd_l = self.pre_delay_l[pd_idx];
        let pd_r = self.pre_delay_r[pd_idx];

        let size = params.reverb_size.clamp(0.0, 0.96);
        let damp = params.reverb_damp.clamp(0.0, 1.0);
        let rt60_s = 0.55 + size * 3.8;
        let damp_hz = 12_500.0 * (1_600.0_f32 / 12_500.0).powf(damp);
        let damp_coeff = one_pole_coeff(damp_hz, self.sample_rate);
        let tank_hp_a = one_pole_coeff(85.0, self.sample_rate);
        let mod_amp_samp = 0.00046 * self.sample_rate;
        let mut wet_l = 0.0;
        let mut wet_r = 0.0;
        if params.reverb_enabled {
            // 6-tap asymmetric stereo studio Early Reflections (7.3..24.7 ms) for front-to-back 3D depth
            let tap_pd = |buf: &[f32], ms: f32, pos: usize| -> f32 {
                let s = ((ms * 0.001 * self.sample_rate).round() as usize).clamp(1, buf.len() - 1);
                buf[(pos + buf.len() - s) % buf.len()]
            };
            let er_raw_l = tap_pd(&self.pre_delay_l, 7.3, self.pre_delay_pos) * 0.36
                + tap_pd(&self.pre_delay_l, 13.9, self.pre_delay_pos) * 0.26
                + tap_pd(&self.pre_delay_l, 21.1, self.pre_delay_pos) * 0.18
                + tap_pd(&self.pre_delay_r, 16.3, self.pre_delay_pos) * 0.12;
            let er_raw_r = tap_pd(&self.pre_delay_r, 8.9, self.pre_delay_pos) * 0.36
                + tap_pd(&self.pre_delay_r, 16.3, self.pre_delay_pos) * 0.26
                + tap_pd(&self.pre_delay_r, 24.7, self.pre_delay_pos) * 0.18
                + tap_pd(&self.pre_delay_l, 13.9, self.pre_delay_pos) * 0.12;
            let er_lp_a = one_pole_coeff(5800.0, self.sample_rate);
            self.rev_er_lp_l += er_lp_a * (er_raw_l - self.rev_er_lp_l);
            self.rev_er_lp_r += er_lp_a * (er_raw_r - self.rev_er_lp_r);

            let mut diff_l = pd_l;
            let mut diff_r = pd_r;
            for ap in &mut self.rev_ap_l {
                diff_l = ap.process(diff_l, 0.58);
            }
            for ap in &mut self.rev_ap_r {
                diff_r = ap.process(diff_r, 0.58);
            }

            let mut y = [0.0_f32; 8];
            for i in 0..8 {
                self.fdn_lfo_phase[i] =
                    (self.fdn_lfo_phase[i] + FDN_LFO_HZ[i] / self.sample_rate).fract();
                let lfo = (self.fdn_lfo_phase[i] * TAU).sin() * mod_amp_samp;
                let len = self.fdn_lines[i].len();
                let d = (self.fdn_base[i] + lfo).clamp(2.0, (len - 3) as f32);
                let tap = read_ring_hermite(&self.fdn_lines[i], self.fdn_pos[i], d);
                self.fdn_lp[i] += damp_coeff * (tap - self.fdn_lp[i]);
                self.fdn_hp[i] += tank_hp_a * (self.fdn_lp[i] - self.fdn_hp[i]);
                y[i] = self.fdn_lp[i] - self.fdn_hp[i];
            }
            // Balanced decorrelated stereo FDN tank + 3D Early Reflections + Octave-Up Shimmer Tail
            let late_l = (y[0] + y[2] - y[4] - y[6] + 0.35 * (y[1] - y[5])) * 0.46;
            let late_r = (y[1] + y[3] - y[5] - y[7] + 0.35 * (y[2] - y[6])) * 0.46;

            let shimmer_amt = params.reverb_shimmer.clamp(0.0, 1.0);
            let (shim_out_l, shim_out_r) = if shimmer_amt > 0.002 {
                let s_sr = self.sample_rate;
                let s_len = self.shimmer_buf_l.len();
                self.shimmer_buf_l[self.shimmer_pos] = late_l;
                self.shimmer_buf_r[self.shimmer_pos] = late_r;

                let win_len = (0.055 * s_sr).clamp(256.0, (s_len - 16) as f32);
                let pa = self.shimmer_grain;
                let pb = (pa + 0.5).fract();
                let da = (1.0 - pa) * win_len + 4.0;
                let db = (1.0 - pb) * win_len + 4.0;
                let wa = 0.5 - 0.5 * (pa * TAU).cos();
                let wb = 0.5 - 0.5 * (pb * TAU).cos();

                let raw_sl = read_ring_hermite(&self.shimmer_buf_l, self.shimmer_pos, da) * wa
                    + read_ring_hermite(&self.shimmer_buf_l, self.shimmer_pos, db) * wb;
                let raw_sr = read_ring_hermite(&self.shimmer_buf_r, self.shimmer_pos, da) * wa
                    + read_ring_hermite(&self.shimmer_buf_r, self.shimmer_pos, db) * wb;

                self.shimmer_pos = (self.shimmer_pos + 1) % s_len;
                self.shimmer_grain = (pa + 1.0 / win_len).fract();

                let sh_hp = one_pole_coeff(350.0, s_sr);
                let sh_lp = one_pole_coeff(6_200.0, s_sr);
                self.shimmer_hp_l += sh_hp * (raw_sl - self.shimmer_hp_l);
                self.shimmer_hp_r += sh_hp * (raw_sr - self.shimmer_hp_r);
                let hpl = raw_sl - self.shimmer_hp_l;
                let hpr = raw_sr - self.shimmer_hp_r;
                self.shimmer_lp_l += sh_lp * (hpl - self.shimmer_lp_l);
                self.shimmer_lp_r += sh_lp * (hpr - self.shimmer_lp_r);

                self.shimmer_fb_l = soft_tanh(self.shimmer_lp_l * 1.15);
                self.shimmer_fb_r = soft_tanh(self.shimmer_lp_r * 1.15);
                (self.shimmer_lp_l, self.shimmer_lp_r)
            } else {
                self.shimmer_fb_l = 0.0;
                self.shimmer_fb_r = 0.0;
                (0.0, 0.0)
            };

            wet_l = late_l + self.rev_er_lp_l * 0.24 + shim_out_l * (shimmer_amt * 0.42);
            wet_r = late_r + self.rev_er_lp_r * 0.24 + shim_out_r * (shimmer_amt * 0.42);

            let mut mix = y;
            fwht8(&mut mix);
            let inj_l = diff_l + self.shimmer_fb_l * (shimmer_amt * 0.26);
            let inj_r = diff_r + self.shimmer_fb_r * (shimmer_amt * 0.26);
            let in_mid = (inj_l + inj_r) * 0.5;
            let in_side = (inj_l - inj_r) * 0.5;
            const MID_SIGNS: [f32; 8] = [1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, -1.0];
            const SIDE_SIGNS: [f32; 8] = [1.0, -1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0];
            for i in 0..8 {
                let len = self.fdn_lines[i].len();
                let p = self.fdn_pos[i];
                let g_i = 10.0_f32
                    .powf(-3.0 * (FDN_MS[i] * 0.001) / rt60_s)
                    .clamp(0.30, 0.985);
                let inject = in_mid * 0.38 * MID_SIGNS[i] + in_side * 0.34 * SIDE_SIGNS[i];
                self.fdn_lines[i][p] = 2.0 * soft_tanh((inject + mix[i] * g_i) / 2.0);
                self.fdn_pos[i] = (p + 1) % len;
            }
        }
        self.pre_delay_pos = (self.pre_delay_pos + 1) % pd_cap;

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

        let wd = self.wet_duck.clamp(0.4, 1.0);
        let mut xl = in_l + (self.delay_tone_l * delay_mix + wet_l * reverb_mix) * wd;
        let mut xr = in_r + (self.delay_tone_r * delay_mix + wet_r * reverb_mix) * wd;

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

        // 7. True Studio Mastering Chain: 28Hz 12dB Subsonic HPF -> Elliptical M/S & Width Polish ->
        //    Biquad Sub Shelf & Band-Limited Bass Exciter -> Biquad Clarity/Air EQ ->
        //    SSL Soft-Knee Bus Glue -> 2x-Oversampled Tape Warmth -> 2.5ms Lookahead Limiter
        let sr = self.sample_rate;
        let param_slew = one_pole_coeff(35.0, sr);
        self.warmth += param_slew * (params.analog_warmth.clamp(0.0, 1.0) - self.warmth);
        self.crisp += param_slew * (params.master_crisp.clamp(0.0, 1.0) - self.crisp);
        self.punch += param_slew * (params.club_punch.clamp(0.0, 1.0) - self.punch);

        let punch = self.punch;
        let crisp = self.crisp;
        let warm_amt = self.warmth;

        if (punch - self.last_eq_punch).abs() > 0.0005 {
            self.last_eq_punch = punch;
            self.club_sub_shelf.set_low_shelf(sr, 58.0, 0.72, punch * 1.6);
        }
        if (crisp - self.last_eq_crisp).abs() > 0.0005 {
            self.last_eq_crisp = crisp;
            let mud_db = -1.8 * crisp;
            let pres_db = 1.6 * crisp;
            let air_db = 2.0 * crisp;
            self.eq_mud_l.set_peaking(sr, 235.0, 0.95, mud_db);
            self.eq_mud_r.set_peaking(sr, 235.0, 0.95, mud_db);
            self.eq_pres_l.set_peaking(sr, 3_000.0, 0.75, pres_db);
            self.eq_pres_r.set_peaking(sr, 3_000.0, 0.75, pres_db);
            self.eq_air_l.set_peaking(sr, 10_200.0, 0.62, air_db);
            self.eq_air_r.set_peaking(sr, 10_200.0, 0.62, air_db);
        }

        // 7a) 28 Hz 12dB/oct Butterworth subsonic & DC cleanup HPF
        xl = self.subsonic_l.process(xl);
        xr = self.subsonic_r.process(xr);

        // 7b) Elliptical M/S sub mono-maker + upper stereo stage + sculpted bass shelf/exciter
        let mut mid_p = (xl + xr) * 0.5;
        let side_in = (xl - xr) * 0.5;
        let side_hp = self.side_sub_hp.process(side_in);
        let mono_amt = (0.25 + crisp * 0.72).clamp(0.0, 0.97);
        let tight_side = side_in * (1.0 - mono_amt) + side_hp * mono_amt;

        let width_a = one_pole_coeff(260.0, sr);
        self.side_width_hp += width_a * (tight_side - self.side_width_hp);
        let upper_side = tight_side - self.side_width_hp;
        let low_side = self.side_width_hp;
        let final_side = low_side + upper_side * (1.12 + crisp * 0.24);

        if punch > 0.001 {
            let shelf_m = self.club_sub_shelf.process(mid_p);
            let b = self.club_exc_in_bp.process(mid_p);
            let bc = b.clamp(-1.5, 1.5);
            let raw_h = soft_tanh(b * 2.0 + 0.20 * bc * bc) - b * 2.0;
            let h_clean = self.club_exc_out_bp.process(raw_h);
            mid_p = shelf_m + h_clean * (0.28 * punch);
        }
        xl = mid_p + final_side;
        xr = mid_p - final_side;

        // 7c) Linear Musical Biquad Polish (Mud Carve, Presence Lift, Studio Air Bell + 17.5kHz Silk LPF)
        if crisp > 0.001 {
            xl = self.eq_air_l.process(self.eq_pres_l.process(self.eq_mud_l.process(xl)));
            xr = self.eq_air_r.process(self.eq_pres_r.process(self.eq_mud_r.process(xr)));
        }
        xl = self.ultra_lp_l.process(xl);
        xr = self.ultra_lp_r.process(xr);

        // 7d) SSL G-Bus Style Soft-Knee Linked Stereo Glue Compressor (24 ms attack / 95 ms release)
        if punch > 0.001 {
            let sc_hp_a = one_pole_coeff(120.0, sr);
            self.comp_sc_hp += sc_hp_a * (xl - self.comp_sc_hp);
            self.comp_sc_hp_r += sc_hp_a * (xr - self.comp_sc_hp_r);
            let sc_det = (xl - self.comp_sc_hp).abs().max((xr - self.comp_sc_hp_r).abs());
            let env_c = if sc_det > self.comp_env {
                1.0 - (-1.0 / (0.024 * sr)).exp()
            } else {
                1.0 - (-1.0 / (0.095 * sr)).exp()
            };
            self.comp_env += env_c * (sc_det - self.comp_env);

            let env_db = 20.0 * self.comp_env.max(1e-6).log10();
            let thresh_db = -8.0_f32;
            let knee_db = 4.0_f32;
            let ratio = 1.0 + punch * 1.5;
            let slope = 1.0 - 1.0 / ratio;
            let over_db = env_db - thresh_db;
            let gr_db = if over_db <= -knee_db * 0.5 {
                0.0
            } else if over_db < knee_db * 0.5 {
                let x = over_db + knee_db * 0.5;
                -slope * (x * x) / (2.0 * knee_db)
            } else {
                -slope * over_db
            };
            let gr = 10.0_f32.powf(gr_db / 20.0);
            let makeup = 1.0 + punch * 0.10;
            let glue = gr * makeup;
            xl *= glue;
            xr *= glue;
        }

        // 7e) Half-Band 2x-Oversampled Analog Console / Tape Saturation (phase-coherent, before limiter)
        let up_l = self.tape_up_l.process(xl);
        let up_r = self.tape_up_r.process(xr);
        if warm_amt > 0.001 {
            let drive = 1.0 + warm_amt * 0.75;
            let norm = 1.0 + warm_amt * 0.35;
            let asym = 0.04 * warm_amt;
            let sat_step = |x: f32| -> f32 {
                let xc = x.clamp(-2.0, 2.0);
                let s = 1.45 * soft_tanh((x * drive + asym * xc * xc) / 1.45) / norm;
                x * (1.0 - warm_amt) + s * warm_amt
            };
            let tl = self.tape_decim_l.process(sat_step(up_l[0]), sat_step(up_l[1]));
            let tr = self.tape_decim_r.process(sat_step(up_r[0]), sat_step(up_r[1]));

            let dc_a = one_pole_coeff(20.0, sr);
            self.tape_dc_l += dc_a * (tl - self.tape_dc_l);
            self.tape_dc_r += dc_a * (tr - self.tape_dc_r);
            let clean_tl = tl - self.tape_dc_l;
            let clean_tr = tr - self.tape_dc_r;

            let tape_lp_a = one_pole_coeff(14_500.0, sr);
            self.tape_lp_l += tape_lp_a * (clean_tl - self.tape_lp_l);
            self.tape_lp_r += tape_lp_a * (clean_tr - self.tape_lp_r);
            let top_polish = warm_amt * 0.16;
            xl = clean_tl * (1.0 - top_polish) + self.tape_lp_l * top_polish;
            xr = clean_tr * (1.0 - top_polish) + self.tape_lp_r * top_polish;
        } else {
            let _ = self.tape_decim_l.process(up_l[0], up_l[1]);
            let _ = self.tape_decim_r.process(up_r[0], up_r[1]);
        }

        // 7f) True Studio 2.5 ms Lookahead Peak Limiter
        let punch_gain = 1.10 + crisp * 0.04 + punch * 0.06;
        let pre_l = xl * punch_gain;
        let pre_r = xr * punch_gain;

        let del_l = self.lim_buf_l[self.lim_pos];
        let del_r = self.lim_buf_r[self.lim_pos];
        self.lim_buf_l[self.lim_pos] = pre_l;
        self.lim_buf_r[self.lim_pos] = pre_r;

        let old_p = self.lim_peak_ring[self.lim_pos];
        let cur_p = pre_l.abs().max(pre_r.abs());
        self.lim_peak_ring[self.lim_pos] = cur_p;
        self.lim_pos = (self.lim_pos + 1) % self.lim_peak_ring.len();

        if cur_p >= self.lim_window_max {
            self.lim_window_max = cur_p;
        } else if old_p >= self.lim_window_max - 1e-6 {
            let mut m = 0.0_f32;
            for &v in &self.lim_peak_ring {
                if v > m {
                    m = v;
                }
            }
            self.lim_window_max = m;
        }

        let target_g = if self.lim_window_max > 0.91 {
            (0.91 / self.lim_window_max).clamp(0.15, 1.0)
        } else {
            1.0
        };
        let lim_c = if target_g < self.lim_gain_smooth {
            one_pole_coeff(280.0, sr)
        } else {
            one_pole_coeff(4.5, sr)
        };
        self.lim_gain_smooth += lim_c * (target_g - self.lim_gain_smooth);

        let out_l = soft_ceiling(del_l * self.lim_gain_smooth, 0.91, 0.05).clamp(-0.96, 0.96);
        let out_r = soft_ceiling(del_r * self.lim_gain_smooth, 0.91, 0.05).clamp(-0.96, 0.96);
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
