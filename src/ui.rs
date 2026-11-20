use crate::{
    dsp::sampler::{load_audio_to_mono_f32, BUILTIN_SAMPLE_NAMES},
    engine::{export_stems, export_wav, AudioEngine, LiveEvent},
    mic::MicRecorder,
    midi::{MidiManager, MidiMsg},
    model::{
        apply_extra_track_preset, apply_vibe_to_project, apply_voice_preset, build_auto_chord_pads,
        generate_auto_beat, generate_extra_track_seq, generate_melodic_tracks, load_pattern,
        load_project, note_name, root_name, save_pattern, save_project, voice_preset_names,
        AutoChordPad, ChordType, DelayDivision, DrumModel, DrumVoiceId, EgTarget, ExtraTrack,
        GlobalFilterMode, LfoMode, LfoTarget, LfoWave, LoopMode, MonologueOscWave, MoogWave,
        Osc2ModMode, Project, SamplerMode, SavedSample, Scale, SongSection, Tb303Wave, TrackId,
        Vibe, MAX_EXTRA_TRACKS, NOTE_NAMES, PROGRESSIONS,
    },
    settings::{
        restart_app, spawn_install_update, spawn_update_check, AppSettings, StudioTheme,
        UpdateStatus, CURRENT_VERSION, RELEASES_URL,
    },
};
use eframe::egui::{self, Color32, Key, RichText, Rounding, Sense, Stroke, Vec2};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StudioTab {
    Studio,
    Sampler,
    ArrangeParty,
    SoundLab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestRepeatMode {
    Quarter,
    Eighth,
    Drone,
}

pub struct FunKeySynthApp {
    engine: AudioEngine,
    tab: StudioTab,
    selected_track: TrackId,
    selected_extra: Option<usize>,
    selected_step: usize,
    bar_page: usize,
    follow_playhead: bool,
    octave_offset: i8,
    record_live: bool,
    held_pad_idx: Option<usize>,
    held_mouse_pad: Option<usize>,
    stutter_held: bool,
    scope_buf: Vec<f32>,
    roll_counter: u32,
    toast: Option<(String, f64)>,
    // Sampler visual waveform state (Volca 8-part)
    volca_ui: crate::volca_ui::VolcaUiState,
    app_icon: egui::TextureHandle,
    midi: MidiManager,
    mic: MicRecorder,
    rec_motion: bool,
    settings: AppSettings,
    show_settings: bool,
    update_status: Arc<Mutex<UpdateStatus>>,
    // Sound Lab / Sampler Auto-Repeat Test Note
    test_repeat_on: bool,
    test_repeat_mode: TestRepeatMode,
    test_repeat_next_t: f64,
    test_repeat_off_t: Option<f64>,
    test_repeat_step: usize,
    test_repeat_active: Option<(TrackId, u8, bool)>, // (track, note, is_chord)
}

impl FunKeySynthApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = AppSettings::load();
        settings.apply_to_egui(&cc.egui_ctx);
        let update_status = Arc::new(Mutex::new(UpdateStatus::Idle));
        if settings.check_updates_on_startup {
            spawn_update_check(update_status.clone());
        }
        let icon_img = egui::ColorImage::from_rgba_unmultiplied([64, 64], include_bytes!("icon_64.rgba"));
        let app_icon = cc.egui_ctx.load_texture("app_icon", icon_img, egui::TextureOptions::LINEAR);
        let project = Project::default();
        let engine = AudioEngine::start(project);
        Self {
            engine,
            tab: StudioTab::Studio,
            selected_track: TrackId::Drums,
            selected_extra: None,
            selected_step: 0,
            bar_page: 0,
            follow_playhead: true,
            octave_offset: 0,
            record_live: false,
            held_pad_idx: None,
            held_mouse_pad: None,
            stutter_held: false,
            scope_buf: Vec::with_capacity(256),
            roll_counter: 1,
            toast: None,
            volca_ui: crate::volca_ui::VolcaUiState::default(),
            app_icon,
            midi: MidiManager::default(),
            mic: MicRecorder::default(),
            rec_motion: false,
            settings,
            show_settings: false,
            update_status,
            test_repeat_on: false,
            test_repeat_mode: TestRepeatMode::Quarter,
            test_repeat_next_t: 0.0,
            test_repeat_off_t: None,
            test_repeat_step: 0,
            test_repeat_active: None,
        }
    }

    fn stop_test_repeat_note(&mut self) {
        if let Some((tr, note, is_chord)) = self.test_repeat_active.take() {
            if is_chord {
                self.engine.send_live(LiveEvent::ReleaseChord);
            } else {
                self.engine.send_live(LiveEvent::ReleaseNote { track: tr, note });
            }
        }
        self.test_repeat_off_t = None;
    }

    fn update_test_repeat(&mut self, ctx: &egui::Context, proj: &Project, playing: bool) {
        // Auto-pause test repeater if main transport starts playing
        if playing || !self.test_repeat_on {
            if self.test_repeat_active.is_some() {
                self.stop_test_repeat_note();
            }
            return;
        }
        let now = ctx.input(|i| i.time);
        let target = if self.tab == StudioTab::Sampler {
            TrackId::Sampler
        } else {
            self.selected_track
        };

        // If user switched target track while holding drone/note, release previous cleanly
        if let Some((prev_tr, _, _)) = self.test_repeat_active {
            if prev_tr != target {
                self.stop_test_repeat_note();
                self.test_repeat_next_t = now;
            }
        }

        if self.test_repeat_mode == TestRepeatMode::Drone && target != TrackId::Drums {
            if self.test_repeat_active.is_none() {
                let note = match target {
                    TrackId::Moog | TrackId::Tb303 => 36 + (proj.root_note % 12),
                    TrackId::Sampler => proj.sampler.root_note,
                    _ => 60 + (proj.root_note % 12),
                };
                if target == TrackId::MoogPad {
                    let pads = build_auto_chord_pads(proj.root_note, proj.scale);
                    if let Some(pad) = pads.first() {
                        self.engine.send_live(LiveEvent::PlayChord {
                            notes: pad.chord_notes.clone(),
                            bass: None,
                            velocity: 0.85,
                            slot: pad.slot,
                        });
                        self.test_repeat_active = Some((target, note, true));
                    }
                } else {
                    self.engine.send_live(LiveEvent::PlayNote {
                        track: target,
                        note,
                        velocity: 0.88,
                    });
                    self.test_repeat_active = Some((target, note, false));
                }
            }
            return;
        }

        // Release gate if due
        if let Some(off_t) = self.test_repeat_off_t {
            if now >= off_t {
                self.stop_test_repeat_note();
            }
        }

        // Trigger next repeating hit
        if now >= self.test_repeat_next_t {
            self.stop_test_repeat_note();
            let beat_secs = 60.0 / proj.bpm.clamp(40.0, 240.0) as f64;
            let interval = match self.test_repeat_mode {
                TestRepeatMode::Quarter | TestRepeatMode::Drone => beat_secs,
                TestRepeatMode::Eighth => beat_secs * 0.5,
            };
            self.test_repeat_next_t = now + interval;
            let step = self.test_repeat_step;
            self.test_repeat_step = self.test_repeat_step.wrapping_add(1);

            match target {
                TrackId::Drums => {
                    // Cycle Kick -> ClosedHat -> Snare -> ClosedHat so you hear kit tuning live
                    let lane = [0u8, 4, 1, 4][step % 4];
                    self.engine.send_live(LiveEvent::PlayNote {
                        track: TrackId::Drums,
                        note: lane,
                        velocity: 0.90,
                    });
                }
                TrackId::MoogPad => {
                    let pads = build_auto_chord_pads(proj.root_note, proj.scale);
                    if let Some(pad) = pads.get((step / 2) % 4) {
                        self.engine.send_live(LiveEvent::PlayChord {
                            notes: pad.chord_notes.clone(),
                            bass: None,
                            velocity: 0.84,
                            slot: pad.slot,
                        });
                        self.test_repeat_active = Some((target, 60, true));
                        self.test_repeat_off_t = Some(now + interval * 0.82);
                    }
                }
                TrackId::Sampler => {
                    if self.tab == StudioTab::Sampler {
                        let p_idx = self.volca_ui.selected_part;
                        self.engine.send_live(LiveEvent::TriggerSamplePart {
                            part: p_idx,
                            velocity: 0.92,
                        });
                    } else {
                        let note = proj.sampler.root_note;
                        self.engine.send_live(LiveEvent::PlayNote {
                            track: TrackId::Sampler,
                            note,
                            velocity: 0.90,
                        });
                        self.test_repeat_active = Some((target, note, false));
                        self.test_repeat_off_t = Some(now + interval * 0.78);
                    }
                }
                _ => {
                    // Musical 4-note test phrase: Root, 5th, Octave, 7th/Root
                    let base = match target {
                        TrackId::Moog | TrackId::Tb303 => 36 + (proj.root_note % 12),
                        _ => 60 + (proj.root_note % 12),
                    };
                    let offset = [0u8, 7, 12, 3][step % 4];
                    let note = (base + offset).min(96);
                    self.engine.send_live(LiveEvent::PlayNote {
                        track: target,
                        note,
                        velocity: 0.88,
                    });
                    self.test_repeat_active = Some((target, note, false));
                    self.test_repeat_off_t = Some(now + interval * 0.65);
                }
            }
        }
    }

    fn set_toast(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time) + 3.2));
    }

    fn next_seed(&mut self) -> u32 {
        self.roll_counter = self.roll_counter.wrapping_add(1);
        0x51A7_1337u32.wrapping_mul(self.roll_counter).wrapping_add(0x9E37_79B9)
    }

    fn handle_midi(
        &mut self,
        p: &mut Project,
        pads: &[AutoChordPad],
        playing: bool,
        cur_step: usize,
    ) {
        let msgs = self.midi.drain();
        for msg in msgs {
            match msg {
                MidiMsg::NoteOn { note, velocity } => {
                    if self.tab == StudioTab::Sampler && (36..44).contains(&note) {
                        let part = (note - 36) as usize;
                        self.volca_ui.selected_part = part;
                        p.mixer.channels[TrackId::Sampler.index()].mute = false;
                        self.engine.send_live(LiveEvent::TriggerSamplePart { part, velocity });
                        if self.record_live && playing {
                            let lane = &mut p.current_pattern_mut().sample_lanes[part];
                            let s_idx = cur_step % lane.step_count.as_usize();
                            lane.steps[s_idx].active = true;
                            lane.steps[s_idx].velocity = velocity;
                        }
                    } else if p.auto_chord_mode {
                        let idx = (note as usize) % pads.len().max(1);
                        self.trigger_pad_down(idx, p, pads, playing, cur_step);
                    } else {
                        let target = if self.selected_track == TrackId::Drums {
                            TrackId::Monologue
                        } else {
                            self.selected_track
                        };
                        self.engine.send_live(LiveEvent::PlayNote {
                            track: target,
                            note,
                            velocity,
                        });
                        if self.record_live && playing {
                            if let Some(seq) = p.current_pattern_mut().melodic_mut(target) {
                                let s_idx = cur_step % seq.step_count.as_usize();
                                let st = &mut seq.steps[s_idx];
                                st.active = true;
                                st.note = note;
                                st.velocity = velocity;
                                st.gate = 1.5;
                            }
                        }
                    }
                }
                MidiMsg::NoteOff { note } => {
                    if self.tab == StudioTab::Sampler && (36..44).contains(&note) {
                        let part = (note - 36) as usize;
                        if p.sample_parts[part].loop_mode != LoopMode::OneShot {
                            self.engine.send_live(LiveEvent::ReleaseSamplePart { part });
                        }
                    } else if p.auto_chord_mode {
                        self.engine.send_live(LiveEvent::ReleaseChord);
                    } else {
                        let target = if self.selected_track == TrackId::Drums {
                            TrackId::Monologue
                        } else {
                            self.selected_track
                        };
                        self.engine.send_live(LiveEvent::ReleaseNote { track: target, note });
                    }
                }
                MidiMsg::Cc { cc, value } => match cc {
                    1 | 74 => {
                        p.fx.global_filter_cutoff = 120.0 * (18_000.0_f32 / 120.0).powf(value);
                        if self.rec_motion && playing {
                            p.current_pattern_mut().motion.filter[cur_step % 64] = Some(value);
                        }
                    }
                    71 => {
                        p.fx.global_filter_resonance = value * 0.85;
                        p.tb303.resonance = (value * 0.92).clamp(0.1, 0.94);
                    }
                    91 => {
                        p.fx.reverb_mix = value;
                        p.fx.delay_mix = (value * 0.92).min(0.85);
                        if self.rec_motion && playing {
                            p.current_pattern_mut().motion.space[cur_step % 64] = Some(value);
                        }
                    }
                    7 => {
                        p.mixer.master_volume = (value * 1.15).clamp(0.0, 1.15);
                    }
                    16 => {
                        p.sampler_iso_bass = value * 2.0;
                        if self.rec_motion && playing {
                            p.current_pattern_mut().motion.iso_bass[cur_step % 64] = Some(value * 2.0);
                        }
                    }
                    17 => {
                        p.sampler_iso_treble = value * 2.0;
                        if self.rec_motion && playing {
                            p.current_pattern_mut().motion.iso_treble[cur_step % 64] = Some(value * 2.0);
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    fn handle_keyboard(
        &mut self,
        ctx: &egui::Context,
        p: &mut Project,
        pads: &[AutoChordPad],
        playing: bool,
        cur_step: usize,
    ) {
        if ctx.wants_keyboard_input() {
            return;
        }

        // Spacebar: Play / Stop
        if ctx.input(|i| i.key_pressed(Key::Space)) {
            self.engine.playing.store(!playing, Ordering::Relaxed);
        }

        // Octave shift for Scale Lead mode
        if ctx.input(|i| i.key_pressed(Key::OpenBracket)) {
            self.octave_offset = (self.octave_offset - 1).clamp(-2, 2);
        }
        if ctx.input(|i| i.key_pressed(Key::CloseBracket)) {
            self.octave_offset = (self.octave_offset + 1).clamp(-2, 2);
        }

        const PAD_KEYS: [Key; 8] = [
            Key::A,
            Key::S,
            Key::D,
            Key::F,
            Key::G,
            Key::H,
            Key::J,
            Key::K,
        ];

        for (idx, &key) in PAD_KEYS.iter().enumerate() {
            if ctx.input(|i| i.key_pressed(key)) {
                self.trigger_pad_down(idx, p, pads, playing, cur_step);
                self.held_pad_idx = Some(idx);
            }
            if ctx.input(|i| i.key_released(key)) && self.held_pad_idx == Some(idx) {
                self.trigger_pad_up(idx, p);
                self.held_pad_idx = None;
            }
        }
    }

    fn scale_lead_note(&self, p: &Project, idx: usize) -> u8 {
        let base = (60 + self.octave_offset as i32 * 12).clamp(36, 84) as u8;
        let notes = p.scale.notes_in_range(p.root_note, base, (base + 24).min(104));
        notes.get(idx % notes.len().max(1)).copied().unwrap_or(60)
    }

    fn trigger_pad_down(
        &mut self,
        idx: usize,
        p: &mut Project,
        pads: &[AutoChordPad],
        playing: bool,
        cur_step: usize,
    ) {
        if p.auto_chord_mode {
            if let Some(pad) = pads.get(idx) {
                self.engine.send_live(LiveEvent::PlayChord {
                    notes: pad.chord_notes.clone(),
                    bass: Some(pad.bass_midi),
                    velocity: 0.86,
                    slot: pad.slot,
                });
                if self.record_live && playing {
                    let pat = p.current_pattern_mut();
                    let s_idx = cur_step % pat.moog_pad.step_count.as_usize();
                    let st = &mut pat.moog_pad.steps[s_idx];
                    st.active = true;
                    st.note = pad.chord_notes.first().copied().unwrap_or(60);
                    st.chord = pad.slot.chord;
                    st.velocity = 0.82;
                    st.gate = 8.0;
                    let b_idx = cur_step % pat.moog.step_count.as_usize();
                    let bst = &mut pat.moog.steps[b_idx];
                    bst.active = true;
                    bst.note = pad.bass_midi;
                    bst.velocity = 0.88;
                    bst.gate = 4.0;
                }
            }
        } else {
            let note = self.scale_lead_note(p, idx);
            let target = if self.selected_track == TrackId::Drums {
                TrackId::Monologue
            } else {
                self.selected_track
            };
            self.engine.send_live(LiveEvent::PlayNote {
                track: target,
                note,
                velocity: 0.88,
            });
            if self.record_live && playing {
                let pat = p.current_pattern_mut();
                if let Some(seq) = pat.melodic_mut(target) {
                    let s_idx = cur_step % seq.step_count.as_usize();
                    let st = &mut seq.steps[s_idx];
                    st.active = true;
                    st.note = note;
                    st.velocity = 0.86;
                    st.gate = 1.5;
                }
            }
        }
    }

    fn trigger_pad_up(&mut self, idx: usize, p: &Project) {
        if p.auto_chord_mode {
            self.engine.send_live(LiveEvent::ReleaseChord);
        } else {
            let note = self.scale_lead_note(p, idx);
            let target = if self.selected_track == TrackId::Drums {
                TrackId::Monologue
            } else {
                self.selected_track
            };
            self.engine.send_live(LiveEvent::ReleaseNote {
                track: target,
                note,
            });
        }
    }
}

impl eframe::App for FunKeySynthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        let playing = self.engine.playing.load(Ordering::Relaxed);
        let cur_step = self.engine.current_step.load(Ordering::Relaxed);
        let telemetry = self.engine.telemetry();
        self.engine.scope.snapshot(&mut self.scope_buf);

        // Lock project once for the frame
        let mut proj = match self.engine.project.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        let pads = build_auto_chord_pads(proj.root_note, proj.scale);
        let accent = if self.settings.use_vibe_accent {
            vibe_color(proj.vibe)
        } else {
            let [r, g, b] = self.settings.custom_accent;
            Color32::from_rgb(r, g, b)
        };
        let theme = self.settings.theme;

        self.handle_keyboard(ctx, &mut proj, &pads, playing, cur_step);
        self.handle_midi(&mut proj, &pads, playing, cur_step);
        self.update_test_repeat(ctx, &proj, playing);

        if playing && self.follow_playhead {
            self.bar_page = (cur_step / 16) % 4;
        }

        // ==================== TOP VIBE BAR ====================
        egui::TopBottomPanel::top("top_vibe_bar")
            .frame(
                egui::Frame::none()
                    .fill(theme.bar_bg())
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                    .stroke(Stroke::new(1.0_f32, theme.border())),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // Top-left Bear Paw on Keys App Icon + Brand Title
                    ui.add(
                        egui::Image::new(&self.app_icon)
                            .fit_to_exact_size(Vec2::new(28.0, 28.0))
                            .rounding(Rounding::same(5.0)),
                    );
                    ui.label(
                        RichText::new("FUN KEY SYNTH")
                            .strong()
                            .size(15.0)
                            .color(accent),
                    );
                    ui.separator();

                    // Play / Stop (Space)
                    let play_btn = egui::Button::new(
                        RichText::new(if playing { "■ STOP" } else { "▶ PLAY" })
                            .strong()
                            .size(13.5)
                            .color(if playing {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(14, 18, 24)
                            }),
                    )
                    .fill(if playing {
                        Color32::from_rgb(226, 72, 88)
                    } else {
                        Color32::from_rgb(72, 224, 148)
                    })
                    .min_size(Vec2::new(82.0, 28.0));
                    if ui
                        .add(play_btn)
                        .on_hover_text("Play / Stop (Spacebar)")
                        .clicked()
                    {
                        self.engine.playing.store(!playing, Ordering::Relaxed);
                    }

                    // BPM & Swing
                    ui.add(
                        egui::DragValue::new(&mut proj.bpm)
                            .range(60.0..=180.0)
                            .speed(0.5)
                            .suffix(" BPM"),
                    );
                    let mut swing_pct = 50.0 + proj.swing * 50.0;
                    if ui
                        .add(
                            egui::DragValue::new(&mut swing_pct)
                                .range(50.0..=72.0)
                                .speed(0.3)
                                .prefix("Swing ")
                                .suffix("%"),
                        )
                        .changed()
                    {
                        proj.swing = ((swing_pct - 50.0) / 50.0).clamp(0.0, 0.45);
                    }

                    // Key Selector
                    egui::ComboBox::from_id_salt("key_root")
                        .width(52.0)
                        .selected_text(format!("Key {}", root_name(proj.root_note)))
                        .show_ui(ui, |ui| {
                            for (i, &n) in NOTE_NAMES.iter().enumerate() {
                                if ui
                                    .selectable_value(&mut proj.root_note, i as u8, n)
                                    .changed()
                                {
                                    let seed = self.next_seed();
                                    let (bass, pad, arp, acid, samp) = generate_melodic_tracks(
                                        proj.vibe,
                                        proj.root_note,
                                        proj.progression_idx,
                                        seed,
                                    );
                                    let pat = proj.current_pattern_mut();
                                    pat.moog = bass;
                                    pat.moog_pad = pad;
                                    pat.monologue = arp;
                                    pat.tb303 = acid;
                                    pat.sampler = samp;
                                }
                            }
                        });

                    // Scale Picker (12 musical scales — updates Auto-Chords & keyboard quantization!)
                    egui::ComboBox::from_id_salt("scale_picker")
                        .width(106.0)
                        .selected_text(proj.scale.name())
                        .show_ui(ui, |ui| {
                            for &sc in &Scale::ALL {
                                ui.selectable_value(&mut proj.scale, sc, sc.name());
                            }
                        });

                    // Mode Toggles: Auto-Beat, Auto-Chord, Party Mode
                    toggle_pill(
                        ui,
                        "AUTO-BEAT",
                        &mut proj.auto_beat_mode,
                        Color32::from_rgb(255, 168, 52),
                        "Automatically plays groovy drums matched to your Vibe & Beat Energy",
                    );
                    toggle_pill(
                        ui,
                        "AUTO-CHORD",
                        &mut proj.auto_chord_mode,
                        Color32::from_rgb(92, 198, 255),
                        "1-key lush voice-led chords on keys A-K",
                    );
                    if toggle_pill(
                        ui,
                        if proj.party_mode { "PARTY ON" } else { "PARTY MODE" },
                        &mut proj.party_mode,
                        Color32::from_rgb(244, 92, 186),
                        "Generates and evolves full tracks on the fly for your chosen Vibe!",
                    ) && proj.party_mode
                        && !playing
                    {
                        self.engine.playing.store(true, Ordering::Relaxed);
                    }

                    // Live Chord Readout Badge
                    let chord_text = if telemetry.active_chord.is_empty() {
                        "Ready".to_string()
                    } else if !telemetry.party_section.is_empty() {
                        format!(
                            "{}  •  {} {}/{}",
                            telemetry.active_chord,
                            telemetry.party_section,
                            telemetry.party_bar,
                            telemetry.party_section_bars
                        )
                    } else {
                        format!("Chord: {}", telemetry.active_chord)
                    };
                    egui::Frame::none()
                        .fill(Color32::from_rgb(26, 32, 46))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(egui::Margin::symmetric(10.0, 4.0))
                        .stroke(Stroke::new(1.0_f32, accent.gamma_multiply(0.6)))
                        .show(ui, |ui| {
                            ui.label(RichText::new(chord_text).strong().size(12.5).color(accent));
                        });

                    // Right-aligned Views + MIDI + Prefs + Save/Load/Export
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Prefs / Theme & Update Check button
                        if ui
                            .button("Prefs")
                            .on_hover_text("Color themes, custom accent & GitHub update check settings")
                            .clicked()
                        {
                            self.show_settings = !self.show_settings;
                        }

                        // Non-intrusive 1-click Update / Install / Restart badge
                        let upd_snapshot = self
                            .update_status
                            .lock()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        match upd_snapshot {
                            UpdateStatus::UpdateAvailable {
                                latest_tag,
                                url,
                                asset_url,
                                asset_name,
                            } => {
                                let label = if asset_url.is_some() {
                                    format!("Install {latest_tag}")
                                } else {
                                    format!("Update {latest_tag}")
                                };
                                let hover = if asset_url.is_some() {
                                    "Click to download and install this update automatically"
                                } else {
                                    "New release available on GitHub — click to open Releases"
                                };
                                let ubtn = egui::Button::new(
                                    RichText::new(label)
                                        .strong()
                                        .size(11.5)
                                        .color(Color32::from_rgb(12, 14, 20)),
                                )
                                .fill(Color32::from_rgb(72, 224, 148));
                                if ui.add(ubtn).on_hover_text(hover).clicked() {
                                    if let (Some(a_url), Some(a_name)) = (asset_url, asset_name) {
                                        spawn_install_update(
                                            self.update_status.clone(),
                                            latest_tag,
                                            a_url,
                                            a_name,
                                        );
                                    } else {
                                        ctx.open_url(egui::OpenUrl::new_tab(url));
                                    }
                                }
                            }
                            UpdateStatus::Installing { latest_tag, stage } => {
                                let ubtn = egui::Button::new(
                                    RichText::new(format!("Installing {latest_tag}..."))
                                        .strong()
                                        .size(11.5)
                                        .color(Color32::from_rgb(12, 14, 20)),
                                )
                                .fill(Color32::from_rgb(255, 198, 66));
                                let _ = ui.add_enabled(false, ubtn).on_disabled_hover_text(stage);
                            }
                            UpdateStatus::Installed { latest_tag } => {
                                let ubtn = egui::Button::new(
                                    RichText::new(format!("Restart for {latest_tag}"))
                                        .strong()
                                        .size(11.5)
                                        .color(Color32::from_rgb(12, 14, 20)),
                                )
                                .fill(Color32::from_rgb(82, 240, 168));
                                if ui
                                    .add(ubtn)
                                    .on_hover_text("Update installed! Click to relaunch into the new version")
                                    .clicked()
                                {
                                    restart_app();
                                }
                            }
                            _ => {}
                        }

                        if ui
                            .button("Stems")
                            .on_hover_text("Export Master + 6 separate stereo WAV stems for DAW mixing")
                            .clicked()
                        {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                let bars = if proj.song_mode {
                                    proj.song_sections.iter().map(|s| s.bars.max(1) as usize).sum::<usize>().clamp(4, 64)
                                } else {
                                    16
                                };
                                match export_stems(&proj, &dir, bars) {
                                    Ok(n) => self.set_toast(ctx, format!("Exported {n} stereo WAV stems!")),
                                    Err(e) => self.set_toast(ctx, format!("Stem export error: {e}")),
                                }
                            }
                        }
                        if ui.button("WAV").on_hover_text("Export stereo mix WAV").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name(format!("{}.wav", proj.vibe.name().to_lowercase().replace(' ', "_")))
                                .add_filter("WAV Audio", &["wav"])
                                .save_file()
                            {
                                let bars = if proj.song_mode {
                                    proj.song_sections.iter().map(|s| s.bars.max(1) as usize).sum::<usize>().clamp(4, 64)
                                } else {
                                    16
                                };
                                match export_wav(&proj, &path, bars) {
                                    Ok(()) => self.set_toast(ctx, format!("Exported {bars} bars WAV!")),
                                    Err(e) => self.set_toast(ctx, format!("Export error: {e}")),
                                }
                            }
                        }
                        if ui.button("Load").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Fun Key Project", &["json"])
                                .pick_file()
                            {
                                match load_project(&path) {
                                    Ok(mut loaded) => {
                                        loaded.sample_generation = proj.sample_generation.wrapping_add(1);
                                        for (i, sp) in loaded.sample_parts.iter_mut().enumerate() {
                                            sp.generation = proj
                                                .sample_parts
                                                .get(i)
                                                .map(|old| old.generation.wrapping_add(1))
                                                .unwrap_or(1);
                                        }
                                        proj = loaded;
                                        self.set_toast(ctx, "Project loaded (all patterns, synths & samples)!");
                                    }
                                    Err(e) => self.set_toast(ctx, format!("Load error: {e}")),
                                }
                            }
                        }
                        if ui.button("Save").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("my_vibe_track.json")
                                .add_filter("Fun Key Project", &["json"])
                                .save_file()
                            {
                                match save_project(&path, &proj) {
                                    Ok(()) => self.set_toast(ctx, "Project saved!"),
                                    Err(e) => self.set_toast(ctx, format!("Save error: {e}")),
                                }
                            }
                        }

                        // Compact USB MIDI Input Picker
                        let midi_lbl = if let Some(ref name) = self.midi.connected_name {
                            format!("MIDI: {}", name.chars().take(10).collect::<String>())
                        } else {
                            "MIDI".to_string()
                        };
                        egui::ComboBox::from_id_salt("midi_port_combo")
                            .width(84.0)
                            .selected_text(midi_lbl)
                            .show_ui(ui, |ui| {
                                if ui.button("Rescan MIDI Devices").clicked() {
                                    self.midi.refresh_ports();
                                }
                                if ui.selectable_label(self.midi.connected_name.is_none(), "Off").clicked() {
                                    self.midi.disconnect();
                                }
                                let ports = self.midi.ports.clone();
                                for (i, pname) in ports.iter().enumerate() {
                                    let is_conn = self.midi.connected_name.as_deref() == Some(pname.as_str());
                                    if ui.selectable_label(is_conn, pname).clicked() {
                                        match self.midi.connect(i) {
                                            Ok(()) => self.set_toast(ctx, format!("Connected MIDI: {pname}")),
                                            Err(e) => self.set_toast(ctx, e),
                                        }
                                    }
                                }
                            });

                        ui.separator();

                        for &(tab, label) in &[
                            (StudioTab::SoundLab, "SOUND LAB"),
                            (StudioTab::ArrangeParty, "ARRANGE & PARTY"),
                            (StudioTab::Sampler, "SAMPLER"),
                            (StudioTab::Studio, "STUDIO"),
                        ] {
                            let active = self.tab == tab;
                            let btn = egui::Button::new(
                                RichText::new(label)
                                    .strong()
                                    .size(12.0)
                                    .color(if active { Color32::WHITE } else { Color32::from_rgb(160, 170, 192) }),
                            )
                            .fill(if active {
                                Color32::from_rgb(46, 56, 78)
                            } else {
                                Color32::TRANSPARENT
                            });
                            if ui.add(btn).clicked() {
                                self.tab = tab;
                            }
                        }
                    });
                });
            });

        // ==================== BOTTOM DOCK: PLAYABLE AUTO-CHORD / SCALE PADS ====================
        egui::TopBottomPanel::bottom("playable_pads_dock")
            .frame(
                egui::Frame::none()
                    .fill(theme.bar_bg())
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                    .stroke(Stroke::new(1.0_f32, theme.border())),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(175.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(if proj.auto_chord_mode {
                                    "AUTO-CHORD PADS"
                                } else {
                                    "SCALE-LOCKED KEYS"
                                })
                                .strong()
                                .size(12.0)
                                .color(accent),
                            );
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(proj.auto_chord_mode, "Chords")
                                .clicked()
                            {
                                proj.auto_chord_mode = true;
                            }
                            if ui
                                .selectable_label(!proj.auto_chord_mode, "Lead")
                                .clicked()
                            {
                                proj.auto_chord_mode = false;
                            }
                            ui.checkbox(&mut self.record_live, "Rec")
                                .on_hover_text("Record pads into pattern while playing");
                        });
                        ui.horizontal(|ui| {
                            if proj.auto_chord_mode {
                                ui.checkbox(&mut proj.auto_chord_follow, "Band Follow")
                                    .on_hover_text("Bass & Arp follow the chord pad you hold!");
                            } else {
                                ui.label(RichText::new(format!("Octave {:+}", self.octave_offset)).size(11.0).weak());
                                if ui.small_button("-").clicked() {
                                    self.octave_offset = (self.octave_offset - 1).clamp(-2, 2);
                                }
                                if ui.small_button("+").clicked() {
                                    self.octave_offset = (self.octave_offset + 1).clamp(-2, 2);
                                }
                            }
                        });
                    });

                    // 8 Large Playable Pads (Keys A S D F G H J K)
                    let avail = (ui.available_width() - 180.0).max(320.0);
                    let pad_w = ((avail - 7.0 * 6.0) / 8.0).clamp(52.0, 128.0);
                    let pad_h = 56.0;
                    for (idx, pad) in pads.iter().enumerate() {
                        let is_held =
                            self.held_pad_idx == Some(idx) || self.held_mouse_pad == Some(idx);
                        let main_label = if proj.auto_chord_mode {
                            pad.chord_name.clone()
                        } else {
                            note_name(self.scale_lead_note(&proj, idx))
                        };
                        let bg = if is_held {
                            accent
                        } else {
                            Color32::from_rgb(26, 32, 46)
                        };
                        let fg = if is_held {
                            Color32::from_rgb(12, 15, 22)
                        } else {
                            Color32::from_rgb(232, 238, 250)
                        };
                        let (rect, resp) =
                            ui.allocate_exact_size(Vec2::new(pad_w, pad_h), Sense::click_and_drag());
                        if resp.is_pointer_button_down_on() && self.held_mouse_pad != Some(idx) {
                            self.trigger_pad_down(idx, &mut proj, &pads, playing, cur_step);
                            self.held_mouse_pad = Some(idx);
                        } else if !resp.is_pointer_button_down_on() && self.held_mouse_pad == Some(idx)
                        {
                            self.trigger_pad_up(idx, &proj);
                            self.held_mouse_pad = None;
                        }
                        let painter = ui.painter();
                        painter.rect_filled(rect, Rounding::same(8.0), bg);
                        painter.rect_stroke(
                            rect,
                            Rounding::same(8.0),
                            Stroke::new(
                                if is_held { 2.0_f32 } else { 1.0_f32 },
                                if is_held { Color32::WHITE } else { accent.gamma_multiply(0.45) },
                            ),
                        );
                        painter.text(
                            rect.left_top() + Vec2::new(7.0, 5.0),
                            egui::Align2::LEFT_TOP,
                            pad.key_label,
                            egui::FontId::proportional(10.5),
                            if is_held {
                                Color32::from_rgb(20, 24, 32)
                            } else {
                                Color32::from_rgb(130, 145, 175)
                            },
                        );
                        painter.text(
                            rect.center() + Vec2::new(0.0, 5.0),
                            egui::Align2::CENTER_CENTER,
                            main_label,
                            egui::FontId::proportional(14.5),
                            fg,
                        );
                    }

                    // Mini Oscilloscope + Live Stereo VU & Sidechain Meter + Status
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let scope_rect = ui.allocate_exact_size(Vec2::new(124.0, 42.0), Sense::hover()).0;
                            draw_scope(ui.painter(), scope_rect, &self.scope_buf, accent);
                            let meter_rect = ui.allocate_exact_size(Vec2::new(34.0, 42.0), Sense::hover()).0;
                            draw_vu_meters(
                                ui.painter(),
                                meter_rect,
                                telemetry.peak_l,
                                telemetry.peak_r,
                                telemetry.sidechain_duck,
                            );
                        });
                        let status_text = if let Some((ref msg, until)) = self.toast {
                            if ctx.input(|i| i.time) < until {
                                msg.as_str()
                            } else {
                                self.engine.status.as_str()
                            }
                        } else {
                            self.engine.status.as_str()
                        };
                        ui.label(RichText::new(status_text).size(10.5).weak());
                    });
                });
            });

        // ==================== MAIN CENTRAL VIEW ====================
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(theme.panel_bg())
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    StudioTab::Studio => self.ui_studio(ui, ctx, &mut proj, playing, cur_step, accent),
                    StudioTab::Sampler => {
                        if let Some(msg) = crate::volca_ui::render_volca_sampler(
                            ui,
                            ctx,
                            &mut self.volca_ui,
                            &self.engine,
                            &mut self.mic,
                            &mut proj,
                            &telemetry,
                            playing,
                            cur_step,
                            &mut self.bar_page,
                            &mut self.follow_playhead,
                            self.record_live,
                            &mut self.rec_motion,
                            &mut self.test_repeat_on,
                            &mut self.test_repeat_next_t,
                            accent,
                        ) {
                            self.set_toast(ctx, msg);
                        }
                    }
                    StudioTab::ArrangeParty => {
                        self.ui_arrange_party(ui, &mut proj, &telemetry, playing, accent)
                    }
                    StudioTab::SoundLab => self.ui_sound_lab(ui, ctx, &mut proj, accent),
                });
            });

        // Settings & Color Theme Window
        if self.show_settings {
            let mut open = self.show_settings;
            let mut changed = false;
            egui::Window::new("Preferences — Theme & Updates")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .default_width(360.0)
                .show(ctx, |ui| {
                    ui.label(RichText::new("COLOR THEME").strong().size(12.5).color(accent));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for &th in &StudioTheme::ALL {
                            let active = self.settings.theme == th;
                            let [ar, ag, ab] = th.default_accent();
                            let btn = egui::Button::new(
                                RichText::new(th.name())
                                    .strong()
                                    .size(11.5)
                                    .color(if active { Color32::from_rgb(12, 14, 20) } else { Color32::WHITE }),
                            )
                            .fill(if active {
                                Color32::from_rgb(ar, ag, ab)
                            } else {
                                Color32::from_rgb(32, 38, 52)
                            });
                            if ui.add(btn).clicked() {
                                self.settings.theme = th;
                                self.settings.custom_accent = th.default_accent();
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui
                            .checkbox(&mut self.settings.use_vibe_accent, "Dynamic Vibe Accent Color")
                            .changed()
                        {
                            changed = true;
                        }
                        if !self.settings.use_vibe_accent {
                            ui.label("Custom:");
                            if ui
                                .color_edit_button_srgb(&mut self.settings.custom_accent)
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });

                    ui.separator();
                    ui.label(RichText::new("GITHUB UPDATES").strong().size(12.5).color(accent));
                    ui.add_space(4.0);
                    if ui
                        .checkbox(
                            &mut self.settings.check_updates_on_startup,
                            "Check GitHub for updates on startup",
                        )
                        .changed()
                    {
                        changed = true;
                    }

                    let upd = self
                        .update_status
                        .lock()
                        .map(|g| g.clone())
                        .unwrap_or_default();
                    ui.horizontal(|ui| {
                        ui.label(format!("Installed: v{CURRENT_VERSION}"));
                        if ui.button("Check Now").clicked() {
                            spawn_update_check(self.update_status.clone());
                        }
                        if ui.button("Releases Page").clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(RELEASES_URL));
                        }
                    });
                    match upd {
                        UpdateStatus::Idle => {
                            ui.label(RichText::new("Update check idle.").weak());
                        }
                        UpdateStatus::Checking => {
                            ui.label(RichText::new("Checking GitHub releases...").color(accent));
                        }
                        UpdateStatus::UpToDate => {
                            ui.label(
                                RichText::new("You are on the latest version!")
                                    .color(Color32::from_rgb(72, 224, 148)),
                            );
                        }
                        UpdateStatus::UpdateAvailable {
                            latest_tag,
                            url,
                            asset_url,
                            asset_name,
                        } => {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("New version available: {latest_tag}"))
                                        .strong()
                                        .color(Color32::from_rgb(72, 224, 148)),
                                );
                                if let (Some(a_url), Some(a_name)) = (asset_url, asset_name) {
                                    let ibtn = egui::Button::new(
                                        RichText::new("Install Now")
                                            .strong()
                                            .color(Color32::from_rgb(12, 14, 20)),
                                    )
                                    .fill(Color32::from_rgb(72, 224, 148));
                                    if ui.add(ibtn).clicked() {
                                        spawn_install_update(
                                            self.update_status.clone(),
                                            latest_tag,
                                            a_url,
                                            a_name,
                                        );
                                    }
                                }
                                if ui.button("GitHub").clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(url));
                                }
                            });
                        }
                        UpdateStatus::Installing { latest_tag, stage } => {
                            ui.label(
                                RichText::new(format!("Installing {latest_tag}: {stage}"))
                                    .strong()
                                    .color(Color32::from_rgb(255, 198, 66)),
                            );
                        }
                        UpdateStatus::Installed { latest_tag } => {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("Installed {latest_tag}!"))
                                        .strong()
                                        .color(Color32::from_rgb(82, 240, 168)),
                                );
                                let rbtn = egui::Button::new(
                                    RichText::new("Restart Now")
                                        .strong()
                                        .color(Color32::from_rgb(12, 14, 20)),
                                )
                                .fill(Color32::from_rgb(82, 240, 168));
                                if ui.add(rbtn).clicked() {
                                    restart_app();
                                }
                            });
                        }
                        UpdateStatus::OfflineOrError(msg) => {
                            ui.label(RichText::new(format!("Status: {msg}")).weak());
                        }
                    }
                });
            self.show_settings = open;
            if changed {
                self.settings.apply_to_egui(ctx);
                self.settings.save();
            }
        }

        // Commit project edits back to audio thread
        if let Ok(mut guard) = self.engine.project.lock() {
            *guard = proj;
        }
    }
}

