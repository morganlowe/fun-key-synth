pub mod drums;
pub mod fm;
pub mod fx;
pub mod monologue;
pub mod moog;
pub mod multisample;
pub mod sampler;
pub mod soundfont;
pub mod tb303;
pub mod wavetable;

use std::f32::consts::PI;

pub use drums::DrumMachine;
pub use fm::FmSynth;
pub use fx::MasterFx;
pub use monologue::MonologueSynth;
pub use moog::MoogSynth;
pub use multisample::{
    philharmonia_bank_loaded_count, philharmonia_bank_status, preload_philharmonia_bank_async,
    MultiSampleSynth, NUM_PHILHARMONIA_BANKS, PHILHARMONIA_PERC_LABELS,
};
pub use sampler::{
    generate_builtin_sample, generate_builtin_sample_stereo, load_audio_to_stereo_f32,
    load_wav_to_mono_f32, load_wav_to_stereo_f32, Sk1Sampler,
};
pub use soundfont::{SoundFontBank, SoundFontSynth};
pub use tb303::Tb303Synth;
pub use wavetable::WavetableSynth;

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

/// Non-periodic Ornstein-Uhlenbeck / dual-pole thermal analog drift generator.
/// Produces organic, non-repeating slow wander (`[-1.0, 1.0]`) for VCO pitch, filter cutoff, and PWM.
#[derive(Debug, Clone)]
pub struct AnalogDrift {
    rng: Xorshift32,
    slow: f32,
    fast: f32,
}

impl AnalogDrift {
    pub fn new(seed: u32) -> Self {
        Self {
            rng: Xorshift32::new(seed.wrapping_mul(0x9E37_79B9).max(1)),
            slow: 0.0,
            fast: 0.0,
        }
    }

    #[inline]
    pub fn next(&mut self, sr: f32) -> f32 {
        let w = self.rng.next_bipolar();
        // Slow thermal drift (~0.35 Hz) + subtle component flicker (~2.6 Hz), variance-normalized
        let a_slow = 2.2 / sr.max(8000.0);
        let a_fast = 16.0 / sr.max(8000.0);
        self.slow += (6.0 * a_slow).sqrt() * w * 0.48 - a_slow * self.slow;
        self.fast += (6.0 * a_fast).sqrt() * w * 0.26 - a_fast * self.fast;
        self.slow = self.slow.clamp(-1.0, 1.0);
        self.fast = self.fast.clamp(-1.0, 1.0);
        (self.slow * 0.78 + self.fast * 0.22).clamp(-1.0, 1.0)
    }
}

/// Steep 2-branch polyphase elliptic IIR allpass half-band 2x upsampler
/// (>55 dB image rejection, <0.001 dB passband flatness up to 18.5 kHz).
#[derive(Debug, Clone, Copy, Default)]
pub struct HalfBandUp2x {
    ax0: f32,
    ay0: f32,
    ax1: f32,
    ay1: f32,
    bx0: f32,
    by0: f32,
    bx1: f32,
    by1: f32,
}

impl HalfBandUp2x {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Returns `[even_sample, odd_sample]` at 2x sample rate.
    #[inline]
    pub fn process(&mut self, x: f32) -> [f32; 2] {
        const A0: f32 = 0.07986643;
        const A1: f32 = 0.54535365;
        const B0: f32 = 0.28382934;
        const B1: f32 = 0.83441189;

        let a_s0 = A0 * (x - self.ay0) + self.ax0;
        self.ax0 = x;
        self.ay0 = a_s0;
        let y0 = A1 * (a_s0 - self.ay1) + self.ax1;
        self.ax1 = a_s0;
        self.ay1 = y0;

        let b_s0 = B0 * (x - self.by0) + self.bx0;
        self.bx0 = x;
        self.by0 = b_s0;
        let y1 = B1 * (b_s0 - self.by1) + self.bx1;
        self.bx1 = b_s0;
        self.by1 = y1;

        if !y0.is_finite() || !y1.is_finite() {
            self.reset();
            [0.0, 0.0]
        } else {
            [y0, y1]
        }
    }
}

/// Steep 2-branch polyphase elliptic IIR allpass half-band 2x decimator
/// (>55 dB stopband alias rejection, <0.001 dB passband ripple up to 18.5 kHz).
#[derive(Debug, Clone, Copy, Default)]
pub struct HalfBandDecim2x {
    ax0: f32,
    ay0: f32,
    ax1: f32,
    ay1: f32,
    bx0: f32,
    by0: f32,
    bx1: f32,
    by1: f32,
}

