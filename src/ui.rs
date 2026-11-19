use crate::{
    engine::Engine,
    model::{load_project, save_project, MelodicTrackSeq, Project, StepCount, TrackId},
};
use eframe::egui;

pub struct GrooveboxApp {
    engine: Engine,
    track: TrackId,
    page: usize,
    status: String,
    page_view: PageView,
    selected_step: usize,
}
#[derive(Clone, Copy, PartialEq)]
enum PageView {
    Sequencer,
    Song,
    Mixer,
    Global,
    Sound,
    Effects,
}
impl GrooveboxApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            engine: Engine::new(Project::default()),
            track: TrackId::Drums,
            page: 0,
            status: String::new(),
            page_view: PageView::Sequencer,
            selected_step: 0,
        };
        app.style();
        app
    }
    fn style(&mut self) {}
    fn seq_mut(p: &mut Project, t: TrackId) -> Option<&mut MelodicTrackSeq> {
        let pat = p.current_pattern_mut();
        match t {
            TrackId::Moog => Some(&mut pat.moog),
            TrackId::MoogPad => Some(&mut pat.moog_pad),
            TrackId::Monologue => Some(&mut pat.monologue),
            TrackId::Tb303 => Some(&mut pat.tb303),
            TrackId::Sampler => Some(&mut pat.sampler),
            _ => None,
        }
    }
    fn toggle(&mut self, idx: usize) {
        let mut p = self
            .engine
            .project
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match self.track {
            TrackId::Drums => {
                for lane in &mut p.current_pattern_mut().drums.lanes {
                    let i = idx % lane.step_count.as_usize();
                    lane.steps[i].active = !lane.steps[i].active;
                }
            }
            TrackId::MidiOut => {
                let s = &mut p.current_pattern_mut().midi_out.steps[idx];
                s.active = !s.active;
            }
            _ => {
                if let Some(s) = Self::seq_mut(&mut p, self.track) {
                    let i = idx % s.step_count.as_usize();
                    s.steps[i].active = !s.steps[i].active;
                    if self.track == TrackId::MoogPad && s.steps[i].active {
                        s.steps[i].gate = 1.0;
                    }
                }
            }
        }
    }

    fn slider(
        ui: &mut egui::Ui,
        label: &str,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
    ) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(egui::Slider::new(value, range).show_value(true));
        });
    }

    fn sound_panel(ui: &mut egui::Ui, p: &mut Project, track: TrackId) {
        egui::ScrollArea::vertical().show(ui, |ui| match track {
            TrackId::Moog => {
                ui.heading("Moog style • three oscillator voice");
                ui.horizontal(|ui| {
                    ui.label("Load sound");
                    egui::ComboBox::from_id_salt("moog_quick_sounds")
                        .selected_text("Choose a Moog sound")
                        .show_ui(ui, |ui| {
                            for (id, name) in [
                                (0, "Deep Bass"),
                                (1, "Sub Pulse"),
                                (2, "Classic Lead"),
                                (3, "Soaring Lead"),
                                (4, "Analog Brass"),
                                (5, "Chord Pad"),
                                (6, "Glass Pad"),
                            ] {
                                if ui.selectable_label(false, name).clicked() {
                                    Self::apply_moog_preset(p, id);
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut p.moog.poly_mode, "Polyphonic");
                    Self::slider(ui, "Glide", &mut p.moog.glide, 0.0..=1.0);
                });
                ui.separator();
                ui.label("OSCILLATORS");
                for (label, wave, level) in [
                    ("OSC 1", &mut p.moog.osc1_wave, &mut p.moog.osc1_level),
                    ("OSC 2", &mut p.moog.osc2_wave, &mut p.moog.osc2_level),
                    ("OSC 3", &mut p.moog.osc3_wave, &mut p.moog.osc3_level),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        egui::ComboBox::from_id_salt(label)
                            .selected_text(wave.name())
                            .show_ui(ui, |ui| {
                                for v in crate::model::MoogWave::ALL {
                                    ui.selectable_value(wave, v, v.name());
                                }
                            });
                        Self::slider(ui, "Level", level, 0.0..=1.0);
                    });
                }
                Self::slider(ui, "Osc 2 detune", &mut p.moog.osc2_detune, -50.0..=50.0);
                ui.horizontal(|ui| {
                    ui.label("Osc 2 octave");
                    ui.add(egui::Slider::new(&mut p.moog.osc2_octave, -2..=2));
                });
                ui.separator();
                ui.label("LADDER FILTER");
                Self::slider(ui, "Cutoff Hz", &mut p.moog.cutoff, 30.0..=18000.0);
                Self::slider(ui, "Resonance", &mut p.moog.resonance, 0.0..=0.96);
                Self::slider(ui, "Envelope amount", &mut p.moog.env_amount, -1.0..=1.0);
                Self::slider(ui, "Key tracking", &mut p.moog.key_track, 0.0..=1.0);
                Self::slider(ui, "Drive", &mut p.moog.drive, 0.0..=1.0);
                ui.separator();
                ui.label("FILTER ENVELOPE");
                Self::slider(ui, "Attack", &mut p.moog.filt_attack, 0.001..=3.0);
                Self::slider(ui, "Decay", &mut p.moog.filt_decay, 0.01..=5.0);
                Self::slider(ui, "Sustain", &mut p.moog.filt_sustain, 0.0..=1.0);
                Self::slider(ui, "Release", &mut p.moog.filt_release, 0.01..=5.0);
                ui.separator();
                ui.label("AMP ENVELOPE");
                Self::slider(ui, "Attack", &mut p.moog.amp_attack, 0.001..=3.0);
                Self::slider(ui, "Decay", &mut p.moog.amp_decay, 0.01..=5.0);
                Self::slider(ui, "Sustain", &mut p.moog.amp_sustain, 0.0..=1.0);
                Self::slider(ui, "Release", &mut p.moog.amp_release, 0.01..=5.0);
            }
            TrackId::MoogPad => {
                ui.heading("Second independent Moog • pads / layers");
                if ui.button("Load Soaring Pad sound").clicked() {
                    p.moog_pad.poly_mode = true;
                    p.moog_pad.osc1_wave = crate::model::MoogWave::Triangle;
                    p.moog_pad.osc2_wave = crate::model::MoogWave::Saw;
                    p.moog_pad.osc2_detune = 8.0;
                    p.moog_pad.cutoff = 5200.0;
                    p.moog_pad.resonance = 0.16;
                    p.moog_pad.filt_attack = 0.55;
                    p.moog_pad.filt_release = 2.8;
                    p.moog_pad.amp_attack = 0.65;
                    p.moog_pad.amp_decay = 1.2;
                    p.moog_pad.amp_sustain = 0.82;
                    p.moog_pad.amp_release = 3.2;
                    p.moog_pad.lfo_to_filter = 0.2;
                }
                ui.checkbox(&mut p.moog_pad.poly_mode, "Polyphonic pad");
                ui.horizontal(|ui| {
                    ui.label("OSC 1");
                    for w in crate::model::MoogWave::ALL {
                        ui.selectable_value(&mut p.moog_pad.osc1_wave, w, w.name());
                    }
                });
                Self::slider(ui, "Cutoff", &mut p.moog_pad.cutoff, 30.0..=18000.0);
                Self::slider(ui, "Resonance", &mut p.moog_pad.resonance, 0.0..=0.96);
                Self::slider(ui, "Detune", &mut p.moog_pad.osc2_detune, -50.0..=50.0);
                Self::slider(
                    ui,
                    "Filter attack",
                    &mut p.moog_pad.filt_attack,
                    0.001..=3.0,
                );
                Self::slider(
                    ui,
                    "Filter release",
                    &mut p.moog_pad.filt_release,
                    0.01..=5.0,
                );
                Self::slider(ui, "Amp attack", &mut p.moog_pad.amp_attack, 0.001..=3.0);
                Self::slider(ui, "Amp sustain", &mut p.moog_pad.amp_sustain, 0.0..=1.0);
                Self::slider(ui, "Amp release", &mut p.moog_pad.amp_release, 0.01..=5.0);
            }
            TrackId::Monologue => {
                ui.heading("Monologue style • bite and motion");
                ui.horizontal(|ui| {
                    ui.label("OSC 1");
                    for v in crate::model::MonologueOscWave::ALL {
                        ui.selectable_value(&mut p.monologue.vco1_wave, v, v.name());
                    }
                    Self::slider(ui, "Level", &mut p.monologue.vco1_level, 0.0..=1.0);
                });
                ui.horizontal(|ui| {
                    ui.label("OSC 2");
                    for v in crate::model::MonologueOscWave::ALL {
                        ui.selectable_value(&mut p.monologue.vco2_wave, v, v.name());
                    }
                    Self::slider(ui, "Level", &mut p.monologue.vco2_level, 0.0..=1.0);
                });
                ui.separator();
                ui.label("FILTER");
                Self::slider(ui, "Cutoff Hz", &mut p.monologue.cutoff, 30.0..=18000.0);
                Self::slider(ui, "Resonance", &mut p.monologue.resonance, 0.0..=0.95);
                Self::slider(ui, "Drive", &mut p.monologue.drive, 0.0..=1.0);
                Self::slider(ui, "EG intensity", &mut p.monologue.eg_int, -1.0..=1.0);
                Self::slider(ui, "EG attack", &mut p.monologue.eg_attack, 0.001..=3.0);
                Self::slider(ui, "EG decay", &mut p.monologue.eg_decay, 0.01..=5.0);
                ui.checkbox(&mut p.monologue.amp_sustain, "Sustain amp");
                Self::slider(ui, "Glide", &mut p.monologue.glide, 0.0..=1.0);
            }
            TrackId::Tb303 => {
                ui.heading("Acid bass • diode ladder");
                ui.horizontal(|ui| {
                    ui.label("Wave");
                    ui.selectable_value(&mut p.tb303.wave, crate::model::Tb303Wave::Saw, "SAW");
                    ui.selectable_value(
                        &mut p.tb303.wave,
                        crate::model::Tb303Wave::Square,
                        "SQUARE",
                    );
                });
                Self::slider(ui, "Tuning", &mut p.tb303.tuning, -12.0..=12.0);
                Self::slider(ui, "Cutoff Hz", &mut p.tb303.cutoff, 40.0..=10000.0);
                Self::slider(ui, "Resonance", &mut p.tb303.resonance, 0.0..=0.98);
                Self::slider(ui, "Env modulation", &mut p.tb303.env_mod, 0.0..=1.0);
                Self::slider(ui, "Decay", &mut p.tb303.decay, 0.06..=1.8);
                Self::slider(ui, "Accent", &mut p.tb303.accent, 0.0..=1.0);
                Self::slider(ui, "Overdrive", &mut p.tb303.overdrive, 0.0..=1.0);
            }
            TrackId::Drums => {
                ui.heading("808 / 909 voice shaping");
                ui.label("Select a drum lane, then tune its voice below.");
                for (i, v) in p.drums.voices.iter_mut().enumerate() {
                    ui.collapsing(crate::model::DrumVoiceId::ALL[i].full_name(), |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut v.model,
                                crate::model::DrumModel::Tr808,
                                "808",
                            );
                            ui.selectable_value(
                                &mut v.model,
                                crate::model::DrumModel::Tr909,
                                "909",
                            );
                            ui.checkbox(&mut v.mute, "Mute");
                        });
                        Self::slider(ui, "Tune", &mut v.tune, -1.0..=1.0);
                        Self::slider(ui, "Tone", &mut v.tone, 0.0..=1.0);
                        Self::slider(ui, "Decay", &mut v.decay, 0.0..=1.0);
                        Self::slider(ui, "Snap", &mut v.snappy, 0.0..=1.0);
                        Self::slider(ui, "Level", &mut v.level, 0.0..=1.5);
                    });
                }
            }
            TrackId::Sampler => {
                ui.heading("SK-1 sampler");
                Self::slider(ui, "Filter cutoff", &mut p.sampler.cutoff, 150.0..=18000.0);
                Self::slider(ui, "Resonance", &mut p.sampler.resonance, 0.0..=0.9);
                Self::slider(ui, "Bit crush", &mut p.sampler.bit_crush, 0.0..=1.0);
                Self::slider(ui, "Attack", &mut p.sampler.attack, 0.001..=3.0);
                Self::slider(ui, "Decay", &mut p.sampler.decay, 0.01..=5.0);
                Self::slider(ui, "Sustain", &mut p.sampler.sustain, 0.0..=1.0);
                Self::slider(ui, "Release", &mut p.sampler.release, 0.01..=5.0);
                ui.checkbox(&mut p.sampler.reverse, "Reverse");
            }
            TrackId::MidiOut => {
                ui.heading("MIDI output");
                ui.label("External MIDI routing is not connected yet.");
            }
        });
    }

    fn effects_panel(ui: &mut egui::Ui, p: &mut Project) {
        ui.heading("MASTER EFFECTS");
        ui.label("Tempo-synced delay, room reverb, and analog saturation.");
        ui.separator();
        ui.label("DELAY");
        ui.horizontal(|ui| {
            ui.label("Time division");
            for d in crate::model::DelayDivision::ALL {
                ui.selectable_value(&mut p.fx.delay_div, d, d.name());
            }
        });
        Self::slider(ui, "Feedback", &mut p.fx.delay_feedback, 0.0..=0.92);
        Self::slider(ui, "Tone", &mut p.fx.delay_tone, 0.0..=1.0);
        Self::slider(ui, "Mix", &mut p.fx.delay_mix, 0.0..=1.0);
        ui.separator();
        ui.label("REVERB");
        Self::slider(ui, "Size", &mut p.fx.reverb_size, 0.0..=0.98);
        Self::slider(ui, "Damping", &mut p.fx.reverb_damp, 0.0..=1.0);
        Self::slider(ui, "Mix", &mut p.fx.reverb_mix, 0.0..=1.0);
        ui.separator();
        ui.label("MASTER COLOR");
        Self::slider(ui, "Analog warmth", &mut p.fx.analog_warmth, 0.0..=1.0);
        Self::slider(ui, "Master volume", &mut p.mixer.master_volume, 0.0..=1.25);
    }

    fn global_panel(ui: &mut egui::Ui, p: &mut Project) {
        ui.heading("GLOBAL PERFORMANCE");
        ui.label("These controls affect the full mix, regardless of the selected instrument.");
        ui.separator();
        ui.label("MASTER FILTER");
        ui.horizontal(|ui| {
            for mode in [
                crate::model::GlobalFilterMode::LowPass,
                crate::model::GlobalFilterMode::BandPass,
                crate::model::GlobalFilterMode::HighPass,
            ] {
                ui.selectable_value(&mut p.fx.global_filter_mode, mode, format!("{mode:?}"));
            }
        });
        Self::slider(ui, "Cutoff", &mut p.fx.global_filter_cutoff, 20.0..=18000.0);
        Self::slider(
            ui,
            "Resonance",
            &mut p.fx.global_filter_resonance,
            0.0..=0.96,
        );
        ui.separator();
        ui.label("LIVE RHYTHM / CIRCUIT BEND");
        Self::slider(ui, "Chop", &mut p.fx.chop, 0.0..=1.0);
        Self::slider(ui, "Repeat", &mut p.fx.repeat, 0.0..=1.0);
        Self::slider(ui, "Glitch", &mut p.fx.glitch, 0.0..=1.0);
        Self::slider(ui, "Circuit bend", &mut p.fx.circuit_bend, 0.0..=1.0);
        if ui.button("Clear live effects").clicked() {
            p.fx.chop = 0.0;
            p.fx.repeat = 0.0;
            p.fx.glitch = 0.0;
            p.fx.circuit_bend = 0.0;
            p.fx.global_filter_cutoff = 18000.0;
            p.fx.global_filter_resonance = 0.08;
        }
    }

    fn song_panel(ui: &mut egui::Ui, p: &mut Project) {
        ui.heading("SONG ARRANGEMENT");
        ui.label("Build a song from pattern sections. Drops create a one-beat break; fills add a drum run in the final bar.");
        ui.checkbox(
            &mut p.song_mode,
            "Play arrangement instead of looping the selected pattern",
        );
        if ui
            .button("✨ Auto make party song from this pattern")
            .clicked()
        {
            Self::generate_party_song(p);
        }
        ui.separator();
        let names = p
            .patterns
            .iter()
            .map(|x| x.name.clone())
            .collect::<Vec<_>>();
        let mut remove = None;
        let mut up = None;
        let mut down = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for i in 0..p.song_sections.len() {
                ui.horizontal(|ui| {
                    ui.label(format!("{:02}", i + 1));
                    let section = &mut p.song_sections[i];
                    ui.add(egui::TextEdit::singleline(&mut section.name).desired_width(105.0));
                    egui::ComboBox::from_id_salt(("song_pattern", i))
                        .selected_text(names[section.pattern.min(names.len() - 1)].clone())
                        .show_ui(ui, |ui| {
                            for (n, name) in names.iter().enumerate() {
                                ui.selectable_value(&mut section.pattern, n, name);
                            }
                        });
                    ui.label("bars");
                    ui.add(egui::DragValue::new(&mut section.bars).range(1..=16));
                    ui.checkbox(&mut section.bass_drop, "Bass drop");
                    ui.checkbox(&mut section.fill, "Fill");
                    if ui.small_button("↑").clicked() && i > 0 {
                        up = Some(i)
                    }
                    if ui.small_button("↓").clicked() && i + 1 < p.song_sections.len() {
                        down = Some(i)
                    }
                    if ui.small_button("×").clicked() {
                        remove = Some(i)
                    }
                });
            }
        });
        if let Some(i) = remove {
            p.song_sections.remove(i);
        }
        if let Some(i) = up {
            p.song_sections.swap(i, i - 1);
        }
        if let Some(i) = down {
            p.song_sections.swap(i, i + 1);
        }
        ui.separator();
        if ui.button("+ Add section from selected pattern").clicked() {
            let n = p.song_sections.len() + 1;
            p.song_sections.push(crate::model::SongSection {
                name: format!("Section {n}"),
                pattern: p.active_pattern,
                bars: 4,
                bass_drop: false,
                fill: false,
            });
        }
    }

    fn pattern_has_content(p: &crate::model::Pattern) -> bool {
        p.moog.steps.iter().any(|s| s.active)
            || p.moog_pad.steps.iter().any(|s| s.active)
            || p.monologue.steps.iter().any(|s| s.active)
            || p.tb303.steps.iter().any(|s| s.active)
            || p.sampler.steps.iter().any(|s| s.active)
            || p.drums
                .lanes
                .iter()
                .any(|l| l.steps.iter().any(|s| s.active))
    }

    fn generate_party_song(p: &mut Project) {
        let source = p.active_pattern.min(p.patterns.len() - 1);
        let base = p.patterns[source].clone();
        let slots = (0..p.patterns.len())
            .filter(|i| *i != source && !Self::pattern_has_content(&p.patterns[*i]))
            .take(3)
            .collect::<Vec<_>>();
        let mut variants = [source, source, source];
        for (variant, slot) in slots.iter().copied().enumerate() {
            let mut pattern = base.clone();
            match variant {
                0 => {
                    pattern.name = "Party Intro".into();
                    for (i, step) in pattern.tb303.steps.iter_mut().enumerate() {
                        if i % 2 == 1 {
                            step.active = false;
                        }
                    }
                    for lane in &mut pattern.drums.lanes {
                        for (i, step) in lane.steps.iter_mut().enumerate() {
                            if i % 4 != 0 {
                                step.active = false;
                            }
                        }
                    }
                }
                1 => {
                    pattern.name = "Party Build".into();
                    for i in 0..pattern.drums.lanes[4].steps.len() {
                        if i % 2 == 0 {
                            pattern.drums.lanes[4].steps[i].active = true;
                            pattern.drums.lanes[4].steps[i].velocity =
                                if i % 4 == 0 { 0.86 } else { 0.62 };
                        }
                    }
                    for i in (0..pattern.drums.lanes[6].steps.len()).step_by(8) {
                        pattern.drums.lanes[6].steps[i].active = true;
                    }
                }
                _ => {
                    pattern.name = "Party Break".into();
                    pattern.drums.clear();
                    for i in (0..pattern.drums.lanes[4].steps.len()).step_by(4) {
                        pattern.drums.lanes[4].steps[i].active = true;
                    }
                    for step in &mut pattern.tb303.steps {
                        step.active = false;
                    }
                    for step in &mut pattern.moog.steps {
                        step.active = false;
                    }
                }
            }
            p.patterns[slot] = pattern;
            p.patterns[slot].normalize();
            variants[variant] = slot;
        }
        let [intro, build, breakdown] = variants;
        p.song_sections = vec![
            crate::model::SongSection {
                name: "Intro".into(),
                pattern: intro,
                bars: 4,
                bass_drop: false,
                fill: false,
            },
            crate::model::SongSection {
                name: "Build".into(),
                pattern: build,
                bars: 4,
                bass_drop: false,
                fill: true,
            },
            crate::model::SongSection {
                name: "Breakdown".into(),
                pattern: breakdown,
                bars: 4,
                bass_drop: false,
                fill: false,
            },
            crate::model::SongSection {
                name: "Drop".into(),
                pattern: source,
                bars: 8,
                bass_drop: true,
                fill: false,
            },
            crate::model::SongSection {
                name: "Build 2".into(),
                pattern: build,
                bars: 4,
                bass_drop: false,
                fill: true,
            },
            crate::model::SongSection {
                name: "Final Drop".into(),
                pattern: source,
                bars: 8,
                bass_drop: true,
                fill: true,
            },
            crate::model::SongSection {
                name: "Outro".into(),
                pattern: intro,
                bars: 4,
                bass_drop: false,
                fill: false,
            },
        ];
        p.song_mode = true;
    }

    fn mixer_panel(ui: &mut egui::Ui, p: &mut Project) {
        ui.heading("TRACK MIXER");
        ui.label("Balance the synths and drums before the master effects.");
        ui.separator();
        if ui.button("Reset to clearer synth balance").clicked() {
            p.mixer.channels[0].volume = 1.12;
            p.mixer.channels[1].volume = 0.88;
            p.mixer.channels[2].volume = 0.96;
            p.mixer.channels[3].volume = 0.56;
            p.mixer.channels[4].volume = 0.72;
            p.mixer.moog_pad_volume = 0.78;
        }
        let mix_tracks = [
            TrackId::Moog,
            TrackId::Monologue,
            TrackId::Tb303,
            TrackId::Drums,
            TrackId::Sampler,
            TrackId::MidiOut,
        ];
        for (i, channel) in p.mixer.channels.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(mix_tracks[i].short_name());
                ui.add_sized(
                    [210.0, 20.0],
                    egui::Slider::new(&mut channel.volume, 0.0..=1.5).show_value(true),
                );
                ui.checkbox(&mut channel.mute, "Mute");
                ui.checkbox(&mut channel.solo, "Solo");
            });
        }
        ui.horizontal(|ui| {
            ui.label("MOOG PAD");
            ui.add_sized(
                [210.0, 20.0],
                egui::Slider::new(&mut p.mixer.moog_pad_volume, 0.0..=1.5),
            );
            ui.checkbox(&mut p.mixer.moog_pad_mute, "Mute");
            ui.checkbox(&mut p.mixer.moog_pad_solo, "Solo");
        });
        ui.separator();
        Self::slider(ui, "Master", &mut p.mixer.master_volume, 0.0..=1.25);
    }

    fn apply_moog_preset(p: &mut Project, id: u8) {
        p.moog.poly_mode = matches!(id, 4 | 5 | 6);
        match id {
            0 => {
                p.moog.cutoff = 300.0;
                p.moog.resonance = 0.62;
                p.moog.osc2_octave = -1;
                p.moog.env_amount = 0.82;
                p.moog.filt_decay = 0.36;
                p.moog.filt_sustain = 0.05;
                p.moog.amp_attack = 0.004;
                p.moog.amp_release = 0.14;
            }
            1 => {
                p.moog.cutoff = 180.0;
                p.moog.resonance = 0.38;
                p.moog.osc2_octave = -2;
                p.moog.osc2_level = 0.48;
                p.moog.env_amount = 0.45;
                p.moog.amp_attack = 0.008;
                p.moog.amp_release = 0.22;
            }
            2 => {
                p.moog.cutoff = 2100.0;
                p.moog.resonance = 0.32;
                p.moog.osc2_detune = 8.0;
                p.moog.env_amount = 0.5;
                p.moog.amp_attack = 0.006;
                p.moog.amp_release = 0.3;
            }
            3 => {
                p.moog.cutoff = 6800.0;
                p.moog.resonance = 0.24;
                p.moog.osc2_detune = 14.0;
                p.moog.env_amount = 0.34;
                p.moog.amp_attack = 0.012;
                p.moog.amp_release = 0.65;
                p.moog.lfo_to_pitch = 0.03;
            }
            4 => {
                p.moog.cutoff = 3600.0;
                p.moog.resonance = 0.2;
                p.moog.osc1_wave = crate::model::MoogWave::Saw;
                p.moog.osc2_detune = 5.0;
                p.moog.filt_attack = 0.06;
                p.moog.filt_release = 0.8;
                p.moog.amp_attack = 0.05;
                p.moog.amp_release = 0.9;
            }
            5 => {
                p.moog.osc1_wave = crate::model::MoogWave::Triangle;
                p.moog.cutoff = 2600.0;
                p.moog.resonance = 0.14;
                p.moog.filt_attack = 0.4;
                p.moog.filt_decay = 1.6;
                p.moog.filt_sustain = 0.68;
                p.moog.filt_release = 2.8;
                p.moog.amp_attack = 0.5;
                p.moog.amp_decay = 1.2;
                p.moog.amp_sustain = 0.82;
                p.moog.amp_release = 3.0;
                p.moog.lfo_to_filter = 0.15;
            }
            _ => {
                p.moog.osc1_wave = crate::model::MoogWave::Triangle;
                p.moog.osc2_wave = crate::model::MoogWave::WidePulse;
                p.moog.cutoff = 9800.0;
                p.moog.resonance = 0.18;
                p.moog.osc2_detune = 4.0;
                p.moog.filt_attack = 0.8;
                p.moog.filt_release = 2.5;
                p.moog.amp_attack = 0.8;
                p.moog.amp_release = 3.2;
                p.moog.lfo_to_filter = 0.22;
            }
        }
    }
}
impl eframe::App for GrooveboxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    egui::RichText::new("RETROBOX")
                        .color(egui::Color32::from_rgb(238, 122, 66))
                        .strong(),
                );
                let playing = self.engine.is_playing();
                if ui
                    .button(if playing { "■ STOP" } else { "▶ PLAY" })
                    .clicked()
                {
                    self.engine.set_playing(!playing);
                }
                if ui.button("New").clicked() {
                    *self
                        .engine
                        .project
                        .lock()
                        .unwrap_or_else(|e| e.into_inner()) = Project::default();
                }
                if ui.button("Open").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Retrobox project", &["json"])
                        .pick_file()
                    {
                        match load_project(&path) {
                            Ok(p) => {
                                *self
                                    .engine
                                    .project
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner()) = p
                            }
                            Err(e) => self.status = e,
                        }
                    }
                }
                if ui.button("Save").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Retrobox project", &["json"])
                        .set_file_name("groove.json")
                        .save_file()
                    {
                        let p = self
                            .engine
                            .project
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        if let Err(e) = save_project(&path, &p) {
                            self.status = e
                        } else {
                            self.status = format!("Saved {}", path.display())
                        }
                    }
                }
                ui.separator();
                let mut p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if ui
                    .button(if p.song_mode {
                        "SONG MODE"
                    } else {
                        "PATTERN MODE"
                    })
                    .clicked()
                {
                    p.song_mode = !p.song_mode;
                }
                ui.label("BPM");
                ui.add(
                    egui::DragValue::new(&mut p.bpm)
                        .range(40.0..=240.0)
                        .speed(0.5),
                );
                ui.label("Swing");
                ui.add(egui::Slider::new(&mut p.swing, 0.0..=0.45).show_value(false));
            });
        });
        egui::SidePanel::left("tracks")
            .resizable(false)
            .default_width(150.0)
            .show(ctx, |ui| {
                ui.label("INSTRUMENTS");
                ui.separator();
                for t in TrackId::ALL {
                    let selected = self.track == t;
                    if ui.selectable_label(selected, t.name()).clicked() {
                        self.track = t;
                    }
                }
                ui.add_space(16.0);
                ui.label("PATTERN BANK");
                let mut p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                for i in 0..p.patterns.len() {
                    let label = p.patterns[i].name.clone();
                    if ui.selectable_label(p.active_pattern == i, label).clicked() {
                        p.active_pattern = i;
                    }
                }
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (view, label) in [
                    (PageView::Sequencer, "SEQUENCER"),
                    (PageView::Song, "SONG"),
                    (PageView::Mixer, "MIXER"),
                    (PageView::Sound, "SOUND"),
                    (PageView::Effects, "EFFECTS"),
                    (PageView::Global, "GLOBAL"),
                ] {
                    if ui.selectable_label(self.page_view == view, label).clicked() {
                        self.page_view = view;
                    }
                }
            });
            ui.separator();
            if self.page_view == PageView::Song {
                let mut p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                Self::song_panel(ui, &mut p);
            } else if self.page_view == PageView::Mixer {
                let mut p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                Self::mixer_panel(ui, &mut p);
            } else if self.page_view == PageView::Global {
                let mut p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                Self::global_panel(ui, &mut p);
            } else if self.page_view == PageView::Sound {
                let mut p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                Self::sound_panel(ui, &mut p, self.track);
            } else if self.page_view == PageView::Effects {
                let mut p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                Self::effects_panel(ui, &mut p);
            } else {
                ui.horizontal(|ui| {
                    ui.heading(self.track.name());
                    ui.separator();
                    ui.label("STEP LENGTH");
                    let mut p = self
                        .engine
                        .project
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let old = match self.track {
                        TrackId::Drums => p.current_pattern().drums.step_count,
                        _ => Self::seq_mut(&mut p, self.track)
                            .map(|s| s.step_count)
                            .unwrap_or(StepCount::Steps16),
                    };
                    let mut n = old.as_usize();
                    egui::ComboBox::from_id_salt("length")
                        .selected_text(format!("{n} steps"))
                        .show_ui(ui, |ui| {
                            for count in StepCount::ALL {
                                ui.selectable_value(
                                    &mut n,
                                    count.as_usize(),
                                    format!("{} steps", count.label()),
                                );
                            }
                        });
                    let target = StepCount::from_usize(n);
                    if target != old {
                        if self.track == TrackId::Drums {
                            p.current_pattern_mut()
                                .drums
                                .set_all_step_counts(target, true)
                        } else if let Some(s) = Self::seq_mut(&mut p, self.track) {
                            s.expand_to(target)
                        }
                    }
                    let pages = target.pages();
                    ui.separator();
                    for pg in 0..pages {
                        if ui
                            .selectable_label(self.page == pg, format!("{}", pg + 1))
                            .clicked()
                        {
                            self.page = pg;
                        }
                    }
                });
                ui.add_space(22.0);
                ui.label("SEQUENCER");
                ui.add_space(6.0);
                let grid = egui::Grid::new("steps")
                    .spacing([5.0, 7.0])
                    .min_col_width(36.0);
                grid.show(ui, |ui| {
                    ui.label("");
                    for col in 0..16 {
                        ui.label((self.page * 16 + col + 1).to_string());
                    }
                    ui.end_row();
                    let row_count = if self.track == TrackId::Drums { 8 } else { 1 };
                    for row in 0..row_count {
                        ui.label(if self.track == TrackId::Drums {
                            crate::model::DrumVoiceId::ALL[row].short_name()
                        } else {
                            "NOTE"
                        });
                        for col in 0..16 {
                            let i = self.page * 16 + col;
                            let active = {
                                let p = self
                                    .engine
                                    .project
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                let pat = p.current_pattern();
                                match self.track {
                                    TrackId::Drums => pat.drums.lanes[row].steps[i].active,
                                    TrackId::MidiOut => pat.midi_out.steps[i].active,
                                    TrackId::Moog => pat.moog.steps[i].active,
                                    TrackId::MoogPad => pat.moog_pad.steps[i].active,
                                    TrackId::Monologue => pat.monologue.steps[i].active,
                                    TrackId::Tb303 => pat.tb303.steps[i].active,
                                    TrackId::Sampler => pat.sampler.steps[i].active,
                                }
                            };
                            let color = if active {
                                egui::Color32::from_rgb(235, 119, 62)
                            } else {
                                egui::Color32::from_gray(if col % 4 == 0 { 65 } else { 42 })
                            };
                            let b = egui::Button::new(if active { "●" } else { "·" })
                                .fill(color)
                                .min_size(egui::vec2(34.0, 34.0));
                            if ui.add(b).clicked() {
                                let index = i;
                                self.selected_step = index;
                                if self.track == TrackId::Drums {
                                    let mut p = self
                                        .engine
                                        .project
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner());
                                    let lane = &mut p.current_pattern_mut().drums.lanes[row];
                                    lane.steps[index].active = !lane.steps[index].active;
                                } else {
                                    self.toggle(index)
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(22.0);
                if self.track != TrackId::Drums && self.track != TrackId::MidiOut {
                    let mut p = self
                        .engine
                        .project
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let step =
                        &mut Self::seq_mut(&mut p, self.track).unwrap().steps[self.selected_step];
                    ui.horizontal(|ui| {
                        ui.label(format!("STEP {} NOTE", self.selected_step + 1));
                        ui.add(
                            egui::DragValue::new(&mut step.note)
                                .range(24..=108)
                                .speed(1),
                        );
                        ui.label("VELOCITY");
                        ui.add(egui::Slider::new(&mut step.velocity, 0.05..=1.0).show_value(false));
                        ui.label("GATE");
                        ui.add(egui::Slider::new(&mut step.gate, 0.05..=1.0).show_value(false));
                        if matches!(self.track, TrackId::Moog | TrackId::MoogPad) {
                            egui::ComboBox::from_id_salt("step_chord")
                                .selected_text(step.chord.name())
                                .show_ui(ui, |ui| {
                                    for chord in crate::model::ChordType::ALL {
                                        ui.selectable_value(&mut step.chord, chord, chord.name());
                                    }
                                });
                        }
                        ui.checkbox(&mut step.accent, "Accent");
                        ui.checkbox(&mut step.slide, "Slide");
                    });
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Clear").clicked() {
                        let mut p = self
                            .engine
                            .project
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        match self.track {
                            TrackId::Drums => p.current_pattern_mut().drums.clear(),
                            TrackId::MidiOut => p.current_pattern_mut().midi_out.clear(),
                            _ => {
                                if let Some(s) = Self::seq_mut(&mut p, self.track) {
                                    s.clear_active()
                                }
                            }
                        }
                    }
                    if ui.button("Random groove").clicked() {
                        let mut p = self
                            .engine
                            .project
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        if self.track == TrackId::Drums {
                            p.current_pattern_mut().drums.randomize_groove(0x51A7)
                        } else {
                            let root = p.root_note;
                            let scale = p.scale;
                            if let Some(s) = Self::seq_mut(&mut p, self.track) {
                                s.randomize(
                                    root,
                                    scale,
                                    3,
                                    0.55,
                                    self.track == TrackId::Tb303,
                                    0x51A7,
                                )
                            }
                        }
                    }
                });
            }
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let p = self
                    .engine
                    .project
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                ui.label(format!(
                    "{}  •  {} BPM  •  {}",
                    p.name,
                    p.bpm,
                    if self.engine.is_playing() {
                        "PLAYING"
                    } else {
                        "STOPPED"
                    }
                ));
                if let Some(e) = &self.engine.audio_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("Audio: {e}"));
                }
                ui.label(&self.status);
                ui.separator();
                ui.label("OUT");
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(260.0, 36.0), egui::Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 3.0, egui::Color32::from_gray(22));
                let samples = self.engine.scope.snapshot();
                let points = samples
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        egui::pos2(
                            rect.left() + i as f32 / (samples.len() - 1) as f32 * rect.width(),
                            rect.center().y - s.clamp(-1.0, 1.0) * rect.height() * 0.45,
                        )
                    })
                    .collect::<Vec<_>>();
                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(1.2_f32, egui::Color32::from_rgb(88, 220, 158)),
                ));
            });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}
