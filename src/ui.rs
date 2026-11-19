use crate::{
    dsp::sampler::{
        generate_builtin_sample, load_audio_to_mono_f32, normalize_peak,
        trim_silence_and_normalize, BUILTIN_SAMPLE_NAMES,
    },
    engine::{export_wav, AudioEngine, EngineTelemetry, LiveEvent},
    model::{
        apply_vibe_to_project, apply_voice_preset, build_auto_chord_pads, generate_auto_beat,
        generate_melodic_tracks, load_project, note_name, root_name, save_project,
        voice_preset_names, AutoChordPad, ChordType, DelayDivision, DrumModel, DrumVoiceId,
        GlobalFilterMode, LoopMode, MonologueOscWave, MoogWave, Project, SamplerMode, SavedSample,
        SongSection, Tb303Wave, TrackId, Vibe, NOTE_NAMES, PROGRESSIONS,
    },
};
use eframe::egui::{self, Color32, Key, RichText, Rounding, Sense, Stroke, Vec2};
use std::sync::atomic::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StudioTab {
    Studio,
    Sampler,
    ArrangeParty,
    SoundLab,
}

pub struct FunKeySynthApp {
    engine: AudioEngine,
    tab: StudioTab,
    selected_track: TrackId,
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
    // Sampler visual waveform state
    sampler_peaks: Vec<(f32, f32)>,
    sampler_duration_secs: f32,
    cached_sample_gen: u64,
    sampler_drag_handle: Option<u8>, // 0 = start, 1 = end
    sampler_audition_note: Option<u8>,
}