impl HalfBandDecim2x {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[inline]
    pub fn process(&mut self, y0: f32, y1: f32) -> f32 {
        // Branch B processes earlier midpoint sample y0; Branch A processes later sample y1
        // Group delay difference tau_A - tau_B = +1.0 2x-sample -> exact phase alignment!
        const A0: f32 = 0.07986643;
        const A1: f32 = 0.54535365;
        const B0: f32 = 0.28382934;
        const B1: f32 = 0.83441189;

        let b_s0 = B0 * (y0 - self.by0) + self.bx0;
        self.bx0 = y0;
        self.by0 = b_s0;
        let b_out = B1 * (b_s0 - self.by1) + self.bx1;
        self.bx1 = b_s0;
        self.by1 = b_out;

        let a_s0 = A0 * (y1 - self.ay0) + self.ax0;
        self.ax0 = y1;
        self.ay0 = a_s0;
        let a_out = A1 * (a_s0 - self.ay1) + self.ax1;
        self.ax1 = a_s0;
        self.ay1 = a_out;

        let out = 0.5 * (a_out + b_out);
        if !out.is_finite() {
            self.reset();
            0.0
        } else {
            out
        }
    }
}

#[inline]
pub fn one_pole_coeff(cutoff_hz: f32, sample_rate: f32) -> f32 {
    let sr = sample_rate.max(8000.0);
    let hz = cutoff_hz.clamp(1.0, sr * 0.48);
    (1.0 - (-std::f32::consts::TAU * hz / sr).exp()).clamp(0.0, 0.9999)
}

#[inline]
pub fn soft_ceiling(x: f32, knee: f32, headroom: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let ax = x.abs();
    if ax <= knee {
        x
    } else {
        let h = headroom.max(1e-4);
        let excess = (ax - knee) / h;
        x.signum() * (knee + h * excess.tanh())
    }
}

#[inline]
pub fn read_ring_hermite(buf: &[f32], write_pos: usize, delay_samples: f32) -> f32 {
    let len = buf.len();
    if len == 0 {
        return 0.0;
    }
    if len < 6 {
        let d = (delay_samples.round() as usize).min(len - 1);
        return buf[(write_pos + len - d) % len];
    }
    let d = delay_samples.clamp(2.0, (len - 3) as f32);
    let i_floor = d.floor() as usize;
    let frac = d - i_floor as f32;
    let idx0 = (write_pos + len - i_floor) % len;
    let idx_m1 = (idx0 + 1) % len;
    let idx1 = (idx0 + len - 1) % len;
    let idx2 = (idx0 + len - 2) % len;

    let ym1 = buf[idx_m1];
    let y0 = buf[idx0];
    let y1 = buf[idx1];
    let y2 = buf[idx2];

    let c0 = y0;
    let c1 = 0.5 * (y1 - ym1);
    let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
    let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
    ((c3 * frac + c2) * frac + c1) * frac + c0
}

/// Direct Form II Transposed RBJ Biquad filter.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new()
    }
}