impl FunKeySynthApp {
    fn ui_studio(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        proj: &mut Project,
        playing: bool,
        cur_step: usize,
        accent: Color32,
    ) {
        // 1. VIBE PICKER STRIP
        card_frame(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("VIBE:").strong().size(12.5).color(Color32::from_rgb(175, 185, 208)));
                for &vibe in &Vibe::ALL {
                    let active = proj.vibe == vibe;
                    let vc = vibe_color(vibe);
                    let btn = egui::Button::new(
                        RichText::new(vibe.name())
                            .strong()
                            .size(12.5)
                            .color(if active { Color32::from_rgb(12, 14, 20) } else { Color32::from_rgb(215, 222, 238) }),
                    )
                    .fill(if active { vc } else { Color32::from_rgb(26, 31, 44) })
                    .stroke(Stroke::new(1.0_f32, if active { vc } else { Color32::from_rgb(44, 52, 72) }));
                    if ui.add(btn).on_hover_text(vibe.tagline()).clicked() {
                        let seed = self.next_seed();
                        apply_vibe_to_project(proj, vibe, None, seed);
                    }
                }
                ui.separator();

                // Chord Progression Picker
                egui::ComboBox::from_id_salt("prog_picker")
                    .width(210.0)
                    .selected_text(PROGRESSIONS[proj.progression_idx % PROGRESSIONS.len()].name)
                    .show_ui(ui, |ui| {
                        for (i, prog) in PROGRESSIONS.iter().enumerate() {
                            if ui
                                .selectable_value(&mut proj.progression_idx, i, prog.name)
                                .changed()
                            {
                                let seed = self.next_seed();
                                let (bass, pad, arp, acid, samp) = generate_melodic_tracks(
                                    proj.vibe,
                                    proj.root_note,
                                    proj.progression_idx,
                                    seed,
                                );
                                let pat = proj.current_pattern_mut();
                                pat.moog = bass;
                                pat.moog_pad = pad;
                                pat.monologue = arp;
                                pat.tb303 = acid;
                                pat.sampler = samp;
                            }
                        }
                    });

                // 1-Click Roll Fresh Vibe!
                let roll_btn = egui::Button::new(
                    RichText::new("Fresh Groove")
                        .strong()
                        .size(12.5)
                        .color(Color32::WHITE),
                )
                .fill(Color32::from_rgb(66, 78, 128));
                if ui
                    .add(roll_btn)
                    .on_hover_text("Generate a fresh beat + chord/bass/arp variation in this Vibe")
                    .clicked()
                {
                    let seed = self.next_seed();
                    let next_prog = (proj.progression_idx + 1) % PROGRESSIONS.len();
                    apply_vibe_to_project(proj, proj.vibe, Some(next_prog), seed);
                }
            });
            ui.label(
                RichText::new(format!(
                    "{}  •  Press Space to Play/Stop, A-K to play in-key chords/notes",
                    proj.vibe.tagline()
                ))
                .size(11.5)
                .weak(),
            );
        });

        ui.add_space(8.0);

        // 2. TRACK RACK (LEFT) + STEP SEQUENCER (RIGHT)
        ui.columns(2, |cols| {
            // Left: 6 Studio Tracks
            cols[0].vertical(|ui| {
                card_frame(ui, |ui| {
                    let active_core = proj.active_tracks.iter().filter(|&&a| a).count();
                    let total_tracks = active_core + proj.extra_tracks.len();
                    let mut add_extra_preset: Option<usize> = None;
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("STUDIO TRACKS ({total_tracks}/{})", 6 + MAX_EXTRA_TRACKS))
                                .strong()
                                .size(12.5)
                                .color(accent),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if proj.extra_tracks.len() < MAX_EXTRA_TRACKS {
                                ui.menu_button(
                                    RichText::new("+ Add Track")
                                        .strong()
                                        .size(11.5)
                                        .color(Color32::from_rgb(82, 232, 180)),
                                    |ui| {
                                        ui.label(RichText::new("NEW SYNTH LAYER").strong().size(11.0).weak());
                                        ui.separator();
                                        let presets = voice_preset_names(TrackId::MoogPad);
                                        for (pi, &pname) in presets.iter().enumerate() {
                                            if ui.button(format!("+ {pname}")).clicked() {
                                                add_extra_preset = Some(pi);
                                                ui.close_menu();
                                            }
                                        }
                                    },
                                );
                            }
                            if active_core < 6 {
                                if ui
                                    .small_button("+ Core")
                                    .on_hover_text("Restore all 6 core studio tracks")
                                    .clicked()
                                {
                                    for (idx, &t) in TrackId::STUDIO_TRACKS.iter().enumerate() {
                                        if !proj.active_tracks[idx] {
                                            proj.active_tracks[idx] = true;
                                            let (_, mute, _) = track_mixer_mut(proj, t);
                                            *mute = false;
                                        }
                                    }
                                }
                            }
                        });
                    });
                    if let Some(pi) = add_extra_preset {
                        let presets = voice_preset_names(TrackId::MoogPad);
                        let palette: [[u8; 3]; 6] = [
                            [255, 198, 72],
                            [112, 196, 255],
                            [255, 122, 92],
                            [198, 138, 255],
                            [92, 236, 188],
                            [255, 118, 196],
                        ];
                        let n = proj.extra_tracks.len();
                        let mut tr = ExtraTrack {
                            name: presets[pi % presets.len()].to_string(),
                            pan: if n % 2 == 0 { -0.28 } else { 0.28 },
                            color: palette[n % palette.len()],
                            ..ExtraTrack::default()
                        };
                        apply_extra_track_preset(&mut tr, pi);
                        proj.extra_tracks.push(tr);
                        let seed = self.next_seed();
                        let seq = generate_extra_track_seq(
                            proj.root_note,
                            proj.progression_idx,
                            pi,
                            seed,
                        );
                        for pat in &mut proj.patterns {
                            pat.extra_seqs.push(seq.clone());
                        }
                        self.selected_extra = Some(n);
                    }
                    ui.add_space(4.0);

                    let mut removed_track_idx: Option<usize> = None;
                    for (row_idx, &track) in TrackId::STUDIO_TRACKS.iter().enumerate() {
                        if !proj.active_tracks[row_idx] {
                            continue;
                        }
                        let selected = self.selected_extra.is_none() && self.selected_track == track;
                        let tc = track_color(track);
                        let row_bg = if selected {
                            Color32::from_rgb(30, 38, 56)
                        } else {
                            Color32::from_rgb(20, 24, 34)
                        };
                        egui::Frame::none()
                            .fill(row_bg)
                            .rounding(Rounding::same(6.0))
                            .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                            .stroke(Stroke::new(
                                if selected { 1.5_f32 } else { 1.0_f32 },
                                if selected { tc } else { Color32::from_rgb(34, 40, 56) },
                            ))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Select button with color badge (Double-click = Sound Lab, Right-click = options)
                                    let is_muted_now = if track == TrackId::MoogPad {
                                        proj.mixer.moog_pad_mute
                                    } else {
                                        proj.mixer.channels[track.index()].mute
                                    };
                                    let sel_btn = egui::Button::new(
                                        RichText::new(if is_muted_now {
                                            format!("M {}", track.role_name())
                                        } else {
                                            track.role_name().to_string()
                                        })
                                        .strong()
                                        .size(12.0)
                                        .color(if is_muted_now {
                                            Color32::from_rgb(120, 128, 148)
                                        } else if selected {
                                            tc
                                        } else {
                                            Color32::from_rgb(220, 226, 240)
                                        }),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .min_size(Vec2::new(86.0, 22.0));
                                    let sel_resp = ui
                                        .add(sel_btn)
                                        .on_hover_text("Click: select  •  Double-click: open full synth  •  Right-click: instrument menu");
                                    if sel_resp.clicked() {
                                        self.selected_track = track;
                                        self.selected_extra = None;
                                    }
                                    if sel_resp.double_clicked() {
                                        self.selected_track = track;
                                        self.selected_extra = None;
                                        self.tab = if track == TrackId::Sampler {
                                            StudioTab::Sampler
                                        } else {
                                            StudioTab::SoundLab
                                        };
                                    }
                                    sel_resp.context_menu(|ui| {
                                        ui.set_min_width(210.0);
                                        ui.label(RichText::new(track.role_name()).strong().color(tc));
                                        ui.separator();
                                        let open_lbl = if track == TrackId::Sampler {
                                            "Open Pro Sampler Workbench"
                                        } else {
                                            "Open Full Synth in Sound Lab"
                                        };
                                        if ui.button(open_lbl).clicked() {
                                            self.selected_track = track;
                                            self.tab = if track == TrackId::Sampler {
                                                StudioTab::Sampler
                                            } else {
                                                StudioTab::SoundLab
                                            };
                                            ui.close_menu();
                                        }
                                        {
                                            let (_, mute, solo) = track_mixer_mut(proj, track);
                                            if ui.button(if *mute { "Unmute Track" } else { "M Mute Track" }).clicked() {
                                                *mute = !*mute;
                                                ui.close_menu();
                                            }
                                            if ui.button(if *solo { "Unsolo Track" } else { "Solo Track" }).clicked() {
                                                *solo = !*solo;
                                                ui.close_menu();
                                            }
                                        }
                                        ui.separator();
                                        // Quick tone / pan / sends right in context menu!
                                        match track {
                                            TrackId::Moog => {
                                                ui.add(egui::Slider::new(&mut proj.moog.cutoff, 60.0..=16_000.0).logarithmic(true).text("Cutoff"));
                                                ui.add(egui::Slider::new(&mut proj.moog.resonance, 0.0..=0.92).text("Res"));
                                                ui.add(egui::Slider::new(&mut proj.moog.drive, 0.0..=1.0).text("Drive"));
                                            }
                                            TrackId::MoogPad => {
                                                ui.add(egui::Slider::new(&mut proj.moog_pad.cutoff, 60.0..=16_000.0).logarithmic(true).text("Cutoff"));
                                                ui.add(egui::Slider::new(&mut proj.moog_pad.resonance, 0.0..=0.92).text("Res"));
                                                ui.add(egui::Slider::new(&mut proj.moog_pad.amp_release, 0.05..=2.5).text("Release"));
                                            }
                                            TrackId::Monologue => {
                                                ui.add(egui::Slider::new(&mut proj.monologue.cutoff, 60.0..=16_000.0).logarithmic(true).text("Cutoff"));
                                                ui.add(egui::Slider::new(&mut proj.monologue.resonance, 0.0..=0.92).text("Res"));
                                                ui.add(egui::Slider::new(&mut proj.monologue.eg_decay, 0.02..=2.0).text("Decay"));
                                            }
                                            TrackId::Tb303 => {
                                                ui.add(egui::Slider::new(&mut proj.tb303.cutoff, 80.0..=6000.0).logarithmic(true).text("Cutoff"));
                                                ui.add(egui::Slider::new(&mut proj.tb303.resonance, 0.0..=0.96).text("Res"));
                                                ui.add(egui::Slider::new(&mut proj.tb303.env_mod, 0.0..=1.0).text("Env Mod"));
                                            }
                                            TrackId::Sampler => {
                                                ui.add(egui::Slider::new(&mut proj.sampler.cutoff, 120.0..=18_000.0).logarithmic(true).text("Cutoff"));
                                                ui.add(egui::Slider::new(&mut proj.sampler.transpose, -24..=24).text("Pitch"));
                                            }
                                            TrackId::Drums => {
                                                ui.horizontal(|ui| {
                                                    if ui.small_button("808 Kit").clicked() {
                                                        proj.drums.set_all_model(DrumModel::Tr808);
                                                    }
                                                    if ui.small_button("909 Kit").clicked() {
                                                        proj.drums.set_all_model(DrumModel::Tr909);
                                                    }
                                                    if ui.small_button("Hybrid").clicked() {
                                                        proj.drums.set_hybrid();
                                                    }
                                                });
                                            }
                                            TrackId::MidiOut => {}
                                        }
                                        if track != TrackId::MoogPad {
                                            let ch = &mut proj.mixer.channels[track.index()];
                                            ui.add(egui::Slider::new(&mut ch.pan, -1.0..=1.0).text("Pan"));
                                            ui.add(egui::Slider::new(&mut ch.delay_send, 0.0..=1.0).text("Delay"));
                                            ui.add(egui::Slider::new(&mut ch.reverb_send, 0.0..=1.0).text("Reverb"));
                                        } else {
                                            ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_delay_send, 0.0..=1.0).text("Delay"));
                                            ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_reverb_send, 0.0..=1.0).text("Reverb"));
                                        }
                                        ui.separator();
                                        if ui.button("Randomize Pattern").clicked() {
                                            let seed = self.next_seed();
                                            if track == TrackId::Drums {
                                                let drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                                                proj.current_pattern_mut().drums = drums;
                                            } else {
                                                let (bass, pad, arp, acid, samp) = generate_melodic_tracks(
                                                    proj.vibe,
                                                    proj.root_note,
                                                    proj.progression_idx,
                                                    seed,
                                                );
                                                let pat = proj.current_pattern_mut();
                                                match track {
                                                    TrackId::Moog => pat.moog = bass,
                                                    TrackId::MoogPad => pat.moog_pad = pad,
                                                    TrackId::Monologue => pat.monologue = arp,
                                                    TrackId::Tb303 => pat.tb303 = acid,
                                                    TrackId::Sampler => pat.sampler = samp,
                                                    _ => {}
                                                }
                                            }
                                            ui.close_menu();
                                        }
                                        if ui.button("X Clear Pattern").clicked() {
                                            let pat = proj.current_pattern_mut();
                                            if track == TrackId::Drums {
                                                for lane in &mut pat.drums.lanes {
                                                    lane.clear();
                                                }
                                            } else if let Some(seq) = pat.melodic_mut(track) {
                                                seq.clear();
                                            }
                                            ui.close_menu();
                                        }
                                    });

                                    // Instant Voice Preset Picker (or Volca 8-Pad shortcut for Sampler)
                                    if track == TrackId::Sampler {
                                        let label = if proj.sample_parts[0].sample.builtin_id.is_none() {
                                            format!("{} ->", proj.sample_parts[0].name)
                                        } else {
                                            "8-Part Volca ->".to_string()
                                        };
                                        let vbtn = egui::Button::new(
                                            RichText::new(label)
                                                .size(11.5)
                                                .color(Color32::from_rgb(255, 214, 110)),
                                        )
                                        .fill(Color32::from_rgb(36, 34, 46))
                                        .min_size(Vec2::new(112.0, 22.0));
                                        if ui
                                            .add(vbtn)
                                            .on_hover_text("Open 8-Part Volca Performance Sampler (load MP3/WAV, auto-chop, Isolator EQ)")
                                            .clicked()
                                        {
                                            self.tab = StudioTab::Sampler;
                                        }
                                    } else {
                                        let presets = voice_preset_names(track);
                                        let cur_preset = proj.voice_presets[row_idx] % presets.len().max(1);
                                        egui::ComboBox::from_id_salt(("vp", row_idx))
                                            .width(126.0)
                                            .selected_text(presets[cur_preset])
                                            .show_ui(ui, |ui| {
                                                for (pi, &pname) in presets.iter().enumerate() {
                                                    if ui.selectable_label(cur_preset == pi, pname).clicked() {
                                                        apply_voice_preset(proj, track, pi);
                                                    }
                                                }
                                            });
                                    }

                                    // Mute / Solo / Volume
                                    let (vol, mute, solo) = track_mixer_mut(proj, track);
                                    let m_btn = egui::Button::new(
                                        RichText::new("M").strong().size(11.0).color(if *mute {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_rgb(160, 168, 188)
                                        }),
                                    )
                                    .fill(if *mute {
                                        Color32::from_rgb(200, 62, 74)
                                    } else {
                                        Color32::from_rgb(32, 38, 52)
                                    });
                                    if ui.add(m_btn).clicked() {
                                        *mute = !*mute;
                                    }

                                    let s_btn = egui::Button::new(
                                        RichText::new("S").strong().size(11.0).color(if *solo {
                                            Color32::from_rgb(12, 14, 20)
                                        } else {
                                            Color32::from_rgb(160, 168, 188)
                                        }),
                                    )
                                    .fill(if *solo {
                                        Color32::from_rgb(245, 202, 72)
                                    } else {
                                        Color32::from_rgb(32, 38, 52)
                                    });
                                    if ui.add(s_btn).clicked() {
                                        *solo = !*solo;
                                    }

                                    ui.add(
                                        egui::Slider::new(vol, 0.0..=1.15)
                                            .show_value(false)
                                            .trailing_fill(true),
                                    );

                                    // Per-track Randomize & Clear
                                    if ui.small_button("RND").on_hover_text("Generate fresh pattern for this track").clicked() {
                                        let seed = self.next_seed();
                                        if track == TrackId::Drums {
                                            let drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                                            proj.current_pattern_mut().drums = drums;
                                        } else if track == TrackId::Sampler {
                                            let pat = proj.current_pattern_mut();
                                            for l in &mut pat.sample_lanes {
                                                l.clear();
                                            }
                                            for bar in 0..4 {
                                                let base = bar * 16;
                                                for &(step, pad_i) in &[(0, 0), (3, 1), (6, 2), (8, 3), (10, 4), (12, 5), (14, 6), (15, 7)] {
                                                    let p = (pad_i + bar + (seed as usize)) % 8;
                                                    let s = &mut pat.sample_lanes[p].steps[base + step];
                                                    s.active = true;
                                                    s.velocity = if step % 4 == 0 { 0.92 } else { 0.78 };
                                                }
                                            }
                                            proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                                        } else {
                                            let (bass, pad, arp, acid, samp) = generate_melodic_tracks(
                                                proj.vibe,
                                                proj.root_note,
                                                proj.progression_idx,
                                                seed,
                                            );
                                            let pat = proj.current_pattern_mut();
                                            match track {
                                                TrackId::Moog => pat.moog = bass,
                                                TrackId::MoogPad => pat.moog_pad = pad,
                                                TrackId::Monologue => pat.monologue = arp,
                                                TrackId::Tb303 => pat.tb303 = acid,
                                                TrackId::Sampler => pat.sampler = samp,
                                                _ => {}
                                            }
                                        }
                                    }
                                    if ui.small_button("CLR").on_hover_text("Clear track pattern").clicked() {
                                        let pat = proj.current_pattern_mut();
                                        if track == TrackId::Drums {
                                            for lane in &mut pat.drums.lanes {
                                                lane.clear();
                                            }
                                        } else if track == TrackId::Sampler {
                                            pat.sampler.clear();
                                            for lane in &mut pat.sample_lanes {
                                                lane.clear();
                                            }
                                        } else if let Some(seq) = pat.melodic_mut(track) {
                                            seq.clear();
                                        }
                                    }
                                    // Subtract track from mix
                                    let sub_btn = egui::Button::new(
                                        RichText::new("−")
                                            .strong()
                                            .size(12.0)
                                            .color(Color32::from_rgb(235, 130, 140)),
                                    )
                                    .fill(Color32::from_rgb(44, 26, 32));
                                    if ui
                                        .add(sub_btn)
                                        .on_hover_text("Remove track from mix (mutes & hides it; re-add below anytime)")
                                        .clicked()
                                    {
                                        removed_track_idx = Some(row_idx);
                                    }
                                });
                            });
                        ui.add_space(4.0);
                    }

                    if let Some(rm_idx) = removed_track_idx {
                        let rm_track = TrackId::STUDIO_TRACKS[rm_idx];
                        proj.active_tracks[rm_idx] = false;
                        let (_, mute, solo) = track_mixer_mut(proj, rm_track);
                        *mute = true;
                        *solo = false;
                        if self.selected_track == rm_track {
                            if let Some((next_idx, _)) =
                                proj.active_tracks.iter().enumerate().find(|(_, &a)| a)
                            {
                                self.selected_track = TrackId::STUDIO_TRACKS[next_idx];
                            }
                        }
                    }

                    // Render user-added Extra Synth Tracks (tracks 7..=16)
                    let mut remove_extra_idx: Option<usize> = None;
                    let pad_presets = voice_preset_names(TrackId::MoogPad);
                    for ei in 0..proj.extra_tracks.len() {
                        let selected = self.selected_extra == Some(ei);
                        let [cr, cg, cb] = proj.extra_tracks[ei].color;
                        let tc = Color32::from_rgb(cr, cg, cb);
                        let row_bg = if selected {
                            Color32::from_rgb(30, 38, 56)
                        } else {
                            Color32::from_rgb(20, 24, 34)
                        };
                        egui::Frame::none()
                            .fill(row_bg)
                            .rounding(Rounding::same(6.0))
                            .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                            .stroke(Stroke::new(
                                if selected { 1.5_f32 } else { 1.0_f32 },
                                if selected { tc } else { Color32::from_rgb(34, 40, 56) },
                            ))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let is_muted_now = proj.extra_tracks[ei].mute;
                                    let lbl = if is_muted_now {
                                        format!("M {}", proj.extra_tracks[ei].name)
                                    } else {
                                        proj.extra_tracks[ei].name.clone()
                                    };
                                    let sel_btn = egui::Button::new(
                                        RichText::new(lbl)
                                            .strong()
                                            .size(11.5)
                                            .color(if is_muted_now {
                                                Color32::from_rgb(120, 128, 148)
                                            } else if selected {
                                                tc
                                            } else {
                                                Color32::from_rgb(220, 226, 240)
                                            }),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .min_size(Vec2::new(86.0, 22.0));
                                    let resp = ui
                                        .add(sel_btn)
                                        .on_hover_text("Click: edit steps  •  Double-click: open in Sound Lab");
                                    if resp.clicked() {
                                        self.selected_extra = Some(ei);
                                    }
                                    if resp.double_clicked() {
                                        self.selected_extra = Some(ei);
                                        self.tab = StudioTab::SoundLab;
                                    }

                                    // Preset dropdown
                                    let cur_p = proj.extra_tracks[ei].preset_idx % pad_presets.len();
                                    egui::ComboBox::from_id_salt(("extra_vp", ei))
                                        .width(126.0)
                                        .selected_text(pad_presets[cur_p])
                                        .show_ui(ui, |ui| {
                                            for (pi, &pname) in pad_presets.iter().enumerate() {
                                                if ui.selectable_label(cur_p == pi, pname).clicked() {
                                                    apply_extra_track_preset(&mut proj.extra_tracks[ei], pi);
                                                    proj.extra_tracks[ei].name = pname.to_string();
                                                }
                                            }
                                        });

                                    // Mute / Solo / Volume
                                    let tr = &mut proj.extra_tracks[ei];
                                    let m_btn = egui::Button::new(
                                        RichText::new("M").strong().size(11.0).color(if tr.mute {
                                            Color32::WHITE
                                        } else {
                                            Color32::from_rgb(160, 168, 188)
                                        }),
                                    )
                                    .fill(if tr.mute {
                                        Color32::from_rgb(200, 62, 74)
                                    } else {
                                        Color32::from_rgb(32, 38, 52)
                                    });
                                    if ui.add(m_btn).clicked() {
                                        tr.mute = !tr.mute;
                                    }

                                    let s_btn = egui::Button::new(
                                        RichText::new("S").strong().size(11.0).color(if tr.solo {
                                            Color32::from_rgb(12, 14, 20)
                                        } else {
                                            Color32::from_rgb(160, 168, 188)
                                        }),
                                    )
                                    .fill(if tr.solo {
                                        Color32::from_rgb(245, 202, 72)
                                    } else {
                                        Color32::from_rgb(32, 38, 52)
                                    });
                                    if ui.add(s_btn).clicked() {
                                        tr.solo = !tr.solo;
                                    }

                                    ui.add(
                                        egui::Slider::new(&mut tr.volume, 0.0..=1.15)
                                            .show_value(false)
                                            .trailing_fill(true),
                                    );

                                    if ui
                                        .small_button("RND")
                                        .on_hover_text("Generate fresh chord-locked pattern for this track")
                                        .clicked()
                                    {
                                        let seed = self.next_seed();
                                        let seq = generate_extra_track_seq(
                                            proj.root_note,
                                            proj.progression_idx,
                                            proj.extra_tracks[ei].preset_idx,
                                            seed,
                                        );
                                        if let Some(slot) = proj.current_pattern_mut().extra_seqs.get_mut(ei) {
                                            *slot = seq;
                                        }
                                    }
                                    if ui.small_button("CLR").on_hover_text("Clear track pattern").clicked() {
                                        if let Some(slot) = proj.current_pattern_mut().extra_seqs.get_mut(ei) {
                                            slot.clear();
                                        }
                                    }
                                    let sub_btn = egui::Button::new(
                                        RichText::new("−")
                                            .strong()
                                            .size(12.0)
                                            .color(Color32::from_rgb(235, 130, 140)),
                                    )
                                    .fill(Color32::from_rgb(44, 26, 32));
                                    if ui
                                        .add(sub_btn)
                                        .on_hover_text("Delete this synth track from the mix")
                                        .clicked()
                                    {
                                        remove_extra_idx = Some(ei);
                                    }
                                });
                            });
                        ui.add_space(4.0);
                    }
                    if let Some(rm_e) = remove_extra_idx {
                        proj.extra_tracks.remove(rm_e);
                        for pat in &mut proj.patterns {
                            if rm_e < pat.extra_seqs.len() {
                                pat.extra_seqs.remove(rm_e);
                            }
                        }
                        if self.selected_extra == Some(rm_e) {
                            self.selected_extra = None;
                        } else if let Some(cur) = self.selected_extra {
                            if cur > rm_e {
                                self.selected_extra = Some(cur - 1);
                            }
                        }
                    }

                    // Quick "+ Add Track" bar for any tracks currently subtracted from the mix
                    if proj.active_tracks.iter().any(|&a| !a) {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("ADD TO MIX:")
                                    .strong()
                                    .size(11.0)
                                    .color(Color32::from_rgb(160, 172, 198)),
                            );
                            for (idx, &track) in TrackId::STUDIO_TRACKS.iter().enumerate() {
                                if proj.active_tracks[idx] {
                                    continue;
                                }
                                let tc = track_color(track);
                                let add_btn = egui::Button::new(
                                    RichText::new(format!("+ {}", track.role_name()))
                                        .strong()
                                        .size(11.5)
                                        .color(tc),
                                )
                                .fill(Color32::from_rgb(24, 30, 44))
                                .stroke(Stroke::new(1.0_f32, tc.gamma_multiply(0.65)));
                                if ui
                                    .add(add_btn)
                                    .on_hover_text("Add this track back into the mix and unmute it")
                                    .clicked()
                                {
                                    proj.active_tracks[idx] = true;
                                    let (_, mute, _) = track_mixer_mut(proj, track);
                                    *mute = false;
                                    self.selected_track = track;
                                    // If the track currently has no active steps, generate a chord-locked part!
                                    let is_empty = {
                                        let pat = proj.current_pattern();
                                        match track {
                                            TrackId::Drums => !pat.drums.lanes.iter().any(|l| l.steps.iter().any(|s| s.active)),
                                            TrackId::Moog => !pat.moog.steps.iter().any(|s| s.active),
                                            TrackId::MoogPad => !pat.moog_pad.steps.iter().any(|s| s.active),
                                            TrackId::Monologue => !pat.monologue.steps.iter().any(|s| s.active),
                                            TrackId::Tb303 => !pat.tb303.steps.iter().any(|s| s.active),
                                            TrackId::Sampler => {
                                                !pat.sampler.steps.iter().any(|s| s.active)
                                                    && !pat.sample_lanes.iter().any(|l| l.steps.iter().any(|s| s.active))
                                            }
                                            _ => false,
                                        }
                                    };
                                    if is_empty {
                                        let seed = self.next_seed();
                                        if track == TrackId::Drums {
                                            let drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                                            proj.current_pattern_mut().drums = drums;
                                        } else {
                                            let (bass, pad, arp, acid, samp) = generate_melodic_tracks(
                                                proj.vibe,
                                                proj.root_note,
                                                proj.progression_idx,
                                                seed,
                                            );
                                            let pat = proj.current_pattern_mut();
                                            match track {
                                                TrackId::Moog => pat.moog = bass,
                                                TrackId::MoogPad => pat.moog_pad = pad,
                                                TrackId::Monologue => pat.monologue = arp,
                                                TrackId::Tb303 => pat.tb303 = acid,
                                                TrackId::Sampler => pat.sampler = samp,
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                });
            });

            // Right: Clean 16-Step Sequencer for Selected Track
            cols[1].vertical(|ui| {
                card_frame(ui, |ui| {
                    let (seq_title, tc) = if let Some(ei) = self.selected_extra.filter(|&i| i < proj.extra_tracks.len()) {
                        let [cr, cg, cb] = proj.extra_tracks[ei].color;
                        (
                            format!("{} SEQUENCER", proj.extra_tracks[ei].name.to_uppercase()),
                            Color32::from_rgb(cr, cg, cb),
                        )
                    } else {
                        (
                            format!("{} SEQUENCER", self.selected_track.role_name()),
                            track_color(self.selected_track),
                        )
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(seq_title)
                                .strong()
                                .size(12.5)
                                .color(tc),
                        );
                        ui.separator();
                        // Bar page selector (Bar 1..4)
                        for b in 0..4 {
                            let label = format!("Bar {}", b + 1);
                            if ui.selectable_label(self.bar_page == b, label).clicked() {
                                self.bar_page = b;
                                self.follow_playhead = false;
                            }
                        }
                        ui.checkbox(&mut self.follow_playhead, "Follow");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("Load Pat")
                                .on_hover_text("Load a saved pattern (.json) into the active Bank")
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Fun Key Pattern", &["json"])
                                    .pick_file()
                                {
                                    match load_pattern(&path) {
                                        Ok(pat) => {
                                            let cur = proj.active_pattern;
                                            proj.patterns[cur] = pat;
                                            self.set_toast(ctx, format!("Loaded pattern into Bank {}!", (b'A' + cur as u8) as char));
                                        }
                                        Err(e) => self.set_toast(ctx, format!("Pattern load error: {e}")),
                                    }
                                }
                            }
                            if ui
                                .small_button("Save Pat")
                                .on_hover_text("Save the current Bank pattern (all tracks, Volca lanes, Step FX & Motion) to a reusable .json file")
                                .clicked()
                            {
                                let bank_ch = (b'A' + proj.active_pattern as u8) as char;
                                if let Some(path) = rfd::FileDialog::new()
                                    .set_file_name(format!("pattern_{bank_ch}.json"))
                                    .add_filter("Fun Key Pattern", &["json"])
                                    .save_file()
                                {
                                    match save_pattern(&path, proj.current_pattern()) {
                                        Ok(()) => self.set_toast(ctx, format!("Saved Bank {bank_ch} pattern!")),
                                        Err(e) => self.set_toast(ctx, format!("Pattern save error: {e}")),
                                    }
                                }
                            }
                            if ui
                                .small_button("Dup->Next")
                                .on_hover_text("Duplicate current Bank pattern into the next Bank so you can build variations")
                                .clicked()
                            {
                                let src = proj.active_pattern;
                                let dst = (src + 1) % proj.patterns.len().max(1);
                                proj.patterns[dst] = proj.patterns[src].clone();
                                proj.active_pattern = dst;
                                self.set_toast(
                                    ctx,
                                    format!(
                                        "Duplicated Bank {} -> Bank {}",
                                        (b'A' + src as u8) as char,
                                        (b'A' + dst as u8) as char
                                    ),
                                );
                            }
                            // Pattern bank A..H quick switch
                            for b in (0..8).rev() {
                                let label = format!("{}", (b'A' + b as u8) as char);
                                if ui.selectable_label(proj.active_pattern == b, label).clicked() {
                                    proj.active_pattern = b;
                                }
                            }
                            ui.label(RichText::new("Bank:").size(11.0).weak());
                        });
                    });

                    ui.add_space(6.0);

                    let page_start = self.bar_page * 16;
                    let play_step = if playing { Some(cur_step % 64) } else { None };

                    if self.selected_extra.is_none() && self.selected_track == TrackId::Drums {
                        // Auto-Beat inline controls
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Beat Energy:").size(11.5));
                            if ui
                                .add(egui::Slider::new(&mut proj.beat_energy, 0.1..=1.0).show_value(false))
                                .changed()
                                && !proj.auto_beat_mode
                            {
                                let seed = proj.party_seed;
                                let drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                                proj.current_pattern_mut().drums = drums;
                            }
                            if ui.small_button("New Beat").clicked() {
                                let seed = self.next_seed();
                                proj.party_seed = seed;
                                let drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                                proj.current_pattern_mut().drums = drums;
                            }
                            if ui.small_button("Fill Now").clicked() {
                                self.engine.send_live(LiveEvent::TriggerFill);
                            }
                            ui.label(RichText::new("Click drum name = Mute  •  Right-click drum = Tune/Options  •  Right-click step = Ratchet").size(10.5).weak());
                        });
                        ui.add_space(4.0);

                        // 8-lane compact Drum Beat Grid (16 steps per bar page)
                        let active_pat = proj.active_pattern;
                        let cell_w = ((ui.available_width() - 62.0 - 15.0 * 3.0) / 16.0).clamp(18.0, 34.0);
                        let cell_h = 22.0;
                        let mut solo_drum_lane: Option<usize> = None;
                        let mut unmute_all_drums = false;

                        for lane_idx in 0..8 {
                            let voice = DrumVoiceId::from_index(lane_idx).unwrap();
                            let is_muted = proj.drums.voices[lane_idx].mute;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;

                                // Clickable drum voice label: Left-click = Mute, Right-click = Full Voice Options & Tuning!
                                let lbl_btn = egui::Button::new(
                                    RichText::new(if is_muted {
                                        format!("X {}", voice.short_name())
                                    } else {
                                        voice.short_name().to_string()
                                    })
                                    .size(10.5)
                                    .strong()
                                    .color(if is_muted {
                                        Color32::from_rgb(115, 122, 140)
                                    } else {
                                        Color32::from_rgb(228, 234, 248)
                                    }),
                                )
                                .fill(if is_muted {
                                    Color32::from_rgb(38, 22, 28)
                                } else {
                                    Color32::from_rgb(24, 29, 42)
                                })
                                .min_size(Vec2::new(54.0, cell_h));

                                let lbl_resp = ui
                                    .add(lbl_btn)
                                    .on_hover_text("Left-click: Mute/Unmute  •  Right-click: Tune, 808/909 & Fill options");
                                if lbl_resp.clicked() {
                                    proj.drums.voices[lane_idx].mute = !proj.drums.voices[lane_idx].mute;
                                }
                                lbl_resp.context_menu(|ui| {
                                    ui.set_min_width(220.0);
                                    ui.label(RichText::new(voice.full_name()).strong().color(tc));
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        if ui.button("Audition").clicked() {
                                            proj.drums.voices[lane_idx].mute = false;
                                            self.engine.send_live(LiveEvent::PlayNote {
                                                track: TrackId::Drums,
                                                note: lane_idx as u8,
                                                velocity: 0.92,
                                            });
                                        }
                                        if ui.button(if proj.drums.voices[lane_idx].mute { "Unmute" } else { "Mute" }).clicked() {
                                            proj.drums.voices[lane_idx].mute = !proj.drums.voices[lane_idx].mute;
                                        }
                                        if ui.button("Solo").clicked() {
                                            solo_drum_lane = Some(lane_idx);
                                            ui.close_menu();
                                        }
                                        if ui.button("All On").clicked() {
                                            unmute_all_drums = true;
                                            ui.close_menu();
                                        }
                                    });
                                    ui.separator();
                                    let vp = &mut proj.drums.voices[lane_idx];
                                    ui.horizontal(|ui| {
                                        ui.label("Model:");
                                        ui.selectable_value(&mut vp.model, DrumModel::Tr808, "TR-808");
                                        ui.selectable_value(&mut vp.model, DrumModel::Tr909, "TR-909");
                                    });
                                    ui.add(egui::Slider::new(&mut vp.level, 0.0..=1.25).text("Level"));
                                    ui.add(egui::Slider::new(&mut vp.tune, -12.0..=12.0).text("Tune"));
                                    ui.add(egui::Slider::new(&mut vp.decay, 0.05..=1.0).text("Decay"));
                                    ui.add(egui::Slider::new(&mut vp.tone, 0.0..=1.0).text("Tone"));
                                    ui.add(egui::Slider::new(&mut vp.snappy, 0.0..=1.0).text("Snappy"));
                                    ui.add(egui::Slider::new(&mut vp.pan, -1.0..=1.0).text("Pan"));
                                    ui.separator();
                                    ui.label(RichText::new("Quick Fill (current bar):").size(11.0).weak());
                                    let lane = &mut proj.patterns[active_pat].drums.lanes[lane_idx];
                                    ui.horizontal(|ui| {
                                        if ui.small_button("4x4").clicked() {
                                            for c in [0, 4, 8, 12] {
                                                let s = &mut lane.steps[(page_start + c) % 64];
                                                s.active = true;
                                                s.velocity = 0.94;
                                            }
                                        }
                                        if ui.small_button("2 & 4").clicked() {
                                            for c in [4, 12] {
                                                let s = &mut lane.steps[(page_start + c) % 64];
                                                s.active = true;
                                                s.velocity = 0.90;
                                            }
                                        }
                                        if ui.small_button("Offbeats").clicked() {
                                            for c in [2, 6, 10, 14] {
                                                let s = &mut lane.steps[(page_start + c) % 64];
                                                s.active = true;
                                                s.velocity = 0.76;
                                            }
                                        }
                                        if ui.small_button("8ths").clicked() {
                                            for c in (0..16).step_by(2) {
                                                let s = &mut lane.steps[(page_start + c) % 64];
                                                s.active = true;
                                                s.velocity = 0.72;
                                            }
                                        }
                                        if ui.small_button("Clear").clicked() {
                                            lane.clear();
                                            ui.close_menu();
                                        }
                                    });
                                });

                                let lane = &mut proj.patterns[active_pat].drums.lanes[lane_idx];
                                for col in 0..16 {
                                    let idx = (page_start + col) % lane.step_count.as_usize();
                                    let st = &mut lane.steps[idx];
                                    let is_play = play_step == Some(page_start + col);
                                    let beat_tint = if (col / 4) % 2 == 0 {
                                        Color32::from_rgb(26, 31, 44)
                                    } else {
                                        Color32::from_rgb(20, 24, 35)
                                    };
                                    let fill = if st.active {
                                        if is_muted {
                                            Color32::from_rgb(70, 56, 60)
                                        } else if is_play {
                                            Color32::WHITE
                                        } else if st.ratchet > 1 {
                                            Color32::from_rgb(255, 118, 192)
                                        } else if st.accent {
                                            tc
                                        } else {
                                            tc.gamma_multiply(0.76)
                                        }
                                    } else if is_play {
                                        Color32::from_rgb(56, 68, 96)
                                    } else {
                                        beat_tint
                                    };
                                    let (rect, resp) =
                                        ui.allocate_exact_size(Vec2::new(cell_w, cell_h), Sense::click());
                                    if resp.clicked() {
                                        st.active = !st.active;
                                        if st.active {
                                            st.velocity = 0.85;
                                            st.ratchet = 1;
                                        }
                                    }
                                    resp.context_menu(|ui| {
                                        ui.set_min_width(190.0);
                                        ui.label(RichText::new(format!("{} — Step {}", voice.short_name(), idx + 1)).strong().color(tc));
                                        ui.separator();
                                        ui.horizontal(|ui| {
                                            ui.label("Ratchet:");
                                            for r in 1..=4u8 {
                                                if ui.selectable_label(st.ratchet == r, format!("{r}x")).clicked() {
                                                    st.ratchet = r;
                                                    st.active = true;
                                                }
                                            }
                                        });
                                        if ui.add(egui::Slider::new(&mut st.pitch_offset, -12..=12).suffix(" st").text("Pitch")).changed() {
                                            st.active = true;
                                        }
                                        let mut p_on = st.pan_lock.is_some();
                                        let mut p_val = st.pan_lock.unwrap_or(0.0);
                                        ui.horizontal(|ui| {
                                            if ui.checkbox(&mut p_on, "Pan").changed() {
                                                st.pan_lock = if p_on { Some(p_val) } else { None };
                                            }
                                            if p_on && ui.add(egui::Slider::new(&mut p_val, -1.0..=1.0).show_value(false)).changed() {
                                                st.pan_lock = Some(p_val);
                                                st.active = true;
                                            }
                                        });
                                        ui.checkbox(&mut st.accent, "Accent");
                                        ui.add(egui::Slider::new(&mut st.velocity, 0.1..=1.0).text("Vel").max_decimals(2));
                                        if ui.small_button("Reset Step FX").clicked() {
                                            st.clear_step_fx();
                                            st.ratchet = 1;
                                        }
                                    });
                                    let painter = ui.painter();
                                    painter.rect_filled(rect, Rounding::same(4.0), fill);
                                    if st.active && (st.ratchet > 1 || st.has_step_fx()) {
                                        let b = if st.ratchet > 1 {
                                            format!("{}x", st.ratchet)
                                        } else {
                                            "*".to_string()
                                        };
                                        painter.text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            b,
                                            egui::FontId::proportional(9.5),
                                            Color32::from_rgb(12, 14, 20),
                                        );
                                    }
                                }
                            });
                        }
                        if let Some(s_idx) = solo_drum_lane {
                            for (i, vp) in proj.drums.voices.iter_mut().enumerate() {
                                vp.mute = i != s_idx;
                            }
                        }
                        if unmute_all_drums {
                            for vp in &mut proj.drums.voices {
                                vp.mute = false;
                            }
                        }
                    } else if self.selected_extra.is_none() && self.selected_track == TrackId::Sampler {
                        // 8-Lane Volca Sampler Grid right inside STUDIO!
                        ui.horizontal(|ui| {
                            if ui.small_button("Open Volca Sampler Workbench ->").clicked() {
                                self.tab = StudioTab::Sampler;
                            }
                            ui.label(RichText::new("Click P1–P8 = Mute  •  Right-click step = Ratchet").size(10.5).weak());
                        });
                        ui.add_space(4.0);
                        let active_pat = proj.active_pattern;
                        let cell_w = ((ui.available_width() - 62.0 - 15.0 * 3.0) / 16.0).clamp(18.0, 34.0);
                        let cell_h = 22.0;
                        for part_idx in 0..8 {
                            let is_muted = proj.sample_parts[part_idx].mute;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                let lbl = egui::Button::new(
                                    RichText::new(if is_muted {
                                        format!("X P{}", part_idx + 1)
                                    } else {
                                        format!("P{}", part_idx + 1)
                                    })
                                    .size(10.5)
                                    .strong()
                                    .color(if is_muted { Color32::GRAY } else { tc }),
                                )
                                .fill(if is_muted {
                                    Color32::from_rgb(38, 22, 28)
                                } else {
                                    Color32::from_rgb(24, 29, 42)
                                })
                                .min_size(Vec2::new(54.0, cell_h));
                                if ui
                                    .add(lbl)
                                    .on_hover_text(format!("{} — Click to Mute/Unmute", proj.sample_parts[part_idx].name))
                                    .clicked()
                                {
                                    proj.sample_parts[part_idx].mute = !proj.sample_parts[part_idx].mute;
                                }
                                let lane = &mut proj.patterns[active_pat].sample_lanes[part_idx];
                                for col in 0..16 {
                                    let idx = (page_start + col) % lane.step_count.as_usize();
                                    let st = &mut lane.steps[idx];
                                    let is_play = play_step == Some(page_start + col);
                                    let beat_tint = if (col / 4) % 2 == 0 {
                                        Color32::from_rgb(26, 31, 44)
                                    } else {
                                        Color32::from_rgb(20, 24, 35)
                                    };
                                    let fill = if st.active {
                                        if is_muted {
                                            Color32::from_rgb(70, 56, 60)
                                        } else if is_play {
                                            Color32::WHITE
                                        } else if st.ratchet > 1 {
                                            Color32::from_rgb(255, 118, 192)
                                        } else {
                                            tc.gamma_multiply(0.84)
                                        }
                                    } else if is_play {
                                        Color32::from_rgb(56, 68, 96)
                                    } else {
                                        beat_tint
                                    };
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), Sense::click());
                                    if resp.clicked() {
                                        st.active = !st.active;
                                        if st.active {
                                            st.velocity = 0.88;
                                            st.ratchet = 1;
                                            proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                                        }
                                    }
                                    if resp.secondary_clicked() {
                                        st.active = true;
                                        st.ratchet = (st.ratchet % 4) + 1;
                                    }
                                    let painter = ui.painter();
                                    painter.rect_filled(rect, Rounding::same(4.0), fill);
                                    if st.active && st.ratchet > 1 {
                                        painter.text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            format!("{}x", st.ratchet),
                                            egui::FontId::proportional(9.5),
                                            Color32::from_rgb(12, 14, 20),
                                        );
                                    }
                                }
                            });
                        }
                    } else {
                        // Melodic 16-Step Pad Grid with Human-Readable Note Names!
                        let target = self.selected_track;
                        let extra_sel = self.selected_extra;
                        let pat = proj.current_pattern_mut();
                        let seq_opt = if let Some(ei) = extra_sel {
                            pat.extra_seqs.get_mut(ei)
                        } else {
                            pat.melodic_mut(target)
                        };
                        if let Some(seq) = seq_opt {
                            let len = seq.step_count.as_usize();
                            let pad_w = ((ui.available_width() - 7.0 * 6.0) / 8.0).clamp(44.0, 88.0);
                            let pad_h = 46.0;
                            for row in 0..2 {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for col in 0..8 {
                                        let step_in_page = row * 8 + col;
                                        let idx = (page_start + step_in_page) % len;
                                        let st = &mut seq.steps[idx];
                                        let is_play = play_step == Some(page_start + step_in_page);
                                        let is_sel = self.selected_step == idx;
                                        let fill = if st.active {
                                            if is_play {
                                                Color32::WHITE
                                            } else {
                                                tc.gamma_multiply(0.82)
                                            }
                                        } else if is_play {
                                            Color32::from_rgb(52, 64, 92)
                                        } else {
                                            Color32::from_rgb(24, 29, 42)
                                        };
                                        let (rect, resp) = ui.allocate_exact_size(
                                            Vec2::new(pad_w, pad_h),
                                            Sense::click(),
                                        );
                                        if resp.clicked() {
                                            st.active = !st.active;
                                            self.selected_step = idx;
                                        }
                                        if resp.secondary_clicked() {
                                            self.selected_step = idx;
                                        }
                                        let painter = ui.painter();
                                        painter.rect_filled(rect, Rounding::same(6.0), fill);
                                        painter.rect_stroke(
                                            rect,
                                            Rounding::same(6.0),
                                            Stroke::new(
                                                if is_sel { 2.0_f32 } else { 1.0_f32 },
                                                if is_sel {
                                                    Color32::WHITE
                                                } else {
                                                    Color32::from_rgb(44, 52, 72)
                                                },
                                            ),
                                        );
                                        let step_num = format!("{}", step_in_page + 1);
                                        painter.text(
                                            rect.left_top() + Vec2::new(5.0, 3.0),
                                            egui::Align2::LEFT_TOP,
                                            step_num,
                                            egui::FontId::proportional(9.5),
                                            if st.active {
                                                Color32::from_rgb(16, 20, 28)
                                            } else {
                                                Color32::from_rgb(110, 122, 148)
                                            },
                                        );
                                        if st.active {
                                            let label = if st.chord != ChordType::Single {
                                                format!("{}{}", note_name(st.note), st.chord.name())
                                            } else {
                                                note_name(st.note)
                                            };
                                            painter.text(
                                                rect.center() + Vec2::new(0.0, 5.0),
                                                egui::Align2::CENTER_CENTER,
                                                label,
                                                egui::FontId::proportional(11.5),
                                                Color32::from_rgb(12, 15, 22),
                                            );
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                            }

                            // Clean Step Inspector (right-click any step to inspect without toggling!)
                            let sel_idx = self.selected_step % len;
                            let st = &mut seq.steps[sel_idx];
                            ui.separator();
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!("Step {}:", sel_idx + 1))
                                        .strong()
                                        .color(tc),
                                );
                                if ui.small_button("-").clicked() {
                                    st.note = st.note.saturating_sub(1).max(24);
                                }
                                ui.label(RichText::new(note_name(st.note)).strong());
                                if ui.small_button("+").clicked() {
                                    st.note = (st.note + 1).min(96);
                                }
                                egui::ComboBox::from_id_salt("step_chord")
                                    .width(74.0)
                                    .selected_text(st.chord.name())
                                    .show_ui(ui, |ui| {
                                        for &ct in &ChordType::ALL {
                                            ui.selectable_value(&mut st.chord, ct, ct.name());
                                        }
                                    });
                                ui.add(egui::Slider::new(&mut st.velocity, 0.1..=1.0).text("Vel").max_decimals(2));
                                ui.add(egui::Slider::new(&mut st.gate, 0.25..=16.0).text("Len").max_decimals(1));
                                ui.checkbox(&mut st.accent, "Acc");
                                ui.checkbox(&mut st.slide, "Slide");
                                // Per-Step Filter / Res P-Locks for Synths!
                                let mut c_val = st.motion.cutoff.unwrap_or(0.6);
                                if ui.add(egui::Slider::new(&mut c_val, 0.0..=1.0).text("Filt Lock").max_decimals(2)).changed() {
                                    st.motion.cutoff = Some(c_val);
                                    st.active = true;
                                }
                                let mut r_val = st.motion.resonance.unwrap_or(0.3);
                                if ui.add(egui::Slider::new(&mut r_val, 0.0..=1.0).text("Res Lock").max_decimals(2)).changed() {
                                    st.motion.resonance = Some(r_val);
                                    st.active = true;
                                }
                                if st.motion.any_locked() && ui.small_button("Clr Lock").clicked() {
                                    st.motion.clear();
                                }
                            });
                            ui.label(RichText::new("Left-click = toggle step  •  Right-click = select step to edit").size(10.5).weak());
                        }
                    }
                });
            });
        });

        ui.add_space(8.0);

        // 3. VIBE MACROS & LIVE DJ PERFORMANCE STRIP
        card_frame(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("VIBE MACROS").strong().size(12.0).color(accent));
                let has_motion = proj.current_pattern().motion.has_any();
                let rm_btn = egui::Button::new(
                    RichText::new(if self.rec_motion { "REC MOTION ON" } else { "REC MOTION" })
                        .strong()
                        .size(11.0)
                        .color(Color32::WHITE),
                )
                .fill(if self.rec_motion {
                    Color32::from_rgb(225, 58, 72)
                } else {
                    Color32::from_rgb(36, 42, 58)
                });
                if ui
                    .add(rm_btn)
                    .on_hover_text("Turn ON while playing, then sweep Filter, Space, Sidechain, Glitch, or Isolator to record live automation!")
                    .clicked()
                {
                    self.rec_motion = !self.rec_motion;
                }
                if has_motion && ui.small_button("Clr Motion").on_hover_text("Clear recorded motion automation on this pattern").clicked() {
                    proj.current_pattern_mut().motion.clear();
                }
                ui.separator();

                // Logarithmic Filter Macro
                let mut filt_norm = ((proj.fx.global_filter_cutoff.clamp(120.0, 18_000.0) / 120.0).ln()
                    / (18_000.0_f32 / 120.0).ln())
                .clamp(0.0, 1.0);
                ui.label("Filter");
                if ui
                    .add(egui::Slider::new(&mut filt_norm, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    proj.fx.global_filter_cutoff = 120.0 * (18_000.0_f32 / 120.0).powf(filt_norm);
                    if self.rec_motion && playing {
                        proj.current_pattern_mut().motion.filter[cur_step % 64] = Some(filt_norm);
                    }
                }

                // Space Macro (Reverb + Delay together!)
                let mut space = ((proj.fx.reverb_mix + proj.fx.delay_mix) * 0.5).clamp(0.0, 1.0);
                ui.label("Space");
                if ui
                    .add(egui::Slider::new(&mut space, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    proj.fx.reverb_mix = space;
                    proj.fx.delay_mix = (space * 0.92).min(0.85);
                    if self.rec_motion && playing {
                        proj.current_pattern_mut().motion.space[cur_step % 64] = Some(space);
                    }
                }

                ui.label("Sidechain");
                if ui
                    .add(egui::Slider::new(&mut proj.sidechain_amount, 0.0..=1.0).show_value(false))
                    .changed()
                    && self.rec_motion
                    && playing
                {
                    let sc = proj.sidechain_amount;
                    proj.current_pattern_mut().motion.sidechain[cur_step % 64] = Some(sc);
                }

                ui.label("Warmth");
                ui.add(egui::Slider::new(&mut proj.fx.analog_warmth, 0.0..=1.0).show_value(false));

                ui.label("Glitch");
                if ui
                    .add(egui::Slider::new(&mut proj.fx.glitch, 0.0..=0.85).show_value(false))
                    .changed()
                    && self.rec_motion
                    && playing
                {
                    let gl = proj.fx.glitch;
                    proj.current_pattern_mut().motion.glitch[cur_step % 64] = Some(gl);
                }

                ui.label("Master");
                ui.add(egui::Slider::new(&mut proj.mixer.master_volume, 0.0..=1.15).show_value(false));
            });

            ui.add_space(6.0);

            // Live DJ Performance Buttons
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("DJ LIVE:").strong().size(12.0).color(Color32::from_rgb(255, 168, 52)));
                ui.separator();

                let drop_btn = egui::Button::new(RichText::new("BASS DROP").strong().color(Color32::WHITE))
                    .fill(Color32::from_rgb(208, 66, 98))
                    .min_size(Vec2::new(104.0, 28.0));
                if ui.add(drop_btn).on_hover_text("Fires a key-locked sub bass dive!").clicked() {
                    self.engine.send_live(LiveEvent::TriggerBassDrop);
                }

                let mut drum_cut = self.engine.drum_cut.load(Ordering::Relaxed);
                let cut_btn = egui::Button::new(
                    RichText::new(if drum_cut { "DRUM CUT ON" } else { "DRUM CUT" })
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(if drum_cut {
                    Color32::from_rgb(235, 92, 52)
                } else {
                    Color32::from_rgb(44, 52, 74)
                })
                .min_size(Vec2::new(108.0, 28.0));
                if ui.add(cut_btn).clicked() {
                    drum_cut = !drum_cut;
                    self.engine.drum_cut.store(drum_cut, Ordering::Relaxed);
                }

                let build_resp = ui.add(
                    egui::Button::new(RichText::new("HOLD SNARE BUILD").strong().color(Color32::WHITE))
                        .fill(if self.engine.snare_build.load(Ordering::Relaxed) {
                            Color32::from_rgb(245, 165, 36)
                        } else {
                            Color32::from_rgb(44, 52, 74)
                        })
                        .min_size(Vec2::new(140.0, 28.0)),
                );
                self.engine
                    .snare_build
                    .store(build_resp.is_pointer_button_down_on(), Ordering::Relaxed);

                let stutter_resp = ui.add(
                    egui::Button::new(RichText::new("HOLD STUTTER").strong().color(Color32::WHITE))
                        .fill(if self.stutter_held {
                            Color32::from_rgb(138, 92, 246)
                        } else {
                            Color32::from_rgb(44, 52, 74)
                        })
                        .min_size(Vec2::new(118.0, 28.0)),
                );
                let down = stutter_resp.is_pointer_button_down_on();
                if down != self.stutter_held {
                    self.stutter_held = down;
                    proj.fx.repeat = if down { 0.92 } else { 0.0 };
                }

                if ui
                    .add(
                        egui::Button::new(RichText::new("FILL").strong().color(Color32::WHITE))
                            .fill(Color32::from_rgb(44, 52, 74))
                            .min_size(Vec2::new(68.0, 28.0)),
                    )
                    .clicked()
                {
                    self.engine.send_live(LiveEvent::TriggerFill);
                }
            });
        });
    }

    fn load_sample_dialog(&mut self, ctx: &egui::Context, proj: &mut Project) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio (MP3, WAV, FLAC, OGG)", &["mp3", "wav", "flac", "ogg"])
            .pick_file()
        {
            match load_audio_to_mono_f32(&path, 48_000) {
                Ok(pcm) => {
                    let sname = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Custom Sample")
                        .to_string();
                    let secs = pcm.len() as f32 / 48_000.0;
                    proj.sample_data = SavedSample::from_f32_pcm(sname.clone(), 48_000, &pcm, None);
                    proj.sampler.start = 0.0;
                    proj.sampler.end = 1.0;
                    proj.sampler.speed = 1.0;
                    proj.sampler.transpose = 0;
                    proj.sampler.tune_cents = 0.0;
                    proj.sampler.key_pitch = false; // Play loaded MP3/WAV at exact 1.0x natural speed!
                    proj.sampler.mode = SamplerMode::HiFi;
                    proj.sampler.cutoff = 16_000.0;
                    proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                    proj.sample_generation = proj.sample_generation.wrapping_add(1);
                    self.set_toast(ctx, format!("Loaded {sname} ({secs:.2}s) @ 1.0x speed"));
                }
                Err(e) => self.set_toast(ctx, format!("Sample load error: {e}")),
            }
        }
    }

    fn ui_arrange_party(
        &mut self,
        ui: &mut egui::Ui,
        proj: &mut Project,
        tel: &crate::engine::EngineTelemetry,
        playing: bool,
        accent: Color32,
    ) {
        // 1. PARTY MODE DIRECTOR CARD
        card_frame(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("PARTY MODE — LOCAL VIBE SONG GENERATOR")
                        .strong()
                        .size(14.0)
                        .color(Color32::from_rgb(244, 92, 186)),
                );
                ui.separator();
                if toggle_pill(
                    ui,
                    if proj.party_mode { "PARTY ACTIVE" } else { "START PARTY" },
                    &mut proj.party_mode,
                    Color32::from_rgb(244, 92, 186),
                    "Automatically cycles Intro -> Groove -> Build -> Drop -> Break -> Drop 2 in your Vibe!",
                ) && proj.party_mode
                    && !playing
                {
                    self.engine.playing.store(true, Ordering::Relaxed);
                }
                if ui.button("Next Section").clicked() {
                    self.engine.send_live(LiveEvent::NextPartySection);
                }
                if ui.button("Drop Now").clicked() {
                    self.engine.send_live(LiveEvent::TriggerBassDrop);
                }
                if ui.button("Fresh Party Song").clicked() {
                    let seed = self.next_seed();
                    let next_prog = (proj.progression_idx + 1) % PROGRESSIONS.len();
                    apply_vibe_to_project(proj, proj.vibe, Some(next_prog), seed);
                    proj.party_mode = true;
                    self.engine.playing.store(true, Ordering::Relaxed);
                }
            });

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Party Energy:");
                ui.add(egui::Slider::new(&mut proj.party_energy, 0.15..=1.0).max_decimals(2));
                ui.checkbox(&mut proj.party_auto_evolve, "Auto-Evolve Progressions");
                ui.separator();
                ui.label("Chord Speed:");
                for &bpc in &[1u8, 2, 4] {
                    ui.selectable_value(&mut proj.bars_per_chord, bpc, format!("{bpc} bar{}", if bpc > 1 { "s" } else { "" }));
                }
            });

            if proj.party_mode {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "LIVE: {}  •  Bar {} / {}  •  Active Chord: {}",
                        tel.party_section, tel.party_bar, tel.party_section_bars, tel.active_chord
                    ))
                    .strong()
                    .color(accent),
                );
            }
        });

        ui.add_space(10.0);

        // 2. VISUAL SONG ARRANGER
        card_frame(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("SONG ARRANGER").strong().size(14.0).color(accent));
                ui.separator();
                toggle_pill(
                    ui,
                    if proj.song_mode { "SONG MODE ON" } else { "SONG MODE OFF" },
                    &mut proj.song_mode,
                    Color32::from_rgb(72, 224, 148),
                    "Play through the sections below in order (when Party Mode is off)",
                );
                if ui.button("+ Add Section").clicked() {
                    proj.song_sections.push(SongSection {
                        name: format!("Part {}", proj.song_sections.len() + 1),
                        pattern: proj.active_pattern,
                        bars: 4,
                        bass_drop: false,
                        fill: true,
                        track_mask: 0b111111,
                    });
                }
                if ui.button("Reset Default Arrangement").clicked() {
                    proj.song_sections = crate::model::default_vibe_sections(proj.active_pattern);
                }
            });

            ui.add_space(8.0);

            let mut remove_idx = None;
            egui::Grid::new("song_arranger_grid")
                .striped(true)
                .spacing(Vec2::new(10.0, 6.0))
                .show(ui, |ui| {
                    ui.strong("Section");
                    ui.strong("Bars");
                    ui.strong("Drums");
                    ui.strong("Bass");
                    ui.strong("Chords");
                    ui.strong("Arp");
                    ui.strong("303");
                    ui.strong("Samp");
                    ui.strong("Drop");
                    ui.strong("Fill");
                    ui.strong("");
                    ui.end_row();

                    for (idx, sec) in proj.song_sections.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut sec.name).desired_width(96.0));
                        ui.add(egui::DragValue::new(&mut sec.bars).range(1..=16));
                        for bit in 0..6 {
                            let mut on = (sec.track_mask & (1 << bit)) != 0;
                            if ui.checkbox(&mut on, "").changed() {
                                if on {
                                    sec.track_mask |= 1 << bit;
                                } else {
                                    sec.track_mask &= !(1 << bit);
                                }
                            }
                        }
                        ui.checkbox(&mut sec.bass_drop, "");
                        ui.checkbox(&mut sec.fill, "");
                        if ui.small_button("X").clicked() {
                            remove_idx = Some(idx);
                        }
                        ui.end_row();
                    }
                });
            if let Some(i) = remove_idx {
                if proj.song_sections.len() > 1 {
                    proj.song_sections.remove(i);
                }
            }
        });
    }

    fn ui_sound_lab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, proj: &mut Project, accent: Color32) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("EDIT SOUND:").strong().color(accent));
            for &track in &TrackId::STUDIO_TRACKS {
                let active = self.selected_extra.is_none() && self.selected_track == track;
                if ui.selectable_label(active, track.role_name()).clicked() {
                    self.selected_track = track;
                    self.selected_extra = None;
                }
            }
            for (ei, tr) in proj.extra_tracks.iter().enumerate() {
                let active = self.selected_extra == Some(ei);
                if ui
                    .selectable_label(active, format!("{} #{}", tr.name, ei + 7))
                    .clicked()
                {
                    self.selected_extra = Some(ei);
                }
            }
        });
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            // Left: Full Hardware Synth / Drum Engine Controls
            cols[0].vertical(|ui| {
                card_frame(ui, |ui| {
                    let active_extra = self.selected_extra.filter(|&i| i < proj.extra_tracks.len());
                    let (lab_title, tc) = if let Some(ei) = active_extra {
                        let [cr, cg, cb] = proj.extra_tracks[ei].color;
                        (
                            format!("{} — EXTRA POLY SYNTH", proj.extra_tracks[ei].name.to_uppercase()),
                            Color32::from_rgb(cr, cg, cb),
                        )
                    } else {
                        (
                            format!("{} — FULL HARDWARE SYNTH", self.selected_track.role_name()),
                            track_color(self.selected_track),
                        )
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(lab_title)
                                .strong()
                                .size(13.0)
                                .color(tc),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Hands-free Auto-Repeat / Drone Test Note Toggle!
                            if toggle_pill(
                                ui,
                                if self.test_repeat_on { "TEST ON" } else { "TEST NOTE" },
                                &mut self.test_repeat_on,
                                Color32::from_rgb(72, 224, 148),
                                "Hands-free repeating test phrase or held drone while you tweak knobs!",
                            ) {
                                if self.test_repeat_on {
                                    self.test_repeat_next_t = ctx.input(|i| i.time);
                                    self.test_repeat_step = 0;
                                } else {
                                    self.stop_test_repeat_note();
                                }
                            }
                            for &(mode, lbl) in &[
                                (TestRepeatMode::Drone, "Drone"),
                                (TestRepeatMode::Eighth, "1/8"),
                                (TestRepeatMode::Quarter, "1/4"),
                            ] {
                                if ui.selectable_label(self.test_repeat_mode == mode, lbl).clicked() {
                                    self.test_repeat_mode = mode;
                                    self.stop_test_repeat_note();
                                    self.test_repeat_next_t = ctx.input(|i| i.time);
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);

                    // Quick Synth / Voice Preset Bar right inside Sound Lab!
                    let lab_presets = if active_extra.is_some() {
                        voice_preset_names(TrackId::MoogPad)
                    } else {
                        voice_preset_names(self.selected_track)
                    };
                    if !lab_presets.is_empty() {
                        let cur_p = if let Some(ei) = active_extra {
                            proj.extra_tracks[ei].preset_idx % lab_presets.len()
                        } else {
                            let slot_idx = TrackId::STUDIO_TRACKS
                                .iter()
                                .position(|&t| t == self.selected_track)
                                .unwrap_or(0);
                            proj.voice_presets[slot_idx] % lab_presets.len()
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("PRESETS:")
                                    .strong()
                                    .size(11.0)
                                    .color(Color32::from_rgb(165, 176, 200)),
                            );
                            for (pi, &pname) in lab_presets.iter().enumerate() {
                                let active = cur_p == pi;
                                let pbtn = egui::Button::new(
                                    RichText::new(pname)
                                        .strong()
                                        .size(11.5)
                                        .color(if active {
                                            Color32::from_rgb(12, 14, 20)
                                        } else {
                                            Color32::from_rgb(212, 220, 236)
                                        }),
                                )
                                .fill(if active {
                                    tc
                                } else {
                                    Color32::from_rgb(26, 31, 44)
                                })
                                .stroke(Stroke::new(
                                    1.0_f32,
                                    if active { tc } else { Color32::from_rgb(48, 56, 78) },
                                ));
                                if ui.add(pbtn).clicked() {
                                    if let Some(ei) = active_extra {
                                        apply_extra_track_preset(&mut proj.extra_tracks[ei], pi);
                                        proj.extra_tracks[ei].name = pname.to_string();
                                    } else {
                                        apply_voice_preset(proj, self.selected_track, pi);
                                    }
                                }
                            }
                        });
                        ui.add_space(6.0);
                        ui.separator();
                    }

                    if let Some(ei) = active_extra {
                        moog_controls(ui, &mut proj.extra_tracks[ei].synth, "extra_synth", tc);
                    } else {
                    match self.selected_track {
                        TrackId::Moog => moog_controls(ui, &mut proj.moog, "moog_bass", tc),
                        TrackId::MoogPad => moog_controls(ui, &mut proj.moog_pad, "moog_pad", tc),
                        TrackId::Monologue => monologue_controls(ui, &mut proj.monologue, tc),
                        TrackId::Tb303 => {
                            let a = &mut proj.tb303;
                            ui.label(RichText::new("OSCILLATOR & TUNING").strong().size(11.5).color(tc));
                            ui.horizontal(|ui| {
                                ui.label("Wave:");
                                ui.selectable_value(&mut a.wave, Tb303Wave::Saw, "Saw");
                                ui.selectable_value(&mut a.wave, Tb303Wave::Square, "Square");
                            });
                            ui.add(egui::Slider::new(&mut a.tuning, -12.0..=12.0).suffix(" st").text("Tuning"));
                            ui.separator();
                            ui.label(RichText::new("DIODE LADDER FILTER & ACID").strong().size(11.5).color(tc));
                            ui.add(egui::Slider::new(&mut a.cutoff, 80.0..=6000.0).logarithmic(true).suffix(" Hz").text("Cutoff"));
                            ui.add(egui::Slider::new(&mut a.resonance, 0.0..=0.96).text("Resonance"));
                            ui.add(egui::Slider::new(&mut a.env_mod, 0.0..=1.0).text("Env Mod"));
                            ui.add(egui::Slider::new(&mut a.decay, 0.05..=1.5).suffix(" s").text("Decay"));
                            ui.add(egui::Slider::new(&mut a.accent, 0.0..=1.0).text("Accent"));
                            ui.add(egui::Slider::new(&mut a.overdrive, 0.0..=1.0).text("Overdrive"));
                        }
                        TrackId::Drums => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Kit Preset:");
                                if ui.button("All 808").clicked() {
                                    proj.drums.set_all_model(DrumModel::Tr808);
                                }
                                if ui.button("All 909").clicked() {
                                    proj.drums.set_all_model(DrumModel::Tr909);
                                }
                                if ui.button("Hybrid 808/909").clicked() {
                                    proj.drums.set_hybrid();
                                }
                                if ui.button("Unmute All").clicked() {
                                    for v in &mut proj.drums.voices {
                                        v.mute = false;
                                    }
                                }
                            });
                            ui.add(egui::Slider::new(&mut proj.drums.accent_level, 0.0..=1.0).text("Global Accent"));
                            ui.separator();
                            for (i, v) in proj.drums.voices.iter_mut().enumerate() {
                                let name = DrumVoiceId::from_index(i).map(|d| d.full_name()).unwrap_or("Drum");
                                egui::CollapsingHeader::new(
                                    RichText::new(format!(
                                        "{}  [{}]{}",
                                        name,
                                        match v.model {
                                            DrumModel::Tr808 => "808",
                                            DrumModel::Tr909 => "909",
                                        },
                                        if v.mute { "  (MUTED)" } else { "" }
                                    ))
                                    .strong()
                                    .color(if v.mute { Color32::GRAY } else { tc }),
                                )
                                .default_open(i < 2)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        if ui.small_button("Hit").clicked() {
                                            v.mute = false;
                                            self.engine.send_live(LiveEvent::PlayNote {
                                                track: TrackId::Drums,
                                                note: i as u8,
                                                velocity: 0.92,
                                            });
                                        }
                                        ui.checkbox(&mut v.mute, "Mute");
                                        ui.selectable_value(&mut v.model, DrumModel::Tr808, "808");
                                        ui.selectable_value(&mut v.model, DrumModel::Tr909, "909");
                                    });
                                    ui.add(egui::Slider::new(&mut v.level, 0.0..=1.25).text("Level"));
                                    ui.add(egui::Slider::new(&mut v.tune, -12.0..=12.0).suffix(" st").text("Tune"));
                                    ui.add(egui::Slider::new(&mut v.decay, 0.05..=1.0).text("Decay"));
                                    ui.add(egui::Slider::new(&mut v.tone, 0.0..=1.0).text("Tone"));
                                    ui.add(egui::Slider::new(&mut v.snappy, 0.0..=1.0).text("Snappy"));
                                    ui.add(egui::Slider::new(&mut v.pan, -1.0..=1.0).text("Pan"));
                                });
                            }
                        }
                        TrackId::Sampler => {
                            ui.horizontal(|ui| {
                                ui.label("Sample:");
                                egui::ComboBox::from_id_salt("builtin_samp")
                                    .selected_text(&proj.sample_data.name)
                                    .show_ui(ui, |ui| {
                                        for (i, &sname) in BUILTIN_SAMPLE_NAMES.iter().enumerate() {
                                            if ui
                                                .selectable_label(
                                                    proj.sample_data.builtin_id == Some(i as u8),
                                                    sname,
                                                )
                                                .clicked()
                                            {
                                                proj.sample_data.builtin_id = Some(i as u8);
                                                proj.sample_data.name = sname.to_string();
                                                proj.sample_data.pcm_i16_b64.clear();
                                                proj.sample_generation = proj.sample_generation.wrapping_add(1);
                                            }
                                        }
                                    });
                                if ui.button("Load MP3 / WAV…").clicked() {
                                    self.load_sample_dialog(ctx, proj);
                                }
                                if ui.button("Open Waveform Sampler ->").clicked() {
                                    self.tab = StudioTab::Sampler;
                                }
                            });
                            let s = &mut proj.sampler;
                            ui.horizontal(|ui| {
                                ui.label("Grit Mode:");
                                ui.selectable_value(&mut s.mode, SamplerMode::Sk1LoFi, "8-bit SK-1");
                                ui.selectable_value(&mut s.mode, SamplerMode::Vintage12Bit, "12-bit");
                                ui.selectable_value(&mut s.mode, SamplerMode::HiFi, "Hi-Fi");
                            });
                            ui.horizontal(|ui| {
                                ui.label("Loop:");
                                ui.selectable_value(&mut s.loop_mode, LoopMode::OneShot, "One-Shot");
                                ui.selectable_value(&mut s.loop_mode, LoopMode::Loop, "Loop");
                                ui.selectable_value(&mut s.loop_mode, LoopMode::PingPong, "Ping-Pong");
                                ui.checkbox(&mut s.reverse, "Reverse");
                            });
                            ui.add(egui::Slider::new(&mut s.start, 0.0..=0.98).text("Start"));
                            ui.add(egui::Slider::new(&mut s.end, 0.02..=1.0).text("End"));
                            ui.add(egui::Slider::new(&mut s.transpose, -24..=24).suffix(" st").text("Transpose"));
                            ui.add(egui::Slider::new(&mut s.tune_cents, -100.0..=100.0).suffix(" ¢").text("Fine Tune"));
                            ui.add(egui::Slider::new(&mut s.cutoff, 150.0..=16_000.0).logarithmic(true).text("Cutoff Hz"));
                            ui.add(egui::Slider::new(&mut s.resonance, 0.0..=0.88).text("Resonance"));
                            ui.add(egui::Slider::new(&mut s.attack, 0.001..=0.5).text("Attack"));
                            ui.add(egui::Slider::new(&mut s.release, 0.02..=2.0).text("Release"));
                        }
                        TrackId::MidiOut => {}
                    }
                    }
                });
            });

            // Right: Stereo FX Rack + Full Stereo Mixer
            cols[1].vertical(|ui| {
                card_frame(ui, |ui| {
                    ui.label(RichText::new("STEREO FX & MASTER").strong().size(13.0).color(accent));
                    ui.add_space(4.0);
                    let fx = &mut proj.fx;
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut fx.delay_enabled, "Ping-Pong Delay");
                        egui::ComboBox::from_id_salt("fx_delay_div")
                            .width(72.0)
                            .selected_text(fx.delay_div.name())
                            .show_ui(ui, |ui| {
                                for &d in &DelayDivision::ALL {
                                    ui.selectable_value(&mut fx.delay_div, d, d.name());
                                }
                            });
                    });
                    ui.add(egui::Slider::new(&mut fx.delay_mix, 0.0..=1.0).text("Delay Mix"));
                    ui.add(egui::Slider::new(&mut fx.delay_feedback, 0.0..=0.85).text("Delay Feedback"));
                    ui.add(egui::Slider::new(&mut fx.delay_tone, 0.0..=1.0).text("Delay Tone"));
                    ui.separator();

                    ui.checkbox(&mut fx.reverb_enabled, "Lush Stereo Reverb");
                    ui.add(egui::Slider::new(&mut fx.reverb_mix, 0.0..=1.0).text("Reverb Mix"));
                    ui.add(egui::Slider::new(&mut fx.reverb_size, 0.1..=0.95).text("Reverb Size"));
                    ui.add(egui::Slider::new(&mut fx.reverb_damp, 0.0..=1.0).text("Reverb Damp"));
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut fx.global_filter_enabled, "Master Filter");
                        ui.selectable_value(&mut fx.global_filter_mode, GlobalFilterMode::LowPass, "LP");
                        ui.selectable_value(&mut fx.global_filter_mode, GlobalFilterMode::HighPass, "HP");
                        ui.selectable_value(&mut fx.global_filter_mode, GlobalFilterMode::BandPass, "BP");
                    });
                    ui.add(egui::Slider::new(&mut fx.global_filter_cutoff, 80.0..=18_000.0).logarithmic(true).text("Cutoff Hz"));
                    ui.add(egui::Slider::new(&mut fx.global_filter_resonance, 0.0..=0.85).text("Resonance"));
                    ui.add(egui::Slider::new(&mut fx.analog_warmth, 0.0..=1.0).text("Tape Warmth"));
                });

                ui.add_space(8.0);

                card_frame(ui, |ui| {
                    ui.label(RichText::new("STEREO MIXER & SENDS").strong().size(13.0).color(accent));
                    ui.add_space(4.0);
                    egui::Grid::new("stereo_mix_grid").striped(true).show(ui, |ui| {
                        ui.strong("Track");
                        ui.strong("Pan");
                        ui.strong("Delay");
                        ui.strong("Reverb");
                        ui.end_row();

                        for &track in &TrackId::STUDIO_TRACKS {
                            ui.label(RichText::new(track.role_name()).strong().color(track_color(track)));
                            if track == TrackId::MoogPad {
                                ui.label(RichText::new("Wide").weak());
                                ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_delay_send, 0.0..=1.0).show_value(false));
                                ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_reverb_send, 0.0..=1.0).show_value(false));
                            } else {
                                let ch = &mut proj.mixer.channels[track.index()];
                                ui.add(egui::Slider::new(&mut ch.pan, -1.0..=1.0).show_value(false));
                                ui.add(egui::Slider::new(&mut ch.delay_send, 0.0..=1.0).show_value(false));
                                ui.add(egui::Slider::new(&mut ch.reverb_send, 0.0..=1.0).show_value(false));
                            }
                            ui.end_row();
                        }
                    });
                });
            });
        });
    }
}

