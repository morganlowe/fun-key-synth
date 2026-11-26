use std::sync::Arc;

use crate::model::{
    midi_to_hz, EgTarget, LfoMode, LfoTarget, LfoWave, MonologueOscWave, MonologueParams,
    MotionLocks, Osc2ModMode, SynthEngine,
};

use super::{
    one_pole_coeff, osc_analog_pulse, osc_analog_saw, osc_sub_warm, osc_tri, osc_tri_aa,
    read_ring_hermite, soft_ceiling, soft_tanh, AdsrEnv, AmpCabSim, AnalogDrift, FmSynth,
    MultiSampleSynth, SoundFontBank, SoundFontSynth, StereoPhaser, Svf12Filter, WavetableSynth,
    Xorshift32,
};

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
    phase1_b: f32,
    phase2_b: f32,
    sub_phase: f32,
    pwm_phase: f32,
    lfo_phase: f32,
    lfo_oneshot_done: bool,
    filter_l: Svf12Filter,
    filter_r: Svf12Filter,
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
    dc_l: f32,
    dc_r: f32,
    side_hp: f32,
    note_age_s: f32,
    drift_phase: f32,
    pitch_scoop: f32,
    trig_count: u32,
    silk_lp_l: f32,
    silk_lp_r: f32,
    formant1_l: Svf12Filter,
    formant1_r: Svf12Filter,
    formant2_l: Svf12Filter,
    formant2_r: Svf12Filter,
    thermal: AnalogDrift,
    chiff_env: f32,
    chiff_bp: Svf12Filter,
    rng: Xorshift32,
    vib_phase: f32,
    ens_buf_l: Vec<f32>,
    ens_buf_r: Vec<f32>,
    ens_pos: usize,
    ens_phase: f32,
    ens_hp_l: f32,
    ens_hp_r: f32,
    active_engine: SynthEngine,
    fm: FmSynth,
    wt: WavetableSynth,
    ms: MultiSampleSynth,
    sf2: SoundFontSynth,
    phaser: StereoPhaser,
    amp_cab: AmpCabSim,
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
            phase2: 0.17,
            phase1_b: 0.37,
            phase2_b: 0.61,
            sub_phase: 0.0,
            pwm_phase: 0.19,
            lfo_phase: 0.0,
            lfo_oneshot_done: false,
            filter_l: Svf12Filter::new(sr),
            filter_r: Svf12Filter::new(sr),
            eg: AdsrEnv::new(sr),
            amp_env: AdsrEnv::new(sr),
            sample_rate: sr,
            motion_locks: MotionLocks::default(),
            motion_smooth: true,
            slew_cutoff: 1850.0,
            slew_res: 0.42,
            slew_drive: 0.42,
            slew_shape: 0.35,
            dc_l: 0.0,
            dc_r: 0.0,
            side_hp: 0.0,
            note_age_s: 0.0,
            drift_phase: 0.13,
            pitch_scoop: 0.0,
            trig_count: 0,
            silk_lp_l: 0.0,
            silk_lp_r: 0.0,
            formant1_l: Svf12Filter::new(sr),
            formant1_r: Svf12Filter::new(sr),
            formant2_l: Svf12Filter::new(sr),
            formant2_r: Svf12Filter::new(sr),
            thermal: AnalogDrift::new(0x4B4F_5247),
            chiff_env: 0.0,
            chiff_bp: Svf12Filter::new(sr),
            rng: Xorshift32::new(0x4D4F_4E4F),
            vib_phase: 0.0,
            ens_buf_l: vec![0.0; ((sr * 0.045) as usize).max(512)],
            ens_buf_r: vec![0.0; ((sr * 0.045) as usize).max(512)],
            ens_pos: 0,
            ens_phase: 0.23,
            ens_hp_l: 0.0,
            ens_hp_r: 0.0,
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
        self.filter_l.set_sample_rate(self.sample_rate);
        self.filter_r.set_sample_rate(self.sample_rate);
        self.formant1_l.set_sample_rate(self.sample_rate);
        self.formant1_r.set_sample_rate(self.sample_rate);
        self.formant2_l.set_sample_rate(self.sample_rate);
        self.formant2_r.set_sample_rate(self.sample_rate);
        self.chiff_bp.set_sample_rate(self.sample_rate);
        self.eg.set_sample_rate(self.sample_rate);
        self.amp_env.set_sample_rate(self.sample_rate);
        let ens_len = ((self.sample_rate * 0.045) as usize).max(512);
        self.ens_buf_l.resize(ens_len, 0.0);
        self.ens_buf_r.resize(ens_len, 0.0);
        self.ens_buf_l.fill(0.0);
        self.ens_buf_r.fill(0.0);
        self.ens_pos = 0;
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
        if params.engine != self.active_engine {
            self.all_notes_off();
            self.active_engine = params.engine;
        }
        match params.engine {
            SynthEngine::FmDx7 => {
                self.fm.note_on(note, velocity, accent, slide, true, params.glide);
                return;
            }
            SynthEngine::Wavetable => {
                self.wt.note_on(note, velocity, accent, slide, true, params.glide);
                return;
            }
            SynthEngine::PhilharmoniaD50 => {
                self.ms.note_on(note, velocity, accent, true, params.engine_bank);
                return;
            }
            SynthEngine::SoundFont => {
                self.sf2.note_on(note, velocity, true, params.engine_bank as usize);
                return;
            }
            SynthEngine::Analog => {}
        }

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
            self.note_age_s = 0.0;
            self.chiff_env = 1.0;
            self.trig_count = self.trig_count.wrapping_add(1);
            self.pitch_scoop = if self.trig_count % 2 == 0 { 0.022 } else { -0.018 };
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
        self.fm.note_off(note);
        self.wt.note_off(note);
        self.ms.note_off(note);
        self.sf2.note_off(note);
    }

    pub fn all_notes_off(&mut self) {
        self.held = false;
        self.eg.release();
        self.amp_env.release();
        self.fm.all_notes_off();
        self.wt.all_notes_off();
        self.ms.all_notes_off();
        self.sf2.all_notes_off();
    }

    pub fn is_active(&self) -> bool {
        (self.active && self.amp_env.is_active())
            || self.fm.active_voice_count() > 0
            || self.wt.active_voice_count() > 0
            || self.ms.active_voice_count() > 0
            || self.sf2.active_voice_count() > 0
    }

    #[inline]
    pub fn process_sample(&mut self, params: &MonologueParams) -> f32 {
        let (l, r) = self.process_stereo(params);
        (l + r) * 0.5
    }

    #[inline]
    pub fn process_stereo(&mut self, params: &MonologueParams) -> (f32, f32) {
        let sr = self.sample_rate;
        let target_cutoff = self
            .motion_locks
            .cutoff
            .map(|n| 35.0 * (16000.0_f32 / 35.0).powf(n.clamp(0.0, 1.0)))
            .unwrap_or(params.cutoff);
        let target_res = self.motion_locks.resonance.unwrap_or(params.resonance);
        let target_drive = self.motion_locks.drive.unwrap_or(params.drive);
        let target_shape = self.motion_locks.shape.unwrap_or(params.vco1_shape);

        let slew = one_pole_coeff(if self.motion_smooth { 40.0 } else { 1200.0 }, sr);
        self.slew_cutoff += slew * (target_cutoff - self.slew_cutoff);
        self.slew_res += slew * (target_res - self.slew_res);
        self.slew_drive += slew * (target_drive - self.slew_drive);
        self.slew_shape += slew * (target_shape - self.slew_shape);

        if params.engine != self.active_engine {
            self.all_notes_off();
            self.active_engine = params.engine;
        }
        let sustain_lvl = if params.amp_sustain { 0.65 } else { 0.0 };
        let dec_s = params.eg_decay.max(0.24);
        let rel_s = params.eg_decay.max(0.22);
        match params.engine {
            SynthEngine::FmDx7 => {
                return self.fm.process_stereo(
                    params.engine_bank,
                    params.engine_morph,
                    self.slew_cutoff,
                    self.slew_res,
                    self.slew_drive,
                    params.eg_int,
                    params.eg_attack.max(0.002),
                    dec_s,
                    sustain_lvl,
                    rel_s,
                    params.glide,
                    true,
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
                    params.eg_int,
                    params.eg_attack.max(0.002),
                    dec_s,
                    sustain_lvl * 0.6,
                    rel_s,
                    params.eg_attack.max(0.002),
                    dec_s,
                    sustain_lvl,
                    rel_s,
                    params.glide,
                    true,
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
                    params.eg_attack.max(0.002),
                    dec_s,
                    sustain_lvl,
                    rel_s,
                    true,
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

        if !self.active {
            return (0.0, 0.0);
        }

        // Monologue AD-type EG (or sustain when amp_sustain is enabled)
        let eg_sustain = 0.0;
        let eg_val = self.eg.next(params.eg_attack, params.eg_decay, eg_sustain, 0.08);
        let amp_val = if params.amp_sustain {
            self.amp_env.next(0.004, 0.05, 1.0, 0.048)
        } else {
            self.amp_env
                .next(params.eg_attack.max(0.003), params.eg_decay.max(0.04), 0.0, 0.05)
        };

        if !self.amp_env.is_active() {
            self.active = false;
            return (0.0, 0.0);
        }

        self.note_age_s += 1.0 / sr;
        self.drift_phase = (self.drift_phase + 0.37 / sr).fract();

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
            let vib_bloom = 0.25 + 0.75 * ((self.note_age_s - 0.055) / 0.16).clamp(0.0, 1.0);
            lfo_mod * 7.0 * vib_bloom
        } else {
            0.0
        };
        let therm = self.thermal.next(sr);
        let aftertouch = ((self.note_age_s - 0.15) / 0.38).clamp(0.0, 1.0);
        self.vib_phase = (self.vib_phase + (5.15 + therm * 0.25) / sr).fract();
        let natural_vib = if params.lfo_target != LfoTarget::Pitch {
            (self.vib_phase * std::f32::consts::TAU).sin()
                * aftertouch
                * (0.045 + self.velocity * 0.045)
        } else {
            0.0
        };
        let drift_semi = (self.drift_phase * std::f32::consts::TAU).sin() * 0.006
            + therm * 0.015
            + self.pitch_scoop;
        self.pitch_scoop *= (-1.0 / (0.007 * sr)).exp();

        let m1 = self.current_midi + eg_pitch1 + lfo_pitch + natural_vib + drift_semi;
        let m2 = self.current_midi
            + params.vco2_octave as f32 * 12.0
            + params.vco2_pitch
            + eg_pitch1
            + eg_pitch2
            + lfo_pitch
            + natural_vib
            + drift_semi;

        let f1 = midi_to_hz(m1).clamp(16.0, sr * 0.42);
        let f2 = midi_to_hz(m2).clamp(16.0, sr * 0.42);
        let f1_b = midi_to_hz(m1 + 0.075).clamp(16.0, sr * 0.42);
        let f2_b = midi_to_hz(m2 - 0.075).clamp(16.0, sr * 0.42);

        let dt1 = f1 / sr;
        let dt2 = f2 / sr;
        let dt1_b = f1_b / sr;
        let dt2_b = f2_b / sr;
        let dt_sub = (f1 * 0.5).clamp(16.0, sr * 0.42) / sr;

        let prev_p1 = self.phase1;
        self.phase1 = (self.phase1 + dt1).fract();
        self.phase2 = (self.phase2 + dt2).fract();
        self.phase1_b = (self.phase1_b + dt1_b).fract();
        self.phase2_b = (self.phase2_b + dt2_b).fract();
        self.sub_phase = (self.sub_phase + dt_sub).fract();

        if params.mod_mode == Osc2ModMode::Sync && self.phase1 < prev_p1 {
            // Hard sync VCO2 to VCO1 phase wrap
            self.phase2 = ((self.phase1 / dt1.max(1e-6)) * dt2).fract();
            self.phase2_b = ((self.phase1 / dt1.max(1e-6)) * dt2_b).fract();
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
        self.pwm_phase = (self.pwm_phase + 0.42 / sr).fract();
        let pwm_lfo = (self.pwm_phase * std::f32::consts::TAU).sin();

        let vco1 = monologue_wave(params.vco1_wave, self.phase1, dt1, shape1, eg_val, pwm_lfo);
        let vco1_b = monologue_wave(params.vco1_wave, self.phase1_b, dt1_b, shape1, eg_val, pwm_lfo);
        let (p2_mod, p2_b_mod) = if params.mod_mode == Osc2ModMode::Off {
            let xmod_reg = ((self.current_midi - 48.0) / 18.0).clamp(0.2, 1.0);
            let xmod = vco1 * (0.018 + shape1 * 0.028) * xmod_reg;
            (
                (self.phase2 + xmod).rem_euclid(1.0),
                (self.phase2_b - xmod).rem_euclid(1.0),
            )
        } else {
            (self.phase2, self.phase2_b)
        };
        let vco2 = monologue_wave(params.vco2_wave, p2_mod, dt2, shape2, eg_val, -pwm_lfo);
        let vco2_b = monologue_wave(params.vco2_wave, p2_b_mod, dt2_b, shape2, eg_val, -pwm_lfo);
        let sub_scale = ((64.0 - self.current_midi) / 24.0).clamp(0.12, 1.0);
        let sub_body = osc_sub_warm(self.sub_phase) * 0.22 * sub_scale;

        let chiff = if self.chiff_env > 0.0005 {
            self.chiff_env *= (-1.0 / (0.0095 * sr)).exp();
            let chiff_hz =
                (2100.0 + ((self.current_midi - 48.0).max(0.0) * 55.0)).clamp(1800.0, 5200.0);
            let n = self.rng.next_bipolar();
            self.chiff_bp.process_bp(n, chiff_hz, 0.42)
                * self.chiff_env
                * (0.09 + self.velocity * 0.12)
        } else {
            0.0
        };

        let (mut osc_l, mut osc_r) = match params.mod_mode {
            Osc2ModMode::Ring => {
                let fold_amt = (shape1 * 0.8 + self.slew_drive * 0.5).clamp(0.0, 1.2);
                let ring_raw = (vco1 * vco2) * params.vco2_level * 0.9;
                let ring_folded = ring_raw + (ring_raw * (1.0 + fold_amt * 1.9)).sin() * 0.22;
                let clean_sub = if self.current_midi < 56.0 {
                    sub_body * 0.85
                } else {
                    sub_body * 0.5
                };
                let core = vco1 * params.vco1_level * 0.48 + ring_folded + clean_sub;
                let spread = (vco1_b * params.vco1_level - vco2_b * params.vco2_level) * 0.18;
                ((core + spread) * 0.58, (core - spread) * 0.58)
            }
            Osc2ModMode::Sync => {
                let core = vco1 * params.vco1_level * 0.58 + vco2 * params.vco2_level * 0.62 + sub_body * 0.6;
                let spread = (vco1_b * params.vco1_level - vco2_b * params.vco2_level) * 0.18;
                ((core + spread) * 0.58, (core - spread) * 0.58)
            }
            Osc2ModMode::Off => {
                let core = vco1 * params.vco1_level * 0.56 + vco2 * params.vco2_level * 0.56 + sub_body;
                let l = (core + vco1_b * params.vco1_level * 0.44 + vco2_b * params.vco2_level * 0.12) * 0.52;
                let r = (core + vco2_b * params.vco2_level * 0.44 + vco1_b * params.vco1_level * 0.12) * 0.52;
                (l, r)
            }
        };
        osc_l += chiff;
        osc_r += chiff;

        let eg_cutoff = if params.eg_target == EgTarget::Cutoff {
            (params.eg_int + self.accent * 0.45) * eg_val * 3.3
        } else {
            self.accent * eg_val * 1.35
        };
        let lfo_cutoff = if params.lfo_target == LfoTarget::Cutoff {
            lfo_mod * 2.3
        } else {
            0.0
        };
        let vel_oct = (self.velocity - 0.65) * 1.25;
        let key_oct = ((self.current_midi - 60.0) / 12.0 * 0.32).clamp(-0.75, 0.85);
        let expr_oct = aftertouch * (0.22 + self.velocity * 0.20);

        let cutoff = self.slew_cutoff
            * (1.0 + therm * 0.016)
            * 2.0_f32.powf(eg_cutoff + lfo_cutoff + vel_oct + key_oct + expr_oct);
        let mut filt_l = self.filter_l.process_lp_analog(osc_l, cutoff, self.slew_res, self.slew_drive);
        let mut filt_r = self.filter_r.process_lp_analog(osc_r, cutoff, self.slew_res, self.slew_drive);

        // Dual-vowel talking formant bandpass layer for aggressive Ring-Mod bass growls
        if params.mod_mode == Osc2ModMode::Ring && self.slew_drive > 0.30 {
            let vowel_pos = (eg_val * 0.55 + (lfo_mod * 0.5 + 0.5) * 0.45).clamp(0.0, 1.0);
            let f1_hz = 340.0 + vowel_pos * 480.0;
            let f2_hz = 950.0 + vowel_pos * 1350.0;
            let f1_l = self.formant1_l.process_bp(filt_l, f1_hz, 0.58);
            let f1_r = self.formant1_r.process_bp(filt_r, f1_hz, 0.58);
            let f2_l = self.formant2_l.process_bp(filt_l, f2_hz, 0.58);
            let f2_r = self.formant2_r.process_bp(filt_r, f2_hz, 0.58);
            let formant_mix = ((self.slew_drive - 0.30) / 0.55).clamp(0.0, 0.45);
            filt_l = filt_l * (1.0 - formant_mix * 0.35) + (f1_l * 0.65 + f2_l * 0.55) * formant_mix * 1.35;
            filt_r = filt_r * (1.0 - formant_mix * 0.35) + (f1_r * 0.65 + f2_r * 0.55) * formant_mix * 1.35;
        }

        // Clean high-headroom Korg analog Triode Drive stage + 20 Hz DC blocker + 150 Hz Side HPF
        let drive_gain = 1.0 + self.slew_drive * 2.0;
        let drive_norm = 1.0 + self.slew_drive * 0.35;
        let asym = 0.075 + self.slew_drive * 0.065;
        let drv_l = 1.6 * soft_tanh((filt_l * drive_gain + asym * filt_l * filt_l) / 1.6) / drive_norm;
        let drv_r = 1.6 * soft_tanh((filt_r * drive_gain + asym * filt_r * filt_r) / 1.6) / drive_norm;

        let dc_alpha = one_pole_coeff(20.0, sr);
        self.dc_l += dc_alpha * (drv_l - self.dc_l);
        self.dc_r += dc_alpha * (drv_r - self.dc_r);
        let clean_l = drv_l - self.dc_l;
        let clean_r = drv_r - self.dc_r;

        let mid = (clean_l + clean_r) * 0.5;
        let side = (clean_l - clean_r) * 0.5;
        let side_hp_alpha = one_pole_coeff(150.0, sr);
        self.side_hp += side_hp_alpha * (side - self.side_hp);
        let side_clean = side - self.side_hp;

        let gain = amp_val * (self.velocity + self.accent * 0.25) * 0.86;
        let mut out_l = (mid + side_clean) * gain;
        let mut out_r = (mid - side_clean) * gain;

        // Quadrature BBD Stereo Dimension Chorus (>200 Hz)
        let ens_len = self.ens_buf_l.len();
        if ens_len >= 64 {
            let hp_a = one_pole_coeff(200.0, sr);
            self.ens_hp_l += hp_a * (out_l - self.ens_hp_l);
            self.ens_hp_r += hp_a * (out_r - self.ens_hp_r);
            let hp_l = out_l - self.ens_hp_l;
            let hp_r = out_r - self.ens_hp_r;
            let mid_hp = (hp_l + hp_r) * 0.5;
            self.ens_buf_l[self.ens_pos] = mid_hp + (hp_l - hp_r) * 0.35;
            self.ens_buf_r[self.ens_pos] = mid_hp + (hp_r - hp_l) * 0.35;

            self.ens_phase = (self.ens_phase + 0.58 / sr).fract();
            let lfo_a = (self.ens_phase * std::f32::consts::TAU).sin();
            let lfo_b = (self.ens_phase * std::f32::consts::TAU).cos();

            let d_l1 = (0.0072 + lfo_a * 0.0024) * sr;
            let d_l2 = (0.0113 - lfo_b * 0.0021) * sr;
            let d_r1 = (0.0094 + lfo_b * 0.0024) * sr;
            let d_r2 = (0.0131 - lfo_a * 0.0021) * sr;

            let tap_l = 0.5
                * (read_ring_hermite(&self.ens_buf_l, self.ens_pos, d_l1)
                    + read_ring_hermite(&self.ens_buf_r, self.ens_pos, d_l2));
            let tap_r = 0.5
                * (read_ring_hermite(&self.ens_buf_r, self.ens_pos, d_r1)
                    + read_ring_hermite(&self.ens_buf_l, self.ens_pos, d_r2));

            let reg_scale = ((self.current_midi - 46.0) / 16.0).clamp(0.25, 1.0);
            let chorus_wet = if params.mod_mode == Osc2ModMode::Off {
                0.22
            } else {
                0.14
            } * reg_scale;
            out_l = out_l * (1.0 - chorus_wet * 0.22) + (tap_l - tap_r * 0.25) * chorus_wet;
            out_r = out_r * (1.0 - chorus_wet * 0.22) + (tap_r - tap_l * 0.25) * chorus_wet;
            self.ens_pos = (self.ens_pos + 1) % ens_len;
        }

        if params.amp_cab > 0.002 {
            (out_l, out_r) = self.amp_cab.process_stereo(out_l, out_r, params.amp_cab);
        }
        if params.phaser_mix > 0.002 {
            (out_l, out_r) = self.phaser.process_stereo(out_l, out_r, params.phaser_mix);
        }

        // High-register analog silk smoother: prevents arcade sharpness on bright upper-octave runs
        let high_t = ((self.current_midi - 68.0) / 18.0).clamp(0.0, 1.0);
        let silk_hz = if params.mod_mode == Osc2ModMode::Off {
            11_500.0 - high_t * 3_700.0
        } else {
            12_500.0 - high_t * 2_500.0
        };
        let silk_a = one_pole_coeff(silk_hz, sr);
        self.silk_lp_l += silk_a * (out_l - self.silk_lp_l);
        self.silk_lp_r += silk_a * (out_r - self.silk_lp_r);

        (
            soft_ceiling(self.silk_lp_l, 1.4, 0.4),
            soft_ceiling(self.silk_lp_r, 1.4, 0.4),
        )
    }
}

#[inline]
fn monologue_wave(
    wave: MonologueOscWave,
    phase: f32,
    dt: f32,
    shape: f32,
    eg_val: f32,
    pwm_lfo: f32,
) -> f32 {
    match wave {
        MonologueOscWave::Saw => {
            let s = osc_analog_saw(phase, dt);
            let comb_shift = (0.34 + 0.14 * pwm_lfo + shape * 0.16).clamp(0.12, 0.85);
            let s_comb = osc_analog_saw((phase + comb_shift).fract(), dt);
            let oct_saw = osc_analog_saw((phase * 2.0).fract(), (dt * 2.0).min(0.45));
            s * (0.82 - shape * 0.18) - s_comb * (0.24 + shape * 0.12) + oct_saw * shape * 0.28
        }
        MonologueOscWave::Triangle => {
            let t = osc_tri_aa(phase, dt);
            let warm = soft_tanh(t * (1.0 + shape * 1.6)) * 0.85;
            let sparkle = (phase * 2.0 * std::f32::consts::TAU).sin() * shape * eg_val * 0.28;
            warm + sparkle
        }
        MonologueOscWave::Square => {
            let duty = (0.50 - shape * 0.36 + 0.045 * pwm_lfo).clamp(0.10, 0.90);
            osc_analog_pulse(phase, dt, duty)
        }
    }
}
