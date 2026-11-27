use std::f32::consts::TAU;
use std::sync::Arc;

use crate::model::{midi_to_hz, MoogParams, MoogWave, MotionLocks, SynthEngine};

use super::{
    one_pole_coeff, osc_analog_pulse, osc_analog_saw, osc_sub_warm, osc_tri_aa, osc_trisaw,
    soft_ceiling, soft_tanh, AdsrEnv, AmpCabSim, AnalogDrift, FmSynth, MoogLadderFilter,
    MultiSampleSynth, SoundFontBank, SoundFontSynth, StereoPhaser, WavetableSynth, Xorshift32,
};

const MAX_VOICES: usize = 8;
const VOICE_PANS: [f32; MAX_VOICES] = [-0.42, 0.42, -0.28, 0.28, -0.48, 0.48, -0.18, 0.18];

#[derive(Debug, Clone)]
struct MoogVoice {
    active: bool,
    held: bool,
    slide_active: bool,
    note: u8,
    current_midi: f32,
    target_midi: f32,
    velocity: f32,
    accent: f32,
    age: u64,
    phase1: f32,
    phase2: f32,
    phase3: f32,
    phase1_b: f32,
    phase1_c: f32,
    phase2_b: f32,
    phase3_b: f32,
    sub_phase: f32,
    pwm_phase: f32,
    pitch_env: f32,
    drift_phase: f32,
    drift_rate: f32,
    pitch_tol_cents: f32,
    cutoff_tol: f32,
    env_rate_tol: f32,
    note_age_s: f32,
    voice_pan: f32,
    filter: MoogLadderFilter,
    filter_r: MoogLadderFilter,
    filt_env: AdsrEnv,
    amp_env: AdsrEnv,
    dc_l: f32,
    dc_r: f32,
    side_hp: f32,
    thermal: AnalogDrift,
    pluck_env: f32,
    pluck_lp: f32,
    svt_hp: f32,
    svt_lp: f32,
    svt_dc: f32,
}

impl MoogVoice {
    fn new(sr: f32, idx: usize) -> Self {
        let fi = idx as f32;
        Self {
            active: false,
            held: false,
            slide_active: false,
            note: 48,
            current_midi: 48.0,
            target_midi: 48.0,
            velocity: 0.8,
            accent: 0.0,
            age: 0,
            phase1: (fi * 0.137).fract(),
            phase2: (fi * 0.293 + 0.19).fract(),
            phase3: (fi * 0.419 + 0.53).fract(),
            phase1_b: (fi * 0.521 + 0.11).fract(),
            phase1_c: (fi * 0.677 + 0.43).fract(),
            phase2_b: (fi * 0.613 + 0.31).fract(),
            phase3_b: (fi * 0.781 + 0.71).fract(),
            sub_phase: (fi * 0.111).fract(),
            pwm_phase: (fi * 0.271).fract(),
            pitch_env: 0.0,
            drift_phase: (fi * 0.37).fract(),
            drift_rate: 0.14 + (fi * 0.029) % 0.21,
            pitch_tol_cents: ((idx as i32 % 5) - 2) as f32 * 1.15,
            cutoff_tol: 0.97 + (fi / (MAX_VOICES as f32 - 1.0)) * 0.06,
            env_rate_tol: 0.95 + (fi / (MAX_VOICES as f32 - 1.0)) * 0.10,
            note_age_s: 0.0,
            voice_pan: VOICE_PANS[idx % MAX_VOICES],
            filter: MoogLadderFilter::new(sr),
            filter_r: MoogLadderFilter::new(sr),
            filt_env: AdsrEnv::new(sr),
            amp_env: AdsrEnv::new(sr),
            dc_l: 0.0,
            dc_r: 0.0,
            side_hp: 0.0,
            thermal: AnalogDrift::new(0x4D4F_4F47_u32.wrapping_add(idx as u32 * 0x1337)),
            pluck_env: 0.0,
            pluck_lp: 0.0,
            svt_hp: 0.0,
            svt_lp: 0.0,
            svt_dc: 0.0,
        }
    }

