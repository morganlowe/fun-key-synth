use crate::{
    dsp::sampler::{
        generate_builtin_sample, load_audio_to_mono_f32, normalize_peak,
        trim_silence_and_normalize, BUILTIN_SAMPLE_NAMES,
    },
    engine::{render_resample_mono, AudioEngine, EngineTelemetry, LiveEvent},
    mic::MicRecorder,
    model::{GlobalFilterMode, LoopMode, Project, SamplerMode, SavedSample, TrackId},
};
use eframe::egui::{self, Color32, RichText, Rounding, Sense, Stroke, Vec2};
use std::sync::atomic::Ordering;

pub struct VolcaUiState {
    pub selected_part: usize,
    pub selected_step: (usize, usize), // (part_idx, step_idx)
    pub held_pad: Option<usize>,
    pub peaks: Vec<(f32, f32)>,
    pub duration_secs: f32,
    pub cached_gen: (usize, u64),
    pub drag_handle: Option<u8>,
}

impl Default for VolcaUiState {
    fn default() -> Self {
        Self {
            selected_part: 0,
            selected_step: (0, 0),
            held_pad: None,
            peaks: Vec::new(),
            duration_secs: 0.0,
            cached_gen: (usize::MAX, u64::MAX),
            drag_handle: None,
        }
    }
}

impl VolcaUiState {
    pub fn sync_waveform(&mut self, proj: &Project) {
        let idx = self.selected_part.min(proj.sample_parts.len().saturating_sub(1));
        let Some(part) = proj.sample_parts.get(idx) else {
            return;
        };
        let key = (idx, part.generation);
        if self.cached_gen == key && !self.peaks.is_empty() {
            return;
        }
        self.cached_gen = key;
        let sr = part.sample.sample_rate.max(8_000);
        let pcm = part.sample.decode_f32_pcm().unwrap_or_else(|| {
            let id = part.sample.builtin_id.unwrap_or(3);
            generate_builtin_sample(id, sr)
        });
        self.duration_secs = pcm.len() as f32 / sr as f32;
        self.peaks = compute_peaks(&pcm, 300);
    }

    pub fn load_file_to_part(&mut self, proj: &mut Project, part_idx: usize) -> Option<String> {
        let path = rfd::FileDialog::new()
            .add_filter("Audio (MP3, WAV, FLAC, OGG)", &["mp3", "wav", "flac", "ogg"])
            .pick_file()?;
        match load_audio_to_mono_f32(&path, 48_000) {
            Ok(pcm) => {
                let sname = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Sample")
                    .to_string();
                let short = sname.trim_end_matches(".mp3").trim_end_matches(".wav").to_string();
                let secs = pcm.len() as f32 / 48_000.0;
                if let Some(part) = proj.sample_parts.get_mut(part_idx) {
                    part.name = short.chars().take(16).collect();
                    part.sample = SavedSample::from_f32_pcm(sname.clone(), 48_000, &pcm, None);
                    part.start = 0.0;
                    part.end = 1.0;
                    part.speed = 1.0;
                    part.transpose = 0;
                    part.tune_cents = 0.0;
                    part.reverse = false;
                    part.mode = SamplerMode::HiFi;
                    part.cutoff = 16_000.0;
                    part.mute = false;
                    part.generation = part.generation.wrapping_add(1);
                }
                proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                Some(format!("Loaded {sname} ({secs:.2}s) → Pad {}", part_idx + 1))
            }
            Err(e) => Some(format!("Sample load error: {e}")),
        }
    }

    pub fn load_and_autochop(&mut self, proj: &mut Project) -> Option<String> {
        let path = rfd::FileDialog::new()
            .add_filter("Audio (MP3, WAV, FLAC, OGG)", &["mp3", "wav", "flac", "ogg"])
            .pick_file()?;
        match load_audio_to_mono_f32(&path, 48_000) {
            Ok(pcm) => {
                let sname = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Chop")
                    .to_string();
                let base = sname
                    .trim_end_matches(".mp3")
                    .trim_end_matches(".wav")
                    .chars()
                    .take(10)
                    .collect::<String>();
                let secs = pcm.len() as f32 / 48_000.0;
                let saved = SavedSample::from_f32_pcm(sname.clone(), 48_000, &pcm, None);
                for (i, part) in proj.sample_parts.iter_mut().take(8).enumerate() {
                    part.name = format!("{} #{}", base, i + 1);
                    part.sample = saved.clone();
                    part.start = i as f32 / 8.0;
                    part.end = (i + 1) as f32 / 8.0;
                    part.speed = 1.0;
                    part.transpose = 0;
                    part.tune_cents = 0.0;
                    part.reverse = false;
                    part.loop_mode = LoopMode::OneShot;
                    part.mode = SamplerMode::HiFi;
                    part.cutoff = 16_000.0;
                    part.choke_group = 1; // MPC / Volca chop choke group!
                    part.mute = false;
                    part.generation = part.generation.wrapping_add(1);
                }
                proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                Some(format!("Auto-chopped {sname} ({secs:.2}s) across all 8 Pads!"))
            }
            Err(e) => Some(format!("Auto-chop error: {e}")),
        }
    }
}

