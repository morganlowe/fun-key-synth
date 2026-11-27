use std::f32::consts::TAU;
use std::sync::{Arc, Mutex, OnceLock};

use crate::model::{DrumKitParams, DrumModel, DrumVoiceId, DrumVoiceParams};

use super::{
    one_pole_coeff, osc_pulse, soft_ceiling, soft_tanh, Biquad, DelayAllpass, Svf12Filter,
    Xorshift32,
};

const METAL_FREQS_808: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
const ROOM_TAPS_L_MS: [f32; 4] = [11.3, 19.7, 29.3, 39.1];
const ROOM_TAPS_R_MS: [f32; 4] = [13.1, 22.9, 32.7, 43.3];

#[derive(Debug)]
struct DrumPcmBank {
    hat_bronze_909: Vec<f32>,
    hat_bronze_side: Vec<f32>,
    cym_bronze_crash: Vec<f32>,
    cym_bronze_side: Vec<f32>,
    kick_beater_pcm: Vec<f32>,
    snare_crack_pcm: Vec<f32>,
    acoustic_shell_pcm: Vec<f32>,
    linn_crack_pcm: Vec<f32>,
}

static PCM_CACHE: OnceLock<Mutex<Vec<(u32, Arc<DrumPcmBank>)>>> = OnceLock::new();

fn get_or_build_pcm_bank(sr: f32) -> Arc<DrumPcmBank> {
    let sr_u32 = sr.round().clamp(8_000.0, 192_000.0) as u32;
    let cache = PCM_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((_, bank)) = guard.iter().find(|(s, _)| *s == sr_u32) {
            return Arc::clone(bank);
        }
    }
    let bank = Arc::new(build_pcm_bank(sr_u32 as f32));
    if let Ok(mut guard) = cache.lock() {
        if !guard.iter().any(|(s, _)| *s == sr_u32) {
            guard.push((sr_u32, Arc::clone(&bank)));
        }
    }
    bank
}

fn normalize_pcm(buf: &mut [f32]) {
    let peak = buf.iter().fold(0.0_f32, |m, &x| m.max(x.abs()));
    if peak > 1e-6 {
        let g = 0.96 / peak;
        for s in buf.iter_mut() {
            *s *= g;
        }
    }
}