fn moog_controls(ui: &mut egui::Ui, m: &mut crate::model::MoogParams, id: &str, tc: Color32) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut m.poly_mode, "Polyphonic");
        ui.add(egui::Slider::new(&mut m.glide, 0.0..=0.6).text("Glide"));
    });

    ui.separator();
    ui.label(RichText::new("OSCILLATORS & CP3 MIXER").strong().size(11.5).color(tc));
    // Osc 1
    ui.horizontal(|ui| {
        ui.label("Osc 1:");
        egui::ComboBox::from_id_salt((id, "w1"))
            .width(86.0)
            .selected_text(m.osc1_wave.name())
            .show_ui(ui, |ui| {
                for &w in &MoogWave::ALL {
                    ui.selectable_value(&mut m.osc1_wave, w, w.name());
                }
            });
        ui.add(egui::Slider::new(&mut m.osc1_level, 0.0..=1.0).text("Lvl"));
    });
    // Osc 2
    ui.horizontal(|ui| {
        ui.label("Osc 2:");
        egui::ComboBox::from_id_salt((id, "w2"))
            .width(86.0)
            .selected_text(m.osc2_wave.name())
            .show_ui(ui, |ui| {
                for &w in &MoogWave::ALL {
                    ui.selectable_value(&mut m.osc2_wave, w, w.name());
                }
            });
        ui.add(egui::DragValue::new(&mut m.osc2_octave).range(-2..=2).prefix("Oct "));
        ui.add(egui::Slider::new(&mut m.osc2_level, 0.0..=1.0).text("Lvl"));
    });
    ui.add(egui::Slider::new(&mut m.osc2_detune, -50.0..=50.0).suffix(" ¢").text("Osc 2 Detune"));
    // Osc 3
    ui.horizontal(|ui| {
        ui.label("Osc 3:");
        egui::ComboBox::from_id_salt((id, "w3"))
            .width(86.0)
            .selected_text(m.osc3_wave.name())
            .show_ui(ui, |ui| {
                for &w in &MoogWave::ALL {
                    ui.selectable_value(&mut m.osc3_wave, w, w.name());
                }
            });
        ui.add(egui::DragValue::new(&mut m.osc3_octave).range(-2..=2).prefix("Oct "));
        ui.add(egui::Slider::new(&mut m.osc3_level, 0.0..=1.0).text("Lvl"));
    });
    ui.add(egui::Slider::new(&mut m.osc3_detune, -50.0..=50.0).suffix(" ¢").text("Osc 3 Detune"));
    ui.add(egui::Slider::new(&mut m.noise_level, 0.0..=0.6).text("Noise"));
    ui.add(egui::Slider::new(&mut m.drive, 0.0..=1.0).text("CP3 Drive"));

    ui.separator();
    ui.label(RichText::new("24dB LADDER FILTER & FILTER ENV").strong().size(11.5).color(tc));
    ui.add(egui::Slider::new(&mut m.cutoff, 30.0..=16_000.0).logarithmic(true).suffix(" Hz").text("Cutoff"));
    ui.add(egui::Slider::new(&mut m.resonance, 0.0..=0.94).text("Resonance"));
    ui.add(egui::Slider::new(&mut m.env_amount, -1.0..=1.0).text("Env Amount"));
    ui.add(egui::Slider::new(&mut m.key_track, 0.0..=1.0).text("Key Track"));
    ui.add(egui::Slider::new(&mut m.filt_attack, 0.002..=1.5).suffix(" s").text("Filt Attack"));
    ui.add(egui::Slider::new(&mut m.filt_decay, 0.02..=2.5).suffix(" s").text("Filt Decay"));
    ui.add(egui::Slider::new(&mut m.filt_sustain, 0.0..=1.0).text("Filt Sustain"));
    ui.add(egui::Slider::new(&mut m.filt_release, 0.02..=2.5).suffix(" s").text("Filt Release"));

    ui.separator();
    ui.label(RichText::new("AMP ENVELOPE & LFO").strong().size(11.5).color(tc));
    ui.add(egui::Slider::new(&mut m.amp_attack, 0.002..=1.5).suffix(" s").text("Amp Attack"));
    ui.add(egui::Slider::new(&mut m.amp_decay, 0.02..=2.5).suffix(" s").text("Amp Decay"));
    ui.add(egui::Slider::new(&mut m.amp_sustain, 0.0..=1.0).text("Amp Sustain"));
    ui.add(egui::Slider::new(&mut m.amp_release, 0.02..=3.0).suffix(" s").text("Amp Release"));
    ui.add(egui::Slider::new(&mut m.lfo_rate, 0.1..=18.0).suffix(" Hz").text("LFO Rate"));
    ui.add(egui::Slider::new(&mut m.lfo_to_pitch, 0.0..=1.0).text("LFO -> Pitch"));
    ui.add(egui::Slider::new(&mut m.lfo_to_filter, 0.0..=1.0).text("LFO -> Filter"));
}