impl FunKeySynthApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_studio_theme(&cc.egui_ctx);
        let project = Project::default();
        let engine = AudioEngine::start(project);
        Self {
            engine,
            tab: StudioTab::Studio,
            selected_track: TrackId::Drums,
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
            sampler_peaks: Vec::new(),
            sampler_duration_secs: 0.0,
            cached_sample_gen: u64::MAX,
            sampler_drag_handle: None,
            sampler_audition_note: None,
        }
    }

    fn sync_waveform_cache(&mut self, proj: &Project) {
        if self.cached_sample_gen == proj.sample_generation && !self.sampler_peaks.is_empty() {
            return;
        }
        self.cached_sample_gen = proj.sample_generation;
        let sr = proj.sample_data.sample_rate.max(8_000);
        let pcm = proj.sample_data.decode_f32_pcm().unwrap_or_else(|| {
            let id = proj.sample_data.builtin_id.unwrap_or(3);
            generate_builtin_sample(id, sr)
        });
        self.sampler_duration_secs = pcm.len() as f32 / sr as f32;
        self.sampler_peaks = compute_waveform_peaks(&pcm, 320);
    }

    fn set_toast(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time) + 3.2));
    }

    fn next_seed(&mut self) -> u32 {
        self.roll_counter = self.roll_counter.wrapping_add(1);
        0x51A7_1337u32.wrapping_mul(self.roll_counter).wrapping_add(0x9E37_79B9)
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
        let accent = vibe_color(proj.vibe);

        self.handle_keyboard(ctx, &mut proj, &pads, playing, cur_step);

        if playing && self.follow_playhead {
            self.bar_page = (cur_step / 16) % 4;
        }

        // ==================== TOP VIBE BAR ====================
        egui::TopBottomPanel::top("top_vibe_bar")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(16, 19, 27))
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(36, 42, 58))),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    // Brand pill
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
                        if proj.party_mode { "★ PARTY ON" } else { "★ PARTY MODE" },
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

                    // Right-aligned Views + Save/Load/Export
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Export WAV").on_hover_text("Render 16 bars to stereo WAV").clicked() {
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
                                        proj = loaded;
                                        self.set_toast(ctx, "Project loaded!");
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
                    .fill(Color32::from_rgb(14, 17, 24))
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(34, 40, 56))),
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
                            ui.checkbox(&mut self.record_live, "● Rec")
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

                    // Mini Oscilloscope + Status
                    ui.vertical(|ui| {
                        let scope_rect = ui.allocate_exact_size(Vec2::new(160.0, 42.0), Sense::hover()).0;
                        draw_scope(ui.painter(), scope_rect, &self.scope_buf, accent);
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
                    .fill(Color32::from_rgb(12, 14, 20))
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                    StudioTab::Studio => self.ui_studio(ui, &mut proj, playing, cur_step, accent),
                    StudioTab::Sampler => self.ui_sampler(ui, ctx, &mut proj, &telemetry, accent),
                    StudioTab::ArrangeParty => {
                        self.ui_arrange_party(ui, &mut proj, &telemetry, playing, accent)
                    }
                    StudioTab::SoundLab => self.ui_sound_lab(ui, ctx, &mut proj, accent),
                });
            });

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
                    RichText::new("🎲 Fresh Groove")
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
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("STUDIO TRACKS").strong().size(12.5).color(accent));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("Click a track to edit its steps").size(11.0).weak());
                        });
                    });
                    ui.add_space(4.0);

                    for (row_idx, &track) in TrackId::STUDIO_TRACKS.iter().enumerate() {
                        let selected = self.selected_track == track;
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
                                    // Select button with color badge
                                    let sel_btn = egui::Button::new(
                                        RichText::new(track.role_name())
                                            .strong()
                                            .size(12.0)
                                            .color(if selected { tc } else { Color32::from_rgb(220, 226, 240) }),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .min_size(Vec2::new(82.0, 22.0));
                                    if ui.add(sel_btn).clicked() {
                                        self.selected_track = track;
                                    }

                                    // Instant Voice Preset Picker
                                    let presets = voice_preset_names(track);
                                    let cur_preset = proj.voice_presets[row_idx] % presets.len().max(1);
                                    egui::ComboBox::from_id_salt(("vp", row_idx))
                                        .width(108.0)
                                        .selected_text(presets[cur_preset])
                                        .show_ui(ui, |ui| {
                                            for (pi, &pname) in presets.iter().enumerate() {
                                                if ui.selectable_label(cur_preset == pi, pname).clicked() {
                                                    apply_voice_preset(proj, track, pi);
                                                }
                                            }
                                        });

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
                                    if ui.small_button("🎲").on_hover_text("Generate fresh pattern for this track").clicked() {
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
                                    if ui.small_button("✕").on_hover_text("Clear track pattern").clicked() {
                                        let pat = proj.current_pattern_mut();
                                        if track == TrackId::Drums {
                                            for lane in &mut pat.drums.lanes {
                                                lane.clear();
                                            }
                                        } else if let Some(seq) = pat.melodic_mut(track) {
                                            seq.clear();
                                        }
                                    }
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
            });

            // Right: Clean 16-Step Sequencer for Selected Track
            cols[1].vertical(|ui| {
                card_frame(ui, |ui| {
                    let tc = track_color(self.selected_track);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!("{} SEQUENCER", self.selected_track.role_name()))
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
                            // Pattern bank A..D quick switch
                            for b in (0..4).rev() {
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

                    if self.selected_track == TrackId::Drums {
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
                            if ui.small_button("🎲 New Beat").clicked() {
                                let seed = self.next_seed();
                                proj.party_seed = seed;
                                let drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                                proj.current_pattern_mut().drums = drums;
                            }
                            if ui.small_button("⚡ Fill Now").clicked() {
                                self.engine.send_live(LiveEvent::TriggerFill);
                            }
                            ui.label(RichText::new("Right-click step = 2x/3x/4x ratchet").size(10.5).weak());
                        });
                        ui.add_space(4.0);

                        // 8-lane compact Drum Beat Grid (16 steps per bar page)
                        let pat = proj.current_pattern_mut();
                        let cell_w = ((ui.available_width() - 56.0 - 15.0 * 3.0) / 16.0).clamp(18.0, 34.0);
                        let cell_h = 22.0;
                        for lane_idx in 0..8 {
                            let voice = DrumVoiceId::from_index(lane_idx).unwrap();
                            let lane = &mut pat.drums.lanes[lane_idx];
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                ui.add_sized(
                                    Vec2::new(48.0, cell_h),
                                    egui::Label::new(RichText::new(voice.short_name()).size(10.5).strong()),
                                );
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
                                        if is_play {
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
                        let pat = proj.current_pattern_mut();
                        if let Some(seq) = pat.melodic_mut(target) {
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
                }

                ui.label("Sidechain");
                ui.add(egui::Slider::new(&mut proj.sidechain_amount, 0.0..=1.0).show_value(false));

                ui.label("Warmth");
                ui.add(egui::Slider::new(&mut proj.fx.analog_warmth, 0.0..=1.0).show_value(false));

                ui.label("Glitch");
                ui.add(egui::Slider::new(&mut proj.fx.glitch, 0.0..=0.85).show_value(false));

                ui.label("Master");
                ui.add(egui::Slider::new(&mut proj.mixer.master_volume, 0.0..=1.15).show_value(false));
            });

            ui.add_space(6.0);

            // Live DJ Performance Buttons
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("DJ LIVE:").strong().size(12.0).color(Color32::from_rgb(255, 168, 52)));
                ui.separator();

                let drop_btn = egui::Button::new(RichText::new("⚡ BASS DROP").strong().color(Color32::WHITE))
                    .fill(Color32::from_rgb(208, 66, 98))
                    .min_size(Vec2::new(112.0, 28.0));
                if ui.add(drop_btn).on_hover_text("Fires a key-locked sub bass dive!").clicked() {
                    self.engine.send_live(LiveEvent::TriggerBassDrop);
                }

                let mut drum_cut = self.engine.drum_cut.load(Ordering::Relaxed);
                let cut_btn = egui::Button::new(
                    RichText::new(if drum_cut { "🛑 DRUM CUT ON" } else { "🛑 DRUM CUT" })
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(if drum_cut {
                    Color32::from_rgb(235, 92, 52)
                } else {
                    Color32::from_rgb(44, 52, 74)
                })
                .min_size(Vec2::new(118.0, 28.0));
                if ui.add(cut_btn).clicked() {
                    drum_cut = !drum_cut;
                    self.engine.drum_cut.store(drum_cut, Ordering::Relaxed);
                }

                let build_resp = ui.add(
                    egui::Button::new(RichText::new("🌪 HOLD SNARE BUILD").strong().color(Color32::WHITE))
                        .fill(if self.engine.snare_build.load(Ordering::Relaxed) {
                            Color32::from_rgb(245, 165, 36)
                        } else {
                            Color32::from_rgb(44, 52, 74)
                        })
                        .min_size(Vec2::new(148.0, 28.0)),
                );
                self.engine
                    .snare_build
                    .store(build_resp.is_pointer_button_down_on(), Ordering::Relaxed);

                let stutter_resp = ui.add(
                    egui::Button::new(RichText::new("🔁 HOLD STUTTER").strong().color(Color32::WHITE))
                        .fill(if self.stutter_held {
                            Color32::from_rgb(138, 92, 246)
                        } else {
                            Color32::from_rgb(44, 52, 74)
                        })
                        .min_size(Vec2::new(128.0, 28.0)),
                );
                let down = stutter_resp.is_pointer_button_down_on();
                if down != self.stutter_held {
                    self.stutter_held = down;
                    proj.fx.repeat = if down { 0.92 } else { 0.0 };
                }

                if ui
                    .add(
                        egui::Button::new(RichText::new("🥁 FILL").strong().color(Color32::WHITE))
                            .fill(Color32::from_rgb(44, 52, 74))
                            .min_size(Vec2::new(78.0, 28.0)),
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
                    proj.sampler.mode = SamplerMode::HiFi;
                    proj.sampler.cutoff = 14_000.0;
                    proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                    proj.sample_generation = proj.sample_generation.wrapping_add(1);
                    self.set_toast(ctx, format!("Loaded {sname} ({secs:.2}s)"));
                }
                Err(e) => self.set_toast(ctx, format!("Sample load error: {e}")),
            }
        }
    }

    fn ui_sampler(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        proj: &mut Project,
        tel: &EngineTelemetry,
        accent: Color32,
    ) {
        self.sync_waveform_cache(proj);
        let samp_color = track_color(TrackId::Sampler);

        // 1. SAMPLE LOADER & ACTIONS BAR
        card_frame(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("PRO SAMPLER WORKBENCH")
                        .strong()
                        .size(14.0)
                        .color(samp_color),
                );
                ui.separator();

                let load_btn = egui::Button::new(
                    RichText::new("📂 Load MP3 / WAV / FLAC / OGG")
                        .strong()
                        .color(Color32::from_rgb(12, 14, 20)),
                )
                .fill(samp_color);
                if ui.add(load_btn).clicked() {
                    self.load_sample_dialog(ctx, proj);
                }

                egui::ComboBox::from_id_salt("sampler_tab_builtin")
                    .width(165.0)
                    .selected_text(&proj.sample_data.name)
                    .show_ui(ui, |ui| {
                        for (i, &sname) in BUILTIN_SAMPLE_NAMES.iter().enumerate() {
                            if ui
                                .selectable_label(proj.sample_data.builtin_id == Some(i as u8), sname)
                                .clicked()
                            {
                                proj.sample_data.builtin_id = Some(i as u8);
                                proj.sample_data.name = sname.to_string();
                                proj.sample_data.pcm_i16_b64.clear();
                                proj.sampler.start = 0.0;
                                proj.sampler.end = 1.0;
                                proj.sample_generation = proj.sample_generation.wrapping_add(1);
                            }
                        }
                    });

                if ui
                    .button("✂ Auto-Trim")
                    .on_hover_text("Trim leading/trailing silence & normalize peak")
                    .clicked()
                {
                    let sr = proj.sample_data.sample_rate.max(8_000);
                    let pcm = proj.sample_data.decode_f32_pcm().unwrap_or_else(|| {
                        generate_builtin_sample(proj.sample_data.builtin_id.unwrap_or(3), sr)
                    });
                    let trimmed = trim_silence_and_normalize(&pcm, sr);
                    proj.sample_data =
                        SavedSample::from_f32_pcm(proj.sample_data.name.clone(), sr, &trimmed, None);
                    proj.sampler.start = 0.0;
                    proj.sampler.end = 1.0;
                    proj.sample_generation = proj.sample_generation.wrapping_add(1);
                    self.set_toast(ctx, "Trimmed silence & normalized!");
                }

                if ui
                    .button("📊 Normalize")
                    .on_hover_text("Normalize sample volume to full scale")
                    .clicked()
                {
                    let sr = proj.sample_data.sample_rate.max(8_000);
                    let pcm = proj.sample_data.decode_f32_pcm().unwrap_or_else(|| {
                        generate_builtin_sample(proj.sample_data.builtin_id.unwrap_or(3), sr)
                    });
                    let norm = normalize_peak(&pcm);
                    proj.sample_data =
                        SavedSample::from_f32_pcm(proj.sample_data.name.clone(), sr, &norm, None);
                    proj.sample_generation = proj.sample_generation.wrapping_add(1);
                    self.set_toast(ctx, "Normalized sample!");
                }

                // Audition button (hold to play at root note!)
                let root = proj.sampler.root_note;
                let aud_resp = ui.add(
                    egui::Button::new(
                        RichText::new("▶ HOLD AUDITION")
                            .strong()
                            .color(Color32::from_rgb(12, 14, 20)),
                    )
                    .fill(Color32::from_rgb(72, 224, 148)),
                );
                if aud_resp.is_pointer_button_down_on() && self.sampler_audition_note != Some(root) {
                    proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                    self.engine.send_live(LiveEvent::PlayNote {
                        track: TrackId::Sampler,
                        note: root,
                        velocity: 0.92,
                    });
                    self.sampler_audition_note = Some(root);
                } else if !aud_resp.is_pointer_button_down_on()
                    && self.sampler_audition_note == Some(root)
                {
                    self.engine.send_live(LiveEvent::ReleaseNote {
                        track: TrackId::Sampler,
                        note: root,
                    });
                    self.sampler_audition_note = None;
                }

                // Track Volume + Mute right in the workbench
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ch = &mut proj.mixer.channels[TrackId::Sampler.index()];
                    ui.add(egui::Slider::new(&mut ch.volume, 0.0..=1.15).text("Vol").max_decimals(2));
                    let m_btn = egui::Button::new(
                        RichText::new(if ch.mute { "MUTED" } else { "ACTIVE" })
                            .strong()
                            .size(11.0)
                            .color(Color32::WHITE),
                    )
                    .fill(if ch.mute {
                        Color32::from_rgb(200, 62, 74)
                    } else {
                        Color32::from_rgb(46, 132, 92)
                    });
                    if ui.add(m_btn).clicked() {
                        ch.mute = !ch.mute;
                    }
                });
            });

            // Duration & Slice Readout
            let dur = self.sampler_duration_secs;
            let start_s = proj.sampler.start * dur;
            let end_s = proj.sampler.end * dur;
            ui.label(
                RichText::new(format!(
                    "Sample: {}  •  Length: {:.2}s  •  Active Slice: {:.2}s – {:.2}s ({:.2}s)  •  Drag green START / coral END handles directly on the waveform!",
                    proj.sample_data.name,
                    dur,
                    start_s,
                    end_s,
                    (end_s - start_s).max(0.0)
                ))
                .size(11.5)
                .weak(),
            );
        });

        ui.add_space(8.0);

        // 2. INTERACTIVE VISUAL WAVEFORM DISPLAY (Draggable Start/End + Live Playhead!)
        card_frame(ui, |ui| {
            let wf_w = ui.available_width().max(300.0);
            let wf_h = 150.0;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(wf_w, wf_h), Sense::click_and_drag());

            let start_x = rect.left() + proj.sampler.start.clamp(0.0, 0.99) * rect.width();
            let end_x = rect.left() + proj.sampler.end.clamp(0.01, 1.0) * rect.width();

            // Drag interaction for Start & End handles
            if resp.drag_started() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let dist_start = (pos.x - start_x).abs();
                    let dist_end = (pos.x - end_x).abs();
                    self.sampler_drag_handle = if dist_start <= dist_end { Some(0) } else { Some(1) };
                }
            }
            if resp.dragged() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let norm = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    match self.sampler_drag_handle {
                        Some(0) => {
                            proj.sampler.start = norm.min(proj.sampler.end - 0.01).max(0.0);
                        }
                        Some(1) => {
                            proj.sampler.end = norm.max(proj.sampler.start + 0.01).min(1.0);
                        }
                        _ => {}
                    }
                }
            }
            if resp.drag_stopped() {
                self.sampler_drag_handle = None;
            }

            let painter = ui.painter();
            painter.rect_filled(rect, Rounding::same(8.0), Color32::from_rgb(10, 13, 20));

            // Center line
            let mid_y = rect.center().y;
            painter.line_segment(
                [egui::pos2(rect.left(), mid_y), egui::pos2(rect.right(), mid_y)],
                Stroke::new(1.0_f32, Color32::from_rgb(28, 34, 48)),
            );

            // Draw waveform bars
            let n = self.sampler_peaks.len();
            if n > 0 {
                let half_h = rect.height() * 0.44;
                let bar_w = rect.width() / n as f32;
                for (i, &(mn, mx)) in self.sampler_peaks.iter().enumerate() {
                    let x = rect.left() + (i as f32 + 0.5) * bar_w;
                    let norm_x = i as f32 / n as f32;
                    let in_slice = norm_x >= proj.sampler.start && norm_x <= proj.sampler.end;
                    let col = if in_slice {
                        samp_color
                    } else {
                        Color32::from_rgb(66, 74, 92)
                    };
                    let y0 = mid_y - mx.clamp(-1.0, 1.0) * half_h;
                    let y1 = mid_y - mn.clamp(-1.0, 1.0) * half_h;
                    painter.line_segment(
                        [egui::pos2(x, y0), egui::pos2(x, y1.max(y0 + 1.0))],
                        Stroke::new(bar_w.max(1.4), col),
                    );
                }
            }

            // Dimmed out-of-slice regions
            let dim = Color32::from_rgba_unmultiplied(8, 10, 16, 145);
            if start_x > rect.left() {
                painter.rect_filled(
                    egui::Rect::from_min_max(rect.left_top(), egui::pos2(start_x, rect.bottom())),
                    Rounding::ZERO,
                    dim,
                );
            }
            if end_x < rect.right() {
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(end_x, rect.top()), rect.right_bottom()),
                    Rounding::ZERO,
                    dim,
                );
            }

            // Start Handle (Green)
            let start_col = Color32::from_rgb(72, 224, 148);
            painter.line_segment(
                [egui::pos2(start_x, rect.top()), egui::pos2(start_x, rect.bottom())],
                Stroke::new(2.2_f32, start_col),
            );
            let start_tag = egui::Rect::from_min_size(egui::pos2(start_x, rect.top()), Vec2::new(42.0, 16.0));
            painter.rect_filled(start_tag, Rounding::same(3.0), start_col);
            painter.text(
                start_tag.center(),
                egui::Align2::CENTER_CENTER,
                "START",
                egui::FontId::proportional(9.5),
                Color32::from_rgb(10, 14, 20),
            );

            // End Handle (Coral)
            let end_col = Color32::from_rgb(245, 98, 112);
            painter.line_segment(
                [egui::pos2(end_x, rect.top()), egui::pos2(end_x, rect.bottom())],
                Stroke::new(2.2_f32, end_col),
            );
            let end_tag = egui::Rect::from_min_size(
                egui::pos2((end_x - 34.0).max(rect.left()), rect.top()),
                Vec2::new(34.0, 16.0),
            );
            painter.rect_filled(end_tag, Rounding::same(3.0), end_col);
            painter.text(
                end_tag.center(),
                egui::Align2::CENTER_CENTER,
                "END",
                egui::FontId::proportional(9.5),
                Color32::WHITE,
            );

            // Live Playhead!
            if let Some(ph) = tel.sampler_playhead {
                let px = rect.left() + ph.clamp(0.0, 1.0) * rect.width();
                painter.line_segment(
                    [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
                    Stroke::new(2.0_f32, Color32::WHITE),
                );
            }

            painter.rect_stroke(
                rect,
                Rounding::same(8.0),
                Stroke::new(1.0_f32, Color32::from_rgb(44, 52, 72)),
            );

            ui.add_space(6.0);

            // Playable Chromatic / Interval Sample Pads!
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("TRIGGER PADS:").strong().size(11.5).color(samp_color));
                const OFFSETS: [(&str, i8); 8] = [
                    ("-12", -12),
                    ("-7", -7),
                    ("-5", -5),
                    ("ROOT", 0),
                    ("+3", 3),
                    ("+5", 5),
                    ("+7", 7),
                    ("+12", 12),
                ];
                for &(lbl, off) in &OFFSETS {
                    let midi = (proj.sampler.root_note as i16 + off as i16).clamp(24, 96) as u8;
                    let held = self.sampler_audition_note == Some(midi);
                    let btn = egui::Button::new(
                        RichText::new(format!("{lbl}\n{}", note_name(midi)))
                            .strong()
                            .size(11.0)
                            .color(if held { Color32::from_rgb(12, 14, 20) } else { Color32::WHITE }),
                    )
                    .fill(if held {
                        samp_color
                    } else if off == 0 {
                        Color32::from_rgb(48, 56, 78)
                    } else {
                        Color32::from_rgb(28, 34, 48)
                    })
                    .min_size(Vec2::new(66.0, 36.0));
                    let resp = ui.add(btn);
                    if resp.is_pointer_button_down_on() && self.sampler_audition_note != Some(midi) {
                        proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                        self.engine.send_live(LiveEvent::PlayNote {
                            track: TrackId::Sampler,
                            note: midi,
                            velocity: 0.90,
                        });
                        self.sampler_audition_note = Some(midi);
                    } else if !resp.is_pointer_button_down_on()
                        && self.sampler_audition_note == Some(midi)
                    {
                        self.engine.send_live(LiveEvent::ReleaseNote {
                            track: TrackId::Sampler,
                            note: midi,
                        });
                        self.sampler_audition_note = None;
                    }
                }
            });
        });

        ui.add_space(8.0);

        // 3. REAL SAMPLER CONTROLS (4 clean cards in 2x2 grid)
        let s = &mut proj.sampler;
        ui.columns(2, |cols| {
            // Card A: Slice & Playback
            cols[0].vertical(|ui| {
                card_frame(ui, |ui| {
                    ui.label(RichText::new("SLICE & PLAYBACK").strong().size(12.5).color(samp_color));
                    ui.add_space(4.0);
                    let mut st_pct = s.start * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut st_pct, 0.0..=99.0).suffix("%").text("Start"))
                        .changed()
                    {
                        s.start = (st_pct / 100.0).min(s.end - 0.01).max(0.0);
                    }
                    let mut end_pct = s.end * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut end_pct, 1.0..=100.0).suffix("%").text("End"))
                        .changed()
                    {
                        s.end = (end_pct / 100.0).max(s.start + 0.01).min(1.0);
                    }
                    ui.horizontal(|ui| {
                        ui.label("Loop:");
                        ui.selectable_value(&mut s.loop_mode, LoopMode::OneShot, "1-Shot");
                        ui.selectable_value(&mut s.loop_mode, LoopMode::Loop, "Loop");
                        ui.selectable_value(&mut s.loop_mode, LoopMode::PingPong, "Ping-Pong");
                        ui.checkbox(&mut s.reverse, "Reverse");
                        if ui.small_button("Reset Slice").clicked() {
                            s.start = 0.0;
                            s.end = 1.0;
                        }
                    });
                });

                ui.add_space(8.0);

                // Card C: Filter
                card_frame(ui, |ui| {
                    ui.label(RichText::new("SAMPLER FILTER").strong().size(12.5).color(accent));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        ui.selectable_value(&mut s.filter_mode, GlobalFilterMode::LowPass, "Low-Pass");
                        ui.selectable_value(&mut s.filter_mode, GlobalFilterMode::BandPass, "Band-Pass");
                        ui.selectable_value(&mut s.filter_mode, GlobalFilterMode::HighPass, "High-Pass");
                    });
                    ui.add(
                        egui::Slider::new(&mut s.cutoff, 120.0..=18_000.0)
                            .logarithmic(true)
                            .suffix(" Hz")
                            .text("Cutoff"),
                    );
                    ui.add(egui::Slider::new(&mut s.resonance, 0.0..=0.88).text("Resonance"));
                });
            });

            // Card B: Pitch & Lo-Fi Grit
            cols[1].vertical(|ui| {
                card_frame(ui, |ui| {
                    ui.label(RichText::new("PITCH & LO-FI ENGINE").strong().size(12.5).color(samp_color));
                    ui.add_space(4.0);
                    ui.add(egui::Slider::new(&mut s.transpose, -24..=24).suffix(" st").text("Transpose"));
                    ui.add(egui::Slider::new(&mut s.tune_cents, -100.0..=100.0).suffix(" ¢").text("Fine Tune"));
                    ui.horizontal(|ui| {
                        ui.label("Root Note:");
                        if ui.small_button("-").clicked() {
                            s.root_note = s.root_note.saturating_sub(1).max(24);
                        }
                        ui.strong(note_name(s.root_note));
                        if ui.small_button("+").clicked() {
                            s.root_note = (s.root_note + 1).min(96);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Character:");
                        ui.selectable_value(&mut s.mode, SamplerMode::Sk1LoFi, "SK-1 8-Bit");
                        ui.selectable_value(&mut s.mode, SamplerMode::Vintage12Bit, "12-Bit");
                        ui.selectable_value(&mut s.mode, SamplerMode::HiFi, "Hi-Fi");
                    });
                    ui.add(egui::Slider::new(&mut s.bit_crush, 0.0..=1.0).text("Extra Crunch"));
                });

                ui.add_space(8.0);

                // Card D: Amp Envelope (ADSR)
                card_frame(ui, |ui| {
                    ui.label(RichText::new("AMP ENVELOPE (ADSR)").strong().size(12.5).color(accent));
                    ui.add_space(4.0);
                    ui.add(egui::Slider::new(&mut s.attack, 0.001..=1.2).suffix(" s").text("Attack"));
                    ui.add(egui::Slider::new(&mut s.decay, 0.02..=2.5).suffix(" s").text("Decay"));
                    ui.add(egui::Slider::new(&mut s.sustain, 0.0..=1.0).text("Sustain"));
                    ui.add(egui::Slider::new(&mut s.release, 0.01..=3.0).suffix(" s").text("Release"));
                });
            });
        });
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
                    RichText::new("★ PARTY MODE — LOCAL VIBE SONG GENERATOR")
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
                if ui.button("⏭ Next Section").clicked() {
                    self.engine.send_live(LiveEvent::NextPartySection);
                }
                if ui.button("⚡ Drop Now").clicked() {
                    self.engine.send_live(LiveEvent::TriggerBassDrop);
                }
                if ui.button("🎲 Fresh Party Song").clicked() {
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
                    ui.strong("Tex");
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
                        if ui.small_button("✕").clicked() {
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
                if ui
                    .selectable_label(self.selected_track == track, track.role_name())
                    .clicked()
                {
                    self.selected_track = track;
                }
            }
        });
        ui.add_space(8.0);

        ui.columns(2, |cols| {
            // Left: Selected Track DSP Controls
            cols[0].vertical(|ui| {
                card_frame(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{} ENGINE", self.selected_track.role_name()))
                            .strong()
                            .size(13.0)
                            .color(track_color(self.selected_track)),
                    );
                    ui.add_space(6.0);

                    match self.selected_track {
                        TrackId::Moog => moog_controls(ui, &mut proj.moog, "moog_bass"),
                        TrackId::MoogPad => moog_controls(ui, &mut proj.moog_pad, "moog_pad"),
                        TrackId::Monologue => {
                            let m = &mut proj.monologue;
                            egui::ComboBox::from_id_salt("mono_vco1")
                                .selected_text(format!("VCO 1: {:?}", m.vco1_wave))
                                .show_ui(ui, |ui| {
                                    for w in [MonologueOscWave::Saw, MonologueOscWave::Triangle, MonologueOscWave::Square] {
                                        ui.selectable_value(&mut m.vco1_wave, w, format!("{w:?}"));
                                    }
                                });
                            ui.add(egui::Slider::new(&mut m.cutoff, 80.0..=14_000.0).logarithmic(true).text("Cutoff Hz"));
                            ui.add(egui::Slider::new(&mut m.resonance, 0.0..=0.92).text("Resonance"));
                            ui.add(egui::Slider::new(&mut m.eg_int, -1.0..=1.0).text("Filter Env"));
                            ui.add(egui::Slider::new(&mut m.eg_decay, 0.02..=2.0).text("Decay"));
                            ui.add(egui::Slider::new(&mut m.drive, 0.0..=1.0).text("Drive"));
                            ui.add(egui::Slider::new(&mut m.glide, 0.0..=0.5).text("Glide"));
                        }
                        TrackId::Tb303 => {
                            let a = &mut proj.tb303;
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut a.wave, Tb303Wave::Saw, "Saw");
                                ui.selectable_value(&mut a.wave, Tb303Wave::Square, "Square");
                            });
                            ui.add(egui::Slider::new(&mut a.cutoff, 80.0..=6000.0).logarithmic(true).text("Cutoff Hz"));
                            ui.add(egui::Slider::new(&mut a.resonance, 0.0..=0.96).text("Resonance"));
                            ui.add(egui::Slider::new(&mut a.env_mod, 0.0..=1.0).text("Env Mod"));
                            ui.add(egui::Slider::new(&mut a.decay, 0.05..=1.5).text("Decay"));
                            ui.add(egui::Slider::new(&mut a.accent, 0.0..=1.0).text("Accent"));
                            ui.add(egui::Slider::new(&mut a.overdrive, 0.0..=1.0).text("Overdrive"));
                        }
                        TrackId::Drums => {
                            ui.horizontal(|ui| {
                                ui.label("Kit Model:");
                                if ui.button("808").clicked() {
                                    proj.drums.set_all_model(DrumModel::Tr808);
                                }
                                if ui.button("909").clicked() {
                                    proj.drums.set_all_model(DrumModel::Tr909);
                                }
                                if ui.button("Hybrid 808/909").clicked() {
                                    proj.drums.set_hybrid();
                                }
                            });
                            ui.add_space(4.0);
                            for (i, v) in proj.drums.voices.iter_mut().enumerate() {
                                let name = DrumVoiceId::from_index(i).map(|d| d.short_name()).unwrap_or("DRM");
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(name).strong().size(11.0));
                                    ui.add(egui::Slider::new(&mut v.level, 0.0..=1.2).text("Lvl").max_decimals(2));
                                    ui.add(egui::Slider::new(&mut v.decay, 0.05..=1.0).text("Dec").max_decimals(2));
                                    ui.add(egui::Slider::new(&mut v.tune, -12.0..=12.0).text("Tune").max_decimals(1));
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
                                if ui.button("Open Waveform Sampler →").clicked() {
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
                            ui.add(egui::Slider::new(&mut s.cutoff, 150.0..=16_000.0).logarithmic(true).text("Cutoff Hz"));
                            ui.add(egui::Slider::new(&mut s.resonance, 0.0..=0.88).text("Resonance"));
                            ui.add(egui::Slider::new(&mut s.attack, 0.001..=0.5).text("Attack"));
                            ui.add(egui::Slider::new(&mut s.release, 0.02..=2.0).text("Release"));
                        }
                        TrackId::MidiOut => {}
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

fn moog_controls(ui: &mut egui::Ui, m: &mut crate::model::MoogParams, id: &str) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut m.poly_mode, "Polyphonic");
        egui::ComboBox::from_id_salt((id, "w1"))
            .selected_text(format!("Osc 1: {}", m.osc1_wave.name()))
            .show_ui(ui, |ui| {
                for &w in &MoogWave::ALL {
                    ui.selectable_value(&mut m.osc1_wave, w, w.name());
                }
            });
    });
    ui.add(egui::Slider::new(&mut m.cutoff, 60.0..=16_000.0).logarithmic(true).text("Cutoff Hz"));
    ui.add(egui::Slider::new(&mut m.resonance, 0.0..=0.92).text("Resonance"));
    ui.add(egui::Slider::new(&mut m.env_amount, -1.0..=1.0).text("Filter Env"));
    ui.add(egui::Slider::new(&mut m.drive, 0.0..=1.0).text("Drive"));
    ui.add(egui::Slider::new(&mut m.osc2_detune, -30.0..=30.0).text("Detune"));
    ui.add(egui::Slider::new(&mut m.amp_attack, 0.002..=1.2).text("Attack"));
    ui.add(egui::Slider::new(&mut m.amp_decay, 0.05..=2.0).text("Decay"));
    ui.add(egui::Slider::new(&mut m.amp_sustain, 0.0..=1.0).text("Sustain"));
    ui.add(egui::Slider::new(&mut m.amp_release, 0.02..=2.5).text("Release"));
    ui.add(egui::Slider::new(&mut m.glide, 0.0..=0.5).text("Glide"));
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

fn compute_waveform_peaks(pcm: &[f32], bins: usize) -> Vec<(f32, f32)> {
    if pcm.is_empty() || bins == 0 {
        return Vec::new();
    }
    let bins = bins.min(pcm.len());
    let mut out = Vec::with_capacity(bins);
    for i in 0..bins {
        let start = (i * pcm.len()) / bins;
        let end = (((i + 1) * pcm.len()) / bins).max(start + 1).min(pcm.len());
        let mut mn = 0.0_f32;
        let mut mx = 0.0_f32;
        for &s in &pcm[start..end] {
            if s < mn {
                mn = s;
            }
            if s > mx {
                mx = s;
            }
        }
        out.push((mn, mx));
    }
    out
}

fn apply_studio_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(14, 17, 24);
    visuals.panel_fill = Color32::from_rgb(12, 14, 20);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(20, 24, 34);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(26, 32, 46);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(36, 44, 64);
    visuals.widgets.active.bg_fill = Color32::from_rgb(48, 58, 84);
    visuals.selection.bg_fill = Color32::from_rgb(64, 152, 236);
    ctx.set_visuals(visuals);
}