fn build_pcm_bank(sr: f32) -> DrumPcmBank {
    let mut rng = Xorshift32::new(0x909_808_42);

    // 1. 909 / Studio FM-coupled bronze hi-hat pair + dispersive stereo overhead plate (0.75 s)
    let hat_len = (0.75 * sr).round() as usize;
    let mut hat_bronze_909 = vec![0.0_f32; hat_len];
    let mut hat_bronze_side = vec![0.0_f32; hat_len];
    {
        const N_HAT: usize = 48;
        let mut phases = [0.0_f32; N_HAT];
        let mut base_freqs = [0.0_f32; N_HAT];
        let mut amp = [0.0_f32; N_HAT];
        let mut decay = [0.0_f32; N_HAT];
        let max_modal_hz = 14_800.0_f32.min(sr * 0.44);
        for m in 0..N_HAT {
            let frac = m as f32 / (N_HAT - 1) as f32;
            let base_f = 2150.0 + frac.powf(1.06) * 12_650.0;
            let jitter = 1.0 + rng.next_bipolar() * 0.028;
            base_freqs[m] = (base_f * jitter).min(max_modal_hz);
            phases[m] = rng.next_f32();
            amp[m] = (0.55 + 0.45 * rng.next_f32()) * (1.0 - frac * 0.40);
            let tau_s = 0.74 - frac * 0.38;
            decay[m] = (-1.0 / (tau_s * sr)).exp();
        }
        let mut ap_l1 = DelayAllpass::new(((0.00173 * sr).round() as usize).max(2));
        let mut ap_l2 = DelayAllpass::new(((0.00289 * sr).round() as usize).max(3));
        let mut ap_r1 = DelayAllpass::new(((0.00211 * sr).round() as usize).max(2));
        let mut ap_r2 = DelayAllpass::new(((0.00337 * sr).round() as usize).max(3));
        let mut hp_lp = 0.0_f32;
        let mut air_lp = 0.0_f32;
        let hp_a = one_pole_coeff(5800.0, sr);
        let lp_a = one_pole_coeff(14_500.0, sr);
        for i in 0..hat_len {
            let t = i as f32 / sr;
            let chirp = 1.0 + 0.028 * (-t / 0.012).exp();
            let fm_depth = 0.24 * (-t / 0.11).exp();
            let mut sum_l = 0.0_f32;
            let mut sum_r = 0.0_f32;
            let mut prev = 0.0_f32;
            for m in 0..N_HAT {
                let f = (base_freqs[m] * chirp).min(max_modal_hz);
                phases[m] = (phases[m] + f / sr).fract();
                amp[m] *= decay[m];
                let v = (phases[m] * TAU + prev * fm_depth).sin() * amp[m];
                prev = v;
                if m % 2 == 0 {
                    sum_l += v;
                    sum_r += v * 0.68;
                } else {
                    sum_l += v * 0.68;
                    sum_r += v;
                }
            }
            let n = rng.next_bipolar();
            hp_lp += hp_a * (n - hp_lp);
            air_lp += lp_a * ((n - hp_lp) - air_lp);
            let air = air_lp * (-t / 0.28).exp();
            let bloom = 1.0 - (-t / 0.0007).exp();
            let raw_l = ap_l2.process(ap_l1.process(sum_l * 0.24 + air * 0.20, 0.52), 0.48) * bloom;
            let raw_r = ap_r2.process(ap_r1.process(sum_r * 0.24 - air * 0.18, 0.52), 0.48) * bloom;
            hat_bronze_909[i] = 0.5 * (raw_l + raw_r);
            hat_bronze_side[i] = 0.5 * (raw_l - raw_r);
        }
        let mut hat_lp_m = Biquad::new();
        let mut hat_lp_s = Biquad::new();
        hat_lp_m.set_lowpass(sr, 13_800.0, 0.7071);
        hat_lp_s.set_lowpass(sr, 13_800.0, 0.7071);
        for i in 0..hat_len {
            hat_bronze_909[i] = hat_lp_m.process(hat_bronze_909[i]);
            hat_bronze_side[i] = hat_lp_s.process(hat_bronze_side[i]);
        }
        let peak_m = hat_bronze_909.iter().fold(0.0_f32, |acc, &x| acc.max(x.abs()));
        if peak_m > 1e-6 {
            let g = 0.96 / peak_m;
            for i in 0..hat_len {
                hat_bronze_909[i] *= g;
                hat_bronze_side[i] *= g * 0.55;
            }
        }
    }

    // 2. 18" Hammered Bronze Crash Cymbal: nonlinear von Karman pitch-glide bow modes +
    //    FM-coupled chaotic shimmer + 4-line dispersive plate waveguide stereo wash (2.2 s)
    let cym_len = (2.2 * sr).round() as usize;
    let mut cym_bronze_crash = vec![0.0_f32; cym_len];
    let mut cym_bronze_side = vec![0.0_f32; cym_len];
    {
        const N_BOW: usize = 18;
        const N_SHIM: usize = 48;
        let max_modal_hz = 15_000.0_f32.min(sr * 0.44);

        let mut bow_ph = [0.0_f32; N_BOW];
        let mut bow_f = [0.0_f32; N_BOW];
        let mut bow_amp = [0.0_f32; N_BOW];
        let mut bow_dec = [0.0_f32; N_BOW];
        for m in 0..N_BOW {
            let frac = m as f32 / (N_BOW - 1) as f32;
            let f0 = 460.0 + frac.powf(1.22) * 2_200.0;
            bow_f[m] = (f0 * (1.0 + rng.next_bipolar() * 0.035)).min(max_modal_hz);
            bow_ph[m] = rng.next_f32();
            bow_amp[m] = (0.45 + 0.55 * rng.next_f32()) * (0.65 + 0.35 * (1.0 - frac));
            let tau_s = 1.95 - frac * 0.55;
            bow_dec[m] = (-1.0 / (tau_s * sr)).exp();
        }

        let mut shim_ph = [0.0_f32; N_SHIM];
        let mut shim_f = [0.0_f32; N_SHIM];
        let mut shim_amp = [0.0_f32; N_SHIM];
        let mut shim_dec = [0.0_f32; N_SHIM];
        for m in 0..N_SHIM {
            let frac = m as f32 / (N_SHIM - 1) as f32;
            let f0 = 2_350.0 + frac.powf(1.10) * 12_450.0;
            shim_f[m] = (f0 * (1.0 + rng.next_bipolar() * 0.032)).min(max_modal_hz);
            shim_ph[m] = rng.next_f32();
            shim_amp[m] = (0.50 + 0.50 * rng.next_f32()) * (1.0 - frac * 0.38);
            let tau_s = 1.55 - frac * 0.90;
            shim_dec[m] = (-1.0 / (tau_s * sr)).exp();
        }

        // Feed-forward orthogonal allpass phase dispersion for stereo plate shimmer (ZERO comb feedback!)
        let mut ap_l1 = DelayAllpass::new(((0.00143 * sr).round() as usize).max(2));
        let mut ap_l2 = DelayAllpass::new(((0.00239 * sr).round() as usize).max(3));
        let mut ap_r1 = DelayAllpass::new(((0.00181 * sr).round() as usize).max(2));
        let mut ap_r2 = DelayAllpass::new(((0.00283 * sr).round() as usize).max(3));

        let mut noise_hp = 0.0_f32;
        let mut noise_lp = 0.0_f32;
        let mut air_hp_r = 0.0_f32;
        let nhp_a = one_pole_coeff(1_850.0, sr);
        let nlp_a = one_pole_coeff(13_800.0, sr);

        for i in 0..cym_len {
            let t = i as f32 / sr;
            // Nonlinear von Karman impact tension pitch-bend (+3.6% settling over 28 ms)
            let tension_bend = 1.0 + 0.036 * (-t / 0.028).exp();

            let mut bow_l = 0.0_f32;
            let mut bow_r = 0.0_f32;
            for m in 0..N_BOW {
                let f = (bow_f[m] * tension_bend).min(max_modal_hz);
                bow_ph[m] = (bow_ph[m] + f / sr).fract();
                bow_amp[m] *= bow_dec[m];
                let v = (bow_ph[m] * TAU).sin() * bow_amp[m];
                if m % 2 == 0 {
                    bow_l += v;
                    bow_r += v * 0.62;
                } else {
                    bow_l += v * 0.62;
                    bow_r += v;
                }
            }
            let bow_mod = (bow_l + bow_r) * 0.16 * (0.30 + 0.70 * (-t / 0.30).exp());

            let mut shim_l = 0.0_f32;
            let mut shim_r = 0.0_f32;
            let mut prev_s = 0.0_f32;
            let cross_fm = 0.28 * (-t / 0.26).exp();
            for m in 0..N_SHIM {
                let f = (shim_f[m] * tension_bend).min(max_modal_hz);
                shim_ph[m] = (shim_ph[m] + f / sr).fract();
                shim_amp[m] *= shim_dec[m];
                let v = (shim_ph[m] * TAU + bow_mod + prev_s * cross_fm).sin() * shim_amp[m];
                prev_s = v;
                if m % 2 == 0 {
                    shim_l += v;
                    shim_r += v * 0.58;
                } else {
                    shim_l += v * 0.58;
                    shim_r += v;
                }
            }

            let n = rng.next_bipolar();
            let nr = rng.next_bipolar();
            noise_hp += nhp_a * (n - noise_hp);
            noise_lp += nlp_a * ((n - noise_hp) - noise_lp);
            air_hp_r += nhp_a * (nr - air_hp_r);
            let air_l = noise_lp * (0.28 * (-t / 0.065).exp() + 0.24 * (-t / 0.48).exp());
            let air_r = (nr - air_hp_r) * (0.26 * (-t / 0.065).exp() + 0.22 * (-t / 0.48).exp());

            let stick_impact = ((t * 2_850.0 * TAU).sin() * 0.34
                + (t * 5_100.0 * TAU).sin() * 0.22
                + noise_lp * 0.72)
                * (-t / 0.0032).exp();

            let wash_bloom = 1.0 - (-t / 0.018).exp();
            let atk_env = 1.0 - (-t / 0.0007).exp();

            let raw_l = (stick_impact * 0.36
                + bow_l * 0.055 * wash_bloom
                + shim_l * 0.24 * (0.40 + 0.60 * wash_bloom)
                + air_l)
                * atk_env;
            let raw_r = (stick_impact * 0.32
                + bow_r * 0.055 * wash_bloom
                + shim_r * 0.24 * (0.40 + 0.60 * wash_bloom)
                + air_r)
                * atk_env;
            let disp_l = ap_l2.process(ap_l1.process(raw_l, 0.48), 0.42);
            let disp_r = ap_r2.process(ap_r1.process(raw_r, 0.48), 0.42);

            cym_bronze_crash[i] = 0.5 * (disp_l + disp_r);
            cym_bronze_side[i] = 0.5 * (disp_l - disp_r);
        }
        let mut cym_lp_m = Biquad::new();
        let mut cym_lp_s = Biquad::new();
        cym_lp_m.set_lowpass(sr, 13_800.0, 0.7071);
        cym_lp_s.set_lowpass(sr, 13_800.0, 0.7071);
        for i in 0..cym_len {
            cym_bronze_crash[i] = cym_lp_m.process(cym_bronze_crash[i]);
            cym_bronze_side[i] = cym_lp_s.process(cym_bronze_side[i]);
        }
        // Layer real studio Philharmonia fortissimo crash cymbal recording!
        if let Some((cl_l, cl_r)) = super::sampler::load_philharmonia_trimmed_stereo(
            "percussion/clash cymbals/clash-cymbals__15_fortissimo_struck-together.mp3",
            sr.round() as u32,
            2.2,
        ) {
            let n = cym_len.min(cl_l.len()).min(cl_r.len());
            for i in 0..n {
                let m = 0.5 * (cl_l[i] + cl_r[i]);
                let s = 0.5 * (cl_l[i] - cl_r[i]);
                cym_bronze_crash[i] = cym_bronze_crash[i] * 0.38 + m * 0.82;
                cym_bronze_side[i] = cym_bronze_side[i] * 0.42 + s * 0.92;
            }
        }
        let peak_m = cym_bronze_crash.iter().fold(0.0_f32, |acc, &x| acc.max(x.abs()));
        if peak_m > 1e-6 {
            let g = 0.96 / peak_m;
            for i in 0..cym_len {
                cym_bronze_crash[i] *= g;
                cym_bronze_side[i] *= (g * 0.65).min(2.2);
            }
        }
    }

    // 3. Acoustic felt/wood kick beater transient (24 ms)
    let beater_len = (0.024 * sr).round() as usize;
    let mut kick_beater_pcm = vec![0.0_f32; beater_len];
    {
        let mut hp_lp = 0.0_f32;
        let mut lp_state = 0.0_f32;
        let hp_a = one_pole_coeff(950.0, sr);
        let lp_a = one_pole_coeff(4200.0, sr);
        for i in 0..beater_len {
            let t = i as f32 / sr;
            let skin_punch =
                ((t * 175.0 * TAU).sin() * 0.65 + (t * 310.0 * TAU).sin() * 0.35) * (-t / 0.0065).exp();
            let n = rng.next_bipolar();
            hp_lp += hp_a * (n - hp_lp);
            lp_state += lp_a * ((n - hp_lp) - lp_state);
            let felt_thwack = lp_state * (-t / 0.0028).exp();
            let env_in = (t / 0.0004).min(1.0);
            kick_beater_pcm[i] = (skin_punch * 0.62 + felt_thwack * 0.95) * env_in;
        }
        normalize_pcm(&mut kick_beater_pcm);
    }

    // 4. Acoustic drumstick / snare rim & real Philharmonia snare-drum transient (65 ms)
    let crack_len = (0.065 * sr).round() as usize;
    let mut snare_crack_pcm = vec![0.0_f32; crack_len];
    {
        let mut hp_lp = 0.0_f32;
        let mut lp_state = 0.0_f32;
        let hp_a = one_pole_coeff(850.0, sr);
        let lp_a = one_pole_coeff(9500.0, sr);
        for i in 0..crack_len {
            let t = i as f32 / sr;
            let wood = ((t * 1420.0 * TAU).sin() * 0.55 + (t * 2680.0 * TAU).sin() * 0.30)
                * (-t / 0.0022).exp();
            let n = rng.next_bipolar();
            hp_lp += hp_a * (n - hp_lp);
            lp_state += lp_a * ((n - hp_lp) - lp_state);
            let snap = lp_state * (-t / 0.0032).exp();
            let env_in = (t / 0.0002).min(1.0);
            snare_crack_pcm[i] = (wood * 0.50 + snap * 0.85) * env_in;
        }
        let mut crack_lp = Biquad::new();
        crack_lp.set_lowpass(sr, 11_500.0, 0.7071);
        for s in snare_crack_pcm.iter_mut() {
            *s = crack_lp.process(*s);
        }
        normalize_pcm(&mut snare_crack_pcm);
        if let Some((sn_l, sn_r)) = super::sampler::load_philharmonia_trimmed_stereo(
            "percussion/snare drum/snare-drum__025_forte_with-snares.mp3",
            sr.round() as u32,
            0.065,
        ) {
            let n = crack_len.min(sn_l.len()).min(sn_r.len());
            for i in 0..n {
                let t = i as f32 / sr;
                let win = (-t / 0.028).exp();
                let m = 0.5 * (sn_l[i] + sn_r[i]);
                snare_crack_pcm[i] = snare_crack_pcm[i] * 0.66 + m * 0.50 * win;
            }
            normalize_pcm(&mut snare_crack_pcm);
        }
    }

    // 5. Acoustic Birch/Maple drum shell multi-mode impulse (45 ms)
    let shell_len = (0.045 * sr).round() as usize;
    let mut acoustic_shell_pcm = vec![0.0_f32; shell_len];
    {
        for i in 0..shell_len {
            let t = i as f32 / sr;
            let modes = (t * 185.0 * TAU).sin() * 0.48 * (-t / 0.018).exp()
                + (t * 292.0 * TAU).sin() * 0.34 * (-t / 0.014).exp()
                + (t * 438.0 * TAU).sin() * 0.24 * (-t / 0.009).exp()
                + (t * 680.0 * TAU).sin() * 0.16 * (-t / 0.006).exp();
            let stick = rng.next_bipolar() * (-t / 0.0018).exp() * 0.52;
            let env_in = (t / 0.00025).min(1.0);
            acoustic_shell_pcm[i] = (modes + stick) * env_in;
        }
        normalize_pcm(&mut acoustic_shell_pcm);
    }

    // 6. Classic 80s 12-bit EPROM companded drum-machine crack (32 ms)
    let linn_len = (0.032 * sr).round() as usize;
    let mut linn_crack_pcm = vec![0.0_f32; linn_len];
    {
        for i in 0..linn_len {
            let t = i as f32 / sr;
            let punch = (t * 210.0 * TAU).sin() * 0.55 * (-t / 0.008).exp()
                + (t * 1120.0 * TAU).sin() * 0.35 * (-t / 0.003).exp();
            let grit = rng.next_bipolar() * (-t / 0.0055).exp() * 0.65;
            let raw = (punch + grit) * (t / 0.0002).min(1.0);
            // 12-bit style companding grit
            linn_crack_pcm[i] = (raw * 48.0).round() / 48.0;
        }
        normalize_pcm(&mut linn_crack_pcm);
    }

    DrumPcmBank {
        hat_bronze_909,
        hat_bronze_side,
        cym_bronze_crash,
        cym_bronze_side,
        kick_beater_pcm,
        snare_crack_pcm,
        acoustic_shell_pcm,
        linn_crack_pcm,
    }
}