fn monologue_controls(ui: &mut egui::Ui, m: &mut crate::model::MonologueParams, tc: Color32) {
    ui.label(RichText::new("VCO 1 & VCO 2 (SYNC / RING)").strong().size(11.5).color(tc));
    ui.horizontal(|ui| {
        ui.label("VCO 1:");
        for &w in &MonologueOscWave::ALL {
            ui.selectable_value(&mut m.vco1_wave, w, w.name());
        }
    });
    ui.add(egui::Slider::new(&mut m.vco1_shape, 0.0..=1.0).text("VCO 1 Shape"));
    ui.add(egui::Slider::new(&mut m.vco1_level, 0.0..=1.0).text("VCO 1 Level"));

    ui.horizontal(|ui| {
        ui.label("VCO 2:");
        for &w in &MonologueOscWave::ALL {
            ui.selectable_value(&mut m.vco2_wave, w, w.name());
        }
        ui.add(egui::DragValue::new(&mut m.vco2_octave).range(-2..=2).prefix("Oct "));
    });
    ui.add(egui::Slider::new(&mut m.vco2_pitch, -12.0..=12.0).suffix(" st").text("VCO 2 Pitch"));
    ui.add(egui::Slider::new(&mut m.vco2_shape, 0.0..=1.0).text("VCO 2 Shape"));
    ui.add(egui::Slider::new(&mut m.vco2_level, 0.0..=1.0).text("VCO 2 Level"));
    ui.horizontal(|ui| {
        ui.label("VCO 2 Mod:");
        for &mm in &Osc2ModMode::ALL {
            ui.selectable_value(&mut m.mod_mode, mm, mm.name());
        }
    });
    ui.add(egui::Slider::new(&mut m.glide, 0.0..=0.5).text("Glide"));

    ui.separator();
    ui.label(RichText::new("12dB FILTER, DRIVE & ENVELOPE").strong().size(11.5).color(tc));
    ui.add(egui::Slider::new(&mut m.cutoff, 30.0..=16_000.0).logarithmic(true).suffix(" Hz").text("Cutoff"));
    ui.add(egui::Slider::new(&mut m.resonance, 0.0..=0.94).text("Resonance"));
    ui.add(egui::Slider::new(&mut m.drive, 0.0..=1.0).text("Drive"));
    ui.horizontal(|ui| {
        ui.label("EG Target:");
        for &t in &EgTarget::ALL {
            ui.selectable_value(&mut m.eg_target, t, t.name());
        }
    });
    ui.add(egui::Slider::new(&mut m.eg_attack, 0.002..=1.5).suffix(" s").text("EG Attack"));
    ui.add(egui::Slider::new(&mut m.eg_decay, 0.02..=2.5).suffix(" s").text("EG Decay"));
    ui.add(egui::Slider::new(&mut m.eg_int, -1.0..=1.0).text("EG Intensity"));
    ui.checkbox(&mut m.amp_sustain, "Amp Gate Sustain (off = punchy AD)");

    ui.separator();
    ui.label(RichText::new("LFO MODULATION").strong().size(11.5).color(tc));
    ui.horizontal(|ui| {
        ui.label("Wave:");
        for &w in &LfoWave::ALL {
            ui.selectable_value(&mut m.lfo_wave, w, w.name());
        }
        ui.separator();
        ui.label("Mode:");
        for &lm in &LfoMode::ALL {
            ui.selectable_value(&mut m.lfo_mode, lm, lm.name());
        }
    });
    ui.horizontal(|ui| {
        ui.label("Target:");
        for &lt in &LfoTarget::ALL {
            ui.selectable_value(&mut m.lfo_target, lt, lt.name());
        }
    });
    ui.add(egui::Slider::new(&mut m.lfo_rate, 0.0..=1.0).text("LFO Rate"));
    ui.add(egui::Slider::new(&mut m.lfo_int, -1.0..=1.0).text("LFO Intensity"));
}