impl Biquad {
    pub fn new() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        let nz1 = self.b1 * x - self.a1 * y + self.z2;
        let nz2 = self.b2 * x - self.a2 * y;
        if !y.is_finite() || !nz1.is_finite() || !nz2.is_finite() {
            self.reset();
            return 0.0;
        }
        self.z1 = nz1;
        self.z2 = nz2;
        y
    }

    #[inline]
    fn norm_set(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        let inv_a0 = 1.0 / a0.max(1e-6);
        self.b0 = b0 * inv_a0;
        self.b1 = b1 * inv_a0;
        self.b2 = b2 * inv_a0;
        self.a1 = a1 * inv_a0;
        self.a2 = a2 * inv_a0;
    }

    pub fn set_lowpass(&mut self, sr: f32, freq: f32, q: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(10.0, sr * 0.46);
        let q = q.clamp(0.15, 18.0);
        let w0 = std::f32::consts::TAU * f / sr;
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        let b1 = 1.0 - cos_w;
        let b0 = b1 * 0.5;
        self.norm_set(b0, b1, b0, 1.0 + alpha, -2.0 * cos_w, 1.0 - alpha);
    }

    pub fn set_highpass(&mut self, sr: f32, freq: f32, q: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(10.0, sr * 0.46);
        let q = q.clamp(0.15, 18.0);
        let w0 = std::f32::consts::TAU * f / sr;
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        let b0 = (1.0 + cos_w) * 0.5;
        let b1 = -(1.0 + cos_w);
        self.norm_set(b0, b1, b0, 1.0 + alpha, -2.0 * cos_w, 1.0 - alpha);
    }

    pub fn set_bandpass(&mut self, sr: f32, freq: f32, q: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(10.0, sr * 0.46);
        let q = q.clamp(0.15, 18.0);
        let w0 = std::f32::consts::TAU * f / sr;
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        self.norm_set(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos_w, 1.0 - alpha);
    }

    pub fn set_peaking(&mut self, sr: f32, freq: f32, q: f32, gain_db: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(10.0, sr * 0.46);
        let q = q.clamp(0.15, 18.0);
        let a = 10.0_f32.powf(gain_db.clamp(-24.0, 24.0) / 40.0);
        let w0 = std::f32::consts::TAU * f / sr;
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        self.norm_set(
            1.0 + alpha * a,
            -2.0 * cos_w,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos_w,
            1.0 - alpha / a,
        );
    }

    pub fn set_low_shelf(&mut self, sr: f32, freq: f32, q: f32, gain_db: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(10.0, sr * 0.46);
        let q = q.clamp(0.15, 18.0);
        let a = 10.0_f32.powf(gain_db.clamp(-24.0, 24.0) / 40.0);
        let w0 = std::f32::consts::TAU * f / sr;
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        self.norm_set(
            a * ((a + 1.0) - (a - 1.0) * cos_w + two_sqrt_a_alpha),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w),
            a * ((a + 1.0) - (a - 1.0) * cos_w - two_sqrt_a_alpha),
            (a + 1.0) + (a - 1.0) * cos_w + two_sqrt_a_alpha,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos_w),
            (a + 1.0) + (a - 1.0) * cos_w - two_sqrt_a_alpha,
        );
    }

    pub fn set_high_shelf(&mut self, sr: f32, freq: f32, q: f32, gain_db: f32) {
        let sr = sr.max(8000.0);
        let f = freq.clamp(10.0, sr * 0.46);
        let q = q.clamp(0.15, 18.0);
        let a = 10.0_f32.powf(gain_db.clamp(-24.0, 24.0) / 40.0);
        let w0 = std::f32::consts::TAU * f / sr;
        let (sin_w, cos_w) = w0.sin_cos();
        let alpha = sin_w / (2.0 * q);
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
        self.norm_set(
            a * ((a + 1.0) + (a - 1.0) * cos_w + two_sqrt_a_alpha),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w),
            a * ((a + 1.0) + (a - 1.0) * cos_w - two_sqrt_a_alpha),
            (a + 1.0) - (a - 1.0) * cos_w + two_sqrt_a_alpha,
            2.0 * ((a - 1.0) - (a + 1.0) * cos_w),
            (a + 1.0) - (a - 1.0) * cos_w - two_sqrt_a_alpha,
        );
    }
}

/// Unitary Schroeder/Dattorro delay allpass diffuser (zero heap alloc in process).
#[derive(Debug, Clone)]
pub struct DelayAllpass {
    buf: Vec<f32>,
    pos: usize,
}