#[derive(Debug, Clone)]
pub struct DrumMachine {
    sample_rate: f32,
    rng: Xorshift32,
    pcm: Arc<DrumPcmBank>,

    // Kick
    kick_active: bool,
    kick_phase: f32,
    kick_amp: f32,
    kick_pitch_env: f32,
    kick_punch_env: f32,
    kick_attack_env: f32,
    kick_beater_pos: usize,
    kick_vel: f32,
    kick_rr_pitch: f32,
    kick_dc: f32,
    kick_tone_lp: f32,

    // Snare (3-mode circular membrane + wood shell resonance + stereo coupled wires + crack PCM)
    snare_active: bool,
    snare_p1: f32,
    snare_p2: f32,
    snare_p3: f32,
    snare_body_env: f32,
    snare_over_env: f32,
    snare_pitch_env: f32,
    snare_noise_env: f32,
    snare_wire_hp_svf: Svf12Filter,
    snare_shell_bp: Svf12Filter,
    snare_wire_ap_r: DelayAllpass,
    snare_wire_lp: f32,
    snare_samples: u32,
    snare_vel: f32,
    snare_rr_pitch: f32,

    // Clap (true decorrelated stereo multi-hand burst)
    clap_active: bool,
    clap_samples: u32,
    clap_tail_env: f32,
    clap_vel: f32,
    clap_hp: f32,
    clap_hp_r: f32,
    clap_bp: Svf12Filter,
    clap_body: Svf12Filter,
    clap_bp_r: Svf12Filter,
    clap_body_r: Svf12Filter,

    // Cowbell / Rim
    cb_active: bool,
    cb_p1: f32,
    cb_p2: f32,
    cb_p3: f32,
    cb_env: f32,
    cb_fast_env: f32,
    cb_crack_pos: usize,
    cb_vel: f32,
    cb_filter: Svf12Filter,

    // 808 Metallic bank shared phase + 12dB band-limited air noise
    metal_phases: [f32; 6],
    air_noise_lp: f32,
    air_noise_lp_bq: Biquad,

    // Closed Hat
    ch_active: bool,
    ch_env: f32,
    ch_pcm_pos: f32,
    ch_vel: f32,
    ch_rr_pitch: f32,
    ch_hp1: Svf12Filter,
    ch_hp2: Svf12Filter,
    ch_tone_lp: f32,

    // Open Hat
    oh_active: bool,
    oh_env: f32,
    oh_pcm_pos: f32,
    oh_vel: f32,
    oh_rr_pitch: f32,
    oh_hp1: Svf12Filter,
    oh_hp2: Svf12Filter,
    oh_tone_lp: f32,

    // Tom (3-mode circular membrane)
    tom_active: bool,
    tom_p1: f32,
    tom_p2: f32,
    tom_p3: f32,
    tom_amp: f32,
    tom_over_env: f32,
    tom_pitch_env: f32,
    tom_crack_pos: usize,
    tom_vel: f32,

    // Cymbal
    cym_active: bool,
    cym_env: f32,
    cym_splash_env: f32,
    cym_pcm_pos: f32,
    cym_vel: f32,
    cym_rr_pitch: f32,
    cym_hp1: Svf12Filter,
    cym_hp2: Svf12Filter,
    cym_bp_low: Svf12Filter,
    cym_bp_mid: Svf12Filter,
    cym_tone_lp: f32,
    cym_side_lp: f32,

    // Sympathetic snare-wire buzz when Kick / Tom excites the room
    symp_snare_env: f32,
    symp_snare_bp: Svf12Filter,

    // Stereo Diffused Early-Reflection Studio Drum Room
    room_ap_l: [DelayAllpass; 2],
    room_ap_r: [DelayAllpass; 2],
    room_l: Vec<f32>,
    room_r: Vec<f32>,
    room_pos: usize,
    room_hp_l: f32,
    room_hp_r: f32,
    room_lp_l: f32,
    room_lp_r: f32,

    // Flam queue (one pending per voice)
    flam_countdown: [u32; 8],
    flam_vel: [f32; 8],

    // Drum Bus Studio Console Contour (95 Hz punch, 380 Hz box carve, 4.2 kHz snap)
    // + 28 Hz 12dB Biquad subsonic HPF + 14.5 kHz studio silk LPF
    bus_punch_l: Biquad,
    bus_punch_r: Biquad,
    bus_box_l: Biquad,
    bus_box_r: Biquad,
    bus_snap_l: Biquad,
    bus_snap_r: Biquad,
    bus_hp_l: Biquad,
    bus_hp_r: Biquad,
    bus_lp_l: Biquad,
    bus_lp_r: Biquad,
}