fn track_mixer_mut(p: &mut Project, track: TrackId) -> (&mut f32, &mut bool, &mut bool) {
    if track == TrackId::MoogPad {
        (
            &mut p.mixer.moog_pad_volume,
            &mut p.mixer.moog_pad_mute,
            &mut p.mixer.moog_pad_solo,
        )
    } else {
        let ch = &mut p.mixer.channels[track.index()];
        (&mut ch.volume, &mut ch.mute, &mut ch.solo)
    }
}

fn card_frame(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(Color32::from_rgb(18, 22, 32))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(12.0))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(34, 41, 58)))
        .show(ui, add_contents);
}

fn toggle_pill(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut bool,
    active_color: Color32,
    hover: &str,
) -> bool {
    let btn = egui::Button::new(
        RichText::new(label)
            .strong()
            .size(12.0)
            .color(if *value {
                Color32::from_rgb(12, 14, 20)
            } else {
                Color32::from_rgb(190, 200, 220)
            }),
    )
    .fill(if *value {
        active_color
    } else {
        Color32::from_rgb(28, 34, 48)
    })
    .stroke(Stroke::new(
        1.0_f32,
        if *value {
            active_color
        } else {
            Color32::from_rgb(48, 56, 78)
        },
    ));
    let clicked = ui.add(btn).on_hover_text(hover).clicked();
    if clicked {
        *value = !*value;
    }
    clicked
}