    fn set_sample_rate(&mut self, sr: f32) {
        self.filter.set_sample_rate(sr);
        self.filter_r.set_sample_rate(sr);
        self.filt_env.set_sample_rate(sr);
        self.amp_env.set_sample_rate(sr);
    }
}

#[derive(Debug, Clone)]
pub struct MoogSynth {
    voices: [MoogVoice; MAX_VOICES],
    lfo_phase: f32,
    rng: Xorshift32,
    age_counter: u64,
    sample_rate: f32,
    motion_cutoff: Option<f32>,
    motion_res: Option<f32>,
    motion_drive: Option<f32>,
    motion_detune: Option<f32>,
    slew_cutoff: f32,
    slew_res: f32,
    slew_drive: f32,
    active_engine: SynthEngine,
    fm: FmSynth,
    wt: WavetableSynth,
    ms: MultiSampleSynth,
    sf2: SoundFontSynth,
    phaser: StereoPhaser,
    amp_cab: AmpCabSim,
}

impl MoogSynth {
    pub fn new(sample_rate: f32) -> Self {
        let sr = sample_rate.max(8000.0);
        Self {
            voices: std::array::from_fn(|i| MoogVoice::new(sr, i)),
            lfo_phase: 0.0,
            rng: Xorshift32::new(0xA11CE5),
            age_counter: 0,
            sample_rate: sr,
            motion_cutoff: None,
            motion_res: None,
            motion_drive: None,
            motion_detune: None,
            slew_cutoff: 1350.0,
            slew_res: 0.36,
            slew_drive: 0.30,
            active_engine: SynthEngine::Analog,
            fm: FmSynth::new(sr),
            wt: WavetableSynth::new(sr),
            ms: MultiSampleSynth::new(sr),
            sf2: SoundFontSynth::new(sr),
            phaser: StereoPhaser::new(sr),
            amp_cab: AmpCabSim::new(sr),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr.max(8000.0);
        for v in &mut self.voices {
            v.set_sample_rate(self.sample_rate);
        }
        self.fm.set_sample_rate(self.sample_rate);
        self.wt.set_sample_rate(self.sample_rate);
        self.ms.set_sample_rate(self.sample_rate);
        self.sf2.set_sample_rate(self.sample_rate);
        self.phaser.set_sample_rate(self.sample_rate);
        self.amp_cab.set_sample_rate(self.sample_rate);
    }

    pub fn set_soundfont_bank(&mut self, bank: Option<Arc<SoundFontBank>>) -> Option<Arc<SoundFontBank>> {
        self.sf2.set_bank(bank)
    }

    pub fn set_motion_locks(&mut self, motion: MotionLocks) {
        self.motion_cutoff = motion.cutoff;
        self.motion_res = motion.resonance;
        self.motion_drive = motion.drive;
        self.motion_detune = motion.shape;
    }

    pub fn clear_motion_locks(&mut self) {
        self.motion_cutoff = None;
        self.motion_res = None;
        self.motion_drive = None;
        self.motion_detune = None;
    }

    pub fn note_on(&mut self, note: u8, velocity: f32, accent: bool, slide: bool, params: &MoogParams) {
        if params.engine != self.active_engine {
            self.all_notes_off();
            self.active_engine = params.engine;
        }
        match params.engine {
            SynthEngine::FmDx7 => {
                self.fm.note_on(note, velocity, accent, slide, params.poly_mode, params.glide);
                return;
            }
            SynthEngine::Wavetable => {
                self.wt.note_on(note, velocity, accent, slide, params.poly_mode, params.glide);
                return;
            }
            SynthEngine::PhilharmoniaD50 => {
                self.ms.note_on(note, velocity, accent, params.poly_mode, params.engine_bank);
                return;
            }
            SynthEngine::SoundFont => {
                self.sf2.note_on(note, velocity, params.poly_mode, params.engine_bank as usize);
                return;
            }
            SynthEngine::Analog => {}
        }

        self.age_counter += 1;
        let vel = velocity.clamp(0.05, 1.0);
        let acc = if accent { 0.35 } else { 0.0 };

        if !params.poly_mode {
            // Mono mode: use voice 0, glide from previous pitch, release any other poly voices smoothly
            for v in &mut self.voices[1..] {
                v.held = false;
                v.slide_active = false;
                v.amp_env.release();
                v.filt_env.release();
            }
            let v = &mut self.voices[0];
            let was_active = v.active && v.amp_env.is_active();
            v.note = note;
            v.target_midi = note as f32;
            v.slide_active = slide && was_active;
            if !v.slide_active && params.glide <= 0.005 {
                v.current_midi = note as f32;
            }
            v.velocity = vel;
            v.accent = acc;
            v.held = true;
            v.active = true;
            v.age = self.age_counter;
            if !(slide && was_active) {
                v.note_age_s = 0.0;
                v.filt_env.trigger();
                v.amp_env.trigger();
                v.pitch_env = if params.amp_attack < 0.01 { 1.0 } else { 0.0 };
                v.pluck_env = if params.amp_attack < 0.015 { 1.0 } else { 0.0 };
            }
            return;
        }

        // Poly mode: reuse same note if active, else idle voice, else oldest voice
        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.note == note)
            .or_else(|| self.voices.iter().position(|v| !v.active || !v.amp_env.is_active()))
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| (v.held as u64 * 1_000_000_000) + v.age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });

        let v = &mut self.voices[idx];
        let was_active = v.active && v.amp_env.is_active();
        v.note = note;
        v.target_midi = note as f32;
        v.slide_active = slide && was_active;
        if !v.slide_active && !slide {
            v.current_midi = note as f32;
        }
        v.velocity = vel;
        v.accent = acc;
        v.held = true;
        v.active = true;
        v.age = self.age_counter;
        if !(slide && was_active) {
            v.note_age_s = 0.0;
            v.filt_env.trigger();
            v.amp_env.trigger();
        }
    }

    pub fn note_off(&mut self, note: u8) {
        for v in &mut self.voices {
            if v.active && v.held && v.note == note {
                v.held = false;
                v.filt_env.release();
                v.amp_env.release();
            }
        }
        self.fm.note_off(note);
        self.wt.note_off(note);
        self.ms.note_off(note);
        self.sf2.note_off(note);
    }

    pub fn all_notes_off(&mut self) {
        for v in &mut self.voices {
            v.held = false;
            v.filt_env.release();
            v.amp_env.release();
        }
        self.fm.all_notes_off();
        self.wt.all_notes_off();
        self.ms.all_notes_off();
        self.sf2.all_notes_off();
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|v| v.active && v.amp_env.is_active()).count()
            + self.fm.active_voice_count()
            + self.wt.active_voice_count()
            + self.ms.active_voice_count()
            + self.sf2.active_voice_count()
    }

    #[cfg(test)]
    pub fn current_mono_midi(&self) -> f32 {
        self.voices[0].current_midi
    }

    #[inline]
    pub fn process_sample(&mut self, params: &MoogParams) -> f32 {
        let (l, r) = self.process_stereo(params);
        (l + r) * 0.5
    }

    #[inline]
    pub fn process_stereo(&mut self, params: &MoogParams) -> (f32, f32) {
        let sr = self.sample_rate;
        let target_cutoff = self
            .motion_cutoff
            .map(|n| 35.0 * (16000.0_f32 / 35.0).powf(n.clamp(0.0, 1.0)))
            .unwrap_or(params.cutoff);
        let target_res = self.motion_res.unwrap_or(params.resonance);
        let target_drive = self.motion_drive.unwrap_or(params.drive);
        let detune_scale = self.motion_detune.map(|s| 0.3 + s * 2.0).unwrap_or(1.0);

        let slew = one_pole_coeff(55.0, sr);
        self.slew_cutoff += slew * (target_cutoff - self.slew_cutoff);
        self.slew_res += slew * (target_res - self.slew_res);
        self.slew_drive += slew * (target_drive - self.slew_drive);

        if params.engine != self.active_engine {
            self.all_notes_off();
            self.active_engine = params.engine;
        }
        match params.engine {
            SynthEngine::FmDx7 => {
                return self.fm.process_stereo(
                    params.engine_bank,
                    params.engine_morph,
                    self.slew_cutoff,
                    self.slew_res,
                    self.slew_drive,
                    params.env_amount,
                    params.amp_attack,
                    params.amp_decay,
                    params.amp_sustain,
                    params.amp_release,
                    params.glide,
                    params.poly_mode,
                    params.phaser_mix,
                    params.amp_cab,
                );
            }
            SynthEngine::Wavetable => {
                return self.wt.process_stereo(
                    params.engine_bank,
                    params.engine_morph,
                    self.slew_cutoff,
                    self.slew_res,
                    self.slew_drive,
                    params.env_amount,
                    params.filt_attack,
                    params.filt_decay,
                    params.filt_sustain,
                    params.filt_release,
                    params.amp_attack,
                    params.amp_decay,
                    params.amp_sustain,
                    params.amp_release,
                    params.glide,
                    params.poly_mode,
                    params.phaser_mix,
                    params.amp_cab,
                );
            }
            SynthEngine::PhilharmoniaD50 => {
                return self.ms.process_stereo(
                    params.engine_bank,
                    params.engine_morph,
                    self.slew_cutoff,
                    self.slew_res,
                    self.slew_drive,
                    params.amp_attack,
                    params.amp_decay,
                    params.amp_sustain,
                    params.amp_release,
                    params.poly_mode,
                    params.phaser_mix,
                    params.amp_cab,
                );
            }
            SynthEngine::SoundFont => {
                return self.sf2.process_stereo(
                    self.slew_cutoff,
                    self.slew_res,
                    self.slew_drive,
                    params.phaser_mix,
                    params.amp_cab,
                );
            }
            SynthEngine::Analog => {}
        }

        self.lfo_phase = (self.lfo_phase + params.lfo_rate.clamp(0.05, 30.0) / sr).fract();
        let lfo = (self.lfo_phase * TAU).sin();
        let pitch_mod_semi = lfo * params.lfo_to_pitch * 1.5;
        let filt_lfo_oct = lfo * params.lfo_to_filter * 2.2;

        let dc_alpha = one_pole_coeff(20.0, sr);
        let side_hp_alpha = one_pole_coeff(150.0, sr);

        let mut mix_l = 0.0_f32;
        let mut mix_r = 0.0_f32;
        let mut poly_env_sum = 0.0_f32;

        for v in &mut self.voices {
            if !v.active {
                continue;
            }
            let eff_glide = if v.slide_active {
                params.glide.max(0.045)
            } else {
                params.glide
            };
            if eff_glide <= 0.005 {
                v.current_midi = v.target_midi;
            } else {
                let glide_coeff = 1.0 - (-1.0 / (eff_glide * 0.35 * sr)).exp();
                v.current_midi += glide_coeff * (v.target_midi - v.current_midi);
            }
            v.note_age_s += 1.0 / sr;
            let env_tol = if params.poly_mode { v.env_rate_tol } else { 1.0 };
            let f_env = v.filt_env.next(
                params.filt_attack * env_tol,
                params.filt_decay * env_tol,
                params.filt_sustain,
                params.filt_release,
            );
            let a_env = v.amp_env.next(
                params.amp_attack,
                params.amp_decay,
                params.amp_sustain,
                params.amp_release,
            );

            if !v.amp_env.is_active() {
                v.active = false;
                continue;
            }

            v.drift_phase = (v.drift_phase + v.drift_rate / sr).fract();
            v.pwm_phase = (v.pwm_phase + (0.34 + v.drift_rate * 0.35) / sr).fract();
            let therm = v.thermal.next(sr);
            let pwm_mod = (v.pwm_phase * TAU).sin() * 0.068 + therm * 0.018;
            let drift_cents = (v.drift_phase * TAU).sin() * 0.85 + therm * 1.65 + v.pitch_tol_cents;

            let pitch_punch = if !params.poly_mode && v.pitch_env > 0.001 {
                v.pitch_env *= (-1.0 / (0.0045 * sr)).exp();
                v.pitch_env * 0.42
            } else {
                0.0
            };

            let vib_bloom = if params.lfo_to_pitch > 0.0 {
                0.25 + 0.75 * ((v.note_age_s - 0.07) / 0.18).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let base_midi =
                v.current_midi + pitch_mod_semi * vib_bloom + drift_cents * 0.01 + pitch_punch;

            let f1 = midi_to_hz(base_midi).clamp(16.0, sr * 0.42);
            let d2_cents = params.osc2_detune * detune_scale;
            let d3_cents = params.osc3_detune * detune_scale;
            let f2 = midi_to_hz(base_midi + params.osc2_octave as f32 * 12.0 + d2_cents / 100.0)
                .clamp(16.0, sr * 0.42);
            let f3 = midi_to_hz(base_midi + params.osc3_octave as f32 * 12.0 + d3_cents / 100.0)
                .clamp(16.0, sr * 0.42);

            let dt1 = f1 / sr;
            let dt2 = f2 / sr;
            let dt3 = f3 / sr;
            v.phase1 = (v.phase1 + dt1).fract();
            v.phase2 = (v.phase2 + dt2).fract();
            v.phase3 = (v.phase3 + dt3).fract();

            let o1 = sample_moog_wave(params.osc1_wave, v.phase1, dt1, pwm_mod) * params.osc1_level;
            let o2 = sample_moog_wave(params.osc2_wave, v.phase2, dt2, pwm_mod) * params.osc2_level;
            let o3 = sample_moog_wave(params.osc3_wave, v.phase3, dt3, pwm_mod) * params.osc3_level;
            let noise = self.rng.next_bipolar() * params.noise_level;

            let key_oct = ((v.current_midi - 48.0) / 12.0) * params.key_track;
            let env_oct = (params.env_amount + v.accent * 0.5) * f_env * 4.5;
            let vel_oct = (v.velocity - 0.65) * 1.35;
            let cutoff_hz = self.slew_cutoff
                * v.cutoff_tol
                * (1.0 + therm * 0.018)
                * 2.0_f32.powf(key_oct + env_oct + vel_oct + filt_lfo_oct);
            let pre_gain = 0.65 + self.slew_drive * 1.65;
            let weight_norm = (params.osc1_level + params.osc2_level + params.osc3_level + 0.35)
                .max(1.0)
                .sqrt();

            if !params.poly_mode {
                // Mono Bass Mode: phase-coherent center sub + wound-string pluck bite + VCO->VCF FM + stereo detune
                let dt_sub = (f1 * 0.5).clamp(16.0, sr * 0.42) / sr;
                v.sub_phase = (v.sub_phase + dt_sub).fract();
                let has_sub_osc = (params.osc2_octave < 0 && params.osc2_level > 0.25)
                    || (params.osc3_octave < 0 && params.osc3_level > 0.25);
                let sub_gain = if has_sub_osc { 0.16 } else { 0.26 };
                let sub = osc_sub_warm(v.sub_phase) * sub_gain;

                let pluck_bite = if v.pluck_env > 0.0005 {
                    v.pluck_env *= (-1.0 / (0.0068 * sr)).exp();
                    let raw = self.rng.next_bipolar() * 0.55 + o1 * 0.65;
                    let pluck_a = one_pole_coeff(2400.0, sr);
                    v.pluck_lp += pluck_a * (raw - v.pluck_lp);
                    v.pluck_lp * v.pluck_env * (0.14 + v.velocity * 0.14)
                } else {
                    0.0
                };

                let spread = (d2_cents.abs().max(d3_cents.abs()) / 24.0).clamp(0.0, 0.32);
                let in_l = (o1 + sub + pluck_bite + o2 * (1.0 + spread) + o3 * (1.0 - spread) + noise)
                    / weight_norm;
                let in_r = (o1 + sub + pluck_bite + o2 * (1.0 - spread) + o3 * (1.0 + spread) + noise)
                    / weight_norm;

                let asym = (self.slew_drive - 0.55).max(0.0) * 0.14;
                let mix_sum_l = 1.8 * soft_tanh((in_l * pre_gain + asym * in_l * in_l) / 1.8);
                let mix_sum_r = 1.8 * soft_tanh((in_r * pre_gain + asym * in_r * in_r) / 1.8);

                let hold_swell =
                    ((v.note_age_s - 0.14) / 0.35).clamp(0.0, 1.0) * (0.08 + self.slew_drive * 0.09);
                let fm_amt = (0.035 + self.slew_drive * 0.075).min(0.14);
                let bass_cutoff =
                    (cutoff_hz * (1.0 + hold_swell) * (1.0 + o1 * fm_amt)).clamp(28.0, sr * 0.44);

                let fl = v.filter.process(mix_sum_l, bass_cutoff, self.slew_res);
                let fr = v.filter_r.process(mix_sum_r, bass_cutoff, self.slew_res);

                v.dc_l += dc_alpha * (fl - v.dc_l);
                v.dc_r += dc_alpha * (fr - v.dc_r);
                let cl = fl - v.dc_l;
                let cr = fr - v.dc_r;

                // Lock < 150 Hz bass center mono so club sub hits dead-center
                let mid = (cl + cr) * 0.5;
                let side = (cl - cr) * 0.5;
                v.side_hp += side_hp_alpha * (side - v.side_hp);
                let side_clean = side - v.side_hp;

                // Parallel Ampeg SVT 175-1550 Hz midrange tube purr
                let svt_hp_a = one_pole_coeff(175.0, sr);
                let svt_lp_a = one_pole_coeff(1550.0, sr);
                v.svt_hp += svt_hp_a * (mid - v.svt_hp);
                let mid_hp = mid - v.svt_hp;
                v.svt_lp += svt_lp_a * (mid_hp - v.svt_lp);
                let svt_drive = 1.65 + self.slew_drive * 2.1;
                let svt_raw = soft_tanh(v.svt_lp * svt_drive + 0.18 * v.svt_lp * v.svt_lp);
                v.svt_dc += dc_alpha * (svt_raw - v.svt_dc);
                let svt_purr = svt_raw - v.svt_dc;
                let svt_mix = (0.14 + self.slew_drive * 0.16).clamp(0.14, 0.32);
                let bass_mid = mid * (1.0 - svt_mix * 0.30) + svt_purr * svt_mix;

                let amp = a_env * (v.velocity + v.accent * 0.25) * 0.78;
                mix_l += (bass_mid + side_clean) * amp;
                mix_r += (bass_mid - side_clean) * amp;
            } else {
                // Poly Chord / 7-Oscillator OB-Xa & Jupiter-8 Stereo Unison Mode
                let unison_on = d2_cents.abs() >= 3.0 || d3_cents.abs() >= 3.0;
                let (in_l, in_r) = if unison_on {
                    let spread_d = d2_cents.abs().max(6.0);
                    let f1_b = midi_to_hz(base_midi + (spread_d * 0.55) / 100.0).clamp(16.0, sr * 0.42);
                    let f1_c = midi_to_hz(base_midi - (spread_d * 0.55) / 100.0).clamp(16.0, sr * 0.42);
                    let f2_b = midi_to_hz(
                        base_midi + params.osc2_octave as f32 * 12.0 + (-d2_cents * 1.35) / 100.0,
                    )
                    .clamp(16.0, sr * 0.42);
                    let f3_b = midi_to_hz(
                        base_midi
                            + params.osc3_octave as f32 * 12.0
                            + (d3_cents * 1.35 + 6.0) / 100.0,
                    )
                    .clamp(16.0, sr * 0.42);
                    let dt1b = f1_b / sr;
                    let dt1c = f1_c / sr;
                    let dt2b = f2_b / sr;
                    let dt3b = f3_b / sr;
                    v.phase1_b = (v.phase1_b + dt1b).fract();
                    v.phase1_c = (v.phase1_c + dt1c).fract();
                    v.phase2_b = (v.phase2_b + dt2b).fract();
                    v.phase3_b = (v.phase3_b + dt3b).fract();
                    let o1b = sample_moog_wave(params.osc1_wave, v.phase1_b, dt1b, pwm_mod) * params.osc1_level;
                    let o1c = sample_moog_wave(params.osc1_wave, v.phase1_c, dt1c, pwm_mod) * params.osc1_level;
                    let o2b = sample_moog_wave(params.osc2_wave, v.phase2_b, dt2b, pwm_mod) * params.osc2_level;
                    let o3b = sample_moog_wave(params.osc3_wave, v.phase3_b, dt3b, pwm_mod) * params.osc3_level;
                    let norm_u = weight_norm * 1.35;
                    let l = (o1 * 0.72 + o1b * 0.65 + o1c * 0.28 + o2 * 0.76 + o2b * 0.34 + o3 * 0.38 + o3b * 0.74 + noise) / norm_u;
                    let r = (o1 * 0.72 + o1b * 0.28 + o1c * 0.65 + o2 * 0.34 + o2b * 0.76 + o3 * 0.74 + o3b * 0.38 + noise) / norm_u;
                    (l, r)
                } else {
                    let mono = (o1 + o2 + o3 + noise) / weight_norm;
                    (mono, mono)
                };

                let mix_sum_l = 1.8 * soft_tanh(in_l * pre_gain / 1.8);
                let mix_sum_r = 1.8 * soft_tanh(in_r * pre_gain / 1.8);

                // Smooth continuous 12dB OB-Xa / SEM top-end sheen for open poly pads/brass
                let ob_blend = ((self.slew_cutoff - 1100.0) / 900.0).clamp(0.0, 1.0)
                    * ((0.45 - self.slew_res) / 0.25).clamp(0.0, 1.0)
                    * 0.42;
                let fl = v.filter.process_mode(mix_sum_l, cutoff_hz, self.slew_res, ob_blend);
                let fr = v.filter_r.process_mode(mix_sum_r, cutoff_hz, self.slew_res, ob_blend);

                v.dc_l += dc_alpha * (fl - v.dc_l);
                v.dc_r += dc_alpha * (fr - v.dc_r);
                let cl = fl - v.dc_l;
                let cr = fr - v.dc_r;

                let p = (v.voice_pan + 1.0) * 0.5;
                let pan_l = (1.0 - p).sqrt() * 1.4142;
                let pan_r = p.sqrt() * 1.4142;
                let amp = a_env * (v.velocity + v.accent * 0.25) * 0.62;
                poly_env_sum += a_env;
                mix_l += cl * amp * pan_l;
                mix_r += cr * amp * pan_r;
            }
        }

        if params.poly_mode && poly_env_sum > 1.0 {
            let norm = 1.0 / poly_env_sum.powf(0.22);
            mix_l *= norm;
            mix_r *= norm;
        }

        if params.amp_cab > 0.002 {
            (mix_l, mix_r) = self.amp_cab.process_stereo(mix_l, mix_r, params.amp_cab);
        }
        if params.phaser_mix > 0.002 {
            (mix_l, mix_r) = self.phaser.process_stereo(mix_l, mix_r, params.phaser_mix);
        }

        // Output stage: linear in the normal range, gentle soft-knee ceiling at extremes.
        (soft_ceiling(mix_l, 1.6, 0.4), soft_ceiling(mix_r, 1.6, 0.4))
    }
}

#[inline]
fn sample_moog_wave(wave: MoogWave, phase: f32, dt: f32, pwm_mod: f32) -> f32 {
    match wave {
        MoogWave::Triangle => osc_tri_aa(phase, dt),
        MoogWave::TriSaw => osc_trisaw(phase, dt),
        MoogWave::Saw => osc_analog_saw(phase, dt),
        MoogWave::WidePulse => osc_analog_pulse(phase, dt, (0.35 + pwm_mod).clamp(0.08, 0.92)),
        MoogWave::Square => osc_analog_pulse(phase, dt, (0.50 + pwm_mod).clamp(0.08, 0.92)),
        MoogWave::NarrowPulse => osc_analog_pulse(phase, dt, (0.15 + pwm_mod * 0.6).clamp(0.06, 0.92)),
    }
}