impl DelayAllpass {
    pub fn new(delay_samples: usize) -> Self {
        Self {
            buf: vec![0.0; delay_samples.max(1)],
            pos: 0,
        }
    }

    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }

    #[inline]
    pub fn process(&mut self, x: f32, g: f32) -> f32 {
        let d = self.buf[self.pos];
        let v = x + d * g;
        self.buf[self.pos] = v.clamp(-4.0, 4.0);
        self.pos = (self.pos + 1) % self.buf.len();
        d - v * g
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
pub fn poly_blamp(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 || dt >= 0.5 {
        return 0.0;
    }
    if t < dt {
        let n = t / dt - 1.0;
        -(1.0 / 3.0) * n * n * n
    } else if t > 1.0 - dt {
        let n = (t - 1.0) / dt + 1.0;
        (1.0 / 3.0) * n * n * n
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

/// Analog capacitor-droop pulse/square oscillator: adds subtle RC roof tilt & even-harmonic woodiness
/// so pulse/square waves sound like a vintage Prophet/Moog/303 instead of a hollow 8-bit square wave.
#[inline]
pub fn osc_analog_pulse(phase: f32, dt: f32, duty: f32) -> f32 {
    let duty = duty.clamp(0.05, 0.95);
    let p = osc_pulse(phase, dt, duty);
    let tilt = if phase < duty {
        1.0 - 2.0 * (phase / duty)
    } else {
        -1.0 + 2.0 * ((phase - duty) / (1.0 - duty))
    };
    let shaped = p * 0.89 + tilt * 0.11;
    soft_tanh((shaped + 0.035 * (shaped * shaped - 0.80)) * 1.08) / 1.04
}

#[inline]
pub fn osc_tri(phase: f32) -> f32 {
    1.0 - 4.0 * (phase - 0.5).abs()
}

#[inline]
pub fn osc_tri_aa(phase: f32, dt: f32) -> f32 {
    let mut s = 1.0 - 4.0 * (phase - 0.5).abs();
    let slope_step = 4.0 * dt;
    s -= slope_step * poly_blamp(phase, dt);
    let p2 = (phase + 0.5).fract();
    s += slope_step * poly_blamp(p2, dt);
    s
}

#[inline]
pub fn osc_trisaw(phase: f32, dt: f32) -> f32 {
    0.55 * osc_tri_aa(phase, dt) + 0.45 * osc_analog_saw(phase, dt)
}

#[inline]
pub fn osc_analog_saw(phase: f32, dt: f32) -> f32 {
    let s = osc_saw(phase, dt);
    // Subtle zero-DC RC capacitor charge curvature (adds warm -21 dB 2nd-harmonic fullness)
    let curved = s + 0.085 * (s * s - 0.33333334);
    soft_tanh(curved * 1.10) / 1.055 - 0.028 * (phase * std::f32::consts::TAU).sin()
}

#[inline]
pub fn osc_sub_warm(phase: f32) -> f32 {
    let rad = phase * std::f32::consts::TAU;
    rad.sin() * 0.90 + (rad * 2.0).sin() * 0.10
}

/// 4-pole 24dB/oct (with 12dB OB-Xa blend) Zero-Delay Feedback nonlinear analog ladder
/// with 4-tap cubic 2x upsampling, transistor-pair asymmetry, and elliptic half-band decimation.
#[derive(Debug, Clone)]
pub struct MoogLadderFilter {
    s1: f32,
    s2: f32,
    s3: f32,
    s4: f32,
    up2x: HalfBandUp2x,
    decim2x: HalfBandDecim2x,
    sample_rate: f32,
}

impl MoogLadderFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            s1: 0.0,
            s2: 0.0,
            s3: 0.0,
            s4: 0.0,
            up2x: HalfBandUp2x::new(),
            decim2x: HalfBandDecim2x::new(),
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
        self.up2x.reset();
        self.decim2x.reset();
    }

    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32) -> f32 {
        self.process_mode(input, cutoff_hz, resonance, 0.0)
    }

    #[inline]
    pub fn process_mode(
        &mut self,
        input: f32,
        cutoff_hz: f32,
        resonance: f32,
        ob_12db_blend: f32,
    ) -> f32 {
        let cutoff = cutoff_hz.clamp(20.0, self.sample_rate * 0.44);
        let res_norm = resonance.clamp(0.0, 0.96);
        let os_sr = self.sample_rate * 2.0;
        let g = (PI * cutoff / os_sr).tan().clamp(0.0006, 2.2);
        let cap_g = g / (1.0 + g);
        let om_g = 1.0 - cap_g;
        let g2 = cap_g * cap_g;
        let g3 = g2 * cap_g;
        let g4 = g2 * g2;

        let k = res_norm.powf(1.15) * 3.92;
        let in_comp = 1.0 + k * 0.78;
        let blend = ob_12db_blend.clamp(0.0, 1.0);

        let inputs = self.up2x.process(input);
        let mut outs = [0.0_f32; 2];

        for (si, in_step) in inputs.into_iter().enumerate() {
            let comp_in = in_step * in_comp;
            let s_tot = g3 * om_g * self.s1
                + g2 * om_g * self.s2
                + cap_g * om_g * self.s3
                + om_g * self.s4;
            let u_lin = (comp_in - k * s_tot) / (1.0 + k * g4);
            let u_c = u_lin.clamp(-3.5, 3.5);
            let u = 2.5 * soft_tanh((u_lin + 0.035 * u_c * u_c) / 2.5);

            let v0 = cap_g * (u - self.s1);
            let y0 = v0 + self.s1;
            let st1 = y0 + v0;
            self.s1 = 2.7 * soft_tanh((st1 - 0.025 * st1.clamp(-3.0, 3.0).powi(2)) / 2.7);

            let v1 = cap_g * (y0 - self.s2);
            let y1 = v1 + self.s2;
            self.s2 = 2.7 * soft_tanh((y1 + v1) / 2.7);

            let v2 = cap_g * (y1 - self.s3);
            let y2 = v2 + self.s3;
            self.s3 = 2.7 * soft_tanh((y2 + v2) / 2.7);

            let v3 = cap_g * (y2 - self.s4);
            let y3 = v3 + self.s4;
            self.s4 = 2.7 * soft_tanh((y3 + v3) / 2.7);

            outs[si] = y3 * (1.0 - blend) + y1 * blend;
        }

        let out = self.decim2x.process(outs[0], outs[1]) * (1.0 + res_norm * 0.14);
        if !out.is_finite() {
            self.reset();
            return 0.0;
        }
        out
    }
}