impl DrumMachine {
    fn configure_bus_filters(
        sr: f32,
        bus_punch_l: &mut Biquad,
        bus_punch_r: &mut Biquad,
        bus_box_l: &mut Biquad,
        bus_box_r: &mut Biquad,
        bus_snap_l: &mut Biquad,
        bus_snap_r: &mut Biquad,
        bus_hp_l: &mut Biquad,
        bus_hp_r: &mut Biquad,
        bus_lp_l: &mut Biquad,
        bus_lp_r: &mut Biquad,
    ) {
        bus_punch_l.set_peaking(sr, 95.0, 1.2, 1.0);
        bus_punch_r.set_peaking(sr, 95.0, 1.2, 1.0);
        bus_box_l.set_peaking(sr, 380.0, 1.4, -1.3);
        bus_box_r.set_peaking(sr, 380.0, 1.4, -1.3);
        bus_snap_l.set_peaking(sr, 4200.0, 1.1, 1.2);
        bus_snap_r.set_peaking(sr, 4200.0, 1.1, 1.2);
        bus_hp_l.set_highpass(sr, 28.0, 0.7071);
        bus_hp_r.set_highpass(sr, 28.0, 0.7071);
        bus_lp_l.set_lowpass(sr, 14_500.0, 0.65);
        bus_lp_r.set_lowpass(sr, 14_500.0, 0.65);
    }

    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        let room_cap = ((0.055 * sr).ceil() as usize).max(64);
        let pcm = get_or_build_pcm_bank(sr);
        let mut air_noise_lp_bq = Biquad::new();
        air_noise_lp_bq.set_lowpass(sr, 12_500.0, 0.7071);
        let mut bus_punch_l = Biquad::new();
        let mut bus_punch_r = Biquad::new();
        let mut bus_box_l = Biquad::new();
        let mut bus_box_r = Biquad::new();
        let mut bus_snap_l = Biquad::new();
        let mut bus_snap_r = Biquad::new();
        let mut bus_hp_l = Biquad::new();
        let mut bus_hp_r = Biquad::new();
        let mut bus_lp_l = Biquad::new();
        let mut bus_lp_r = Biquad::new();
        Self::configure_bus_filters(
            sr,
            &mut bus_punch_l,
            &mut bus_punch_r,
            &mut bus_box_l,
            &mut bus_box_r,
            &mut bus_snap_l,
            &mut bus_snap_r,
            &mut bus_hp_l,
            &mut bus_hp_r,
            &mut bus_lp_l,
            &mut bus_lp_r,
        );
        Self {
            sample_rate: sr,
            rng: Xorshift32::new(0x808_909),
            pcm,
            kick_active: false,
            kick_phase: 0.02,
            kick_amp: 0.0,
            kick_pitch_env: 0.0,
            kick_punch_env: 0.0,
            kick_attack_env: 0.0,
            kick_beater_pos: usize::MAX,
            kick_vel: 1.0,
            kick_rr_pitch: 1.0,
            kick_dc: 0.0,
            kick_tone_lp: 0.0,
            snare_active: false,
            snare_p1: 0.0,
            snare_p2: 0.0,
            snare_p3: 0.0,
            snare_body_env: 0.0,
            snare_over_env: 0.0,
            snare_pitch_env: 0.0,
            snare_noise_env: 0.0,
            snare_wire_hp_svf: Svf12Filter::new(sr),
            snare_shell_bp: Svf12Filter::new(sr),
            snare_wire_ap_r: DelayAllpass::new((0.0019 * sr).round() as usize),
            snare_wire_lp: 0.0,
            snare_samples: 0,
            snare_vel: 1.0,
            snare_rr_pitch: 1.0,
            clap_active: false,
            clap_samples: 0,
            clap_tail_env: 0.0,
            clap_vel: 1.0,
            clap_hp: 0.0,
            clap_hp_r: 0.0,
            clap_bp: Svf12Filter::new(sr),
            clap_body: Svf12Filter::new(sr),
            clap_bp_r: Svf12Filter::new(sr),
            clap_body_r: Svf12Filter::new(sr),
            cb_active: false,
            cb_p1: 0.0,
            cb_p2: 0.0,
            cb_p3: 0.0,
            cb_env: 0.0,
            cb_fast_env: 0.0,
            cb_crack_pos: usize::MAX,
            cb_vel: 1.0,
            cb_filter: Svf12Filter::new(sr),
            metal_phases: [0.0; 6],
            air_noise_lp: 0.0,
            air_noise_lp_bq,
            ch_active: false,
            ch_env: 0.0,
            ch_pcm_pos: 0.0,
            ch_vel: 1.0,
            ch_rr_pitch: 1.0,
            ch_hp1: Svf12Filter::new(sr),
            ch_hp2: Svf12Filter::new(sr),
            ch_tone_lp: 0.0,
            oh_active: false,
            oh_env: 0.0,
            oh_pcm_pos: 0.0,
            oh_vel: 1.0,
            oh_rr_pitch: 1.0,
            oh_hp1: Svf12Filter::new(sr),
            oh_hp2: Svf12Filter::new(sr),
            oh_tone_lp: 0.0,
            tom_active: false,
            tom_p1: 0.0,
            tom_p2: 0.0,
            tom_p3: 0.0,
            tom_amp: 0.0,
            tom_over_env: 0.0,
            tom_pitch_env: 0.0,
            tom_crack_pos: usize::MAX,
            tom_vel: 1.0,
            cym_active: false,
            cym_env: 0.0,
            cym_splash_env: 0.0,
            cym_pcm_pos: 0.0,
            cym_vel: 1.0,
            cym_rr_pitch: 1.0,
            cym_hp1: Svf12Filter::new(sr),
            cym_hp2: Svf12Filter::new(sr),
            cym_bp_low: Svf12Filter::new(sr),
            cym_bp_mid: Svf12Filter::new(sr),
            cym_tone_lp: 0.0,
            cym_side_lp: 0.0,
            symp_snare_env: 0.0,
            symp_snare_bp: Svf12Filter::new(sr),
            room_ap_l: [
                DelayAllpass::new((0.0037 * sr).round() as usize),
                DelayAllpass::new((0.0059 * sr).round() as usize),
            ],
            room_ap_r: [
                DelayAllpass::new((0.0043 * sr).round() as usize),
                DelayAllpass::new((0.0067 * sr).round() as usize),
            ],
            room_l: vec![0.0; room_cap],
            room_r: vec![0.0; room_cap],
            room_pos: 0,
            room_hp_l: 0.0,
            room_hp_r: 0.0,
            room_lp_l: 0.0,
            room_lp_r: 0.0,
            flam_countdown: [0; 8],
            flam_vel: [0.0; 8],
            bus_punch_l,
            bus_punch_r,
            bus_box_l,
            bus_box_r,
            bus_snap_l,
            bus_snap_r,
            bus_hp_l,
            bus_hp_r,
            bus_lp_l,
            bus_lp_r,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        self.pcm = get_or_build_pcm_bank(self.sample_rate);
        let room_cap = ((0.055 * self.sample_rate).ceil() as usize).max(64);
        if self.room_l.len() != room_cap {
            self.room_l = vec![0.0; room_cap];
            self.room_r = vec![0.0; room_cap];
            self.room_pos = 0;
        }
        self.room_ap_l = [
            DelayAllpass::new((0.0037 * self.sample_rate).round() as usize),
            DelayAllpass::new((0.0059 * self.sample_rate).round() as usize),
        ];
        self.room_ap_r = [
            DelayAllpass::new((0.0043 * self.sample_rate).round() as usize),
            DelayAllpass::new((0.0067 * self.sample_rate).round() as usize),
        ];
        self.snare_wire_hp_svf.set_sample_rate(self.sample_rate);
        self.snare_shell_bp.set_sample_rate(self.sample_rate);
        self.snare_wire_ap_r = DelayAllpass::new((0.0019 * self.sample_rate).round() as usize);
        self.clap_bp.set_sample_rate(self.sample_rate);
        self.clap_body.set_sample_rate(self.sample_rate);
        self.clap_bp_r.set_sample_rate(self.sample_rate);
        self.clap_body_r.set_sample_rate(self.sample_rate);
        self.cb_filter.set_sample_rate(self.sample_rate);
        self.ch_hp1.set_sample_rate(self.sample_rate);
        self.ch_hp2.set_sample_rate(self.sample_rate);
        self.oh_hp1.set_sample_rate(self.sample_rate);
        self.oh_hp2.set_sample_rate(self.sample_rate);
        self.cym_hp1.set_sample_rate(self.sample_rate);
        self.cym_hp2.set_sample_rate(self.sample_rate);
        self.cym_bp_low.set_sample_rate(self.sample_rate);
        self.cym_bp_mid.set_sample_rate(self.sample_rate);
        self.symp_snare_bp.set_sample_rate(self.sample_rate);
        self.air_noise_lp_bq.set_lowpass(self.sample_rate, 12_500.0, 0.7071);
        Self::configure_bus_filters(
            self.sample_rate,
            &mut self.bus_punch_l,
            &mut self.bus_punch_r,
            &mut self.bus_box_l,
            &mut self.bus_box_r,
            &mut self.bus_snap_l,
            &mut self.bus_snap_r,
            &mut self.bus_hp_l,
            &mut self.bus_hp_r,
            &mut self.bus_lp_l,
            &mut self.bus_lp_r,
        );
    }

