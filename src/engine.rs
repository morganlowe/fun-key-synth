use crate::{
    dsp::{DrumMachine, MonologueSynth, MoogSynth, Sk1Sampler, Tb303Synth},
    model::{DrumVoiceId, Project, TrackId},
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct OutputScope {
    samples: Box<[AtomicU32]>,
    cursor: AtomicUsize,
}

fn party_random(seed: u32, step: usize, lane: u32) -> f32 {
    let mut x = seed ^ (step as u32).wrapping_mul(0x9E37_79B9) ^ lane.wrapping_mul(0x85EB_CA6B);
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    x as f32 / u32::MAX as f32
}

fn party_note(
    root: u8,
    scale: crate::model::Scale,
    base: u8,
    progression: i16,
    degree: usize,
) -> u8 {
    let ints = scale.intervals();
    let n = base as i16 + root as i16 + progression + ints[degree % ints.len()] as i16;
    n.clamp(12, 108) as u8
}

fn party_melodic_step(
    seq: &crate::model::MelodicTrackSeq,
    pos: usize,
    global: usize,
    lane: u32,
    root: u8,
    scale: crate::model::Scale,
    seed: u32,
) -> crate::model::MelodicStep {
    use crate::model::ChordType;
    let len = seq.step_count.as_usize();
    let phase = pos % 16;
    let bar = (global / 16) % 4;
    let progression = [0, 7, 5, 9][(global / 64) % 4];
    let mut step = seq.steps[pos % len];
    let has_pattern = seq.steps[..len].iter().any(|s| s.active);
    if step.active {
        if lane == 1 {
            if step.chord == ChordType::Single {
                step.chord = [
                    ChordType::Minor7,
                    ChordType::Major7,
                    ChordType::Minor7,
                    ChordType::Minor7,
                ][bar];
            }
        } else if party_random(seed, global, lane) < 0.16 {
            step.note = party_note(
                root,
                scale,
                if lane == 2 { 60 } else { 36 },
                progression,
                (party_random(seed, global, lane + 19) * 7.0) as usize,
            );
        }
        return step;
    }
    let hit = match lane {
        0 => {
            matches!(phase, 0 | 3 | 6 | 8 | 10 | 14)
                && (!has_pattern || party_random(seed, global, lane) < 0.20)
        }
        1 => phase == 0 && (!has_pattern || party_random(seed, global, lane) < 0.32),
        2 => {
            matches!(phase, 2 | 6 | 10 | 14)
                && (!has_pattern || party_random(seed, global, lane) < 0.72)
        }
        3 => {
            matches!(phase, 0 | 3 | 7 | 10 | 14)
                && (!has_pattern || party_random(seed, global, lane) < 0.62)
        }
        _ => matches!(phase, 0 | 8) && (!has_pattern || party_random(seed, global, lane) < 0.28),
    };
    if !hit {
        return step;
    }
    let base = match lane {
        0 | 3 => 36,
        1 => 48,
        2 => 60,
        _ => 60,
    };
    let degree = match lane {
        0 => {
            if matches!(phase, 3 | 10 | 14) {
                2
            } else {
                0
            }
        }
        1 => 0,
        2 => (phase / 2) % 7,
        3 => (phase / 3) % 5,
        _ => 3,
    };
    step.active = true;
    step.note = party_note(root, scale, base, progression, degree);
    step.velocity = if phase % 4 == 0 { 0.86 } else { 0.68 };
    step.gate = if lane == 1 { 1.0 } else { 0.62 };
    step.accent = phase % 4 == 0;
    step.slide = lane == 3 && phase % 4 == 3;
    if lane == 1 {
        step.chord = [
            ChordType::Minor7,
            ChordType::Major7,
            ChordType::Minor7,
            ChordType::Minor7,
        ][bar];
    }
    step
}

fn party_drum_step(
    lane: usize,
    seq: &crate::model::DrumLane,
    pos: usize,
    global: usize,
    seed: u32,
) -> crate::model::DrumStep {
    let mut step = seq.steps[pos % seq.step_count.as_usize()];
    let phase = pos % 16;
    let has_pattern = seq.steps[..seq.step_count.as_usize()]
        .iter()
        .any(|s| s.active);
    if step.active {
        return step;
    }
    let r = party_random(seed, global, lane as u32 + 43);
    step.active = match lane {
        0 => matches!(phase, 0 | 4 | 8 | 12) && !has_pattern,
        1 => matches!(phase, 4 | 12) && !has_pattern,
        2 => matches!(phase, 4 | 12) && r < 0.28,
        3 => matches!(phase, 3 | 11 | 14) && r < 0.28,
        4 => phase % 2 == 0 || r < 0.16,
        5 => matches!(phase, 6 | 14) && r < 0.55,
        6 => global % 64 >= 48 && matches!(phase, 12 | 14 | 15) && r < 0.48,
        _ => phase == 0 && global % 64 == 0,
    };
    if step.active {
        step.velocity = if phase % 4 == 0 { 0.82 } else { 0.55 };
        step.accent = phase == 0;
    }
    step
}
impl OutputScope {
    fn new(size: usize) -> Self {
        Self {
            samples: (0..size)
                .map(|_| AtomicU32::new(0.0f32.to_bits()))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            cursor: AtomicUsize::new(0),
        }
    }
    fn push(&self, sample: f32) {
        let i = self.cursor.fetch_add(1, Ordering::Relaxed) % self.samples.len();
        self.samples[i].store(sample.to_bits(), Ordering::Relaxed);
    }
    pub fn snapshot(&self) -> Vec<f32> {
        let end = self.cursor.load(Ordering::Relaxed);
        (0..self.samples.len())
            .map(|i| {
                let ix = (end + i) % self.samples.len();
                f32::from_bits(self.samples[ix].load(Ordering::Relaxed))
            })
            .collect()
    }
}

pub struct Engine {
    pub project: Arc<Mutex<Project>>,
    playing: Arc<Mutex<bool>>,
    stream: Option<cpal::Stream>,
    pub audio_error: Option<String>,
    pub scope: Arc<OutputScope>,
}

impl Engine {
    pub fn new(project: Project) -> Self {
        let project = Arc::new(Mutex::new(project));
        let playing = Arc::new(Mutex::new(false));
        let scope = Arc::new(OutputScope::new(512));
        let mut this = Self {
            project: project.clone(),
            playing: playing.clone(),
            stream: None,
            audio_error: None,
            scope: scope.clone(),
        };
        if let Err(e) = this.start_audio(project, playing, scope) {
            this.audio_error = Some(e);
        }
        this
    }
    pub fn is_playing(&self) -> bool {
        *self.playing.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn set_playing(&self, value: bool) {
        *self.playing.lock().unwrap_or_else(|e| e.into_inner()) = value;
    }
    fn start_audio(
        &mut self,
        project: Arc<Mutex<Project>>,
        playing: Arc<Mutex<bool>>,
        scope: Arc<OutputScope>,
    ) -> Result<(), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        let mut runtime = Runtime::new(rate);
        let err_fn = |e| eprintln!("audio stream error: {e}");
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                &config.into(),
                move |d: &mut [f32], _| {
                    render(d, channels, &project, &playing, &scope, &mut runtime)
                },
                err_fn,
                None,
            ),
            _ => {
                return Err(format!(
                    "Unsupported output sample format: {:?}; choose an f32 device",
                    config.sample_format()
                ))
            }
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }
}