/// TB-303 style 18dB/24dB nonlinear diode ladder filter (polyphase half-band 2x oversampled).
#[derive(Debug, Clone)]
pub struct DiodeLadder303Filter {
    s1: f32,
    s2: f32,
    s3: f32,
    s4: f32,
    hp_state: f32,
    up2x: HalfBandUp2x,
    decim2x: HalfBandDecim2x,
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
            up2x: HalfBandUp2x::new(),
            decim2x: HalfBandDecim2x::new(),
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
        self.up2x.reset();
        self.decim2x.reset();
    }

    #[inline]
    pub fn process(&mut self, input: f32, cutoff_hz: f32, resonance: f32) -> f32 {
        let cutoff = cutoff_hz.clamp(25.0, self.sample_rate * 0.44);
        let res_norm = resonance.clamp(0.0, 0.97);
        let os_sr = self.sample_rate * 2.0;

        // Classic TB-303 diode ladder: first pole at ~1.5x cutoff for liquid 18dB/24dB acid character
        let g0 = (PI * (cutoff * 1.5).min(self.sample_rate * 0.46) / os_sr)
            .tan()
            .clamp(0.001, 2.2);
        let g = (PI * cutoff / os_sr).tan().clamp(0.001, 1.8);
        let cap_g0 = g0 / (1.0 + g0);
        let cap_g = g / (1.0 + g);
        let one_minus_g0 = 1.0 - cap_g0;
        let one_minus_g = 1.0 - cap_g;
        let g_tot = cap_g * cap_g * cap_g * cap_g0;

        let k = res_norm.powf(0.76) * 4.08;
        let hp_alpha = one_pole_coeff(160.0, os_sr);
        let in_comp = 1.0 + res_norm * 2.30;

        let inputs = self.up2x.process(input);
        let mut outs = [0.0_f32; 2];

        for (si, in_step) in inputs.into_iter().enumerate() {
            let comp_in = in_step * in_comp;
            let s_total = cap_g * cap_g * cap_g * one_minus_g0 * self.s1
                + cap_g * cap_g * one_minus_g * self.s2
                + cap_g * one_minus_g * self.s3
                + one_minus_g * self.s4;

            // 160 Hz HPF in feedback loop preserves sub-bass weight at high resonance
            let u_lin = (comp_in - k * (s_total - 0.82 * self.hp_state)) / (1.0 + k * g_tot);
            let uc = u_lin.clamp(-3.5, 3.5);
            let u = 2.4 * soft_tanh((u_lin + 0.05 * uc * uc) / 2.4);

            let v0 = cap_g0 * (u - self.s1);
            let y0 = v0 + self.s1;
            self.s1 = soft_tanh((y0 + v0) * 0.42) / 0.42;

            let v1 = cap_g * (y0 - self.s2);
            let y1 = v1 + self.s2;
            self.s2 = soft_tanh((y1 + v1) * 0.42) / 0.42;

            let v2 = cap_g * (y1 - self.s3);
            let y2 = v2 + self.s3;
            self.s3 = soft_tanh((y2 + v2) * 0.42) / 0.42;

            let v3 = cap_g * (y2 - self.s4);
            let y3 = v3 + self.s4;
            self.s4 = soft_tanh((y3 + v3) * 0.42) / 0.42;

            self.hp_state += hp_alpha * (y3 - self.hp_state);
            outs[si] = y3;
        }
        let out = self.decim2x.process(outs[0], outs[1]);
        if !out.is_finite() {
            self.reset();
            return 0.0;
        }
        out
    }
}

/// Cytomic / Simper Zero-Delay Feedback 12dB State Variable Filter with musical analog Q curve.
#[derive(Debug, Clone)]
pub struct Svf12Filter {
    ic1eq: f32,
    ic2eq: f32,
    up2x: HalfBandUp2x,
    decim2x: HalfBandDecim2x,
    sample_rate: f32,
}