    pub fn trigger(
        &mut self,
        voice: DrumVoiceId,
        velocity: f32,
        accent: bool,
        flam: bool,
        kit: &DrumKitParams,
    ) {
        let vel = (velocity * (1.0 + if accent { kit.accent_level * 0.45 } else { 0.0 }))
            .clamp(0.05, 1.4);
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
                self.kick_phase = 0.02;
                self.kick_amp = 1.0;
                self.kick_pitch_env = 1.0;
                self.kick_punch_env = 1.0;
                self.kick_attack_env = 0.0;
                self.kick_beater_pos = 0;
                self.kick_vel = vel;
                self.kick_rr_pitch = 1.0 + self.rng.next_bipolar() * 0.003;
                self.symp_snare_env = (self.symp_snare_env + vel * 0.28).min(0.65);
            }
            DrumVoiceId::Snare => {
                self.snare_active = true;
                self.snare_p1 = 0.0;
                self.snare_p2 = 0.19;
                self.snare_p3 = 0.41;
                self.snare_body_env = 1.0;
                self.snare_over_env = 1.0;
                self.snare_pitch_env = 1.0;
                self.snare_noise_env = 1.0;
                self.snare_samples = 0;
                self.snare_vel = vel;
                self.snare_rr_pitch = 1.0 + self.rng.next_bipolar() * 0.008;
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
                self.cb_p3 = 0.25;
                self.cb_env = 1.0;
                self.cb_fast_env = 1.0;
                self.cb_crack_pos = 0;
                self.cb_vel = vel;
            }
            DrumVoiceId::ClosedHat => {
                self.ch_active = true;
                self.ch_env = 1.0;
                self.ch_pcm_pos = self.rng.next_f32() * 0.0018 * self.sample_rate;
                self.ch_vel = vel;
                self.ch_rr_pitch = 1.0 + self.rng.next_bipolar() * 0.015;
                // Classic hardware choke: Closed Hat chokes Open Hat!
                self.oh_env *= 0.06;
            }
            DrumVoiceId::OpenHat => {
                self.oh_active = true;
                self.oh_env = 1.0;
                self.oh_pcm_pos = self.rng.next_f32() * 0.0018 * self.sample_rate;
                self.oh_vel = vel;
                self.oh_rr_pitch = 1.0 + self.rng.next_bipolar() * 0.015;
            }
            DrumVoiceId::Tom => {
                self.tom_active = true;
                self.tom_p1 = 0.0;
                self.tom_p2 = 0.21;
                self.tom_p3 = 0.43;
                self.tom_amp = 1.0;
                self.tom_over_env = 1.0;
                self.tom_pitch_env = 1.0;
                self.tom_crack_pos = 0;
                self.tom_vel = vel;
                self.symp_snare_env = (self.symp_snare_env + vel * 0.42).min(0.75);
            }
            DrumVoiceId::Cymbal => {
                self.cym_active = true;
                self.cym_env = 1.0;
                self.cym_splash_env = 1.0;
                self.cym_pcm_pos = 0.0;
                self.cym_vel = vel;
                self.cym_rr_pitch = 1.0 + self.rng.next_bipolar() * 0.015;
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
        let air_hp_a = one_pole_coeff(7_500.0, sr);
        self.air_noise_lp += air_hp_a * (noise - self.air_noise_lp);
        let air_noise = self.air_noise_lp_bq.process(noise - self.air_noise_lp);

        // 808 Schmitt-trigger metallic oscillator bank (only when needed for 808 hats/cymbal)
        let metal_808_needed = (self.ch_active && kit.voices[4].model == DrumModel::Tr808)
            || (self.oh_active && kit.voices[5].model == DrumModel::Tr808)
            || (self.cym_active && kit.voices[7].model == DrumModel::Tr808);
        let mut metal_808 = 0.0_f32;
        if metal_808_needed {
            for i in 0..6 {
                let dt8 = METAL_FREQS_808[i] / sr;
                let p8 = self.metal_phases[i];
                metal_808 += osc_pulse(p8, dt8, 0.5);
                self.metal_phases[i] = (p8 + dt8).fract();
            }
            // TR-808 diode-matrix intermodulation warmth across the 6 metallic oscillators
            metal_808 = soft_tanh(metal_808 * 0.34) * 0.88;
        }

        let mut out_l = 0.0_f32;
        let mut out_r = 0.0_f32;
        let mut room_in_l = 0.0_f32;
        let mut room_in_r = 0.0_f32;

        // 0: Kick (100% dry & tight!)
        if self.kick_active {
            let vp = &kit.voices[0];
            let s = self.process_kick(vp);
            if !vp.mute {
                add_panned(&mut out_l, &mut out_r, s * vp.level * self.kick_vel, vp.pan);
            }
        }
        // 1: Snare (coupled wood-shell body + decorrelated stereo snare wires)
        if self.snare_active {
            let vp = &kit.voices[1];
            let (sn_l, sn_r) = self.process_snare(vp, noise, air_noise);
            if !vp.mute {
                let p = vp.pan.clamp(-1.0, 1.0) * 0.5 + 0.5;
                let gl = (1.0 - p).sqrt() * 1.414;
                let gr = p.sqrt() * 1.414;
                let gain = vp.level * self.snare_vel;
                let sl = sn_l * gain * gl;
                let sr_v = sn_r * gain * gr;
                out_l += sl;
                out_r += sr_v;
                let room_send = match vp.model {
                    DrumModel::Tr909 => 0.26 + vp.decay * 0.24,
                    DrumModel::Acoustic => 0.32 + vp.decay * 0.24,
                    DrumModel::Linn80s => 0.28 + vp.decay * 0.22,
                    DrumModel::Tr808 => 0.18 + vp.decay * 0.14,
                };
                room_in_l += sl * room_send;
                room_in_r += sr_v * room_send;
            }
        }
        // 2: Clap (true stereo spread tail + studio room)
        if self.clap_active {
            let vp = &kit.voices[2];
            let (cl, cr) = self.process_clap(vp, noise, air_noise);
            if !vp.mute {
                let p = vp.pan.clamp(-1.0, 1.0) * 0.5 + 0.5;
                let gl = (1.0 - p).sqrt() * 1.414;
                let gr = p.sqrt() * 1.414;
                let gain = vp.level * self.clap_vel;
                let sl = cl * gain * gl;
                let sr_v = cr * gain * gr;
                out_l += sl;
                out_r += sr_v;
                room_in_l += sl * 0.30;
                room_in_r += sr_v * 0.30;
            }
        }
        // 3: Cowbell / Rim
        if self.cb_active {
            let vp = &kit.voices[3];
            let s = self.process_cowbell_rim(vp);
            if !vp.mute {
                let sig = s * vp.level * self.cb_vel;
                let (sl, sr_v) = pan_split(sig, vp.pan);
                out_l += sl;
                out_r += sr_v;
                room_in_l += sl * 0.14;
                room_in_r += sr_v * 0.14;
            }
        }
        // 4: Closed Hat (stereo bronze overheads, 100% mono-compatible)
        if self.ch_active {
            let vp = &kit.voices[4];
            let (hl, hr) = self.process_ch(vp, metal_808, air_noise);
            if !vp.mute {
                let p = vp.pan.clamp(-1.0, 1.0) * 0.5 + 0.5;
                let gl = (1.0 - p).sqrt() * 1.414;
                let gr = p.sqrt() * 1.414;
                let gain = vp.level * self.ch_vel;
                let sl = hl * gain * gl;
                let sr_v = hr * gain * gr;
                out_l += sl;
                out_r += sr_v;
                room_in_l += sl * 0.09;
                room_in_r += sr_v * 0.09;
            }
        }
        // 5: Open Hat (stereo bronze overheads, 100% mono-compatible)
        if self.oh_active {
            let vp = &kit.voices[5];
            let (hl, hr) = self.process_oh(vp, metal_808, air_noise);
            if !vp.mute {
                let p = vp.pan.clamp(-1.0, 1.0) * 0.5 + 0.5;
                let gl = (1.0 - p).sqrt() * 1.414;
                let gr = p.sqrt() * 1.414;
                let gain = vp.level * self.oh_vel;
                let sl = hl * gain * gl;
                let sr_v = hr * gain * gr;
                out_l += sl;
                out_r += sr_v;
                room_in_l += sl * 0.12;
                room_in_r += sr_v * 0.12;
            }
        }
        // 6: Tom
        if self.tom_active {
            let vp = &kit.voices[6];
            let s = self.process_tom(vp);
            if !vp.mute {
                let sig = s * vp.level * self.tom_vel;
                let (sl, sr_v) = pan_split(sig, vp.pan);
                out_l += sl;
                out_r += sr_v;
                room_in_l += sl * 0.28;
                room_in_r += sr_v * 0.28;
            }
        }
        // 7: Crash / Ride Cymbal (true wide stereo studio overheads + room bloom)
        if self.cym_active {
            let vp = &kit.voices[7];
            let (cl, cr) = self.process_cymbal(vp, metal_808, air_noise);
            if !vp.mute {
                let p = vp.pan.clamp(-1.0, 1.0) * 0.5 + 0.5;
                let gl = (1.0 - p).sqrt() * 1.414;
                let gr = p.sqrt() * 1.414;
                let gain = vp.level * self.cym_vel;
                let sl = cl * gain * gl;
                let sr_v = cr * gain * gr;
                out_l += sl;
                out_r += sr_v;
                room_in_l += sl * 0.26;
                room_in_r += sr_v * 0.26;
            }
        }

        // Physical sympathetic snare-wire buzz when Kick / Tom excites the studio kit!
        if self.symp_snare_env > 0.0006 {
            let rattle = self.symp_snare_bp.process_bp(noise, 2_850.0, 0.34) * self.symp_snare_env;
            self.symp_snare_env *= (-1.0 / (0.026 * sr)).exp();
            if !kit.voices[1].mute && !self.snare_active {
                let buzz = rattle * kit.voices[1].snappy * 0.038;
                out_l += buzz * 1.08;
                out_r += buzz * 0.92;
                room_in_l += buzz * 0.35;
                room_in_r += buzz * 0.35;
            }
        }

        // Studio Diffused Early-Reflection Stereo Room (2-stage allpass + 4 prime taps, 220Hz..5.2kHz)
        let r_len = self.room_l.len();
        let hp_r_a = one_pole_coeff(220.0, sr);
        let lp_r_a = one_pole_coeff(5_200.0, sr);
        self.room_hp_l += hp_r_a * (room_in_l - self.room_hp_l);
        self.room_hp_r += hp_r_a * (room_in_r - self.room_hp_r);
        self.room_lp_l += lp_r_a * ((room_in_l - self.room_hp_l) - self.room_lp_l);
        self.room_lp_r += lp_r_a * ((room_in_r - self.room_hp_r) - self.room_lp_r);
        let d0_l = self.room_ap_l[0].process(self.room_lp_l, 0.55);
        let diff_l = self.room_ap_l[1].process(d0_l, 0.55);
        let d0_r = self.room_ap_r[0].process(self.room_lp_r, 0.55);
        let diff_r = self.room_ap_r[1].process(d0_r, 0.55);
        self.room_l[self.room_pos] = diff_l;
        self.room_r[self.room_pos] = diff_r;

        let mut er_l = 0.0_f32;
        let mut er_r = 0.0_f32;
        const TAP_WEIGHTS: [f32; 4] = [0.34, 0.26, 0.20, 0.14];
        for t in 0..4 {
            let dl = ((ROOM_TAPS_L_MS[t] * 0.001 * sr).round() as usize).min(r_len - 1);
            let dr = ((ROOM_TAPS_R_MS[t] * 0.001 * sr).round() as usize).min(r_len - 1);
            er_l += self.room_l[(self.room_pos + r_len - dl) % r_len] * TAP_WEIGHTS[t];
            er_r += self.room_r[(self.room_pos + r_len - dr) % r_len] * TAP_WEIGHTS[t];
        }
        self.room_pos = (self.room_pos + 1) % r_len;
        out_l += er_l * 0.52 + er_r * 0.18;
        out_r += er_r * 0.52 + er_l * 0.18;

        // Studio drum bus console contour (95Hz punch, 380Hz box carve, 4.2kHz snap)
        // followed by 28 Hz subsonic HPF + 14.5 kHz studio silk LPF + transient ceiling
        let eq_l = self.bus_snap_l.process(self.bus_box_l.process(self.bus_punch_l.process(out_l)));
        let eq_r = self.bus_snap_r.process(self.bus_box_r.process(self.bus_punch_r.process(out_r)));
        let dry_l = self.bus_lp_l.process(self.bus_hp_l.process(eq_l));
        let dry_r = self.bus_lp_r.process(self.bus_hp_r.process(eq_r));
        (
            soft_ceiling(dry_l, 0.98, 0.38),
            soft_ceiling(dry_r, 0.98, 0.38),
        )
    }

    #[inline]
    fn sample_beater(&mut self) -> f32 {
        let len = self.pcm.kick_beater_pcm.len();
        if self.kick_beater_pos < len {
            let s = self.pcm.kick_beater_pcm[self.kick_beater_pos];
            self.kick_beater_pos += 1;
            s
        } else {
            0.0
        }
    }