struct Runtime {
    sr: f32,
    tick_left: f32,
    step: usize,
    was_running: bool,
    notes: [u8; 4],
    gate_left: [f32; 4],
    moog_chord_notes: [u8; 4],
    moog_chord_len: usize,
    pad_chord_notes: [u8; 4],
    pad_chord_len: usize,
    pad_gate_left: f32,
    moog: MoogSynth,
    pad_moog: MoogSynth,
    mono: MonologueSynth,
    acid: Tb303Synth,
    drums: DrumMachine,
    sampler: Sk1Sampler,
    fx: crate::dsp::MasterFx,
}
impl Runtime {
    fn new(sr: f32) -> Self {
        Self {
            sr,
            tick_left: 0.0,
            step: 0,
            was_running: false,
            notes: [48, 60, 36, 60],
            gate_left: [0.0; 4],
            moog_chord_notes: [48; 4],
            moog_chord_len: 1,
            pad_chord_notes: [60; 4],
            pad_chord_len: 1,
            pad_gate_left: 0.0,
            moog: MoogSynth::new(sr),
            pad_moog: MoogSynth::new(sr),
            mono: MonologueSynth::new(sr),
            acid: Tb303Synth::new(sr),
            drums: DrumMachine::new(sr),
            sampler: Sk1Sampler::new(sr),
            fx: crate::dsp::MasterFx::new(sr),
        }
    }
}