impl Svf12Filter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            ic1eq: 0.0,
            ic2eq: 0.0,
            up2x: HalfBandUp2x::new(),
            decim2x: HalfBandDecim2x::new(),
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
        self.up2x.reset();
        self.decim2x.reset();
    }

    #[inline]
    pub fn process_all(&mut self, v0: f32, cutoff_hz: f32, resonance: f32) -> (f32, f32, f32) {
        let cutoff = cutoff_hz.clamp(20.0, self.sample_rate * 0.44);
        let res_norm = resonance.clamp(0.0, 0.96);
        let q = (0.58 + res_norm.powf(2.15) * 9.2).max(0.5);
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

    /// Polyphase half-band 2x-oversampled analog ZDF 12dB SEM/OTA low-pass filter.
    #[inline]
    pub fn process_lp_analog(
        &mut self,
        v0: f32,
        cutoff_hz: f32,
        resonance: f32,
        drive: f32,
    ) -> f32 {
        let cutoff = cutoff_hz.clamp(20.0, self.sample_rate * 0.44);
        let res_norm = resonance.clamp(0.0, 0.95);
        let q = (0.58 + res_norm.powf(2.15) * 9.0).max(0.5);
        let g = (PI * cutoff / (self.sample_rate * 2.0)).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let sat = 1.0 + drive.clamp(0.0, 1.0) * 1.25 + res_norm * 0.65;
        let in_comp = 1.0 + res_norm * 0.22;

        let inputs = self.up2x.process(v0);
        let mut outs = [0.0_f32; 2];

        for (si, v_step) in inputs.into_iter().enumerate() {
            let comp_v0 = v_step * in_comp;
            let v3 = comp_v0 - self.ic2eq;
            let v1_raw = a1 * self.ic1eq + a2 * v3;
            let v1_c = v1_raw.clamp(-3.0, 3.0);
            let v1 = soft_tanh((v1_raw + 0.035 * v1_c * v1_c) * sat) / sat;
            let v2 = self.ic2eq + a2 * self.ic1eq + a3 * v3;

            self.ic1eq = 2.0 * v1 - self.ic1eq;
            self.ic2eq = 2.0 * v2 - self.ic2eq;

            if !v2.is_finite() || !v1.is_finite() {
                self.reset();
                return 0.0;
            }
            outs[si] = v2;
        }
        self.decim2x.process(outs[0], outs[1])
    }
}

/// Roland Juno-60/106 & Dimension-D style Bucket-Brigade Device (BBD) true-stereo analog chorus.
#[derive(Debug, Clone)]
pub struct AnalogStereoChorus {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    write_pos: usize,
    lfo1_phase: f32,
    lfo2_phase: f32,
    lp_l: f32,
    lp_r: f32,
    wet_hp_l: f32,
    wet_hp_r: f32,
    sample_rate: f32,
}

impl AnalogStereoChorus {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let cap = ((0.055 * sr).ceil() as usize).max(64);
        Self {
            buf_l: vec![0.0; cap],
            buf_r: vec![0.0; cap],
            write_pos: 0,
            lfo1_phase: 0.13,
            lfo2_phase: 0.47,
            lp_l: 0.0,
            lp_r: 0.0,
            wet_hp_l: 0.0,
            wet_hp_r: 0.0,
            sample_rate: sr,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        let sr = sr.max(8000.0);
        if (sr - self.sample_rate).abs() > 1.0 {
            *self = Self::new(sr);
        }
    }

    #[inline]
    pub fn process_mono(&mut self, mono: f32, depth: f32) -> (f32, f32) {
        self.process_stereo(mono, mono, depth)
    }

    #[inline]
    pub fn process_stereo(&mut self, in_l: f32, in_r: f32, depth: f32) -> (f32, f32) {
        let d = depth.clamp(0.0, 1.0);
        let len = self.buf_l.len();
        self.buf_l[self.write_pos] = in_l;
        self.buf_r[self.write_pos] = in_r;

        self.lfo1_phase = (self.lfo1_phase + 0.47 / self.sample_rate).fract();
        self.lfo2_phase = (self.lfo2_phase + 0.73 / self.sample_rate).fract();

        if d <= 0.001 {
            self.write_pos = (self.write_pos + 1) % len;
            return (in_l, in_r);
        }

        // Quadrature smooth LFOs for authentic Dimension-D / Juno BBD bloom
        let (lfo1_sin, lfo1_cos) = (self.lfo1_phase * std::f32::consts::TAU).sin_cos();
        let (lfo2_sin, lfo2_cos) = (self.lfo2_phase * std::f32::consts::TAU).sin_cos();

        let ms = self.sample_rate * 0.001;
        let del_l1 = (5.0 + 2.5 * lfo1_sin) * ms;
        let del_l2 = (10.2 + 2.0 * lfo2_cos) * ms;
        let del_r1 = (7.3 - 2.5 * lfo1_cos) * ms;
        let del_r2 = (12.6 - 2.0 * lfo2_sin) * ms;

        let tap_l1 = read_ring_hermite(&self.buf_l, self.write_pos, del_l1);
        let tap_r1 = read_ring_hermite(&self.buf_r, self.write_pos, del_r1);
        let tap_l2 = read_ring_hermite(&self.buf_r, self.write_pos, del_l2);
        let tap_r2 = read_ring_hermite(&self.buf_l, self.write_pos, del_r2);

        self.write_pos = (self.write_pos + 1) % len;

        // Exact 10.5 kHz BBD LPF + 140 Hz wet HPF so sub-bass stays centered & phase-coherent
        let bbd_alpha = one_pole_coeff(10_500.0, self.sample_rate);
        let wet_hp_alpha = one_pole_coeff(140.0, self.sample_rate);
        let raw_wet_l = tap_l1 * 0.65 + tap_l2 * 0.35;
        let raw_wet_r = tap_r1 * 0.65 + tap_r2 * 0.35;
        self.lp_l += bbd_alpha * (raw_wet_l - self.lp_l);
        self.lp_r += bbd_alpha * (raw_wet_r - self.lp_r);
        self.wet_hp_l += wet_hp_alpha * (self.lp_l - self.wet_hp_l);
        self.wet_hp_r += wet_hp_alpha * (self.lp_r - self.wet_hp_r);
        let wet_clean_l = self.lp_l - self.wet_hp_l;
        let wet_clean_r = self.lp_r - self.wet_hp_r;

        let w_l = wet_clean_l * 0.92 - wet_clean_r * 0.45;
        let w_r = wet_clean_r * 0.92 - wet_clean_l * 0.45;
        let wet_gain = 0.78 * d;
        let dry_gain = 1.0 - 0.06 * d;
        (in_l * dry_gain + w_l * wet_gain, in_r * dry_gain + w_r * wet_gain)
    }
}