    #[inline]
    fn read_pcm_varispeed(buf: &[f32], pos: f32) -> Option<f32> {
        let len = buf.len();
        if pos < 0.0 || pos >= (len.saturating_sub(1)) as f32 {
            return None;
        }
        let i0 = pos as usize;
        let i1 = (i0 + 1).min(len - 1);
        let frac = pos - i0 as f32;
        Some(buf[i0] * (1.0 - frac) + buf[i1] * frac)
    }

    #[inline]
    fn process_kick(&mut self, vp: &DrumVoiceParams) -> f32 {
        let sr = self.sample_rate;
        let tune_mult = 2.0_f32.powf(vp.tune.clamp(-12.0, 12.0) / 12.0) * self.kick_rr_pitch;
        let (base_hz, punch_hz, body_sweep_hz, decay_s, punch_dec_s, pitch_dec_s, drive, beater_amt) =
            match vp.model {
                DrumModel::Tr808 => (
                    48.0 * tune_mult,
                    108.0 * tune_mult,
                    28.0 * tune_mult,
                    0.13 + vp.decay * 0.42,
                    0.0115,
                    0.026 + vp.tone * 0.012,
                    1.38 + vp.tone * 0.65,
                    0.26 + vp.snappy * 0.28,
                ),
                DrumModel::Tr909 => (
                    52.5 * tune_mult,
                    135.0 * tune_mult,
                    35.0 * tune_mult,
                    0.10 + vp.decay * 0.30,
                    0.0095,
                    0.022 + vp.tone * 0.010,
                    1.32 + vp.tone * 0.72,
                    0.34 + vp.snappy * 0.42,
                ),
                DrumModel::Acoustic => (
                    58.0 * tune_mult,
                    95.0 * tune_mult,
                    26.0 * tune_mult,
                    0.11 + vp.decay * 0.22,
                    0.0045,
                    0.018 + vp.tone * 0.008,
                    1.12 + vp.tone * 0.42,
                    0.42 + vp.snappy * 0.45,
                ),
                DrumModel::Linn80s => (
                    62.0 * tune_mult,
                    120.0 * tune_mult,
                    32.0 * tune_mult,
                    0.09 + vp.decay * 0.20,
                    0.0060,
                    0.016 + vp.tone * 0.008,
                    1.38 + vp.tone * 0.65,
                    0.38 + vp.snappy * 0.48,
                ),
            };

        let freq = base_hz + punch_hz * self.kick_punch_env + body_sweep_hz * self.kick_pitch_env;
        self.kick_phase = (self.kick_phase + freq / sr).fract();
        let body = (self.kick_phase * TAU).sin();

        let atk_coeff = 1.0 - (-1.0 / (0.0004 * sr)).exp();
        self.kick_attack_env += atk_coeff * (1.0 - self.kick_attack_env);
        let amp = self.kick_amp * self.kick_attack_env;

        let beater = self.sample_beater() * beater_amt;
        let sat = 1.5 * soft_tanh((body * amp * drive + 0.07 * body * body * amp) / 1.5);
        // Dynamic analog kick-body low-pass contour: crisp transient punch -> velvety pure sub tail
        let body_lp_hz = 200.0
            + vp.tone * 220.0
            + 2200.0 * self.kick_punch_env
            + 560.0 * self.kick_pitch_env;
        self.kick_tone_lp += one_pole_coeff(body_lp_hz, sr) * (sat - self.kick_tone_lp);
        let sub_clean = body * amp * 0.30;

        self.kick_amp *= (-1.0 / (decay_s * sr)).exp();
        self.kick_punch_env *= (-1.0 / (punch_dec_s * sr)).exp();
        self.kick_pitch_env *= (-1.0 / (pitch_dec_s * sr)).exp();

        if self.kick_amp < 0.0005 {
            self.kick_active = false;
        }
        let raw = self.kick_tone_lp * 0.68 + sub_clean + beater * 0.38;
        self.kick_dc += one_pole_coeff(22.0, sr) * (raw - self.kick_dc);
        soft_ceiling(raw - self.kick_dc, 1.15, 0.35)
    }

    #[inline]
    fn process_snare(&mut self, vp: &DrumVoiceParams, noise: f32, air_noise: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let tune_mult = 2.0_f32.powf(vp.tune.clamp(-12.0, 12.0) / 12.0) * self.snare_rr_pitch;
        let vel_tone = (vp.tone + (self.snare_vel - 0.85) * 0.20).clamp(0.0, 1.0);
        let (f1, body_dec, over_dec, noise_dec, hp_hz, lp_hz) = match vp.model {
            DrumModel::Tr808 => (
                182.0 * tune_mult,
                0.095 + vp.decay * 0.12,
                0.028,
                0.068 + vp.decay * 0.16,
                980.0,
                6400.0 + vel_tone * 6200.0,
            ),
            DrumModel::Tr909 => (
                198.0 * tune_mult,
                0.100 + vp.decay * 0.14,
                0.030,
                0.082 + vp.decay * 0.22,
                1040.0,
                7600.0 + vel_tone * 6600.0,
            ),
            DrumModel::Acoustic => (
                210.0 * tune_mult,
                0.105 + vp.decay * 0.15,
                0.036,
                0.095 + vp.decay * 0.24,
                900.0,
                8000.0 + vel_tone * 6600.0,
            ),
            DrumModel::Linn80s => (
                178.0 * tune_mult,
                0.118 + vp.decay * 0.17,
                0.042,
                0.115 + vp.decay * 0.26,
                800.0,
                9600.0 + vel_tone * 6800.0,
            ),
        };
        let pitch_bend = 1.0 + 0.16 * self.snare_pitch_env;
        self.snare_p1 = (self.snare_p1 + (f1 * pitch_bend) / sr).fract();
        self.snare_p2 = (self.snare_p2 + (f1 * 1.59 * pitch_bend) / sr).fract();
        self.snare_p3 = (self.snare_p3 + (f1 * 2.14 * pitch_bend) / sr).fract();

        let shell = (self.snare_p1 * TAU).sin() * self.snare_body_env * 0.74
            + (self.snare_p2 * TAU).sin() * self.snare_over_env * 0.24
            + (self.snare_p3 * TAU).sin() * self.snare_over_env * 0.12;

        // Broadband crisp snare wires (12dB SVF HPF + exact 1-pole LPF)
        let wire_hp = self.snare_wire_hp_svf.process_hp(noise, hp_hz, 0.03);
        let lp_a = one_pole_coeff(lp_hz, sr);
        self.snare_wire_lp += lp_a * (wire_hp - self.snare_wire_lp);

        let t_s = self.snare_samples as f32 / sr;
        let wire_bloom = 1.0 - (-t_s / 0.0024).exp();
        let wires = self.snare_wire_lp * self.snare_noise_env * wire_bloom * (0.36 + vp.snappy * 0.82);

        // Coupled wooden drum-shell cavity resonance (~360-400 Hz) excited by skin + bottom wire rattle
        let shell_wood = self.snare_shell_bp.process_bp(
            noise * self.snare_body_env * 0.55 + shell * 0.50 + wires * 0.14,
            (f1 * 1.92).clamp(280.0, 560.0),
            0.36,
        ) * 0.28;
        let body = 1.4 * soft_tanh((shell + shell_wood) * 1.26 / 1.4);

        let crack_idx = self.snare_samples as usize;
        let base_crack = if crack_idx < self.pcm.snare_crack_pcm.len() {
            self.pcm.snare_crack_pcm[crack_idx] * (0.54 + vp.snappy * 0.56)
        } else {
            0.0
        };
        let crack = match vp.model {
            DrumModel::Acoustic => {
                let shell_hit = if crack_idx < self.pcm.acoustic_shell_pcm.len() {
                    self.pcm.acoustic_shell_pcm[crack_idx] * (0.48 + vp.snappy * 0.48)
                } else {
                    0.0
                };
                base_crack * 0.65 + shell_hit
            }
            DrumModel::Linn80s => {
                let linn_hit = if crack_idx < self.pcm.linn_crack_pcm.len() {
                    self.pcm.linn_crack_pcm[crack_idx] * (0.58 + vp.snappy * 0.54)
                } else {
                    0.0
                };
                base_crack * 0.45 + linn_hit
            }
            _ => base_crack,
        };
        let air = if vp.model != DrumModel::Tr808 {
            air_noise * self.snare_noise_env * (0.29 + vel_tone * 0.31) * (0.35 + vp.snappy * 0.65)
        } else {
            air_noise * self.snare_noise_env * (0.14 + vel_tone * 0.16) * (0.35 + vp.snappy * 0.60)
        };

        self.snare_samples = self.snare_samples.saturating_add(1);
        self.snare_body_env *= (-1.0 / (body_dec * sr)).exp();
        self.snare_over_env *= (-1.0 / (over_dec * sr)).exp();
        self.snare_pitch_env *= (-1.0 / (0.008 * sr)).exp();
        self.snare_noise_env *= (-1.0 / (noise_dec * sr)).exp();
        if self.snare_body_env < 0.0005 && self.snare_noise_env < 0.0005 {
            self.snare_active = false;
        }
        let base_body_w = match vp.model {
            DrumModel::Tr909 => 0.78,
            DrumModel::Acoustic => 0.80,
            DrumModel::Linn80s => 0.75,
            DrumModel::Tr808 => 0.74,
        };
        let body_w = base_body_w + 0.08 * (self.snare_vel - 0.85).clamp(-0.3, 0.3);
        let center = body * body_w + crack;
        // Mid/Side stereo snare wire decorrelation: 3D stereo width with 100% phase-clean mono sum!
        let wire_mid = (wires + air) * 0.92;
        let wire_side = self.snare_wire_ap_r.process(wires + air, 0.55) * 0.28;
        (
            soft_ceiling(center + wire_mid + wire_side, 1.15, 0.35),
            soft_ceiling(center + wire_mid - wire_side, 1.15, 0.35),
        )
    }