fn vibe_color(vibe: Vibe) -> Color32 {
    let [r, g, b] = vibe.accent_rgb();
    Color32::from_rgb(r, g, b)
}

fn track_color(track: TrackId) -> Color32 {
    match track {
        TrackId::Drums => Color32::from_rgb(255, 118, 86),
        TrackId::Moog => Color32::from_rgb(72, 198, 255),
        TrackId::MoogPad => Color32::from_rgb(192, 132, 252),
        TrackId::Monologue => Color32::from_rgb(74, 232, 178),
        TrackId::Tb303 => Color32::from_rgb(192, 255, 72),
        TrackId::Sampler => Color32::from_rgb(255, 198, 72),
        TrackId::MidiOut => Color32::GRAY,
    }
}

fn draw_scope(painter: &egui::Painter, rect: egui::Rect, samples: &[f32], color: Color32) {
    painter.rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(10, 13, 20));
    painter.rect_stroke(
        rect,
        Rounding::same(6.0),
        Stroke::new(1.0_f32, Color32::from_rgb(36, 44, 62)),
    );
    if samples.len() < 2 {
        return;
    }
    let mid_y = rect.center().y;
    let half_h = rect.height() * 0.44;
    let step_x = rect.width() / (samples.len() - 1) as f32;
    let points: Vec<egui::Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, &s)| egui::pos2(rect.left() + i as f32 * step_x, mid_y - s.clamp(-1.0, 1.0) * half_h))
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5_f32, color)));
}

