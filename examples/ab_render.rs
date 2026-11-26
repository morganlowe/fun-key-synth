//! Offline A/B renderer: builds one Project per vibe and renders N bars of the
//! exact production DSP pipeline to WAV for objective old-vs-new comparison.
//!
//! Usage: cargo run --release --example ab_render -- <outdir> [vibe_name]

use fun_key_synth::model::project::Project;
use fun_key_synth::model::vibe::{apply_vibe_to_project, Vibe};

use std::path::PathBuf;

const VIBES: &[(&str, Vibe)] = &[
    ("deep_house", Vibe::DeepHouse),
    ("electro_funk", Vibe::ElectroFunk),
    ("imperial_bass", Vibe::ImperialBass),
    ("arena80s", Vibe::Arena80s),
    ("dubstep", Vibe::Dubstep),
    ("synthwave", Vibe::Synthwave),
    ("lofi_chill", Vibe::LofiChill),
    ("indie_rock", Vibe::IndieRock),
    ("euphoric_trance", Vibe::EuphoricTrance),
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let outdir = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "/tmp/abtest".into()));
    let filter = args.get(2).cloned();

    std::fs::create_dir_all(&outdir).expect("mkdir");
    let seed: u32 = 20261124;

    if filter.as_ref().map(|f| "default_startup".contains(f.as_str())).unwrap_or(true) {
        let p = Project::default();
        let path = outdir.join("default_startup.wav");
        println!("Rendering default_startup -> {}", path.display());
        if let Err(e) = fun_key_synth::engine::export_wav(&p, &path, 4) {
            eprintln!("  FAILED: {e}");
        }
    }

    for (name, vibe) in VIBES {
        if let Some(f) = &filter {
            if !name.contains(f.as_str()) {
                continue;
            }
        }
        let mut p = Project::default();
        apply_vibe_to_project(&mut p, *vibe, None, seed);
        let bars = 4usize;
        let path = outdir.join(format!("{name}.wav"));
        println!("Rendering {name} -> {}", path.display());
        if let Err(e) = fun_key_synth::engine::export_wav(&p, &path, bars) {
            eprintln!("  FAILED: {e}");
        }
    }
    if filter.as_ref().map(|f| "crash_audition".contains(f.as_str())).unwrap_or(true) {
        use fun_key_synth::dsp::DrumMachine;
        use fun_key_synth::model::{DrumModel, DrumVoiceId};
        let sr = 48_000.0_f32;
        let mut dm = DrumMachine::new(sr);
        let mut p909 = Project::default();
        p909.drums.voices[7].model = DrumModel::Tr909;
        let mut p808 = Project::default();
        p808.drums.voices[7].model = DrumModel::Tr808;
        let mut samples_l = Vec::new();
        let mut samples_r = Vec::new();
        // 1.5s Studio/909 Crash + Philharmonia layer, then 1.5s TR-808 Crash
        dm.trigger(DrumVoiceId::Cymbal, 1.0, false, false, &p909.drums);
        for _ in 0..(1.5 * sr) as usize {
            let (l, r) = dm.process_sample(&p909.drums);
            samples_l.push(l);
            samples_r.push(r);
        }
        dm.trigger(DrumVoiceId::Cymbal, 1.0, false, false, &p808.drums);
        for _ in 0..(1.5 * sr) as usize {
            let (l, r) = dm.process_sample(&p808.drums);
            samples_l.push(l);
            samples_r.push(r);
        }
        let path = outdir.join("crash_audition.wav");
        println!("Rendering crash_audition -> {}", path.display());
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).expect("wav");
        for (&l, &r) in samples_l.iter().zip(samples_r.iter()) {
            w.write_sample(l.clamp(-1.0, 1.0)).ok();
            w.write_sample(r.clamp(-1.0, 1.0)).ok();
        }
        w.finalize().ok();
    }
    if filter.as_ref().map(|f| "engine_showcase".contains(f.as_str())).unwrap_or(true) {
        use fun_key_synth::dsp::{MasterFx, MoogSynth};
        use fun_key_synth::model::{MoogParams, SynthEngine};
        let sr = 44_100.0_f32;
        let mut synth = MoogSynth::new(sr);
        let mut fx = MasterFx::new(sr);
        let mut fxp = Project::default().fx;
        fxp.reverb_enabled = true;
        fxp.delay_enabled = true;
        fxp.reverb_mix = 0.42;
        fxp.reverb_size = 0.66;
        fxp.reverb_shimmer = 0.36;
        fxp.delay_mix = 0.32;

        let mut samples_l = Vec::new();
        let mut samples_r = Vec::new();

        // 6 showcase patches x 2.0s each through lush Shimmer Hall + Ping-Pong Delay
        let stages: [(&str, SynthEngine, u8, f32, f32, f32, &[u8]); 6] = [
            ("DX7 Rhodes", SynthEngine::FmDx7, 0, 0.60, 0.30, 0.0, &[60, 64, 67, 71, 74]),
            ("DX7 Bell", SynthEngine::FmDx7, 1, 0.62, 0.15, 0.0, &[64, 67, 71, 76]),
            ("Virus 7x HyperSaw", SynthEngine::Wavetable, 0, 0.72, 0.10, 0.0, &[48, 60, 64, 67, 71]),
            ("Philharmonia Strings", SynthEngine::PhilharmoniaD50, 0, 0.35, 0.0, 0.0, &[48, 55, 64, 72]),
            ("Real Guitar + AmpCab", SynthEngine::PhilharmoniaD50, 2, 0.18, 0.0, 0.80, &[52, 59, 64, 68, 71]),
            ("D-50 Fantasia", SynthEngine::PhilharmoniaD50, 3, 0.48, 0.28, 0.0, &[60, 67, 72, 74]),
        ];

        for &(_label, eng, bank, morph, phaser, cab, chord) in &stages {
            let mut mp = MoogParams::default();
            mp.poly_mode = true;
            mp.engine = eng;
            mp.engine_bank = bank;
            mp.engine_morph = morph;
            mp.phaser_mix = phaser;
            mp.amp_cab = cab;
            mp.cutoff = 4200.0;
            mp.amp_attack = 0.012;
            mp.amp_decay = 0.85;
            mp.amp_sustain = 0.72;
            mp.amp_release = 0.45;
            synth.all_notes_off();
            let total_s = (2.0 * sr) as usize;
            let rel_s = (1.55 * sr) as usize;
            let mut triggered = [false; 8];
            for i in 0..total_s {
                for (k, &n) in chord.iter().enumerate().take(8) {
                    if !triggered[k] && i >= k * (0.032 * sr) as usize {
                        synth.note_on(n, 0.88, false, false, &mp);
                        triggered[k] = true;
                    }
                }
                if i == rel_s {
                    synth.all_notes_off();
                }
                let (sl, sr_v) = synth.process_stereo(&mp);
                let (ol, or) = fx.process_stereo(
                    sl * 0.72,
                    sr_v * 0.72,
                    sl * 0.28,
                    sr_v * 0.28,
                    sl * 0.48,
                    sr_v * 0.48,
                    &fxp,
                    118.0,
                );
                samples_l.push(ol);
                samples_r.push(or);
            }
        }
        let path = outdir.join("engine_showcase.wav");
        println!("Rendering engine_showcase -> {}", path.display());
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).expect("wav");
        for (&l, &r) in samples_l.iter().zip(samples_r.iter()) {
            w.write_sample(l.clamp(-1.0, 1.0)).ok();
            w.write_sample(r.clamp(-1.0, 1.0)).ok();
        }
        w.finalize().ok();
    }
    if filter.as_ref().map(|f| "party_audition".contains(f.as_str())).unwrap_or(true) {
        use fun_key_synth::model::{PartyMixMode, PartyPace};
        let mut p = Project::default();
        apply_vibe_to_project(&mut p, Vibe::ElectroFunk, None, seed);
        p.party_mode = true;
        p.party_auto_evolve = true;
        p.party_mix_mode = PartyMixMode::AllVibes;
        p.party_pace = PartyPace::Fast;
        p.party_sound_morph = true;
        p.party_energy = 0.80;
        let path = outdir.join("party_audition.wav");
        println!("Rendering party_audition -> {}", path.display());
        if let Err(e) = fun_key_synth::engine::export_wav(&p, &path, 12) {
            eprintln!("  FAILED: {e}");
        }
    }
    if filter.as_ref().map(|f| "philharmonia_showcase".contains(f.as_str())).unwrap_or(true) {
        use fun_key_synth::dsp::{MasterFx, MoogSynth};
        use fun_key_synth::model::{MoogParams, SynthEngine};
        let sr = 44_100.0_f32;
        let mut synth = MoogSynth::new(sr);
        let mut fx = MasterFx::new(sr);
        let mut fxp = Project::default().fx;
        fxp.reverb_enabled = true;
        fxp.delay_enabled = true;
        fxp.reverb_mix = 0.38;
        fxp.reverb_size = 0.68;
        fxp.reverb_shimmer = 0.14;
        fxp.delay_mix = 0.14;

        let mut samples_l = Vec::new();
        let mut samples_r = Vec::new();
        // 6 solo Philharmonia sections x 2.0s each
        let stages: [(u8, &[u8]); 6] = [
            (8, &[36, 48, 55]),          // Solo Cello
            (6, &[64, 67, 72]),          // Solo Violin
            (10, &[52, 60, 64, 67]),     // Pizzicato Strings
            (17, &[72, 76, 79]),         // Concert Flute
            (24, &[55, 62, 67]),         // Saxophone
            (25, &[48, 55, 49, 62]),     // Philharmonia Percussion Kit
        ];
        for &(bank_id, notes) in &stages {
            let mut mp = MoogParams::default();
            mp.poly_mode = true;
            mp.engine = SynthEngine::PhilharmoniaD50;
            mp.engine_bank = bank_id;
            mp.engine_morph = 0.0;
            mp.cutoff = 14_500.0;
            mp.resonance = 0.08;
            mp.amp_attack = if matches!(bank_id, 10 | 25) { 0.004 } else { 0.024 };
            mp.amp_decay = 1.1;
            mp.amp_sustain = if matches!(bank_id, 10 | 25) { 0.0 } else { 0.85 };
            mp.amp_release = 0.45;
            synth.all_notes_off();
            let total_s = (2.0 * sr) as usize;
            let rel_s = (1.65 * sr) as usize;
            let mut triggered = [false; 6];
            for i in 0..total_s {
                for (k, &n) in notes.iter().enumerate().take(6) {
                    if !triggered[k] && i >= k * (0.24 * sr) as usize {
                        synth.note_on(n, 0.88, false, false, &mp);
                        triggered[k] = true;
                    }
                }
                if i == rel_s {
                    synth.all_notes_off();
                }
                let (sl, sr_v) = synth.process_stereo(&mp);
                let (ol, or) = fx.process_stereo(
                    sl * 0.80,
                    sr_v * 0.80,
                    sl * 0.14,
                    sr_v * 0.14,
                    sl * 0.36,
                    sr_v * 0.36,
                    &fxp,
                    110.0,
                );
                samples_l.push(ol);
                samples_r.push(or);
            }
        }
        let path = outdir.join("philharmonia_showcase.wav");
        println!("Rendering philharmonia_showcase -> {}", path.display());
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).expect("wav");
        for (&l, &r) in samples_l.iter().zip(samples_r.iter()) {
            w.write_sample(l.clamp(-1.0, 1.0)).ok();
            w.write_sample(r.clamp(-1.0, 1.0)).ok();
        }
        w.finalize().ok();
    }
    println!("done");
}