    #[inline]
    fn process_clap(&mut self, vp: &DrumVoiceParams, noise: f32, air_noise: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let t_ms = (self.clap_samples as f32 / sr) * 1000.0;
        self.clap_samples = self.clap_samples.saturating_add(1);

        // Tight, snappy stereo multi-hand clap bursts (crisp hip-hop/club slap!)
        let burst_l = if t_ms < 6.2 {
            (-t_ms / 2.8).exp()
        } else if t_ms < 13.5 {
            (-(t_ms - 6.2) / 3.0).exp()
        } else if t_ms < 21.5 {
            (-(t_ms - 13.5) / 3.2).exp()
        } else if t_ms < 33.0 {
            (-(t_ms - 21.5) / 4.2).exp()
        } else {
            0.0
        };
        let burst_r = if t_ms >= 0.9 && t_ms < 7.1 {
            (-(t_ms - 0.9) / 2.8).exp()
        } else if t_ms >= 7.1 && t_ms < 14.4 {
            (-(t_ms - 7.1) / 3.0).exp()
        } else if t_ms >= 14.4 && t_ms < 22.5 {
            (-(t_ms - 14.4) / 3.2).exp()
        } else if t_ms >= 22.5 && t_ms < 34.2 {
            (-(t_ms - 22.5) / 4.2).exp()
        } else {
            0.0
        };

        let tail_dec = match vp.model {
            DrumModel::Tr808 => 0.032 + vp.decay * 0.095,
            DrumModel::Tr909 => 0.030 + vp.decay * 0.095,
            DrumModel::Acoustic => 0.036 + vp.decay * 0.11,
            DrumModel::Linn80s => 0.034 + vp.decay * 0.105,
        };
        if t_ms >= 21.5 {
            self.clap_tail_env *= (-1.0 / (tail_dec * sr)).exp();
        }
        let env_l = (burst_l * 0.96 + self.clap_tail_env * 0.42).min(1.0);
        let env_r = (burst_r * 0.96 + self.clap_tail_env * 0.42).min(1.0);

        let noise_r = self.rng.next_bipolar();
        let hp_a = one_pole_coeff(460.0, sr);
        self.clap_hp += hp_a * (noise - self.clap_hp);
        self.clap_hp_r += hp_a * (noise_r - self.clap_hp_r);
        let wide_l = noise - self.clap_hp;
        let wide_r = noise_r - self.clap_hp_r;
        let bp_freq = match vp.model {
            DrumModel::Tr808 => 1080.0 + vp.tone * 580.0,
            DrumModel::Tr909 => 1160.0 + vp.tone * 620.0,
            DrumModel::Acoustic => 1050.0 + vp.tone * 540.0,
            DrumModel::Linn80s => 1120.0 + vp.tone * 560.0,
        };
        let filt_l = self.clap_bp.process_bp(wide_l, bp_freq, 0.24) * 1.45;
        let palm_l = self.clap_body.process_bp(wide_l, 590.0 + vp.tone * 80.0, 0.20) * 0.95;
        let filt_r = self.clap_bp_r.process_bp(wide_r, bp_freq * 1.03, 0.24) * 1.45;
        let palm_r = self.clap_body_r.process_bp(wide_r, 610.0 + vp.tone * 80.0, 0.20) * 0.95;
        let crisp_air_l = air_noise * (burst_l * 0.12 + self.clap_tail_env * 0.08) * (0.50 + vp.tone * 0.30) * (0.45 + vp.snappy * 0.45);
        let crisp_air_r = air_noise * (burst_r * 0.11 + self.clap_tail_env * 0.08) * (0.50 + vp.tone * 0.30) * (0.45 + vp.snappy * 0.45);

        let sl = soft_ceiling(((filt_l + palm_l) * env_l * (0.80 + vp.snappy * 0.30)) + crisp_air_l, 1.15, 0.35);
        let sr_v = soft_ceiling(((filt_r + palm_r) * env_r * (0.80 + vp.snappy * 0.30)) + crisp_air_r, 1.15, 0.35);

        if t_ms > 34.5 && self.clap_tail_env < 0.0008 {
            self.clap_active = false;
        }
        (sl, sr_v)
    }

    #[inline]
    fn process_cowbell_rim(&mut self, vp: &DrumVoiceParams) -> f32 {
        let sr = self.sample_rate;
        let tune_mult = 2.0_f32.powf(vp.tune.clamp(-12.0, 12.0) / 12.0);
        match vp.model {
            DrumModel::Tr808 => {
                let dt1 = (540.0 * tune_mult) / sr;
                let dt2 = (800.0 * tune_mult) / sr;
                self.cb_p1 = (self.cb_p1 + dt1).fract();
                self.cb_p2 = (self.cb_p2 + dt2).fract();
                let sq = osc_pulse(self.cb_p1, dt1, 0.5) + osc_pulse(self.cb_p2, dt2, 0.5);
                let bp = self.cb_filter.process_bp(sq * 0.5, 720.0 + vp.tone * 380.0, 0.07);
                self.cb_fast_env *= (-1.0 / (0.025 * sr)).exp();
                self.cb_env *= (-1.0 / ((0.045 + vp.decay * 0.20) * sr)).exp();
                if self.cb_env < 0.0005 {
                    self.cb_active = false;
                }
                soft_tanh(bp * (self.cb_fast_env * 0.7 + self.cb_env * 0.45) * 0.58)
            }
            DrumModel::Tr909 | DrumModel::Acoustic | DrumModel::Linn80s => {
                let (f1, f2, f3, bp_base, dec_base) = match vp.model {
                    DrumModel::Acoustic => (340.0, 780.0, 1620.0, 760.0, 0.018),
                    DrumModel::Linn80s => (520.0, 1340.0, 2680.0, 980.0, 0.012),
                    _ => (440.0, 1180.0, 2450.0, 920.0, 0.014),
                };
                self.cb_p1 = (self.cb_p1 + (f1 * tune_mult) / sr).fract();
                self.cb_p2 = (self.cb_p2 + (f2 * tune_mult) / sr).fract();
                self.cb_p3 = (self.cb_p3 + (f3 * tune_mult) / sr).fract();
                let wood = (self.cb_p1 * TAU).sin() * 0.52
                    + (self.cb_p2 * TAU).sin() * 0.36
                    + (self.cb_p3 * TAU).sin() * 0.22;
                let bp = self.cb_filter.process_bp(wood, bp_base + vp.tone * 780.0, 0.04);
                let crack_src = match vp.model {
                    DrumModel::Acoustic => &self.pcm.acoustic_shell_pcm,
                    DrumModel::Linn80s => &self.pcm.linn_crack_pcm,
                    _ => &self.pcm.snare_crack_pcm,
                };
                let crack = if self.cb_crack_pos < crack_src.len() {
                    let s = crack_src[self.cb_crack_pos];
                    self.cb_crack_pos += 1;
                    s * 0.45
                } else {
                    0.0
                };
                self.cb_env *= (-1.0 / ((dec_base + vp.decay * 0.05) * sr)).exp();
                if self.cb_env < 0.0005 {
                    self.cb_active = false;
                }
                soft_tanh(bp * self.cb_env * 1.15 + crack)
            }
        }
    }

    #[inline]
    fn process_ch(&mut self, vp: &DrumVoiceParams, metal_808: f32, air_noise: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let dec = (0.018 + vp.decay * 0.095) * (0.80 + 0.28 * self.ch_vel.clamp(0.2, 1.2));
        let vel_tone = (vp.tone + (self.ch_vel - 0.85) * 0.22).clamp(0.0, 1.0);
        let (mid, side) = match vp.model {
            DrumModel::Tr909 | DrumModel::Acoustic | DrumModel::Linn80s => {
                let speed = 2.0_f32.powf(vp.tune.clamp(-12.0, 12.0) / 12.0) * self.ch_rr_pitch;
                let pcm_m = Self::read_pcm_varispeed(&self.pcm.hat_bronze_909, self.ch_pcm_pos)
                    .unwrap_or(0.0);
                let pcm_s = Self::read_pcm_varispeed(&self.pcm.hat_bronze_side, self.ch_pcm_pos)
                    .unwrap_or(0.0);
                self.ch_pcm_pos += speed;
                let hp_base = match vp.model {
                    DrumModel::Acoustic => 2800.0,
                    DrumModel::Linn80s => 3500.0,
                    _ => 4200.0,
                };
                let hp1 = self.ch_hp1.process_hp(pcm_m, hp_base + vel_tone * 2000.0, 0.02);
                let lp_hz = (5800.0 + vel_tone * 8200.0).min(sr * 0.44);
                let lp_a = one_pole_coeff(lp_hz, sr);
                self.ch_tone_lp += lp_a * (hp1 - self.ch_tone_lp);
                let mut m = self.ch_tone_lp * self.ch_env * 1.38 + air_noise * self.ch_env * 0.08;
                if vp.model == DrumModel::Linn80s {
                    m = m * 0.65 + ((m * 18.0).round() / 18.0) * 0.35;
                }
                let s = pcm_s * self.ch_env * 0.34;
                (m, s)
            }
            DrumModel::Tr808 => {
                let hp1 = self.ch_hp1.process_hp(metal_808, 5000.0 + vel_tone * 2000.0, 0.02);
                let hp2 = self.ch_hp2.process_hp(hp1, 5000.0 + vel_tone * 2000.0, 0.02);
                (hp2 * self.ch_env * 2.25, air_noise * self.ch_env * 0.06)
            }
        };
        self.ch_env *= (-1.0 / (dec * sr)).exp();
        if self.ch_env < 0.0005 {
            self.ch_active = false;
        }
        (
            soft_ceiling(mid + side, 1.15, 0.35),
            soft_ceiling(mid - side, 1.15, 0.35),
        )
    }