fn render(
    out: &mut [f32],
    channels: usize,
    project: &Arc<Mutex<Project>>,
    playing: &Arc<Mutex<bool>>,
    scope: &Arc<OutputScope>,
    rt: &mut Runtime,
) {
    let p = project.lock().unwrap_or_else(|e| e.into_inner());
    let run = *playing.lock().unwrap_or_else(|e| e.into_inner());
    if rt.was_running && !run {
        rt.moog.all_notes_off();
        rt.pad_moog.all_notes_off();
        rt.mono.all_notes_off();
        rt.acid.all_notes_off();
        rt.sampler.all_notes_off();
        rt.step = 0;
        rt.tick_left = 0.0;
    }
    rt.was_running = run;
    for frame in out.chunks_mut(channels.max(1)) {
        if run {
            if rt.tick_left <= 0.0 {
                let (pat, step, drop_section, fill_section) =
                    if p.song_mode && !p.song_sections.is_empty() {
                        let total: usize = p
                            .song_sections
                            .iter()
                            .map(|s| s.bars.max(1) as usize * 16)
                            .sum();
                        let mut offset = rt.step % total.max(1);
                        let mut selected = &p.song_sections[0];
                        for section in &p.song_sections {
                            let len = section.bars.max(1) as usize * 16;
                            if offset < len {
                                selected = section;
                                break;
                            }
                            offset -= len;
                        }
                        (
                            &p.patterns[selected.pattern.min(p.patterns.len() - 1)],
                            offset,
                            selected.bass_drop,
                            selected.fill,
                        )
                    } else {
                        (p.current_pattern(), rt.step, false, false)
                    };
                let drop_silence = drop_section && step <= 4;
                let global_step = rt.step;
                let note = |seq: &crate::model::MelodicTrackSeq, lane: u32| {
                    if p.party_mode {
                        party_melodic_step(
                            seq,
                            step,
                            global_step,
                            lane,
                            p.root_note,
                            p.scale,
                            p.party_seed,
                        )
                    } else {
                        seq.steps[step % seq.step_count.as_usize()]
                    }
                };
                let pad = note(&pat.moog_pad, 1);
                for n in rt.moog_chord_notes.iter().take(rt.moog_chord_len) {
                    rt.moog.note_off(*n);
                }
                if pad.active || drop_silence {
                    for n in rt.pad_chord_notes.iter().take(rt.pad_chord_len) {
                        rt.pad_moog.note_off(*n);
                    }
                }
                rt.mono.note_off(rt.notes[1]);
                rt.acid.note_off(rt.notes[2]);
                rt.sampler.note_off(rt.notes[3]);
                let a = note(&pat.moog, 0);
                if a.active && !drop_silence {
                    rt.notes[0] = a.note;
                    rt.gate_left[0] = 60.0 / p.bpm.max(1.0) / 4.0 * rt.sr * a.gate;
                    let intervals = a.chord.intervals();
                    rt.moog_chord_len = intervals.len().min(4);
                    for (i, interval) in intervals.iter().take(rt.moog_chord_len).enumerate() {
                        let note = a.note.saturating_add(*interval).min(108);
                        rt.moog_chord_notes[i] = note;
                        rt.moog
                            .note_on(note, a.velocity, a.accent, a.slide, &p.moog);
                    }
                }
                if pad.active && !drop_silence {
                    rt.pad_gate_left = 60.0 / p.bpm.max(1.0) / 4.0 * rt.sr * pad.gate * 16.0;
                    let intervals = pad.chord.intervals();
                    rt.pad_chord_len = intervals.len().min(4);
                    for (i, interval) in intervals.iter().take(rt.pad_chord_len).enumerate() {
                        let n = pad.note.saturating_add(*interval).min(108);
                        rt.pad_chord_notes[i] = n;
                        rt.pad_moog
                            .note_on(n, pad.velocity, pad.accent, pad.slide, &p.moog_pad);
                    }
                }
                let a = note(&pat.monologue, 2);
                if a.active && !drop_silence {
                    rt.notes[1] = a.note;
                    rt.gate_left[1] = 60.0 / p.bpm.max(1.0) / 4.0 * rt.sr * a.gate;
                    rt.mono
                        .note_on(a.note, a.velocity, a.accent, a.slide, &p.monologue);
                }
                let a = note(&pat.tb303, 3);
                if a.active && !drop_silence {
                    rt.notes[2] = a.note;
                    rt.gate_left[2] = 60.0 / p.bpm.max(1.0) / 4.0 * rt.sr * a.gate;
                    rt.acid.note_on(a.note, a.velocity, a.accent, a.slide);
                }
                let a = note(&pat.sampler, 4);
                if a.active && !drop_silence {
                    rt.notes[3] = a.note;
                    rt.gate_left[3] = 60.0 / p.bpm.max(1.0) / 4.0 * rt.sr * a.gate;
                    rt.sampler.note_on(a.note, a.velocity, &p.sampler);
                }
                for (i, lane) in pat.drums.lanes.iter().enumerate() {
                    let s = if p.party_mode {
                        party_drum_step(i, lane, step, global_step, p.party_seed)
                    } else {
                        lane.steps[step % lane.step_count.as_usize()]
                    };
                    if s.active && !drop_silence {
                        if let Some(v) = DrumVoiceId::ALL.get(i) {
                            rt.drums.trigger(*v, s.velocity, s.accent, s.flam, &p.drums);
                        }
                    }
                }
                if drop_section && step == 4 {
                    rt.drums
                        .trigger(DrumVoiceId::Kick, 1.0, true, false, &p.drums);
                    rt.notes[0] = 24;
                    rt.moog_chord_notes = [24; 4];
                    rt.moog_chord_len = 1;
                    rt.gate_left[0] = 60.0 / p.bpm.max(1.0) / 4.0 * rt.sr * 3.0;
                    rt.moog.note_on(24, 1.0, true, false, &p.moog);
                }
                if fill_section && step % 16 >= 12 {
                    let voice = match step % 16 {
                        12 | 14 => DrumVoiceId::Snare,
                        13 => DrumVoiceId::Tom,
                        _ => DrumVoiceId::Cymbal,
                    };
                    rt.drums.trigger(
                        voice,
                        if step % 16 == 15 { 1.0 } else { 0.78 },
                        step % 16 == 15,
                        false,
                        &p.drums,
                    );
                }
                let seconds = 60.0 / p.bpm.max(1.0) / 4.0;
                rt.tick_left = seconds * rt.sr;
                rt.step += 1;
            }
            rt.tick_left -= 1.0;
        }
        if run {
            for i in 0..4 {
                if rt.gate_left[i] > 0.0 {
                    rt.gate_left[i] -= 1.0;
                    if rt.gate_left[i] <= 0.0 {
                        match i {
                            0 => {
                                for n in rt.moog_chord_notes.iter().take(rt.moog_chord_len) {
                                    rt.moog.note_off(*n);
                                }
                            }
                            1 => rt.mono.note_off(rt.notes[i]),
                            2 => rt.acid.note_off(rt.notes[i]),
                            _ => rt.sampler.note_off(rt.notes[i]),
                        }
                    }
                }
            }
            if rt.pad_gate_left > 0.0 {
                rt.pad_gate_left -= 1.0;
                if rt.pad_gate_left <= 0.0 {
                    for n in rt.pad_chord_notes.iter().take(rt.pad_chord_len) {
                        rt.pad_moog.note_off(*n);
                    }
                }
            }
        }
        let solo_active = p.mixer.channels.iter().any(|c| c.solo) || p.mixer.moog_pad_solo;
        let mut mono = 0.0;
        for (index, sample) in [
            rt.moog.process_sample(&p.moog),
            rt.mono.process_sample(&p.monologue),
            rt.acid.process_sample(&p.tb303),
            0.0,
            rt.sampler.process_sample(&p.sampler),
            0.0,
        ]
        .into_iter()
        .enumerate()
        {
            let channel = p.mixer.channels[index];
            if !channel.mute && (!solo_active || channel.solo) {
                mono += sample * channel.volume;
            }
        }
        let pad_sample = rt.pad_moog.process_sample(&p.moog_pad);
        if !p.mixer.moog_pad_mute && (!solo_active || p.mixer.moog_pad_solo) {
            mono += pad_sample * p.mixer.moog_pad_volume;
        }
        let (dl, dr) = rt.drums.process_sample(&p.drums);
        let drum_channel = p.mixer.channels[3];
        if !drum_channel.mute && (!solo_active || drum_channel.solo) {
            mono += (dl + dr) * 0.5 * drum_channel.volume;
        }
        mono = rt.fx.process(mono, &p.fx, p.bpm) * p.mixer.master_volume;
        for (c, s) in frame.iter_mut().enumerate() {
            *s = mono * if channels >= 2 && c == 1 { 0.96 } else { 1.0 };
        }
        scope.push(mono.clamp(-1.0, 1.0));
    }
    let _ = TrackId::ALL;
}