fn draw_vu_meters(
    painter: &egui::Painter,
    rect: egui::Rect,
    peak_l: f32,
    peak_r: f32,
    sidechain_duck: f32,
) {
    painter.rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(10, 13, 20));
    painter.rect_stroke(rect, Rounding::same(4.0), Stroke::new(1.0_f32, Color32::from_rgb(36, 44, 62)));
    let pad = 3.0;
    let inner_h = (rect.height() - pad * 2.0).max(4.0);
    let col_w = ((rect.width() - pad * 4.0) / 3.0).max(4.0);
    let bars = [
        (peak_l.clamp(0.0, 1.0), Color32::from_rgb(72, 224, 148)),
        (peak_r.clamp(0.0, 1.0), Color32::from_rgb(72, 224, 148)),
        (sidechain_duck.clamp(0.0, 1.0), Color32::from_rgb(245, 168, 52)),
    ];
    for (i, (val, base_col)) in bars.iter().enumerate() {
        let x0 = rect.left() + pad + i as f32 * (col_w + pad);
        let h = inner_h * val;
        let col = if i < 2 && *val > 0.90 {
            Color32::from_rgb(235, 72, 86)
        } else if i < 2 && *val > 0.72 {
            Color32::from_rgb(245, 202, 72)
        } else {
            *base_col
        };
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(x0, rect.bottom() - pad - h),
            egui::pos2(x0 + col_w, rect.bottom() - pad),
        );
        painter.rect_filled(bar_rect, Rounding::same(2.0), col);
    }
}