    #[inline]
    fn process_oh(&mut self, vp: &DrumVoiceParams, metal_808: f32, air_noise: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let dec = (0.10 + vp.decay * 0.75) * (0.80 + 0.28 * self.oh_vel.clamp(0.2, 1.2));
        let vel_tone = (vp.tone + (self.oh_vel - 0.85) * 0.22).clamp(0.0, 1.0);
        let (mid, side) = match vp.model {
            DrumModel::Tr909 | DrumModel::Acoustic | DrumModel::Linn80s => {
                let speed = 2.0_f32.powf(vp.tune.clamp(-12.0, 12.0) / 12.0) * self.oh_rr_pitch;
                let pcm_m = Self::read_pcm_varispeed(&self.pcm.hat_bronze_909, self.oh_pcm_pos)
                    .unwrap_or(0.0);
                let pcm_s = Self::read_pcm_varispeed(&self.pcm.hat_bronze_side, self.oh_pcm_pos)
                    .unwrap_or(0.0);
                self.oh_pcm_pos += speed;
                let hp_base = match vp.model {
                    DrumModel::Acoustic => 2600.0,
                    DrumModel::Linn80s => 3200.0,
                    _ => 3800.0,
                };
                let hp1 = self.oh_hp1.process_hp(pcm_m, hp_base + vel_tone * 1800.0, 0.02);
                let lp_hz = (5800.0 + vel_tone * 8200.0).min(sr * 0.44);
                let lp_a = one_pole_coeff(lp_hz, sr);
                self.oh_tone_lp += lp_a * (hp1 - self.oh_tone_lp);
                let mut m = self.oh_tone_lp * self.oh_env * 1.32 + air_noise * self.oh_env * 0.07;
                if vp.model == DrumModel::Linn80s {
                    m = m * 0.65 + ((m * 18.0).round() / 18.0) * 0.35;
                }
                let s = pcm_s * self.oh_env * 0.44;
                (m, s)
            }
            DrumModel::Tr808 => {
                let hp1 = self.oh_hp1.process_hp(metal_808, 4500.0 + vel_tone * 1800.0, 0.02);
                let hp2 = self.oh_hp2.process_hp(hp1, 4500.0 + vel_tone * 1800.0, 0.02);
                (hp2 * self.oh_env * 2.05, air_noise * self.oh_env * 0.08)
            }
        };
        self.oh_env *= (-1.0 / (dec * sr)).exp();
        if self.oh_env < 0.0005 {
            self.oh_active = false;
        }
        (
            soft_ceiling(mid + side, 1.15, 0.35),
            soft_ceiling(mid - side, 1.15, 0.35),
        )
    }

    #[inline]
    fn process_tom(&mut self, vp: &DrumVoiceParams) -> f32 {
        let sr = self.sample_rate;
        let base_hz = 110.0 * 2.0_f32.powf(vp.tune.clamp(-12.0, 12.0) / 12.0);
        let bend = match vp.model {
            DrumModel::Tr808 => 0.28,
            DrumModel::Tr909 => 0.38,
            DrumModel::Acoustic => 0.16,
            DrumModel::Linn80s => 0.32,
        };
        let f1 = base_hz * (1.0 + bend * self.tom_pitch_env);
        self.tom_p1 = (self.tom_p1 + f1 / sr).fract();
        self.tom_p2 = (self.tom_p2 + (f1 * 1.59) / sr).fract();
        self.tom_p3 = (self.tom_p3 + (f1 * 2.14) / sr).fract();

        let head = (self.tom_p1 * TAU).sin() * self.tom_amp * 0.80
            + (self.tom_p2 * TAU).sin() * self.tom_over_env * 0.16
            + (self.tom_p3 * TAU).sin() * self.tom_over_env * 0.08;
        let body = 1.35 * soft_tanh((head * (1.14 + vp.tone * 0.32) + 0.05 * head * head) / 1.35);
        let stick = if self.tom_crack_pos < self.pcm.snare_crack_pcm.len() {
            let s = self.pcm.snare_crack_pcm[self.tom_crack_pos];
            self.tom_crack_pos += 1;
            s * (0.14 + vp.snappy * 0.22)
        } else {
            0.0
        };

        let dec = 0.14 + vp.decay * 0.46;
        self.tom_amp *= (-1.0 / (dec * sr)).exp();
        self.tom_over_env *= (-1.0 / (0.052 * sr)).exp();
        self.tom_pitch_env *= (-1.0 / (0.016 * sr)).exp();
        if self.tom_amp < 0.0005 {
            self.tom_active = false;
        }
        soft_ceiling((body + stick) * 0.78, 1.15, 0.35)
    }

    #[inline]
    fn process_cymbal(&mut self, vp: &DrumVoiceParams, metal_808: f32, air_noise: f32) -> (f32, f32) {
        let sr = self.sample_rate;
        let dec = 0.45 + vp.decay * 1.65;
        let vel_tone = (vp.tone + (self.cym_vel - 0.85) * 0.22).clamp(0.0, 1.0);
        let speed = 2.0_f32.powf(vp.tune.clamp(-12.0, 12.0) / 12.0) * self.cym_rr_pitch;
        let pcm_m = Self::read_pcm_varispeed(&self.pcm.cym_bronze_crash, self.cym_pcm_pos)
            .unwrap_or(0.0);
        let pcm_s = Self::read_pcm_varispeed(&self.pcm.cym_bronze_side, self.cym_pcm_pos)
            .unwrap_or(0.0);
        self.cym_pcm_pos += speed;

        let (mid, side) = match vp.model {
            DrumModel::Tr909 | DrumModel::Acoustic | DrumModel::Linn80s => {
                // Full-bodied hammered crash: keep warm 420-3500 Hz bronze wash & bell intact!
                let hp_base = match vp.model {
                    DrumModel::Acoustic => 420.0,
                    DrumModel::Linn80s => 680.0,
                    _ => 540.0,
                };
                let hp1 = self.cym_hp1.process_hp(pcm_m, hp_base + vel_tone * 440.0, 0.05);
                let lp_hz = (7_400.0 + vel_tone * 7_200.0).min(sr * 0.44);
                let lp_a = one_pole_coeff(lp_hz, sr);
                self.cym_tone_lp += lp_a * (hp1 - self.cym_tone_lp);
                self.cym_side_lp += lp_a * (pcm_s - self.cym_side_lp);
                let env_gain = self.cym_env * 0.86 + self.cym_splash_env * 0.36;
                let m = self.cym_tone_lp * env_gain * 1.12 + air_noise * self.cym_splash_env * 0.08;
                let s = self.cym_side_lp * env_gain * 0.92;
                (m, s)
            }
            DrumModel::Tr808 => {
                // Authentic TR-808 high-pass metallic sizzle resonator + stereo bronze plate shimmer (zero pitched comb!)
                let mid_sizzle = self.cym_bp_mid.process_bp(metal_808, 3_250.0 + vel_tone * 950.0, 0.24)
                    * (self.cym_splash_env * 0.45 + self.cym_env * 0.75);
                let hi_sizzle = self.cym_hp2.process_hp(
                    self.cym_hp1.process_hp(metal_808, 5_600.0, 0.02),
                    5_600.0,
                    0.02,
                ) * self.cym_env
                    * 1.25;
                let bronze_tail = pcm_m * self.cym_env * 0.32;
                let m = mid_sizzle * 0.58 + hi_sizzle * 0.78 + bronze_tail + air_noise * self.cym_splash_env * 0.12;
                let s = pcm_s * self.cym_env * 0.48;
                (m, s)
            }
        };
        self.cym_splash_env *= (-1.0 / (0.068 * sr)).exp();
        self.cym_env *= (-1.0 / (dec * sr)).exp();
        if self.cym_env < 0.0005 || self.cym_pcm_pos >= (self.pcm.cym_bronze_crash.len() - 2) as f32 {
            self.cym_active = false;
        }
        (
            soft_ceiling(mid + side, 1.15, 0.35),
            soft_ceiling(mid - side, 1.15, 0.35),
        )
    }
}

#[inline]
fn pan_split(sig: f32, pan: f32) -> (f32, f32) {
    let p = pan.clamp(-1.0, 1.0) * 0.5 + 0.5;
    let gl = (1.0 - p).sqrt() * 1.414;
    let gr = p.sqrt() * 1.414;
    (sig * gl, sig * gr)
}

#[inline]
fn add_panned(out_l: &mut f32, out_r: &mut f32, sig: f32, pan: f32) {
    let (sl, sr) = pan_split(sig, pan);
    *out_l += sl;
    *out_r += sr;
}