/// 6-stage analog stereo phaser (quadrature LFO sweeping 6 cascaded 1st-order allpass stages).
#[derive(Debug, Clone)]
pub struct StereoPhaser {
    sample_rate: f32,
    phase: f32,
    ap_l: [f32; 6],
    ap_r: [f32; 6],
    fb_l: f32,
    fb_r: f32,
    hp_l: f32,
    hp_r: f32,
}

impl StereoPhaser {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate: sample_rate.max(8000.0),
            phase: 0.17,
            ap_l: [0.0; 6],
            ap_r: [0.0; 6],
            fb_l: 0.0,
            fb_r: 0.0,
            hp_l: 0.0,
            hp_r: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        self.ap_l = [0.0; 6];
        self.ap_r = [0.0; 6];
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.hp_l = 0.0;
        self.hp_r = 0.0;
    }

    #[inline]
    pub fn process_stereo(&mut self, in_l: f32, in_r: f32, amount: f32) -> (f32, f32) {
        let amt = amount.clamp(0.0, 1.0);
        let sr = self.sample_rate;
        self.phase = (self.phase + 0.36 / sr).fract();
        if amt <= 0.002 {
            return (in_l, in_r);
        }
        let (s_l, s_r) = (self.phase * std::f32::consts::TAU).sin_cos();
        let f_l = 240.0 * (2500.0_f32 / 240.0).powf(0.5 + 0.5 * s_l);
        let f_r = 240.0 * (2500.0_f32 / 240.0).powf(0.5 + 0.5 * s_r);

        let ap_coeff = |f: f32| -> f32 {
            let t = (PI * f.clamp(40.0, sr * 0.42) / sr).tan();
            ((1.0 - t) / (1.0 + t)).clamp(-0.98, 0.98)
        };
        let a_l = ap_coeff(f_l);
        let a_r = ap_coeff(f_r);

        let mut xl = (in_l + self.fb_l * 0.42).clamp(-4.0, 4.0);
        let mut xr = (in_r + self.fb_r * 0.42).clamp(-4.0, 4.0);
        for st in 0..6 {
            let yl = -a_l * xl + self.ap_l[st];
            self.ap_l[st] = xl + a_l * yl;
            xl = yl;

            let yr = -a_r * xr + self.ap_r[st];
            self.ap_r[st] = xr + a_r * yr;
            xr = yr;
        }
        self.fb_l = soft_tanh(xl * 0.95);
        self.fb_r = soft_tanh(xr * 0.95);

        let hp_a = one_pole_coeff(140.0, sr);
        self.hp_l += hp_a * (xl - self.hp_l);
        self.hp_r += hp_a * (xr - self.hp_r);
        let wet_l = xl - self.hp_l;
        let wet_r = xr - self.hp_r;

        (
            in_l * (1.0 - amt * 0.35) + wet_l * (amt * 0.62),
            in_r * (1.0 - amt * 0.35) + wet_r * (amt * 0.62),
        )
    }
}

/// Asymmetric 12AX7 tube saturation + Celestion 4x12 guitar/keys speaker cabinet resonator.
#[derive(Debug, Clone)]
pub struct AmpCabSim {
    sample_rate: f32,
    hp_l: Biquad,
    hp_r: Biquad,
    pk1_l: Biquad,
    pk1_r: Biquad,
    dip_l: Biquad,
    dip_r: Biquad,
    pk2_l: Biquad,
    pk2_r: Biquad,
    lp1_l: Biquad,
    lp1_r: Biquad,
    lp2_l: Biquad,
    lp2_r: Biquad,
    dc_l: f32,
    dc_r: f32,
}