const PAD_COLORS: [Color32; 8] = [
    Color32::from_rgb(255, 118, 86),
    Color32::from_rgb(255, 178, 64),
    Color32::from_rgb(245, 222, 72),
    Color32::from_rgb(86, 228, 152),
    Color32::from_rgb(72, 202, 255),
    Color32::from_rgb(122, 148, 255),
    Color32::from_rgb(198, 126, 255),
    Color32::from_rgb(255, 108, 182),
];

pub fn render_volca_sampler(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    state: &mut VolcaUiState,
    engine: &AudioEngine,
    mic: &mut MicRecorder,
    proj: &mut Project,
    tel: &EngineTelemetry,
    playing: bool,
    cur_step: usize,
    bar_page: &mut usize,
    follow_playhead: &mut bool,
    record_live: bool,
    rec_motion: &mut bool,
    test_repeat_on: &mut bool,
    test_repeat_next_t: &mut f64,
    accent: Color32,
) -> Option<String> {
    let mut toast_msg: Option<String> = None;
    let sel = state.selected_part.min(proj.sample_parts.len().saturating_sub(1));
    state.selected_part = sel;
    engine.selected_sample_part.store(sel, Ordering::Relaxed);
    state.sync_waveform(proj);

    let samp_color = Color32::from_rgb(255, 198, 72);

    // 1. TOP VOLCA PERFORMANCE BAR + ANALOG ISOLATOR
    card(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new("VOLCA 8-PART PERFORMANCE SAMPLER")
                    .strong()
                    .size(14.0)
                    .color(samp_color),
            );
            ui.separator();

            if ui
                .add(
                    egui::Button::new(
                        RichText::new("CHOP MP3/WAV -> 8 PADS")
                            .strong()
                            .color(Color32::from_rgb(12, 14, 20)),
                    )
                    .fill(samp_color),
                )
                .on_hover_text("Load an MP3/WAV/FLAC/OGG and slice it across Pads 1–8 with Choke Group 1")
                .clicked()
            {
                toast_msg = state.load_and_autochop(proj);
            }

            if ui
                .button(format!("LOAD PAD {}", sel + 1))
                .on_hover_text("Load an MP3/WAV/FLAC/OGG onto the currently selected Pad")
                .clicked()
            {
                toast_msg = state.load_file_to_part(proj, sel);
            }

            // Live Mic Recording directly onto selected Pad!
            let mic_lbl = if mic.recording {
                format!("STOP MIC ({:.1}s)", mic.recorded_secs())
            } else {
                format!("MIC REC -> P{}", sel + 1)
            };
            let mic_btn = egui::Button::new(
                RichText::new(mic_lbl)
                    .strong()
                    .size(11.5)
                    .color(Color32::WHITE),
            )
            .fill(if mic.recording {
                Color32::from_rgb(225, 58, 72)
            } else {
                Color32::from_rgb(48, 34, 46)
            });
            if ui
                .add(mic_btn)
                .on_hover_text("Record live microphone audio onto the selected Pad (auto-trims & normalizes)")
                .clicked()
            {
                if mic.recording {
                    match mic.stop_and_finish() {
                        Ok(pcm) => {
                            let secs = pcm.len() as f32 / 48_000.0;
                            let p = &mut proj.sample_parts[sel];
                            p.name = format!("Mic P{}", sel + 1);
                            p.sample = SavedSample::from_f32_pcm(p.name.clone(), 48_000, &pcm, None);
                            p.start = 0.0;
                            p.end = 1.0;
                            p.speed = 1.0;
                            p.mode = SamplerMode::HiFi;
                            p.cutoff = 16_000.0;
                            p.mute = false;
                            p.generation = p.generation.wrapping_add(1);
                            proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                            toast_msg = Some(format!("Recorded Mic ({secs:.2}s) -> Pad {}", sel + 1));
                        }
                        Err(e) => toast_msg = Some(e),
                    }
                } else if let Err(e) = mic.start() {
                    toast_msg = Some(e);
                }
            }

            // Internal Resample 2 Bars of live synth/beat straight onto selected Pad!
            if ui
                .button(format!("RESAMPLE 2B -> P{}", sel + 1))
                .on_hover_text("Resample 2 bars of your current track onto this Pad so you can chop/reverse/pitch it")
                .clicked()
            {
                let pcm = render_resample_mono(proj, 2);
                let secs = pcm.len() as f32 / 48_000.0;
                let p = &mut proj.sample_parts[sel];
                p.name = format!("Resamp P{}", sel + 1);
                p.sample = SavedSample::from_f32_pcm(p.name.clone(), 48_000, &pcm, None);
                p.start = 0.0;
                p.end = 1.0;
                p.speed = 1.0;
                p.mode = SamplerMode::HiFi;
                p.cutoff = 16_000.0;
                p.mute = false;
                p.generation = p.generation.wrapping_add(1);
                proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                toast_msg = Some(format!("Resampled 2 bars ({secs:.2}s) -> Pad {}", sel + 1));
            }

            // Auto-Test Toggle
            let test_btn = egui::Button::new(
                RichText::new(if *test_repeat_on { "AUTO-TEST ON" } else { "AUTO-TEST" })
                    .strong()
                    .size(11.5)
                    .color(if *test_repeat_on {
                        Color32::from_rgb(12, 14, 20)
                    } else {
                        Color32::from_rgb(200, 210, 228)
                    }),
            )
            .fill(if *test_repeat_on {
                Color32::from_rgb(72, 224, 148)
            } else {
                Color32::from_rgb(30, 36, 52)
            });
            if ui.add(test_btn).clicked() {
                *test_repeat_on = !*test_repeat_on;
                if *test_repeat_on {
                    proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                    *test_repeat_next_t = ctx.input(|i| i.time);
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ch = &mut proj.mixer.channels[TrackId::Sampler.index()];
                ui.add(egui::Slider::new(&mut ch.volume, 0.0..=1.15).text("Bus Vol").max_decimals(2));
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

        ui.add_space(6.0);

        // Volca Analog Isolator (Bass & Treble DJ Kill/Boost EQ) + Motion Rec!
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("ANALOG ISOLATOR:").strong().size(11.5).color(accent));
            if ui
                .add(
                    egui::Slider::new(&mut proj.sampler_iso_bass, 0.0..=2.0)
                        .text("Bass (Kill / Boost)")
                        .max_decimals(2),
                )
                .changed()
                && *rec_motion
                && playing
            {
                let v = proj.sampler_iso_bass;
                proj.current_pattern_mut().motion.iso_bass[cur_step % 64] = Some(v);
            }
            if ui
                .add(
                    egui::Slider::new(&mut proj.sampler_iso_treble, 0.0..=2.0)
                        .text("Treble (Kill / Boost)")
                        .max_decimals(2),
                )
                .changed()
                && *rec_motion
                && playing
            {
                let v = proj.sampler_iso_treble;
                proj.current_pattern_mut().motion.iso_treble[cur_step % 64] = Some(v);
            }
            if ui.small_button("Kill Bass").clicked() {
                proj.sampler_iso_bass = 0.0;
                if *rec_motion && playing {
                    proj.current_pattern_mut().motion.iso_bass[cur_step % 64] = Some(0.0);
                }
            }
            if ui.small_button("Kill Highs").clicked() {
                proj.sampler_iso_treble = 0.0;
                if *rec_motion && playing {
                    proj.current_pattern_mut().motion.iso_treble[cur_step % 64] = Some(0.0);
                }
            }
            if ui.small_button("Flat EQ").clicked() {
                proj.sampler_iso_bass = 1.0;
                proj.sampler_iso_treble = 1.0;
            }
            ui.separator();
            let rm_btn = egui::Button::new(
                RichText::new(if *rec_motion { "REC MOTION ON" } else { "REC MOTION" })
                    .strong()
                    .size(11.0)
                    .color(Color32::WHITE),
            )
            .fill(if *rec_motion {
                Color32::from_rgb(225, 58, 72)
            } else {
                Color32::from_rgb(36, 42, 58)
            });
            if ui.add(rm_btn).clicked() {
                *rec_motion = !*rec_motion;
            }
        });
    });

    ui.add_space(8.0);

    // 2. 8 LARGE PERFORMANCE PADS (Click/Hold to Trigger & Select, Right-Click for Pad Menu!)
    card(ui, |ui| {
        ui.horizontal(|ui| {
            let avail = ui.available_width();
            let pad_w = ((avail - 7.0 * 6.0) / 8.0).clamp(72.0, 160.0);
            let pad_h = 66.0;
            ui.spacing_mut().item_spacing.x = 6.0;

            let mut solo_pad: Option<usize> = None;
            let mut unmute_all = false;
            let mut load_for_pad: Option<usize> = None;
            let active_pat = proj.active_pattern;

            for i in 0..8 {
                let pc = PAD_COLORS[i];
                let is_sel = sel == i;
                let is_held = state.held_pad == Some(i);
                let is_muted = proj.sample_parts[i].mute;
                let part_name = proj.sample_parts[i].name.clone();
                let is_rev = proj.sample_parts[i].reverse;
                let is_loop = proj.sample_parts[i].loop_mode != LoopMode::OneShot;
                let choke = proj.sample_parts[i].choke_group;

                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(pad_w, pad_h), Sense::click_and_drag());

                if resp.is_pointer_button_down_on() && state.held_pad != Some(i) {
                    state.selected_part = i;
                    state.held_pad = Some(i);
                    proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                    engine.send_live(LiveEvent::TriggerSamplePart {
                        part: i,
                        velocity: 0.92,
                    });
                    if record_live && playing {
                        let lane = &mut proj.patterns[active_pat].sample_lanes[i];
                        let s_idx = cur_step % lane.step_count.as_usize();
                        lane.steps[s_idx].active = true;
                        lane.steps[s_idx].velocity = 0.90;
                    }
                } else if !resp.is_pointer_button_down_on() && state.held_pad == Some(i) {
                    if proj.sample_parts[i].loop_mode != LoopMode::OneShot {
                        engine.send_live(LiveEvent::ReleaseSamplePart { part: i });
                    }
                    state.held_pad = None;
                }

                resp.context_menu(|ui| {
                    ui.label(RichText::new(format!("PAD {} — {}", i + 1, part_name)).strong().color(pc));
                    ui.separator();
                    if ui.button("Load MP3 / WAV...").clicked() {
                        load_for_pad = Some(i);
                        ui.close_menu();
                    }
                    if ui.button(if proj.sample_parts[i].mute { "Unmute Pad" } else { "Mute Pad" }).clicked() {
                        proj.sample_parts[i].mute = !proj.sample_parts[i].mute;
                        ui.close_menu();
                    }
                    if ui.button("Solo Pad").clicked() {
                        solo_pad = Some(i);
                        ui.close_menu();
                    }
                    if ui.button("All Pads On").clicked() {
                        unmute_all = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.checkbox(&mut proj.sample_parts[i].reverse, "Reverse");
                    if ui.button("Clear Pad Steps").clicked() {
                        proj.patterns[active_pat].sample_lanes[i].clear();
                        ui.close_menu();
                    }
                });

                let bg = if is_held {
                    pc
                } else if is_muted {
                    Color32::from_rgb(26, 20, 26)
                } else if is_sel {
                    Color32::from_rgb(34, 42, 62)
                } else {
                    Color32::from_rgb(22, 27, 38)
                };

                let painter = ui.painter();
                painter.rect_filled(rect, Rounding::same(8.0), bg);
                painter.rect_stroke(
                    rect,
                    Rounding::same(8.0),
                    Stroke::new(
                        if is_sel || is_held { 2.0_f32 } else { 1.0_f32 },
                        if is_held {
                            Color32::WHITE
                        } else if is_sel {
                            pc
                        } else {
                            pc.gamma_multiply(0.35)
                        },
                    ),
                );
                let fg = if is_held {
                    Color32::from_rgb(12, 14, 20)
                } else if is_muted {
                    Color32::from_rgb(110, 115, 132)
                } else {
                    Color32::from_rgb(235, 240, 250)
                };

                painter.text(
                    rect.left_top() + Vec2::new(7.0, 5.0),
                    egui::Align2::LEFT_TOP,
                    format!("P{}", i + 1),
                    egui::FontId::proportional(11.0),
                    if is_held { Color32::from_rgb(14, 18, 24) } else { pc },
                );

                let mut badges = String::new();
                if is_muted {
                    badges.push_str("M ");
                }
                if is_rev {
                    badges.push_str("R ");
                }
                if is_loop {
                    badges.push_str("L ");
                }
                if choke > 0 {
                    badges.push_str(&format!("C{choke}"));
                }
                if !badges.is_empty() {
                    painter.text(
                        rect.right_top() + Vec2::new(-6.0, 5.0),
                        egui::Align2::RIGHT_TOP,
                        badges.trim(),
                        egui::FontId::proportional(9.5),
                        if is_held { Color32::from_rgb(14, 18, 24) } else { Color32::from_rgb(175, 188, 212) },
                    );
                }

                painter.text(
                    rect.center() + Vec2::new(0.0, 7.0),
                    egui::Align2::CENTER_CENTER,
                    part_name,
                    egui::FontId::proportional(11.5),
                    fg,
                );
            }

            if let Some(s_i) = solo_pad {
                for (i, p) in proj.sample_parts.iter_mut().enumerate() {
                    p.mute = i != s_i;
                }
            }
            if unmute_all {
                for p in &mut proj.sample_parts {
                    p.mute = false;
                }
            }
            if let Some(li) = load_for_pad {
                toast_msg = state.load_file_to_part(proj, li);
            }
        });
    });

    ui.add_space(8.0);

    // 3. LEFT: 8-LANE VOLCA STEP SEQUENCER  |  RIGHT: SELECTED PAD WAVEFORM & TUNING
    ui.columns(2, |cols| {
        // LEFT: 8-Lane Performance Step Sequencer
        cols[0].vertical(|ui| {
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("8-PART SAMPLE SEQUENCER").strong().size(12.5).color(samp_color));
                    ui.separator();
                    for b in 0..4 {
                        if ui.selectable_label(*bar_page == b, format!("Bar {}", b + 1)).clicked() {
                            *bar_page = b;
                            *follow_playhead = false;
                        }
                    }
                    ui.checkbox(follow_playhead, "Follow");
                });
                ui.label(
                    RichText::new("Click P1-P8 = Mute  |  Click step = Toggle & Select  |  Right-click step = Reverse / Pitch / Speed / Filter / Ratchet")
                        .size(10.5)
                        .weak(),
                );
                ui.add_space(4.0);

                let page_start = (*bar_page) * 16;
                let play_step = if playing { Some(cur_step % 64) } else { None };
                let active_pat = proj.active_pattern;
                let cell_w = ((ui.available_width() - 62.0 - 15.0 * 3.0) / 16.0).clamp(16.0, 34.0);
                let cell_h = 22.0;

                for part_idx in 0..8 {
                    let pc = PAD_COLORS[part_idx];
                    let is_muted = proj.sample_parts[part_idx].mute;
                    let is_sel = sel == part_idx;
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
                            .color(if is_muted {
                                Color32::from_rgb(115, 122, 140)
                            } else if is_sel {
                                pc
                            } else {
                                Color32::from_rgb(225, 232, 245)
                            }),
                        )
                        .fill(if is_muted {
                            Color32::from_rgb(38, 22, 28)
                        } else if is_sel {
                            Color32::from_rgb(34, 42, 62)
                        } else {
                            Color32::from_rgb(24, 29, 42)
                        })
                        .min_size(Vec2::new(50.0, cell_h));

                        let lresp = ui.add(lbl);
                        if lresp.clicked() {
                            proj.sample_parts[part_idx].mute = !proj.sample_parts[part_idx].mute;
                            state.selected_part = part_idx;
                        }
                        lresp.context_menu(|ui| {
                            let lane = &mut proj.patterns[active_pat].sample_lanes[part_idx];
                            ui.label(RichText::new(format!("P{} — {}", part_idx + 1, proj.sample_parts[part_idx].name)).strong().color(pc));
                            ui.separator();
                            if ui.button("Trigger").clicked() {
                                engine.send_live(LiveEvent::TriggerSamplePart { part: part_idx, velocity: 0.92 });
                            }
                            if ui.button("Fill 4x4 (1,5,9,13)").clicked() {
                                for c in [0, 4, 8, 12] {
                                    lane.steps[(page_start + c) % 64].active = true;
                                }
                                ui.close_menu();
                            }
                            if ui.button("Fill Offbeats").clicked() {
                                for c in [2, 6, 10, 14] {
                                    lane.steps[(page_start + c) % 64].active = true;
                                }
                                ui.close_menu();
                            }
                            if ui.button("Clear Lane").clicked() {
                                lane.clear();
                                ui.close_menu();
                            }
                        });

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
                            let is_sel_step = state.selected_step == (part_idx, idx);
                            let fill = if st.active {
                                if is_muted {
                                    Color32::from_rgb(70, 56, 60)
                                } else if is_play {
                                    Color32::WHITE
                                } else if st.reverse {
                                    Color32::from_rgb(64, 218, 255)
                                } else if st.ratchet > 1 {
                                    Color32::from_rgb(255, 118, 192)
                                } else {
                                    pc.gamma_multiply(0.84)
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
                                state.selected_step = (part_idx, idx);
                                if st.active {
                                    st.velocity = 0.88;
                                    proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                                }
                            }
                            resp.context_menu(|ui| {
                                ui.set_min_width(210.0);
                                state.selected_step = (part_idx, idx);
                                ui.label(
                                    RichText::new(format!("STEP FX — P{} Step {}", part_idx + 1, idx + 1))
                                        .strong()
                                        .color(pc),
                                );
                                ui.separator();
                                ui.checkbox(&mut st.active, "Step Active");
                                if ui.checkbox(&mut st.reverse, "Reverse This Step (REV)").changed() && st.reverse {
                                    st.active = true;
                                }
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
                                if ui.add(egui::Slider::new(&mut st.speed_mult, 0.25..=2.0).suffix("x").max_decimals(2).text("Speed")).changed() {
                                    st.active = true;
                                }
                                let mut c_on = st.cutoff_lock.is_some();
                                let mut c_val = st.cutoff_lock.unwrap_or(1.0);
                                ui.horizontal(|ui| {
                                    if ui.checkbox(&mut c_on, "Filter").changed() {
                                        st.cutoff_lock = if c_on { Some(c_val) } else { None };
                                    }
                                    if c_on && ui.add(egui::Slider::new(&mut c_val, 0.0..=1.0).show_value(false)).changed() {
                                        st.cutoff_lock = Some(c_val);
                                        st.active = true;
                                    }
                                });
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
                                if ui.small_button("Reset Step FX").clicked() {
                                    st.clear_step_fx();
                                    st.ratchet = 1;
                                }
                            });
                            let painter = ui.painter();
                            painter.rect_filled(rect, Rounding::same(4.0), fill);
                            if is_sel_step {
                                painter.rect_stroke(rect, Rounding::same(4.0), Stroke::new(1.5_f32, Color32::WHITE));
                            }
                            if st.active {
                                let badge = if st.reverse && st.ratchet > 1 {
                                    format!("R{}", st.ratchet)
                                } else if st.reverse {
                                    "R".to_string()
                                } else if st.ratchet > 1 {
                                    format!("{}x", st.ratchet)
                                } else if st.has_step_fx() {
                                    "*".to_string()
                                } else {
                                    String::new()
                                };
                                if !badge.is_empty() {
                                    painter.text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        badge,
                                        egui::FontId::proportional(9.5),
                                        Color32::from_rgb(12, 14, 20),
                                    );
                                }
                            }
                        }
                    });
                }

                // Inline Per-Step FX (P-Lock) Inspector for Selected Step!
                ui.add_space(6.0);
                ui.separator();
                let (sp_part, sp_step) = state.selected_step;
                let sp_part = sp_part.min(7);
                let sp_step = sp_step.min(63);
                let st = &mut proj.patterns[active_pat].sample_lanes[sp_part].steps[sp_step];
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("STEP FX [P{} S{}]:", sp_part + 1, sp_step + 1))
                            .strong()
                            .size(11.0)
                            .color(PAD_COLORS[sp_part]),
                    );
                    let rev_btn = egui::Button::new(
                        RichText::new(if st.reverse { "REV ON" } else { "REV" })
                            .strong()
                            .size(10.5)
                            .color(if st.reverse { Color32::from_rgb(12, 14, 20) } else { Color32::WHITE }),
                    )
                    .fill(if st.reverse {
                        Color32::from_rgb(64, 218, 255)
                    } else {
                        Color32::from_rgb(36, 42, 58)
                    });
                    if ui.add(rev_btn).on_hover_text("Flip Reverse on just this step!").clicked() {
                        st.reverse = !st.reverse;
                        if st.reverse {
                            st.active = true;
                        }
                    }
                    ui.add(egui::DragValue::new(&mut st.pitch_offset).range(-12..=12).prefix("Pitch ").suffix("st"));
                    ui.add(egui::DragValue::new(&mut st.speed_mult).range(0.25..=2.0).speed(0.05).prefix("Spd ").suffix("x"));
                    ui.add(egui::DragValue::new(&mut st.ratchet).range(1..=4).prefix("Ratch ").suffix("x"));
                    if st.has_step_fx() && ui.small_button("Clr FX").clicked() {
                        st.clear_step_fx();
                        st.ratchet = 1;
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.small_button("Auto-Chop Groove").on_hover_text("Create a groovy rhythmic chop pattern across Pads 1–8").clicked() {
                        let pat = &mut proj.patterns[active_pat];
                        for l in &mut pat.sample_lanes {
                            l.clear();
                        }
                        for bar in 0..4 {
                            let base = bar * 16;
                            for (step, pad_i) in [(0, 0), (3, 1), (6, 2), (8, 3), (10, 4), (12, 5), (14, 6), (15, 7)] {
                                let p = (pad_i + bar) % 8;
                                let s = &mut pat.sample_lanes[p].steps[base + step];
                                s.active = true;
                                s.velocity = if step % 4 == 0 { 0.92 } else { 0.78 };
                            }
                        }
                        proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                    }
                    if ui.small_button("Clear All 8 Lanes").clicked() {
                        for l in &mut proj.patterns[active_pat].sample_lanes {
                            l.clear();
                        }
                    }
                });
            });
        });

        // RIGHT: SELECTED PAD WAVEFORM + VOLCA TUNING CONTROLS
        cols[1].vertical(|ui| {
            let pc = PAD_COLORS[sel];
            card(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("PAD {} — {}", sel + 1, proj.sample_parts[sel].name))
                            .strong()
                            .size(13.0)
                            .color(pc),
                    );
                    egui::ComboBox::from_id_salt("volca_builtin")
                        .width(135.0)
                        .selected_text(&proj.sample_parts[sel].sample.name)
                        .show_ui(ui, |ui| {
                            for (bi, &bname) in BUILTIN_SAMPLE_NAMES.iter().enumerate() {
                                if ui
                                    .selectable_label(proj.sample_parts[sel].sample.builtin_id == Some(bi as u8), bname)
                                    .clicked()
                                {
                                    let p = &mut proj.sample_parts[sel];
                                    p.name = bname.to_string();
                                    p.sample.name = bname.to_string();
                                    p.sample.builtin_id = Some(bi as u8);
                                    p.sample.pcm_i16_b64.clear();
                                    p.start = 0.0;
                                    p.end = 1.0;
                                    p.speed = 1.0;
                                    p.generation = p.generation.wrapping_add(1);
                                }
                            }
                        });
                    if ui.small_button("Trim").clicked() {
                        let p = &mut proj.sample_parts[sel];
                        let sr = p.sample.sample_rate.max(8_000);
                        let pcm = p.sample.decode_f32_pcm().unwrap_or_else(|| {
                            generate_builtin_sample(p.sample.builtin_id.unwrap_or(3), sr)
                        });
                        let trimmed = trim_silence_and_normalize(&pcm, sr);
                        p.sample = SavedSample::from_f32_pcm(p.sample.name.clone(), sr, &trimmed, None);
                        p.start = 0.0;
                        p.end = 1.0;
                        p.generation = p.generation.wrapping_add(1);
                    }
                    if ui.small_button("Norm").clicked() {
                        let p = &mut proj.sample_parts[sel];
                        let sr = p.sample.sample_rate.max(8_000);
                        let pcm = p.sample.decode_f32_pcm().unwrap_or_else(|| {
                            generate_builtin_sample(p.sample.builtin_id.unwrap_or(3), sr)
                        });
                        let norm = normalize_peak(&pcm);
                        p.sample = SavedSample::from_f32_pcm(p.sample.name.clone(), sr, &norm, None);
                        p.generation = p.generation.wrapping_add(1);
                    }
                });

                ui.add_space(4.0);

                // Interactive Waveform Canvas for Selected Pad
                let part = &mut proj.sample_parts[sel];
                let wf_w = ui.available_width().max(260.0);
                let wf_h = 104.0;
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(wf_w, wf_h), Sense::click_and_drag());
                let start_x = rect.left() + part.start.clamp(0.0, 0.99) * rect.width();
                let end_x = rect.left() + part.end.clamp(0.01, 1.0) * rect.width();

                if resp.drag_started() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        state.drag_handle = if (pos.x - start_x).abs() <= (pos.x - end_x).abs() {
                            Some(0)
                        } else {
                            Some(1)
                        };
                    }
                }
                if resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let norm = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                        match state.drag_handle {
                            Some(0) => part.start = norm.min(part.end - 0.01).max(0.0),
                            Some(1) => part.end = norm.max(part.start + 0.01).min(1.0),
                            _ => {}
                        }
                    }
                }
                if resp.drag_stopped() {
                    state.drag_handle = None;
                }

                let painter = ui.painter();
                painter.rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(10, 13, 20));
                let mid_y = rect.center().y;
                let n = state.peaks.len();
                if n > 0 {
                    let half_h = rect.height() * 0.44;
                    let bar_w = rect.width() / n as f32;
                    for (i, &(mn, mx)) in state.peaks.iter().enumerate() {
                        let x = rect.left() + (i as f32 + 0.5) * bar_w;
                        let nx = i as f32 / n as f32;
                        let col = if nx >= part.start && nx <= part.end {
                            pc
                        } else {
                            Color32::from_rgb(62, 70, 86)
                        };
                        let y0 = mid_y - mx.clamp(-1.0, 1.0) * half_h;
                        let y1 = mid_y - mn.clamp(-1.0, 1.0) * half_h;
                        painter.line_segment(
                            [egui::pos2(x, y0), egui::pos2(x, y1.max(y0 + 1.0))],
                            Stroke::new(bar_w.max(1.3), col),
                        );
                    }
                }
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
                painter.line_segment(
                    [egui::pos2(start_x, rect.top()), egui::pos2(start_x, rect.bottom())],
                    Stroke::new(2.0_f32, Color32::from_rgb(72, 224, 148)),
                );
                painter.line_segment(
                    [egui::pos2(end_x, rect.top()), egui::pos2(end_x, rect.bottom())],
                    Stroke::new(2.0_f32, Color32::from_rgb(245, 98, 112)),
                );
                if let Some(ph) = tel.sampler_playhead {
                    let px = rect.left() + ph.clamp(0.0, 1.0) * rect.width();
                    painter.line_segment(
                        [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
                        Stroke::new(2.0_f32, Color32::WHITE),
                    );
                }
                painter.rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0_f32, Color32::from_rgb(44, 52, 72)));

                ui.add_space(6.0);

                // Volca Pad Parameters
                ui.add(egui::Slider::new(&mut part.speed, 0.25..=2.0).suffix("x").max_decimals(2).text("Speed"));
                ui.horizontal(|ui| {
                    for &(spd, lbl) in &[(0.5, "0.5x"), (1.0, "1.0x"), (1.5, "1.5x"), (2.0, "2.0x")] {
                        if ui.selectable_label((part.speed - spd).abs() < 0.01, lbl).clicked() {
                            part.speed = spd;
                        }
                    }
                    ui.separator();
                    ui.checkbox(&mut part.reverse, "Rev");
                    ui.selectable_value(&mut part.loop_mode, LoopMode::OneShot, "1-Shot");
                    ui.selectable_value(&mut part.loop_mode, LoopMode::Loop, "Loop");
                    ui.selectable_value(&mut part.loop_mode, LoopMode::PingPong, "Ping");
                });
                ui.add(egui::Slider::new(&mut part.transpose, -24..=24).suffix(" st").text("Pitch"));
                ui.horizontal(|ui| {
                    ui.label("Choke:");
                    ui.selectable_value(&mut part.choke_group, 0, "Off");
                    ui.selectable_value(&mut part.choke_group, 1, "Grp 1");
                    ui.selectable_value(&mut part.choke_group, 2, "Grp 2");
                    ui.separator();
                    ui.selectable_value(&mut part.mode, SamplerMode::HiFi, "Hi-Fi");
                    ui.selectable_value(&mut part.mode, SamplerMode::Vintage12Bit, "12-Bit");
                    ui.selectable_value(&mut part.mode, SamplerMode::Sk1LoFi, "8-Bit");
                });
                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    ui.selectable_value(&mut part.filter_mode, GlobalFilterMode::LowPass, "LP");
                    ui.selectable_value(&mut part.filter_mode, GlobalFilterMode::BandPass, "BP");
                    ui.selectable_value(&mut part.filter_mode, GlobalFilterMode::HighPass, "HP");
                });
                ui.add(egui::Slider::new(&mut part.cutoff, 120.0..=18_000.0).logarithmic(true).suffix(" Hz").text("Cutoff"));
                ui.add(egui::Slider::new(&mut part.resonance, 0.0..=0.85).text("Resonance"));
                ui.add(egui::Slider::new(&mut part.attack, 0.001..=0.6).suffix(" s").text("Attack"));
                ui.add(egui::Slider::new(&mut part.decay, 0.02..=2.5).suffix(" s").text("Decay"));
                ui.add(egui::Slider::new(&mut part.release, 0.01..=2.0).suffix(" s").text("Release"));
                ui.add(egui::Slider::new(&mut part.level, 0.0..=1.2).text("Level"));
                ui.add(egui::Slider::new(&mut part.pan, -1.0..=1.0).text("Pan"));
                ui.add(egui::Slider::new(&mut part.reverb_send, 0.0..=1.0).text("Reverb Send"));
            });
        });
    });

    toast_msg
}

fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(Color32::from_rgb(18, 22, 32))
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(12.0))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(34, 41, 58)))
        .show(ui, add_contents);
}

fn compute_peaks(pcm: &[f32], bins: usize) -> Vec<(f32, f32)> {
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