impl AmpCabSim {
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            sample_rate: sample_rate.max(8000.0),
            hp_l: Biquad::new(),
            hp_r: Biquad::new(),
            pk1_l: Biquad::new(),
            pk1_r: Biquad::new(),
            dip_l: Biquad::new(),
            dip_r: Biquad::new(),
            pk2_l: Biquad::new(),
            pk2_r: Biquad::new(),
            lp1_l: Biquad::new(),
            lp1_r: Biquad::new(),
            lp2_l: Biquad::new(),
            lp2_r: Biquad::new(),
            dc_l: 0.0,
            dc_r: 0.0,
        };
        s.configure();
        s
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.configure();
        self.dc_l = 0.0;
        self.dc_r = 0.0;
    }

    fn configure(&mut self) {
        let sr = self.sample_rate;
        self.hp_l.set_highpass(sr, 85.0, 0.7071);
        self.hp_r.set_highpass(sr, 85.0, 0.7071);
        self.pk1_l.set_peaking(sr, 110.0, 1.1, 3.2);
        self.pk1_r.set_peaking(sr, 110.0, 1.1, 3.2);
        self.dip_l.set_peaking(sr, 430.0, 1.2, -2.8);
        self.dip_r.set_peaking(sr, 430.0, 1.2, -2.8);
        self.pk2_l.set_peaking(sr, 2100.0, 1.1, 3.5);
        self.pk2_r.set_peaking(sr, 2100.0, 1.1, 3.5);
        self.lp1_l.set_lowpass(sr, 4300.0, 0.75);
        self.lp1_r.set_lowpass(sr, 4300.0, 0.75);
        self.lp2_l.set_lowpass(sr, 4300.0, 0.75);
        self.lp2_r.set_lowpass(sr, 4300.0, 0.75);
    }

    #[inline]
    pub fn process_stereo(&mut self, in_l: f32, in_r: f32, amount: f32) -> (f32, f32) {
        let amt = amount.clamp(0.0, 1.0);
        if amt <= 0.002 {
            return (in_l, in_r);
        }
        let drive = 1.15 + amt * 2.4;
        let dl = soft_tanh(in_l * drive + 0.14 * in_l.clamp(-1.5, 1.5).powi(2));
        let dr = soft_tanh(in_r * drive + 0.14 * in_r.clamp(-1.5, 1.5).powi(2));

        let dc_a = one_pole_coeff(22.0, self.sample_rate);
        self.dc_l += dc_a * (dl - self.dc_l);
        self.dc_r += dc_a * (dr - self.dc_r);
        let cl = dl - self.dc_l;
        let cr = dr - self.dc_r;

        let mut wl = self.hp_l.process(cl);
        wl = self.pk1_l.process(wl);
        wl = self.dip_l.process(wl);
        wl = self.pk2_l.process(wl);
        wl = self.lp2_l.process(self.lp1_l.process(wl));

        let mut wr = self.hp_r.process(cr);
        wr = self.pk1_r.process(wr);
        wr = self.dip_r.process(wr);
        wr = self.pk2_r.process(wr);
        wr = self.lp2_r.process(self.lp1_r.process(wr));

        let norm = 0.88;
        (
            in_l * (1.0 - amt) + wl * norm * amt,
            in_r * (1.0 - amt) + wr * norm * amt,
        )
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
                // True RC capacitor charge toward 1.18x asymptote (snappy analog transient punch)
                let coeff = 1.0 - (-1.85 / (attack_s.max(0.001) * self.sample_rate)).exp();
                self.level += coeff * (1.18 - self.level);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = EnvStage::Decay;
                }
                self.level
            }
            EnvStage::Decay => {
                // Smooth RC exponential discharge toward slight undershoot (snappy start, silky tail, zero linear kink)
                let coeff = 1.0 - (-4.4 / (decay_s.max(0.004) * self.sample_rate)).exp();
                let target = sustain - 0.012;
                self.level += coeff * (target - self.level);
                if self.level <= sustain + 0.0004 {
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
                let coeff = 1.0 - (-4.6 / (release_s.max(0.004) * self.sample_rate)).exp();
                self.level += coeff * (-0.004 - self.level);
                if self.level <= 0.0002 {
                    self.level = 0.0;
                    self.stage = EnvStage::Idle;
                }
                self.level.max(0.0)
            }
        }
    }
}
