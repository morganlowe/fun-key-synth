use crate::{
    dsp::{
        philharmonia_bank_status, preload_philharmonia_bank_async,
        sampler::{load_audio_to_stereo_f32, BUILTIN_SAMPLE_NAMES},
        PHILHARMONIA_PERC_LABELS,
    },
    engine::{export_stems, export_wav, AudioEngine, EngineTelemetry, LiveEvent},
    mic::MicRecorder,
    midi::{export_project_midi, MidiManager, MidiMsg},
    model::{
        apply_extra_track_preset, apply_philharmonia_instrument, apply_vibe_to_project,
        apply_voice_preset, auto_arrange_project_song, build_auto_chord_pads,
        capture_party_song_to_project, detect_chord_from_notes, generate_auto_beat,
        generate_extra_track_seq, generate_melodic_tracks, generate_philharmonia_track_seq,
        load_pattern, load_project, note_name, root_name, save_pattern, save_project,
        voice_preset_category, voice_preset_names, ArpMode, ArpRate, AutoChordPad, ChordType,
        ClockDiv, DelayDivision, DrumModel, DrumVoiceId, EgTarget, ExtraTrack, FilterEnvTrigger,
        GlobalFilterMode, LfoMode, LfoTarget, LfoWave, LoopMode, MidiCcMapping, MidiCcTarget,
        MonologueOscWave, MoogWave, Osc2ModMode, PartyMixMode, PartyPace, Project, SamplerMode,
        SavedSample, Scale, SongSection, StepCount, SynthEngine, Tb303Wave, TrackFilterParams,
        TrackId, Vibe, MAX_EXTRA_TRACKS, NOTE_NAMES, PROGRESSIONS,
    },
    settings::{
        restart_app, spawn_install_update, spawn_update_check, AppSettings, StudioTheme,
        UpdateStatus, UserPatch, UserPatchData, CURRENT_VERSION, RELEASES_URL,
    },
};
use eframe::egui::{self, Color32, Key, RichText, Rounding, Sense, Stroke, Vec2};
use std::collections::VecDeque;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyboardMode {
    Chromatic,
    Scale,
    Chords,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordTarget {
    Core(TrackId),
    Extra(usize),
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
    keyboard_mode: KeyboardMode,
    armed_record: Option<RecordTarget>,
    rec_replace: bool,
    rec_take_cleared: bool,
    step_rec_cursor: usize,
    step_rec_last_step: Option<usize>,
    step_rec_accent: bool,
    step_rec_slide: bool,
    step_rec_chord_buf: Vec<u8>,
    step_rec_chord_start_t: f64,
    held_keys: Vec<(Key, HeldNote)>,
    pending_key_releases: Vec<(Key, f64)>,
    held_midi: Vec<HeldNote>,
    held_mouse: Option<HeldNote>,
    held_chord_key: Option<Key>,
    pending_chord_release: Option<(Key, f64)>,
    held_mouse_pad: Option<usize>,
    rec_held_notes: Vec<(RecordTarget, u8, usize, f64)>,
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
    show_drop_setup: bool,
    update_status: Arc<Mutex<UpdateStatus>>,
    // Sound Lab / Sampler Auto-Repeat Test Note
    test_repeat_on: bool,
    test_repeat_mode: TestRepeatMode,
    test_repeat_next_t: f64,
    test_repeat_off_t: Option<f64>,
    test_repeat_step: usize,
    test_repeat_active: Option<(TrackId, u8, bool)>, // (track, note, is_chord)
    phil_cat: usize,
    user_patches: Vec<UserPatch>,
    preset_search: String,
    preset_cat_filter: usize,
    save_patch_name: String,
    show_save_patch: bool,
    show_preset_browser: bool,
    undo_stack: VecDeque<Project>,
    redo_stack: Vec<Project>,
    drag_base_proj: Option<Project>,
    undo_action_this_frame: bool,
    seq_euclid_hits: usize,
    seq_euclid_rot: usize,
    selected_drum_lane: usize,
    midi_learn_target: Option<MidiCcTarget>,
    midi_bend_val: f32,
    midi_aftertouch_val: f32,
    last_midi_cc: Option<(u8, f32)>,
}

fn key_pressed_no_repeat(i: &egui::InputState, key: Key) -> bool {
    i.events.iter().any(|ev| {
        matches!(
            ev,
            egui::Event::Key {
                key: k,
                pressed: true,
                repeat: false,
                ..
            } if *k == key
        )
    })
}

fn key_any_press_event(i: &egui::InputState, key: Key) -> bool {
    i.events.iter().any(|ev| {
        matches!(
            ev,
            egui::Event::Key {
                key: k,
                pressed: true,
                ..
            } if *k == key
        )
    })
}

const PHIL_SECTIONS: [(&str, &[usize]); 6] = [
    ("Ensembles", &[0, 1, 2, 3, 4, 5]),
    ("Strings", &[6, 7, 8, 9, 10]),
    ("Plucked", &[11, 12]),
    ("Brass", &[13, 14, 15, 16]),
    ("Woodwinds", &[17, 18, 19, 20, 21, 22, 23, 24]),
    ("Percussion", &[25]),
];

fn ui_add_track_menu_contents(
    ui: &mut egui::Ui,
    add_extra_preset: &mut Option<usize>,
    add_phil_bank: &mut Option<u8>,
) {
    let bank_names = SynthEngine::PhilharmoniaD50.bank_names();
    ui.menu_button(
        RichText::new("🎻 Philharmonia Orchestra")
            .strong()
            .color(Color32::from_rgb(245, 206, 112)),
        |ui| {
            for &(sec_name, ids) in &PHIL_SECTIONS {
                ui.menu_button(sec_name, |ui| {
                    for &bi in ids {
                        let lbl = bank_names.get(bi).copied().unwrap_or("Instrument");
                        if ui.button(lbl).clicked() {
                            *add_phil_bank = Some(bi as u8);
                            ui.close_menu();
                        }
                    }
                });
            }
        },
    );
    ui.separator();
    ui.label(RichText::new("SYNTH LAYERS").strong().size(11.0).weak());
    let presets = voice_preset_names(TrackId::MoogPad);
    for (pi, &pname) in presets.iter().enumerate() {
        if ui.button(format!("+ {pname}")).clicked() {
            *add_extra_preset = Some(pi);
            ui.close_menu();
        }
    }
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
        let engine = AudioEngine::start(Project::default());
        let user_patches = AppSettings::load_user_patches();
        Self::with_engine(engine, settings, update_status, app_icon, user_patches)
    }

    #[cfg(test)]
    pub fn new_for_test(ctx: &egui::Context) -> Self {
        let settings = AppSettings::default();
        settings.apply_to_egui(ctx);
        let update_status = Arc::new(Mutex::new(UpdateStatus::Idle));
        let icon_img = egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]);
        let app_icon = ctx.load_texture("app_icon_test", icon_img, egui::TextureOptions::LINEAR);
        let engine = AudioEngine::start_silent(Project::default());
        Self::with_engine(engine, settings, update_status, app_icon, Vec::new())
    }

    fn with_engine(
        engine: AudioEngine,
        settings: AppSettings,
        update_status: Arc<Mutex<UpdateStatus>>,
        app_icon: egui::TextureHandle,
        user_patches: Vec<UserPatch>,
    ) -> Self {
        Self {
            engine,
            tab: StudioTab::Studio,
            selected_track: TrackId::Drums,
            selected_extra: None,
            selected_step: 0,
            bar_page: 0,
            follow_playhead: true,
            octave_offset: 0,
            keyboard_mode: KeyboardMode::Chromatic,
            armed_record: None,
            rec_replace: true,
            rec_take_cleared: false,
            step_rec_cursor: 0,
            step_rec_last_step: None,
            step_rec_accent: false,
            step_rec_slide: false,
            step_rec_chord_buf: Vec::new(),
            step_rec_chord_start_t: 0.0,
            held_keys: Vec::new(),
            pending_key_releases: Vec::new(),
            held_midi: Vec::new(),
            held_mouse: None,
            held_chord_key: None,
            pending_chord_release: None,
            held_mouse_pad: None,
            rec_held_notes: Vec::new(),
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
            show_drop_setup: false,
            update_status,
            test_repeat_on: false,
            test_repeat_mode: TestRepeatMode::Quarter,
            test_repeat_next_t: 0.0,
            test_repeat_off_t: None,
            test_repeat_step: 0,
            test_repeat_active: None,
            phil_cat: 0,
            user_patches,
            preset_search: String::new(),
            preset_cat_filter: 0,
            save_patch_name: String::new(),
            show_save_patch: false,
            show_preset_browser: false,
            undo_stack: VecDeque::with_capacity(32),
            redo_stack: Vec::with_capacity(32),
            drag_base_proj: None,
            undo_action_this_frame: false,
            seq_euclid_hits: 5,
            seq_euclid_rot: 0,
            selected_drum_lane: 0,
            midi_learn_target: None,
            midi_bend_val: 0.0,
            midi_aftertouch_val: 0.0,
            last_midi_cc: None,
        }
    }

    fn perform_undo(&mut self, ctx: &egui::Context, proj: &mut Project) -> bool {
        if let Some(mut prev) = self.undo_stack.pop_back() {
            self.redo_stack.push(proj.clone());
            if self.redo_stack.len() > 32 {
                self.redo_stack.remove(0);
            }
            prev.sample_generation = proj.sample_generation.wrapping_add(1);
            for (i, sp) in prev.sample_parts.iter_mut().enumerate() {
                sp.generation = proj
                    .sample_parts
                    .get(i)
                    .map(|old| old.generation.wrapping_add(1))
                    .unwrap_or(1);
            }
            *proj = prev;
            self.drag_base_proj = None;
            self.undo_action_this_frame = true;
            self.set_toast(ctx, "Undo");
            true
        } else {
            false
        }
    }

    fn perform_redo(&mut self, ctx: &egui::Context, proj: &mut Project) -> bool {
        if let Some(mut next) = self.redo_stack.pop() {
            if self.undo_stack.len() >= 32 {
                self.undo_stack.pop_front();
            }
            self.undo_stack.push_back(proj.clone());
            next.sample_generation = proj.sample_generation.wrapping_add(1);
            for (i, sp) in next.sample_parts.iter_mut().enumerate() {
                sp.generation = proj
                    .sample_parts
                    .get(i)
                    .map(|old| old.generation.wrapping_add(1))
                    .unwrap_or(1);
            }
            *proj = next;
            self.drag_base_proj = None;
            self.undo_action_this_frame = true;
            self.set_toast(ctx, "Redo");
            true
        } else {
            false
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

    fn active_target(&self, p: &Project) -> RecordTarget {
        if let Some(ei) = self.selected_extra.filter(|&i| i < p.extra_tracks.len()) {
            RecordTarget::Extra(ei)
        } else {
            RecordTarget::Core(self.selected_track)
        }
    }

    fn effective_play_target(&self, p: &Project) -> RecordTarget {
        if self.tab == StudioTab::Sampler {
            RecordTarget::Core(TrackId::Sampler)
        } else {
            self.active_target(p)
        }
    }

    fn target_step_count(&self, p: &Project, target: RecordTarget) -> StepCount {
        let pat = p.current_pattern();
        match target {
            RecordTarget::Core(TrackId::Drums) => pat.drums.step_count,
            RecordTarget::Core(TrackId::Sampler) => pat
                .sample_lanes
                .first()
                .map(|l| l.step_count)
                .unwrap_or(pat.sampler.step_count),
            RecordTarget::Core(tr) => pat
                .melodic(tr)
                .map(|s| s.step_count)
                .unwrap_or(StepCount::Steps16),
            RecordTarget::Extra(ei) => pat
                .extra_seqs
                .get(ei)
                .map(|s| s.step_count)
                .unwrap_or(StepCount::Steps64),
        }
    }

    fn set_target_step_count(&self, p: &mut Project, target: RecordTarget, sc: StepCount) {
        let pat = p.current_pattern_mut();
        match target {
            RecordTarget::Core(TrackId::Drums) => pat.drums.set_all_step_counts(sc, false),
            RecordTarget::Core(TrackId::Sampler) => pat.set_sampler_step_counts(sc),
            RecordTarget::Core(tr) => {
                if let Some(seq) = pat.melodic_mut(tr) {
                    seq.step_count = sc;
                }
            }
            RecordTarget::Extra(ei) => {
                if let Some(seq) = pat.extra_seqs.get_mut(ei) {
                    seq.step_count = sc;
                }
            }
        }
    }

    fn clear_target_pattern(&self, p: &mut Project, target: RecordTarget) {
        let pat = p.current_pattern_mut();
        match target {
            RecordTarget::Core(TrackId::Drums) => pat.drums.clear(),
            RecordTarget::Core(TrackId::Sampler) => {
                pat.sampler.clear();
                for l in &mut pat.sample_lanes {
                    l.clear();
                }
            }
            RecordTarget::Core(tr) => {
                if let Some(seq) = pat.melodic_mut(tr) {
                    seq.clear();
                }
            }
            RecordTarget::Extra(ei) => {
                if let Some(seq) = pat.extra_seqs.get_mut(ei) {
                    seq.clear();
                }
            }
        }
    }

    fn target_name(p: &Project, target: RecordTarget) -> String {
        match target {
            RecordTarget::Core(tr) => tr.role_name().to_string(),
            RecordTarget::Extra(ei) => p
                .extra_tracks
                .get(ei)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| format!("Extra #{}", ei + 7)),
        }
    }

    fn target_color(p: &Project, target: RecordTarget) -> Color32 {
        match target {
            RecordTarget::Core(tr) => track_color(tr),
            RecordTarget::Extra(ei) => p
                .extra_tracks
                .get(ei)
                .map(|t| Color32::from_rgb(t.color[0], t.color[1], t.color[2]))
                .unwrap_or(Color32::WHITE),
        }
    }

    fn unmute_target(&self, p: &mut Project, target: RecordTarget) {
        match target {
            RecordTarget::Core(tr) => {
                let (_, mute, _) = track_mixer_mut(p, tr);
                *mute = false;
            }
            RecordTarget::Extra(ei) => {
                if let Some(tr) = p.extra_tracks.get_mut(ei) {
                    tr.mute = false;
                }
            }
        }
    }

    fn target_is_poly_step_rec(&self, p: &Project, target: RecordTarget) -> bool {
        match target {
            RecordTarget::Core(TrackId::Drums) => true,
            RecordTarget::Core(TrackId::MoogPad) => true,
            RecordTarget::Extra(_) => true,
            RecordTarget::Core(TrackId::Moog) => p.moog.poly_mode,
            RecordTarget::Core(TrackId::Sampler) => self.keyboard_mode == KeyboardMode::Scale,
            _ => false,
        }
    }

    fn chromatic_base_midi(&self) -> u8 {
        (48 + self.octave_offset as i32 * 12).clamp(24, 72) as u8
    }

    fn advance_step_rec_cursor(&mut self, len: usize) {
        let len = len.max(1);
        self.step_rec_cursor = (self.step_rec_cursor + 1) % len;
        self.selected_step = self.step_rec_cursor;
        self.bar_page = self.step_rec_cursor / 16;
    }

    fn toggle_arm_record(&mut self, ctx: &egui::Context, p: &mut Project, target: RecordTarget) {
        if self.armed_record == Some(target) {
            self.armed_record = None;
            self.step_rec_chord_buf.clear();
            let name = Self::target_name(p, target);
            self.set_toast(
                ctx,
                format!(
                    "Saved {name} pattern to Bank {}",
                    (b'A' + p.active_pattern as u8) as char
                ),
            );
            return;
        }
        match target {
            RecordTarget::Core(tr) => {
                self.selected_track = tr;
                self.selected_extra = None;
            }
            RecordTarget::Extra(ei) => {
                self.selected_extra = Some(ei);
            }
        }
        self.armed_record = Some(target);
        self.rec_take_cleared = false;
        self.step_rec_last_step = None;
        self.step_rec_chord_buf.clear();
        self.unmute_target(p, target);
        if target == RecordTarget::Core(TrackId::Drums) {
            p.auto_beat_mode = false;
        }
        p.party_mode = false;
        if target != RecordTarget::Core(TrackId::MoogPad)
            && self.keyboard_mode == KeyboardMode::Chords
        {
            self.keyboard_mode = KeyboardMode::Chromatic;
            p.auto_chord_mode = false;
        }
        let sc = self.target_step_count(p, target);
        let len = sc.as_usize();
        self.bar_page = self.bar_page.min(sc.pages().saturating_sub(1));
        self.step_rec_cursor = (self.bar_page * 16).min(len.saturating_sub(1));
        self.selected_step = self.step_rec_cursor;
        let name = Self::target_name(p, target);
        self.set_toast(
            ctx,
            format!(
                "Armed {name} REC ({}) — play to record!",
                if self.rec_replace { "Replace" } else { "Overdub" }
            ),
        );
    }

    fn has_held_for_target(&self, target: RecordTarget) -> bool {
        let matches = |h: HeldNote| match (h, target) {
            (HeldNote::Melodic(t, _), tgt) => t == tgt,
            (HeldNote::Drum(_), RecordTarget::Core(TrackId::Drums)) => true,
            (HeldNote::SamplerPad(_), RecordTarget::Core(TrackId::Sampler)) => true,
            (HeldNote::SamplerPitched(_, _), RecordTarget::Core(TrackId::Sampler)) => true,
            _ => false,
        };
        self.held_keys.iter().any(|&(_, h)| matches(h))
            || self.held_midi.iter().any(|&h| matches(h))
            || self.held_mouse.map(matches).unwrap_or(false)
    }

    fn step_rec_rest(&mut self, p: &mut Project) {
        let Some(target) = self.armed_record else {
            return;
        };
        self.maybe_clear_for_replace(p, target);
        let len = self.target_step_count(p, target).as_usize();
        let cur = self.step_rec_cursor % len;
        let pat = p.current_pattern_mut();
        match target {
            RecordTarget::Core(TrackId::Drums) => {
                for l in &mut pat.drums.lanes {
                    l.steps[cur].active = false;
                }
            }
            RecordTarget::Core(TrackId::Sampler) => {
                pat.sampler.steps[cur].active = false;
                for l in &mut pat.sample_lanes {
                    l.steps[cur].active = false;
                }
            }
            RecordTarget::Core(tr) => {
                if let Some(seq) = pat.melodic_mut(tr) {
                    seq.steps[cur].active = false;
                    seq.steps[cur].accent = false;
                    seq.steps[cur].slide = false;
                }
            }
            RecordTarget::Extra(ei) => {
                if let Some(seq) = pat.extra_seqs.get_mut(ei) {
                    seq.steps[cur].active = false;
                    seq.steps[cur].accent = false;
                    seq.steps[cur].slide = false;
                }
            }
        }
        self.step_rec_chord_buf.clear();
        self.step_rec_last_step = None;
        self.advance_step_rec_cursor(len);
    }

    fn step_rec_tie(&mut self, p: &mut Project) {
        let Some(target) = self.armed_record else {
            return;
        };
        let len = self.target_step_count(p, target).as_usize();
        let cur = self.step_rec_cursor % len;
        let prev = self.step_rec_last_step.unwrap_or((cur + len - 1) % len);
        let pat = p.current_pattern_mut();
        let seq_opt = match target {
            RecordTarget::Core(tr) if tr != TrackId::Drums => pat.melodic_mut(tr),
            RecordTarget::Extra(ei) => pat.extra_seqs.get_mut(ei),
            _ => None,
        };
        if let Some(seq) = seq_opt {
            if seq.steps[prev].active {
                seq.steps[prev].gate = (seq.steps[prev].gate + 1.0).clamp(0.25, 16.0);
                seq.steps[cur].active = false;
            }
        }
        self.step_rec_chord_buf.clear();
        self.advance_step_rec_cursor(len);
    }

    fn step_rec_back(&mut self, p: &Project) {
        let target = self.armed_record.unwrap_or_else(|| self.effective_play_target(p));
        let len = self.target_step_count(p, target).as_usize().max(1);
        self.step_rec_chord_buf.clear();
        self.step_rec_cursor = (self.step_rec_cursor + len - 1) % len;
        self.selected_step = self.step_rec_cursor;
        self.bar_page = self.step_rec_cursor / 16;
    }

    fn step_rec_forward(&mut self, p: &Project) {
        let target = self.armed_record.unwrap_or_else(|| self.effective_play_target(p));
        let len = self.target_step_count(p, target).as_usize().max(1);
        self.step_rec_chord_buf.clear();
        self.advance_step_rec_cursor(len);
    }

    fn maybe_clear_for_replace(&mut self, p: &mut Project, target: RecordTarget) {
        if self.rec_replace && !self.rec_take_cleared {
            self.clear_target_pattern(p, target);
            self.rec_take_cleared = true;
        }
    }

    fn play_and_record_note_down(
        &mut self,
        ctx: &egui::Context,
        p: &mut Project,
        target: RecordTarget,
        note: u8,
        velocity: f32,
        playing: bool,
        cur_step: usize,
    ) -> HeldNote {
        let note = note.clamp(12, 108);
        self.unmute_target(p, target);

        if target == RecordTarget::Core(TrackId::Drums) {
            let lane = if (36..44).contains(&note) {
                (note - 36) as usize
            } else {
                (note as usize) % 8
            };
            return self.trigger_drum_pad_down(ctx, p, lane, velocity, playing, cur_step);
        }

        let now = ctx.input(|i| i.time);
        let shift_acc = ctx.input(|i| i.modifiers.shift);
        let held_kind = match target {
            RecordTarget::Core(TrackId::Sampler) => {
                let part = self.volca_ui.selected_part.min(7);
                let pitch_offset = (note as i32 - 60).clamp(-24, 24) as i8;
                self.engine.send_live(LiveEvent::TriggerSamplePartPitched {
                    part,
                    velocity,
                    pitch_offset,
                });
                HeldNote::SamplerPitched(part, note)
            }
            RecordTarget::Core(tr) => {
                self.engine.send_live(LiveEvent::PlayNote {
                    track: tr,
                    note,
                    velocity,
                });
                HeldNote::Melodic(target, note)
            }
            RecordTarget::Extra(ei) => {
                self.engine.send_live(LiveEvent::PlayExtraNote {
                    idx: ei,
                    note,
                    velocity,
                });
                HeldNote::Melodic(target, note)
            }
        };

        if self.armed_record != Some(target) {
            return held_kind;
        }

        self.maybe_clear_for_replace(p, target);
        if let HeldNote::SamplerPitched(part, _) = held_kind {
            let len = p.current_pattern().sample_lanes[part].effective_len();
            let pitch_offset = (note as i32 - 60).clamp(-24, 24) as i8;
            if playing {
                let phase = self.engine.step_phase();
                let q_global = if phase >= 0.65 { cur_step + 1 } else { cur_step };
                let s_idx = q_global % len;
                let st = &mut p.current_pattern_mut().sample_lanes[part].steps[s_idx];
                st.active = true;
                st.velocity = velocity;
                st.pitch_offset = pitch_offset;
                st.accent = shift_acc || self.step_rec_accent;
                if q_global > cur_step {
                    self.engine.send_live(LiveEvent::SuppressTrackStep {
                        track: Some(TrackId::Sampler),
                        extra_idx: None,
                        lane: Some(part),
                        global_step: q_global,
                    });
                }
            } else {
                let cur = self.step_rec_cursor % len;
                let st = &mut p.current_pattern_mut().sample_lanes[part].steps[cur];
                st.active = true;
                st.velocity = velocity;
                st.pitch_offset = pitch_offset;
                st.accent = shift_acc || self.step_rec_accent;
                self.step_rec_last_step = Some(cur);
                self.advance_step_rec_cursor(len);
            }
            return held_kind;
        }
        let len = self.target_step_count(p, target).as_usize();

        if playing {
            let phase = self.engine.step_phase();
            let q_global = if phase >= 0.65 { cur_step + 1 } else { cur_step };
            let s_idx = q_global % len;
            let legato = !self.target_is_poly_step_rec(p, target) && self.has_held_for_target(target);
            let is_poly = self.target_is_poly_step_rec(p, target);
            let mut chord_notes = Vec::new();
            if is_poly {
                for &(rt, n, si, _) in &self.rec_held_notes {
                    if rt == target && si == s_idx {
                        chord_notes.push(n);
                    }
                }
                chord_notes.push(note);
            }
            let (write_note, write_chord) = if is_poly && chord_notes.len() > 1 {
                detect_chord_from_notes(&chord_notes)
            } else {
                (note, ChordType::Single)
            };

            let pat = p.current_pattern_mut();
            let seq_opt = match target {
                RecordTarget::Core(tr) => pat.melodic_mut(tr),
                RecordTarget::Extra(ei) => pat.extra_seqs.get_mut(ei),
            };
            if let Some(seq) = seq_opt {
                let st = &mut seq.steps[s_idx];
                st.active = true;
                st.note = write_note;
                st.chord = write_chord;
                st.velocity = velocity;
                st.gate = 1.0;
                st.accent = shift_acc || self.step_rec_accent;
                st.slide = legato || self.step_rec_slide;
            }
            self.selected_step = s_idx;
            self.rec_held_notes.push((target, note, s_idx, now));
            if q_global > cur_step {
                let (tr_opt, ex_opt) = match target {
                    RecordTarget::Core(tr) => (Some(tr), None),
                    RecordTarget::Extra(ei) => (None, Some(ei)),
                };
                self.engine.send_live(LiveEvent::SuppressTrackStep {
                    track: tr_opt,
                    extra_idx: ex_opt,
                    lane: None,
                    global_step: q_global,
                });
            }
        } else {
            let is_poly = self.target_is_poly_step_rec(p, target);
            if is_poly {
                if !self.has_held_for_target(target)
                    && !self.step_rec_chord_buf.is_empty()
                    && (now - self.step_rec_chord_start_t) > 0.16
                {
                    self.step_rec_chord_buf.clear();
                    self.advance_step_rec_cursor(len);
                }
                if self.step_rec_chord_buf.is_empty() {
                    self.step_rec_chord_start_t = now;
                }
                if !self.step_rec_chord_buf.contains(&note) {
                    self.step_rec_chord_buf.push(note);
                }
                let (write_note, write_chord) = detect_chord_from_notes(&self.step_rec_chord_buf);
                let cur = self.step_rec_cursor % len;
                let pat = p.current_pattern_mut();
                let seq_opt = match target {
                    RecordTarget::Core(tr) => pat.melodic_mut(tr),
                    RecordTarget::Extra(ei) => pat.extra_seqs.get_mut(ei),
                };
                if let Some(seq) = seq_opt {
                    let st = &mut seq.steps[cur];
                    st.active = true;
                    st.note = write_note;
                    st.chord = write_chord;
                    st.velocity = velocity;
                    st.gate = if target == RecordTarget::Core(TrackId::MoogPad) {
                        4.0
                    } else {
                        1.5
                    };
                    st.accent = shift_acc || self.step_rec_accent;
                    st.slide = false;
                }
                self.selected_step = cur;
            } else {
                let cur = self.step_rec_cursor % len;
                let legato = self.has_held_for_target(target);
                let pat = p.current_pattern_mut();
                let seq_opt = match target {
                    RecordTarget::Core(tr) => pat.melodic_mut(tr),
                    RecordTarget::Extra(ei) => pat.extra_seqs.get_mut(ei),
                };
                if let Some(seq) = seq_opt {
                    let st = &mut seq.steps[cur];
                    st.active = true;
                    st.note = note;
                    st.chord = ChordType::Single;
                    st.velocity = velocity;
                    st.gate = 0.85;
                    st.accent = shift_acc || self.step_rec_accent;
                    st.slide = legato || self.step_rec_slide;
                }
                self.step_rec_last_step = Some(cur);
                self.advance_step_rec_cursor(len);
            }
        }
        held_kind
    }

    fn trigger_drum_pad_down(
        &mut self,
        ctx: &egui::Context,
        p: &mut Project,
        lane: usize,
        velocity: f32,
        playing: bool,
        cur_step: usize,
    ) -> HeldNote {
        let lane = lane % 8;
        self.unmute_target(p, RecordTarget::Core(TrackId::Drums));
        p.drums.voices[lane].mute = false;
        self.engine.send_live(LiveEvent::PlayNote {
            track: TrackId::Drums,
            note: lane as u8,
            velocity,
        });
        let target = RecordTarget::Core(TrackId::Drums);
        if self.armed_record == Some(target) {
            p.auto_beat_mode = false;
            self.maybe_clear_for_replace(p, target);
            let now = ctx.input(|i| i.time);
            let shift_acc = ctx.input(|i| i.modifiers.shift);
            let len = self.target_step_count(p, target).as_usize();
            if playing {
                let phase = self.engine.step_phase();
                let q_global = if phase >= 0.65 { cur_step + 1 } else { cur_step };
                let s_idx = q_global % len;
                let st = &mut p.current_pattern_mut().drums.lanes[lane].steps[s_idx];
                st.active = true;
                st.velocity = velocity;
                st.accent = shift_acc || self.step_rec_accent;
                if q_global > cur_step {
                    self.engine.send_live(LiveEvent::SuppressTrackStep {
                        track: Some(TrackId::Drums),
                        extra_idx: None,
                        lane: Some(lane),
                        global_step: q_global,
                    });
                }
            } else {
                if !self.has_held_for_target(target)
                    && !self.step_rec_chord_buf.is_empty()
                    && (now - self.step_rec_chord_start_t) > 0.16
                {
                    self.step_rec_chord_buf.clear();
                    self.advance_step_rec_cursor(len);
                }
                if self.step_rec_chord_buf.is_empty() {
                    self.step_rec_chord_start_t = now;
                }
                if !self.step_rec_chord_buf.contains(&(lane as u8)) {
                    self.step_rec_chord_buf.push(lane as u8);
                }
                let cur = self.step_rec_cursor % len;
                let st = &mut p.current_pattern_mut().drums.lanes[lane].steps[cur];
                st.active = true;
                st.velocity = velocity;
                st.accent = shift_acc || self.step_rec_accent;
                self.selected_step = cur;
            }
        }
        HeldNote::Drum(lane)
    }

    fn trigger_sampler_pad_down(
        &mut self,
        ctx: &egui::Context,
        p: &mut Project,
        part: usize,
        velocity: f32,
        playing: bool,
        cur_step: usize,
    ) -> HeldNote {
        let part = part % 8;
        self.volca_ui.selected_part = part;
        self.unmute_target(p, RecordTarget::Core(TrackId::Sampler));
        p.sample_parts[part].mute = false;
        self.engine
            .send_live(LiveEvent::TriggerSamplePart { part, velocity });
        let target = RecordTarget::Core(TrackId::Sampler);
        if self.armed_record == Some(target) {
            self.maybe_clear_for_replace(p, target);
            let now = ctx.input(|i| i.time);
            let shift_acc = ctx.input(|i| i.modifiers.shift);
            let len = p.current_pattern().sample_lanes[part].effective_len();
            if playing {
                let phase = self.engine.step_phase();
                let q_global = if phase >= 0.65 { cur_step + 1 } else { cur_step };
                let s_idx = q_global % len;
                let st = &mut p.current_pattern_mut().sample_lanes[part].steps[s_idx];
                st.active = true;
                st.velocity = velocity;
                st.accent = shift_acc || self.step_rec_accent;
                if q_global > cur_step {
                    self.engine.send_live(LiveEvent::SuppressTrackStep {
                        track: Some(TrackId::Sampler),
                        extra_idx: None,
                        lane: Some(part),
                        global_step: q_global,
                    });
                }
            } else {
                if !self.has_held_for_target(target)
                    && !self.step_rec_chord_buf.is_empty()
                    && (now - self.step_rec_chord_start_t) > 0.16
                {
                    self.step_rec_chord_buf.clear();
                    self.advance_step_rec_cursor(len);
                }
                if self.step_rec_chord_buf.is_empty() {
                    self.step_rec_chord_start_t = now;
                }
                if !self.step_rec_chord_buf.contains(&(part as u8)) {
                    self.step_rec_chord_buf.push(part as u8);
                }
                let cur = self.step_rec_cursor % len;
                let st = &mut p.current_pattern_mut().sample_lanes[part].steps[cur];
                st.active = true;
                st.velocity = velocity;
                st.accent = shift_acc || self.step_rec_accent;
                self.selected_step = cur;
            }
        }
        HeldNote::SamplerPad(part)
    }

    fn release_held_note(
        &mut self,
        ctx: &egui::Context,
        p: &mut Project,
        held: HeldNote,
        playing: bool,
    ) {
        let now = ctx.input(|i| i.time);
        match held {
            HeldNote::AutoChord => {
                self.engine.send_live(LiveEvent::ReleaseChord);
            }
            HeldNote::Drum(_) => {
                let target = RecordTarget::Core(TrackId::Drums);
                if !playing
                    && self.armed_record == Some(target)
                    && !self.has_held_for_target(target)
                    && !self.step_rec_chord_buf.is_empty()
                {
                    let len = self.target_step_count(p, target).as_usize();
                    self.step_rec_last_step = Some(self.step_rec_cursor % len);
                    self.step_rec_chord_buf.clear();
                    self.advance_step_rec_cursor(len);
                }
            }
            HeldNote::SamplerPad(part) => {
                if p.sample_parts.get(part).map(|s| s.loop_mode) != Some(LoopMode::OneShot) {
                    self.engine.send_live(LiveEvent::ReleaseSamplePart { part });
                }
                let target = RecordTarget::Core(TrackId::Sampler);
                if !playing
                    && self.armed_record == Some(target)
                    && !self.has_held_for_target(target)
                    && !self.step_rec_chord_buf.is_empty()
                {
                    let len = self.target_step_count(p, target).as_usize();
                    self.step_rec_last_step = Some(self.step_rec_cursor % len);
                    self.step_rec_chord_buf.clear();
                    self.advance_step_rec_cursor(len);
                }
            }
            HeldNote::SamplerPitched(part, _) => {
                if p.sample_parts.get(part).map(|s| s.loop_mode) != Some(LoopMode::OneShot) {
                    self.engine.send_live(LiveEvent::ReleaseSamplePart { part });
                }
            }
            HeldNote::Melodic(target, note) => {
                match target {
                    RecordTarget::Core(tr) => {
                        self.engine.send_live(LiveEvent::ReleaseNote { track: tr, note });
                    }
                    RecordTarget::Extra(ei) => {
                        self.engine
                            .send_live(LiveEvent::ReleaseExtraNote { idx: ei, note });
                    }
                }
                if playing && self.armed_record == Some(target) {
                    if let Some(pos) = self
                        .rec_held_notes
                        .iter()
                        .position(|&(rt, n, _, _)| rt == target && n == note)
                    {
                        let (_, _, s_idx, press_t) = self.rec_held_notes.remove(pos);
                        let step_secs = 60.0 / p.bpm.clamp(40.0, 240.0) as f64 / 4.0;
                        let held_steps = (((now - press_t).max(0.0) / step_secs * 4.0).round() / 4.0)
                            .clamp(0.35, 16.0) as f32;
                        let pat = p.current_pattern_mut();
                        let seq_opt = match target {
                            RecordTarget::Core(tr) => pat.melodic_mut(tr),
                            RecordTarget::Extra(ei) => pat.extra_seqs.get_mut(ei),
                        };
                        if let Some(seq) = seq_opt {
                            if let Some(st) = seq.steps.get_mut(s_idx) {
                                if st.active {
                                    st.gate = held_steps;
                                }
                            }
                        }
                    }
                } else if !playing
                    && self.armed_record == Some(target)
                    && self.target_is_poly_step_rec(p, target)
                    && !self.has_held_for_target(target)
                    && !self.step_rec_chord_buf.is_empty()
                {
                    let len = self.target_step_count(p, target).as_usize();
                    self.step_rec_last_step = Some(self.step_rec_cursor % len);
                    self.step_rec_chord_buf.clear();
                    self.advance_step_rec_cursor(len);
                }
            }
        }
    }

    fn handle_midi(
        &mut self,
        ctx: &egui::Context,
        p: &mut Project,
        pads: &[AutoChordPad],
        playing: bool,
        cur_step: usize,
    ) {
        let msgs = self.midi.drain();
        for msg in msgs {
            match msg {
                MidiMsg::NoteOn { note, velocity } => {
                    let held = if self.tab == StudioTab::Sampler && (36..44).contains(&note) {
                        let part = (note - 36) as usize;
                        self.trigger_sampler_pad_down(ctx, p, part, velocity, playing, cur_step)
                    } else if self.keyboard_mode == KeyboardMode::Chords {
                        let idx = (note as usize) % pads.len().max(1);
                        self.trigger_chord_pad_down(ctx, idx, p, pads, playing, cur_step);
                        HeldNote::AutoChord
                    } else {
                        let target = self.effective_play_target(p);
                        self.play_and_record_note_down(
                            ctx, p, target, note, velocity, playing, cur_step,
                        )
                    };
                    self.held_midi.push(held);
                }
                MidiMsg::NoteOff { note } => {
                    let held = if self.tab == StudioTab::Sampler && (36..44).contains(&note) {
                        let part = (note - 36) as usize;
                        HeldNote::SamplerPad(part)
                    } else if self.keyboard_mode == KeyboardMode::Chords {
                        HeldNote::AutoChord
                    } else {
                        let target = self.effective_play_target(p);
                        match target {
                            RecordTarget::Core(TrackId::Drums) => {
                                let lane = if (36..44).contains(&note) {
                                    (note - 36) as usize
                                } else {
                                    (note as usize) % 8
                                };
                                HeldNote::Drum(lane)
                            }
                            RecordTarget::Core(TrackId::Sampler) => {
                                HeldNote::SamplerPitched(self.volca_ui.selected_part.min(7), note)
                            }
                            _ => HeldNote::Melodic(target, note),
                        }
                    };
                    if let Some(pos) = self.held_midi.iter().position(|&h| h == held) {
                        self.held_midi.remove(pos);
                    }
                    self.release_held_note(ctx, p, held, playing);
                }
                MidiMsg::Cc { cc, value } => {
                    self.last_midi_cc = Some((cc, value));
                    if let Some(learn_tgt) = self.midi_learn_target.take() {
                        p.midi.cc_mappings.retain(|m| m.cc != cc);
                        p.midi.cc_mappings.push(MidiCcMapping { cc, target: learn_tgt });
                        self.set_toast(ctx, format!("MIDI Learn: CC {cc} → {}", learn_tgt.name()));
                    }
                    let mut matched = false;
                    let mapped_targets: Vec<MidiCcTarget> = p
                        .midi
                        .cc_mappings
                        .iter()
                        .filter(|m| m.cc == cc)
                        .map(|m| m.target)
                        .collect();
                    for tgt in mapped_targets {
                        matched = true;
                        self.apply_midi_cc_target(p, tgt, value, playing, cur_step);
                    }
                    if !matched && cc == 11 {
                        self.apply_midi_cc_target(
                            p,
                            MidiCcTarget::TrackCutoff,
                            value,
                            playing,
                            cur_step,
                        );
                    }
                }
                MidiMsg::PitchBend { bend } => {
                    self.midi_bend_val = bend;
                    let semi = bend * (p.midi.pitch_bend_range.clamp(1, 12) as f32);
                    self.engine.send_live(LiveEvent::SetMidiExpression {
                        bend_semi: semi,
                        aftertouch: self.midi_aftertouch_val,
                    });
                }
                MidiMsg::Aftertouch { pressure } => {
                    self.midi_aftertouch_val = pressure;
                    let semi = self.midi_bend_val * (p.midi.pitch_bend_range.clamp(1, 12) as f32);
                    self.engine.send_live(LiveEvent::SetMidiExpression {
                        bend_semi: semi,
                        aftertouch: pressure,
                    });
                }
            }
        }
    }

    fn apply_midi_cc_target(
        &mut self,
        p: &mut Project,
        target: MidiCcTarget,
        value: f32,
        playing: bool,
        cur_step: usize,
    ) {
        let v = value.clamp(0.0, 1.0);
        match target {
            MidiCcTarget::MasterFilter => {
                p.fx.global_filter_cutoff = 120.0 * (18_000.0_f32 / 120.0).powf(v);
                if self.rec_motion && playing {
                    p.current_pattern_mut().motion.filter[cur_step % 64] = Some(v);
                }
            }
            MidiCcTarget::MasterResonance => {
                p.fx.global_filter_resonance = v * 0.85;
                p.tb303.resonance = (v * 0.92).clamp(0.1, 0.94);
            }
            MidiCcTarget::ReverbSpace => {
                p.fx.reverb_mix = v;
                p.fx.delay_mix = (v * 0.92).min(0.85);
                if self.rec_motion && playing {
                    p.current_pattern_mut().motion.space[cur_step % 64] = Some(v);
                }
            }
            MidiCcTarget::DelayMix => {
                p.fx.delay_enabled = true;
                p.fx.delay_mix = v.min(0.90);
            }
            MidiCcTarget::MasterVolume => {
                p.mixer.master_volume = (v * 1.15).clamp(0.0, 1.15);
            }
            MidiCcTarget::SidechainPump => {
                p.sidechain_amount = v;
                if self.rec_motion && playing {
                    p.current_pattern_mut().motion.sidechain[cur_step % 64] = Some(v);
                }
            }
            MidiCcTarget::BeatEnergy => {
                p.beat_energy = 0.1 + v * 0.9;
            }
            MidiCcTarget::TrackVolume => {
                let vol = (v * 1.35).clamp(0.0, 1.35);
                if let Some(ei) = self.selected_extra {
                    if let Some(et) = p.extra_tracks.get_mut(ei) {
                        et.volume = vol;
                    }
                } else if self.selected_track == TrackId::MoogPad {
                    p.mixer.moog_pad_volume = vol;
                } else {
                    p.mixer.channels[self.selected_track.index()].volume = vol;
                }
            }
            MidiCcTarget::TrackCutoff => {
                let hz = 45.0 * (16_000.0_f32 / 45.0).powf(v);
                if let Some(ei) = self.selected_extra {
                    if let Some(et) = p.extra_tracks.get_mut(ei) {
                        et.synth.cutoff = hz;
                    }
                } else {
                    match self.selected_track {
                        TrackId::Moog => p.moog.cutoff = hz,
                        TrackId::MoogPad => p.moog_pad.cutoff = hz,
                        TrackId::Monologue => p.monologue.cutoff = hz,
                        TrackId::Tb303 => p.tb303.cutoff = hz.clamp(80.0, 6000.0),
                        TrackId::Sampler => {
                            p.sampler.filter_enabled = true;
                            p.sampler.cutoff = hz.clamp(150.0, 18_000.0);
                        }
                        TrackId::Drums | TrackId::MidiOut => {}
                    }
                }
            }
            MidiCcTarget::TrackResonance => {
                let r = (v * 0.94).clamp(0.0, 0.94);
                if let Some(ei) = self.selected_extra {
                    if let Some(et) = p.extra_tracks.get_mut(ei) {
                        et.synth.resonance = r;
                    }
                } else {
                    match self.selected_track {
                        TrackId::Moog => p.moog.resonance = r,
                        TrackId::MoogPad => p.moog_pad.resonance = r,
                        TrackId::Monologue => p.monologue.resonance = r,
                        TrackId::Tb303 => p.tb303.resonance = r,
                        TrackId::Sampler => {
                            p.sampler.filter_enabled = true;
                            p.sampler.resonance = r.min(0.88);
                        }
                        TrackId::Drums | TrackId::MidiOut => {}
                    }
                }
            }
            MidiCcTarget::TrackDrive => {
                if let Some(ei) = self.selected_extra {
                    if let Some(et) = p.extra_tracks.get_mut(ei) {
                        et.synth.drive = v;
                    }
                } else {
                    match self.selected_track {
                        TrackId::Moog => p.moog.drive = v,
                        TrackId::MoogPad => p.moog_pad.drive = v,
                        TrackId::Monologue => p.monologue.drive = v,
                        TrackId::Tb303 => p.tb303.overdrive = v,
                        TrackId::Sampler => p.sampler.bit_crush = v * 0.85,
                        TrackId::Drums | TrackId::MidiOut => {}
                    }
                }
            }
            MidiCcTarget::IsoBass => {
                p.sampler_iso_bass = v * 2.0;
                if self.rec_motion && playing {
                    p.current_pattern_mut().motion.iso_bass[cur_step % 64] = Some(v * 2.0);
                }
            }
            MidiCcTarget::IsoTreble => {
                p.sampler_iso_treble = v * 2.0;
                if self.rec_motion && playing {
                    p.current_pattern_mut().motion.iso_treble[cur_step % 64] = Some(v * 2.0);
                }
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

        // Spacebar: Play / Stop (ignore OS key auto-repeat)
        if ctx.input(|i| key_pressed_no_repeat(i, Key::Space)) {
            self.engine.playing.store(!playing, Ordering::Relaxed);
        }

        // Octave shift: Z / [ down, X / ] up (ignore OS key auto-repeat)
        if ctx.input(|i| key_pressed_no_repeat(i, Key::Z) || key_pressed_no_repeat(i, Key::OpenBracket)) {
            self.octave_offset = (self.octave_offset - 1).clamp(-2, 2);
        }
        if ctx.input(|i| key_pressed_no_repeat(i, Key::X) || key_pressed_no_repeat(i, Key::CloseBracket)) {
            self.octave_offset = (self.octave_offset + 1).clamp(-2, 2);
        }

        // Monologue Step Record navigation & Rest/Tie keys when armed & stopped
        if self.armed_record.is_some() && !playing {
            if ctx.input(|i| key_pressed_no_repeat(i, Key::Tab) || key_pressed_no_repeat(i, Key::Period)) {
                self.step_rec_rest(p);
            }
            if ctx.input(|i| key_pressed_no_repeat(i, Key::Minus)) {
                self.step_rec_tie(p);
            }
            if ctx.input(|i| key_pressed_no_repeat(i, Key::Backspace) || key_pressed_no_repeat(i, Key::ArrowLeft)) {
                self.step_rec_back(p);
            }
            if ctx.input(|i| key_pressed_no_repeat(i, Key::ArrowRight)) {
                self.step_rec_forward(p);
            }
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
        // X11 sends synthetic KeyRelease + KeyPress pairs (~0..20ms apart) on key repeat.
        // Debounce key releases briefly and cancel immediately if the key is still down or re-pressed.
        const RELEASE_DEBOUNCE_SECS: f64 = 0.035;
        let now = ctx.input(|i| i.time);

        for idx in 0..self.held_keys.len() {
            let key = self.held_keys[idx].0;
            let (released, down, any_press) =
                ctx.input(|i| (i.key_released(key), i.key_down(key), key_any_press_event(i, key)));
            if down || any_press {
                self.pending_key_releases.retain(|(k, _)| *k != key);
            } else if released && !self.pending_key_releases.iter().any(|(k, _)| *k == key) {
                self.pending_key_releases.push((key, now));
            }
        }
        let mut released_indices = Vec::new();
        for (idx, &(key, _)) in self.held_keys.iter().enumerate() {
            if let Some(&(_, t0)) = self.pending_key_releases.iter().find(|&&(k, _)| k == key) {
                if now - t0 >= RELEASE_DEBOUNCE_SECS {
                    released_indices.push(idx);
                }
            }
        }
        for idx in released_indices.into_iter().rev() {
            let (key, held) = self.held_keys.remove(idx);
            self.pending_key_releases.retain(|(k, _)| *k != key);
            self.release_held_note(ctx, p, held, playing);
        }
        self.pending_key_releases
            .retain(|(k, _)| self.held_keys.iter().any(|(hk, _)| hk == k));
        if !self.pending_key_releases.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        if let Some(ck) = self.held_chord_key {
            let (released, down, any_press) =
                ctx.input(|i| (i.key_released(ck), i.key_down(ck), key_any_press_event(i, ck)));
            if down || any_press {
                self.pending_chord_release = None;
            } else if released && self.pending_chord_release.is_none() {
                self.pending_chord_release = Some((ck, now));
            }
            if let Some((pck, t0)) = self.pending_chord_release {
                if pck == ck && now - t0 >= RELEASE_DEBOUNCE_SECS && !down && !any_press {
                    self.pending_chord_release = None;
                    self.held_chord_key = None;
                    self.release_held_note(ctx, p, HeldNote::AutoChord, playing);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                }
            }
        } else {
            self.pending_chord_release = None;
        }

        let target = self.effective_play_target(p);
        if target == RecordTarget::Core(TrackId::Sampler) {
            const NUM_KEYS: [Key; 8] = [
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
            ];
            for (idx, &key) in NUM_KEYS.iter().enumerate() {
                if ctx.input(|i| key_pressed_no_repeat(i, key))
                    && !self.held_keys.iter().any(|&(k, _)| k == key)
                {
                    let held = self.trigger_sampler_pad_down(ctx, p, idx, 0.90, playing, cur_step);
                    self.held_keys.push((key, held));
                }
            }
            if self.volca_ui.play_mode == crate::volca_ui::VolcaPlayMode::Pads8 {
                for (idx, &key) in PAD_KEYS.iter().enumerate() {
                    if ctx.input(|i| key_pressed_no_repeat(i, key))
                        && !self.held_keys.iter().any(|&(k, _)| k == key)
                    {
                        let held =
                            self.trigger_sampler_pad_down(ctx, p, idx, 0.90, playing, cur_step);
                        self.held_keys.push((key, held));
                    }
                }
            } else {
                let base = self.chromatic_base_midi();
                for &(key, semi, _) in &CHROMATIC_KEY_MAP {
                    if ctx.input(|i| key_pressed_no_repeat(i, key))
                        && !self.held_keys.iter().any(|&(k, _)| k == key)
                    {
                        let note = (base as i16 + semi as i16).clamp(12, 108) as u8;
                        let held = self.play_and_record_note_down(
                            ctx, p, target, note, 0.88, playing, cur_step,
                        );
                        self.held_keys.push((key, held));
                    }
                }
            }
        } else if self.keyboard_mode == KeyboardMode::Chords {
            for (idx, &key) in PAD_KEYS.iter().enumerate() {
                if ctx.input(|i| key_pressed_no_repeat(i, key)) {
                    if self.held_chord_key == Some(key) {
                        // Cancel synthetic X11 repeat release without re-triggering chord or resetting arp.
                        self.pending_chord_release = None;
                    } else {
                        self.pending_chord_release = None;
                        self.trigger_chord_pad_down(ctx, idx, p, pads, playing, cur_step);
                        self.held_chord_key = Some(key);
                    }
                }
            }
        } else if target == RecordTarget::Core(TrackId::Drums) {
            for (idx, &key) in PAD_KEYS.iter().enumerate() {
                if ctx.input(|i| key_pressed_no_repeat(i, key)) && !self.held_keys.iter().any(|&(k, _)| k == key) {
                    let held = self.trigger_drum_pad_down(ctx, p, idx, 0.90, playing, cur_step);
                    self.held_keys.push((key, held));
                }
            }
        } else if self.keyboard_mode == KeyboardMode::Scale {
            for (idx, &key) in PAD_KEYS.iter().enumerate() {
                if ctx.input(|i| key_pressed_no_repeat(i, key)) && !self.held_keys.iter().any(|&(k, _)| k == key) {
                    let note = self.scale_lead_note(p, idx);
                    let held = self.play_and_record_note_down(
                        ctx, p, target, note, 0.88, playing, cur_step,
                    );
                    self.held_keys.push((key, held));
                }
            }
        } else {
            // Monologue-style Chromatic 1.5-octave QWERTY Piano (A W S E D F T G Y H U J K O L P ; ')
            let base = self.chromatic_base_midi();
            for &(key, semi, _) in &CHROMATIC_KEY_MAP {
                if ctx.input(|i| key_pressed_no_repeat(i, key)) && !self.held_keys.iter().any(|&(k, _)| k == key) {
                    let note = (base as i16 + semi as i16).clamp(12, 108) as u8;
                    let held = self.play_and_record_note_down(
                        ctx, p, target, note, 0.88, playing, cur_step,
                    );
                    self.held_keys.push((key, held));
                }
            }
        }
    }

    fn scale_lead_note(&self, p: &Project, idx: usize) -> u8 {
        let base = (60 + self.octave_offset as i32 * 12).clamp(36, 84) as u8;
        let notes = p.scale.notes_in_range(p.root_note, base, (base + 24).min(104));
        notes.get(idx % notes.len().max(1)).copied().unwrap_or(60)
    }

    fn trigger_chord_pad_down(
        &mut self,
        ctx: &egui::Context,
        idx: usize,
        p: &mut Project,
        pads: &[AutoChordPad],
        playing: bool,
        cur_step: usize,
    ) {
        if let Some(pad) = pads.get(idx) {
            self.engine.send_live(LiveEvent::PlayChord {
                notes: pad.chord_notes.clone(),
                bass: Some(pad.bass_midi),
                velocity: 0.86,
                slot: pad.slot,
            });
            if let Some(armed) = self.armed_record {
                self.maybe_clear_for_replace(p, armed);
                let shift_acc = ctx.input(|i| i.modifiers.shift);
                let len = self.target_step_count(p, armed).as_usize();
                let s_idx = if playing {
                    let phase = self.engine.step_phase();
                    let q_global = if phase >= 0.65 { cur_step + 1 } else { cur_step };
                    if q_global > cur_step {
                        let (tr_opt, ex_opt) = match armed {
                            RecordTarget::Core(tr) => (Some(tr), None),
                            RecordTarget::Extra(ei) => (None, Some(ei)),
                        };
                        self.engine.send_live(LiveEvent::SuppressTrackStep {
                            track: tr_opt,
                            extra_idx: ex_opt,
                            lane: None,
                            global_step: q_global,
                        });
                    }
                    q_global % len
                } else {
                    self.step_rec_cursor % len
                };

                let pat = p.current_pattern_mut();
                match armed {
                    RecordTarget::Core(TrackId::Moog) => {
                        let st = &mut pat.moog.steps[s_idx];
                        st.active = true;
                        st.note = pad.bass_midi;
                        st.velocity = 0.88;
                        st.gate = 4.0;
                        st.accent = shift_acc || self.step_rec_accent;
                    }
                    RecordTarget::Core(tr) if tr != TrackId::Drums && tr != TrackId::Sampler => {
                        if let Some(seq) = pat.melodic_mut(tr) {
                            let st = &mut seq.steps[s_idx];
                            st.active = true;
                            st.note = pad.chord_notes.first().copied().unwrap_or(60);
                            st.chord = pad.slot.chord;
                            st.velocity = 0.84;
                            st.gate = 8.0;
                            st.accent = shift_acc || self.step_rec_accent;
                        }
                    }
                    RecordTarget::Extra(ei) => {
                        if let Some(seq) = pat.extra_seqs.get_mut(ei) {
                            let st = &mut seq.steps[s_idx];
                            st.active = true;
                            st.note = pad.chord_notes.first().copied().unwrap_or(60);
                            st.chord = pad.slot.chord;
                            st.velocity = 0.84;
                            st.gate = 8.0;
                            st.accent = shift_acc || self.step_rec_accent;
                        }
                    }
                    _ => {}
                }
                if !playing {
                    self.step_rec_last_step = Some(s_idx);
                    self.advance_step_rec_cursor(len);
                } else {
                    self.selected_step = s_idx;
                }
            }
        }
    }
}

const CHROMATIC_KEY_MAP: [(Key, i8, &str); 18] = [
    (Key::A, 0, "A"),
    (Key::W, 1, "W"),
    (Key::S, 2, "S"),
    (Key::E, 3, "E"),
    (Key::D, 4, "D"),
    (Key::F, 5, "F"),
    (Key::T, 6, "T"),
    (Key::G, 7, "G"),
    (Key::Y, 8, "Y"),
    (Key::H, 9, "H"),
    (Key::U, 10, "U"),
    (Key::J, 11, "J"),
    (Key::K, 12, "K"),
    (Key::O, 13, "O"),
    (Key::L, 14, "L"),
    (Key::P, 15, "P"),
    (Key::Semicolon, 16, ";"),
    (Key::Quote, 17, "'"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeldNote {
    Melodic(RecordTarget, u8),
    Drum(usize),
    SamplerPad(usize),
    SamplerPitched(usize, u8),
    AutoChord,
}

impl eframe::App for FunKeySynthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
        self.render_ui(ctx);
    }
}

#[derive(Clone, Copy)]
enum TopUtilItem {
    Prefs,
    Update,
    MidiFile,
    Stems,
    Wav,
    Load,
    Save,
    Sep,
    Midi,
    Redo,
    Undo,
}

impl FunKeySynthApp {
    fn ui_top_brand_tabs(&mut self, ui: &mut egui::Ui, accent: Color32) {
        ui.add(
            egui::Image::new(&self.app_icon)
                .fit_to_exact_size(Vec2::new(24.0, 24.0))
                .rounding(Rounding::same(5.0)),
        );
        ui.label(
            RichText::new("FUN KEY SYNTH")
                .strong()
                .size(15.0)
                .color(accent),
        );
        ui.separator();

        for &(tab, label) in &[
            (StudioTab::Studio, "STUDIO"),
            (StudioTab::Sampler, "SAMPLER"),
            (StudioTab::ArrangeParty, "ARRANGE & PARTY"),
            (StudioTab::SoundLab, "SOUND LAB"),
        ] {
            let active = self.tab == tab;
            let btn = egui::Button::new(
                RichText::new(label)
                    .strong()
                    .size(12.0)
                    .color(if active {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(160, 170, 192)
                    }),
            )
            .fill(if active {
                Color32::from_rgb(46, 56, 78)
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::new(
                1.0_f32,
                if active { accent.gamma_multiply(0.7) } else { Color32::TRANSPARENT },
            ));
            if ui.add(btn).clicked() {
                self.tab = tab;
            }
        }
    }

    fn ui_top_utils(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, proj: &mut Project, rtl: bool) {
        const ITEMS: [TopUtilItem; 11] = [
            TopUtilItem::Prefs,
            TopUtilItem::Update,
            TopUtilItem::MidiFile,
            TopUtilItem::Stems,
            TopUtilItem::Wav,
            TopUtilItem::Load,
            TopUtilItem::Save,
            TopUtilItem::Sep,
            TopUtilItem::Midi,
            TopUtilItem::Redo,
            TopUtilItem::Undo,
        ];
        let count = ITEMS.len();
        for idx in 0..count {
            let item = if rtl { ITEMS[idx] } else { ITEMS[count - 1 - idx] };
            match item {
                TopUtilItem::Undo => {
                    if ui
                        .add_enabled(!self.undo_stack.is_empty(), egui::Button::new("↶"))
                        .on_hover_text("Undo (Ctrl+Z)")
                        .clicked()
                    {
                        self.perform_undo(ctx, proj);
                    }
                }
                TopUtilItem::Redo => {
                    if ui
                        .add_enabled(!self.redo_stack.is_empty(), egui::Button::new("↷"))
                        .on_hover_text("Redo (Ctrl+Y)")
                        .clicked()
                    {
                        self.perform_redo(ctx, proj);
                    }
                }
                TopUtilItem::Prefs => {
                    if ui
                        .button("Prefs")
                        .on_hover_text("Color themes, custom accent & GitHub update check settings")
                        .clicked()
                    {
                        self.show_settings = !self.show_settings;
                    }
                }
                TopUtilItem::Update => {
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
                }
                TopUtilItem::MidiFile => {
                    if ui
                        .button("MID")
                        .on_hover_text("Export Pattern or full Song Arrangement as a multi-track Standard MIDI (.mid) file")
                        .clicked()
                    {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(format!("{}.mid", proj.vibe.name().to_lowercase().replace(' ', "_")))
                            .add_filter("Standard MIDI", &["mid", "midi"])
                            .save_file()
                        {
                            match export_project_midi(proj, &path) {
                                Ok(()) => self.set_toast(ctx, "Exported Standard MIDI (.mid) tracks!"),
                                Err(e) => self.set_toast(ctx, format!("MIDI export error: {e}")),
                            }
                        }
                    }
                }
                TopUtilItem::Stems => {
                    if ui
                        .button("Stems")
                        .on_hover_text("Export Master + 6 separate stereo WAV stems for DAW mixing")
                        .clicked()
                    {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            let bars = if proj.song_mode {
                                proj.song_sections.iter().map(|s| s.bars.max(1) as usize).sum::<usize>().clamp(4, 128)
                            } else {
                                16
                            };
                            match export_stems(proj, &dir, bars) {
                                Ok(n) => self.set_toast(ctx, format!("Exported {n} stereo WAV stems!")),
                                Err(e) => self.set_toast(ctx, format!("Stem export error: {e}")),
                            }
                        }
                    }
                }
                TopUtilItem::Wav => {
                    if ui.button("WAV").on_hover_text("Export stereo mix WAV").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name(format!("{}.wav", proj.vibe.name().to_lowercase().replace(' ', "_")))
                            .add_filter("WAV Audio", &["wav"])
                            .save_file()
                        {
                            let bars = if proj.song_mode {
                                proj.song_sections.iter().map(|s| s.bars.max(1) as usize).sum::<usize>().clamp(4, 128)
                            } else {
                                16
                            };
                            match export_wav(proj, &path, bars) {
                                Ok(()) => self.set_toast(ctx, format!("Exported {bars} bars WAV!")),
                                Err(e) => self.set_toast(ctx, format!("Export error: {e}")),
                            }
                        }
                    }
                }
                TopUtilItem::Load => {
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
                                    *proj = loaded;
                                    self.keyboard_mode = if proj.auto_chord_mode {
                                        KeyboardMode::Chords
                                    } else {
                                        KeyboardMode::Chromatic
                                    };
                                    self.armed_record = None;
                                    self.set_toast(ctx, "Project loaded (all patterns, synths & samples)!");
                                }
                                Err(e) => self.set_toast(ctx, format!("Load error: {e}")),
                            }
                        }
                    }
                }
                TopUtilItem::Save => {
                    if ui.button("Save").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("my_vibe_track.json")
                            .add_filter("Fun Key Project", &["json"])
                            .save_file()
                        {
                            match save_project(&path, proj) {
                                Ok(()) => self.set_toast(ctx, "Project saved!"),
                                Err(e) => self.set_toast(ctx, format!("Save error: {e}")),
                            }
                        }
                    }
                }
                TopUtilItem::Sep => {
                    ui.separator();
                }
                TopUtilItem::Midi => {
                    let midi_lbl = if let Some(tgt) = self.midi_learn_target {
                        format!("LEARN: {}", tgt.name().chars().take(7).collect::<String>())
                    } else if let Some(ref name) = self.midi.connected_name {
                        format!("MIDI: {}", name.chars().take(10).collect::<String>())
                    } else {
                        "MIDI".to_string()
                    };
                    egui::ComboBox::from_id_salt("midi_port_combo")
                        .width(102.0)
                        .selected_text(midi_lbl)
                        .show_ui(ui, |ui| {
                            ui.set_min_width(250.0);
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
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Pitch Bend:").size(11.0));
                                let mut pb = proj.midi.pitch_bend_range.clamp(1, 12);
                                if ui.add(egui::DragValue::new(&mut pb).range(1..=12).suffix(" st")).changed() {
                                    proj.midi.pitch_bend_range = pb;
                                }
                                if let Some((lcc, lval)) = self.last_midi_cc {
                                    ui.label(RichText::new(format!("Last CC{lcc}={:.0}%", lval * 100.0)).size(10.5).weak());
                                }
                            });
                            ui.label(RichText::new("MIDI CC Learn (click target then twist knob):").size(11.0).strong());
                            for &tgt in &MidiCcTarget::ALL {
                                let mapped_ccs: Vec<u8> = proj
                                    .midi
                                    .cc_mappings
                                    .iter()
                                    .filter(|m| m.target == tgt)
                                    .map(|m| m.cc)
                                    .collect();
                                let cc_str = if mapped_ccs.is_empty() {
                                    "—".to_string()
                                } else {
                                    mapped_ccs
                                        .iter()
                                        .map(|c| format!("CC{c}"))
                                        .collect::<Vec<_>>()
                                        .join(",")
                                };
                                let is_learning = self.midi_learn_target == Some(tgt);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(tgt.name()).size(11.0));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui
                                            .selectable_label(
                                                is_learning,
                                                if is_learning { "Listening…" } else { "Learn" },
                                            )
                                            .clicked()
                                        {
                                            self.midi_learn_target = if is_learning { None } else { Some(tgt) };
                                        }
                                        ui.label(RichText::new(cc_str).size(10.5).weak());
                                    });
                                });
                            }
                        });
                }
            }
        }
    }

    pub fn render_ui(&mut self, ctx: &egui::Context) {
        let playing = self.engine.playing.load(Ordering::Relaxed);
        let cur_step = self.engine.current_step.load(Ordering::Relaxed);
        let telemetry = self.engine.telemetry();
        self.engine.scope.snapshot(&mut self.scope_buf);

        // Lock project once for the frame
        let mut proj = match self.engine.project.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        self.undo_action_this_frame = false;
        if !ctx.wants_keyboard_input() {
            let do_undo = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Z));
            let do_redo = ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND, Key::Y)
                    || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z)
            });
            if do_undo {
                self.perform_undo(ctx, &mut proj);
            } else if do_redo {
                self.perform_redo(ctx, &mut proj);
            }
        }
        let frame_start_proj = proj.clone();
        let pads = build_auto_chord_pads(proj.root_note, proj.scale);
        let accent = if self.settings.use_vibe_accent {
            vibe_color(proj.vibe)
        } else {
            let [r, g, b] = self.settings.custom_accent;
            Color32::from_rgb(r, g, b)
        };
        let theme = self.settings.theme;

        self.handle_keyboard(ctx, &mut proj, &pads, playing, cur_step);
        self.handle_midi(ctx, &mut proj, &pads, playing, cur_step);
        self.update_test_repeat(ctx, &proj, playing);

        if playing && self.follow_playhead {
            let active_target = self.effective_play_target(&proj);
            let active_len = self.target_step_count(&proj, active_target).as_usize();
            self.bar_page = (cur_step % active_len) / 16;
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
                // Row 1: Brand + Primary Navigation Tabs (Left) & MIDI / Save / Load / Export / Prefs (Right)
                if ui.available_width() >= 1240.0 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        self.ui_top_brand_tabs(ui, accent);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            self.ui_top_utils(ui, ctx, &mut proj, true);
                        });
                    });
                } else {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        self.ui_top_brand_tabs(ui, accent);
                    });
                    ui.add_space(3.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        self.ui_top_utils(ui, ctx, &mut proj, false);
                    });
                }

                ui.add_space(4.0);

                // Row 2: Transport, Key / Scale, Mode Pills & Live Chord / DJ Status
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

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
                    .min_size(Vec2::new(82.0, 26.0));
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
                        .width(56.0)
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

                    ui.separator();

                    // Mode Toggles: Auto-Beat, Auto-Chord, Party Mode
                    toggle_pill(
                        ui,
                        "AUTO-BEAT",
                        &mut proj.auto_beat_mode,
                        Color32::from_rgb(255, 168, 52),
                        "Automatically plays groovy drums matched to your Vibe & Beat Energy",
                    );
                    if toggle_pill(
                        ui,
                        "AUTO-CHORD",
                        &mut proj.auto_chord_mode,
                        Color32::from_rgb(92, 198, 255),
                        "1-key lush voice-led chords on keys A-K",
                    ) {
                        self.keyboard_mode = if proj.auto_chord_mode {
                            KeyboardMode::Chords
                        } else {
                            KeyboardMode::Chromatic
                        };
                    }
                    if toggle_pill(
                        ui,
                        if proj.party_mode { "PARTY DJ ON" } else { "PARTY DJ" },
                        &mut proj.party_mode,
                        Color32::from_rgb(244, 92, 186),
                        "Party DJ Mode: continuously generates, plays, and mixes into new songs in your chosen Vibe!",
                    ) && proj.party_mode
                        && !playing
                    {
                        self.engine.playing.store(true, Ordering::Relaxed);
                    }
                    if proj.party_mode {
                        if ui
                            .button("Next Song ⏭")
                            .on_hover_text("DJ mix into a fresh generated song with new sounds, groove & arrangement right now!")
                            .clicked()
                        {
                            proj.party_seed = self.next_seed();
                            proj.party_mode = true;
                            self.engine.send_live(LiveEvent::NextPartySong);
                            self.engine.playing.store(true, Ordering::Relaxed);
                        }
                        if ui
                            .button("Reload Rig")
                            .on_hover_text("Load a fresh set of genre-matched instruments for the current DJ song")
                            .clicked()
                        {
                            self.engine.send_live(LiveEvent::ReloadPartyRig);
                        }
                    }

                    // Live Chord Readout Badge
                    let chord_text = if telemetry.active_chord.is_empty() {
                        "Ready".to_string()
                    } else if !telemetry.party_song_title.is_empty() {
                        let bpm_disp = if telemetry.party_bpm >= 40.0 {
                            telemetry.party_bpm.round() as u32
                        } else {
                            proj.bpm.round() as u32
                        };
                        format!(
                            "DJ {}  •  {} BPM  •  {} {}/{}  •  {}",
                            telemetry.party_song_title,
                            bpm_disp,
                            telemetry.party_section,
                            telemetry.party_bar,
                            telemetry.party_section_bars,
                            telemetry.active_chord
                        )
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
                });
            });

        // ==================== BOTTOM DOCK: CHROMATIC KEYBOARD / DRUM PADS / CHORDS ====================
        egui::TopBottomPanel::bottom("playable_pads_dock")
            .frame(
                egui::Frame::none()
                    .fill(theme.bar_bg())
                    .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                    .stroke(Stroke::new(1.0_f32, theme.border())),
            )
            .show(ctx, |ui| {
                let play_target = self.effective_play_target(&proj);
                let is_armed_here = self.armed_record == Some(play_target);
                let rec_col = Color32::from_rgb(228, 54, 70);
                let tc = Self::target_color(&proj, play_target);

                // Vintage Hardware Arpeggiator Strip (Jupiter-8 / Juno-60 style)
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().item_spacing.y = 2.0;
                    toggle_pill(
                        ui,
                        if proj.arp.enabled { "ARP ON" } else { "ARP" },
                        &mut proj.arp.enabled,
                        Color32::from_rgb(255, 184, 58),
                        "Classic Jupiter-8 / Juno-60 style arpeggiator: hold keys, MIDI notes, or Auto-Chord pads",
                    );
                    toggle_pill(
                        ui,
                        "HOLD",
                        &mut proj.arp.latch,
                        Color32::from_rgb(92, 214, 178),
                        "Latch / Hold arpeggiated notes hands-free until you play a new chord or toggle HOLD off",
                    );
                    ui.separator();
                    for &m in &ArpMode::ALL {
                        ui.selectable_value(&mut proj.arp.mode, m, m.name());
                    }
                    ui.separator();
                    for &r in &ArpRate::ALL {
                        ui.selectable_value(&mut proj.arp.rate, r, r.name());
                    }
                    ui.separator();
                    for oct in 1_u8..=3 {
                        ui.selectable_value(&mut proj.arp.octaves, oct, format!("{oct}O"));
                    }
                    ui.separator();
                    let mut gate_pct = (proj.arp.gate * 100.0).round();
                    if ui
                        .add(
                            egui::DragValue::new(&mut gate_pct)
                                .range(15.0..=95.0)
                                .speed(0.5)
                                .prefix("Gate ")
                                .suffix("%"),
                        )
                        .on_hover_text("Arpeggiator note gate length (15% staccato .. 95% legato)")
                        .changed()
                    {
                        proj.arp.gate = (gate_pct / 100.0).clamp(0.15, 0.95);
                    }
                    ui.checkbox(&mut proj.arp.swing, "Swing")
                        .on_hover_text("Apply global groove swing to 1/16 arpeggiator steps");
                });
                ui.add_space(3.0);

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(186.0);
                        ui.set_max_width(186.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if ui
                                .selectable_label(self.keyboard_mode == KeyboardMode::Chromatic, "Keys")
                                .on_hover_text("25-key chromatic Monologue keyboard (A W S E D F T G Y H U J K O L P ; ')")
                                .clicked()
                            {
                                self.keyboard_mode = KeyboardMode::Chromatic;
                                proj.auto_chord_mode = false;
                            }
                            if ui
                                .selectable_label(self.keyboard_mode == KeyboardMode::Scale, "Scale")
                                .on_hover_text("8 scale-locked pads / 8 sample pads on keys A–K")
                                .clicked()
                            {
                                self.keyboard_mode = KeyboardMode::Scale;
                                proj.auto_chord_mode = false;
                            }
                            if ui
                                .selectable_label(self.keyboard_mode == KeyboardMode::Chords, "Chords")
                                .on_hover_text("8 Auto-Chord voice-led pads on keys A–K")
                                .clicked()
                            {
                                self.keyboard_mode = KeyboardMode::Chords;
                                proj.auto_chord_mode = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let rec_btn = egui::Button::new(
                                RichText::new("● REC")
                                    .strong()
                                    .size(11.0)
                                    .color(Color32::WHITE),
                            )
                            .fill(if is_armed_here {
                                rec_col
                            } else {
                                Color32::from_rgb(44, 34, 42)
                            });
                            if ui
                                .add(rec_btn)
                                .on_hover_text("Toggle Record Mode for the active instrument (step record when stopped, live when playing)")
                                .clicked()
                            {
                                self.toggle_arm_record(ctx, &mut proj, play_target);
                            }
                            if ui
                                .selectable_label(self.rec_replace, "Rep")
                                .on_hover_text("Replace mode: clears track once on the first recorded note")
                                .clicked()
                            {
                                self.rec_replace = true;
                            }
                            if ui
                                .selectable_label(!self.rec_replace, "Ovd")
                                .on_hover_text("Overdub mode: layer notes onto existing steps")
                                .clicked()
                            {
                                self.rec_replace = false;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            if self.keyboard_mode == KeyboardMode::Chords {
                                ui.checkbox(&mut proj.auto_chord_follow, "Band Follow")
                                    .on_hover_text("Bass & Arp follow the chord pad you hold!");
                            } else if play_target == RecordTarget::Core(TrackId::Drums) {
                                ui.label(RichText::new("DRUM PADS (A–K)").strong().size(10.5).color(tc));
                            } else if self.keyboard_mode == KeyboardMode::Scale
                                && play_target == RecordTarget::Core(TrackId::Sampler)
                            {
                                ui.label(RichText::new("8-PAD VOLCA (A–K)").strong().size(10.5).color(tc));
                            } else {
                                let base = if self.keyboard_mode == KeyboardMode::Chromatic {
                                    self.chromatic_base_midi()
                                } else {
                                    (60 + self.octave_offset as i32 * 12).clamp(36, 84) as u8
                                };
                                let span_lbl = if self.keyboard_mode == KeyboardMode::Chromatic {
                                    format!("{}–{}", note_name(base), note_name((base + 24).min(108)))
                                } else {
                                    format!("Oct {:+}", self.octave_offset)
                                };
                                ui.label(RichText::new(span_lbl).size(11.0).color(tc));
                                if ui.small_button("-").on_hover_text("Octave Down (Z or [)").clicked() {
                                    self.octave_offset = (self.octave_offset - 1).clamp(-2, 2);
                                }
                                if ui.small_button("+").on_hover_text("Octave Up (X or ])").clicked() {
                                    self.octave_offset = (self.octave_offset + 1).clamp(-2, 2);
                                }
                            }
                        });
                    });

                    // Middle Playable Surface: 25-Key Chromatic Piano OR 8 Large Performance Pads
                    let pads_total_w = (ui.available_width() - 184.0).max(240.0);
                    let pad_h = 74.0;
                    let show_8_pads = self.keyboard_mode == KeyboardMode::Chords
                        || self.keyboard_mode == KeyboardMode::Scale
                        || play_target == RecordTarget::Core(TrackId::Drums);

                    if show_8_pads {
                        let pad_w = ((pads_total_w - 7.0 * 6.0) / 8.0).clamp(32.0, 150.0);
                        ui.allocate_ui(Vec2::new(pads_total_w, pad_h), |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                for (idx, pad) in pads.iter().enumerate() {
                                    let is_held = self.held_mouse_pad == Some(idx)
                                        || (self.keyboard_mode == KeyboardMode::Chords
                                            && self.held_chord_key.is_some()
                                            && ctx.input(|i| {
                                                i.key_down(
                                                    [
                                                        Key::A,
                                                        Key::S,
                                                        Key::D,
                                                        Key::F,
                                                        Key::G,
                                                        Key::H,
                                                        Key::J,
                                                        Key::K,
                                                    ][idx],
                                                )
                                            }))
                                        || self.held_keys.iter().any(|&(_, h)| match h {
                                            HeldNote::Drum(l) => l == idx,
                                            HeldNote::SamplerPad(p_i) => p_i == idx,
                                            HeldNote::Melodic(_, m) => {
                                                self.keyboard_mode == KeyboardMode::Scale
                                                    && m == self.scale_lead_note(&proj, idx)
                                            }
                                            _ => false,
                                        });

                                    let main_label = if self.keyboard_mode == KeyboardMode::Chords {
                                        pad.chord_name.clone()
                                    } else if play_target == RecordTarget::Core(TrackId::Drums) {
                                        let voice = DrumVoiceId::from_index(idx).unwrap();
                                        let vm = proj.drums.voices[idx].model;
                                        voice.short_name_for_model(vm).to_string()
                                    } else if play_target == RecordTarget::Core(TrackId::Sampler) {
                                        format!("P{}: {}", idx + 1, proj.sample_parts[idx].name.chars().take(5).collect::<String>())
                                    } else {
                                        note_name(self.scale_lead_note(&proj, idx))
                                    };
                                    let pad_accent = if is_armed_here {
                                        rec_col
                                    } else if self.keyboard_mode == KeyboardMode::Chords {
                                        accent
                                    } else {
                                        tc
                                    };
                                    let bg = if is_held {
                                        pad_accent
                                    } else {
                                        Color32::from_rgb(26, 32, 46)
                                    };
                                    let fg = if is_held {
                                        Color32::from_rgb(12, 15, 22)
                                    } else {
                                        Color32::from_rgb(232, 238, 250)
                                    };
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(pad_w, pad_h),
                                        Sense::click_and_drag(),
                                    );
                                    if resp.is_pointer_button_down_on()
                                        && self.held_mouse_pad != Some(idx)
                                    {
                                        if self.keyboard_mode == KeyboardMode::Chords {
                                            self.trigger_chord_pad_down(
                                                ctx, idx, &mut proj, &pads, playing, cur_step,
                                            );
                                            self.held_mouse = Some(HeldNote::AutoChord);
                                        } else if play_target == RecordTarget::Core(TrackId::Drums) {
                                            let held = self.trigger_drum_pad_down(
                                                ctx, &mut proj, idx, 0.90, playing, cur_step,
                                            );
                                            self.held_mouse = Some(held);
                                        } else if play_target == RecordTarget::Core(TrackId::Sampler) {
                                            let held = self.trigger_sampler_pad_down(
                                                ctx, &mut proj, idx, 0.90, playing, cur_step,
                                            );
                                            self.held_mouse = Some(held);
                                        } else {
                                            let note = self.scale_lead_note(&proj, idx);
                                            let held = self.play_and_record_note_down(
                                                ctx, &mut proj, play_target, note, 0.88, playing, cur_step,
                                            );
                                            self.held_mouse = Some(held);
                                        }
                                        self.held_mouse_pad = Some(idx);
                                    } else if !resp.is_pointer_button_down_on()
                                        && self.held_mouse_pad == Some(idx)
                                    {
                                        if let Some(held) = self.held_mouse.take() {
                                            self.release_held_note(ctx, &mut proj, held, playing);
                                        }
                                        self.held_mouse_pad = None;
                                    }
                                    let painter = ui.painter();
                                    painter.rect_filled(rect, Rounding::same(8.0), bg);
                                    painter.rect_stroke(
                                        rect,
                                        Rounding::same(8.0),
                                        Stroke::new(
                                            if is_held { 2.0_f32 } else { 1.0_f32 },
                                            if is_held {
                                                Color32::WHITE
                                            } else {
                                                pad_accent.gamma_multiply(0.45)
                                            },
                                        ),
                                    );
                                    painter.text(
                                        rect.left_top() + Vec2::new(7.0, 5.0),
                                        egui::Align2::LEFT_TOP,
                                        pad.key_label,
                                        egui::FontId::proportional(10.0),
                                        if is_held {
                                            Color32::from_rgb(20, 24, 32)
                                        } else {
                                            Color32::from_rgb(130, 145, 175)
                                        },
                                    );
                                    painter.text(
                                        rect.center() + Vec2::new(0.0, 6.0),
                                        egui::Align2::CENTER_CENTER,
                                        main_label,
                                        egui::FontId::proportional(11.0),
                                        fg,
                                    );
                                }
                            });
                        });
                    } else {
                        // 25-Key (2-Octave) Monologue-Style Chromatic Piano Keyboard
                        let base = self.chromatic_base_midi();
                        let (kb_rect, kb_resp) = ui.allocate_exact_size(
                            Vec2::new(pads_total_w, pad_h),
                            Sense::click_and_drag(),
                        );
                        const WHITE_SEMIS: [i8; 15] =
                            [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23, 24];
                        const BLACK_LAYOUT: [(usize, i8); 10] = [
                            (0, 1),
                            (1, 3),
                            (3, 6),
                            (4, 8),
                            (5, 10),
                            (7, 13),
                            (8, 15),
                            (10, 18),
                            (11, 20),
                            (12, 22),
                        ];
                        let white_w = kb_rect.width() / 15.0;
                        let black_w = (white_w * 0.62).clamp(10.0, 24.0);
                        let black_h = (kb_rect.height() * 0.55).round();

                        let white_rect = |wi: usize| {
                            let x0 = kb_rect.left() + wi as f32 * white_w;
                            let x1 = x0 + white_w - 1.5;
                            egui::Rect::from_min_max(
                                egui::pos2(x0, kb_rect.top()),
                                egui::pos2(x1, kb_rect.bottom()),
                            )
                        };
                        let black_rect = |w_idx: usize| {
                            let cx = kb_rect.left() + (w_idx + 1) as f32 * white_w - 0.75;
                            egui::Rect::from_min_max(
                                egui::pos2(cx - black_w * 0.5, kb_rect.top()),
                                egui::pos2(cx + black_w * 0.5, kb_rect.top() + black_h),
                            )
                        };

                        if kb_resp.is_pointer_button_down_on() {
                            if let Some(pos) = kb_resp.interact_pointer_pos() {
                                let mut hit_semi: Option<i8> = None;
                                for &(w_idx, semi) in &BLACK_LAYOUT {
                                    if black_rect(w_idx).contains(pos) {
                                        hit_semi = Some(semi);
                                        break;
                                    }
                                }
                                if hit_semi.is_none() {
                                    for (wi, &semi) in WHITE_SEMIS.iter().enumerate() {
                                        if white_rect(wi).contains(pos) {
                                            hit_semi = Some(semi);
                                            break;
                                        }
                                    }
                                }
                                if let Some(semi) = hit_semi {
                                    let midi = (base as i16 + semi as i16).clamp(12, 108) as u8;
                                    let same_held = matches!(
                                        self.held_mouse,
                                        Some(HeldNote::Melodic(t, m)) if t == play_target && m == midi
                                    ) || matches!(
                                        self.held_mouse,
                                        Some(HeldNote::SamplerPitched(_, m)) if play_target == RecordTarget::Core(TrackId::Sampler) && m == midi
                                    );
                                    if !same_held {
                                        if let Some(prev) = self.held_mouse.take() {
                                            self.release_held_note(ctx, &mut proj, prev, playing);
                                        }
                                        let held = self.play_and_record_note_down(
                                            ctx, &mut proj, play_target, midi, 0.88, playing, cur_step,
                                        );
                                        self.held_mouse = Some(held);
                                    }
                                }
                            }
                        } else if self.held_mouse.is_some() && self.held_mouse_pad.is_none() {
                            if let Some(prev) = self.held_mouse.take() {
                                self.release_held_note(ctx, &mut proj, prev, playing);
                            }
                        }

                        let key_lbl_for = |semi: i8| -> Option<&'static str> {
                            CHROMATIC_KEY_MAP
                                .iter()
                                .find(|&&(_, s, _)| s == semi)
                                .map(|&(_, _, lbl)| lbl)
                        };
                        let is_midi_held = |midi: u8| -> bool {
                            self.held_keys.iter().any(|&(_, h)| match h {
                                HeldNote::Melodic(_, m) | HeldNote::SamplerPitched(_, m) => m == midi,
                                _ => false,
                            }) || match self.held_mouse {
                                Some(HeldNote::Melodic(_, m)) | Some(HeldNote::SamplerPitched(_, m)) => {
                                    m == midi
                                }
                                _ => false,
                            }
                        };
                        let active_col = if is_armed_here { rec_col } else { tc };
                        let root_pc = proj.root_note % 12;
                        let scale_ivs = proj.scale.intervals();
                        let painter = ui.painter();

                        // 1. Draw 15 White Keys
                        for (wi, &semi) in WHITE_SEMIS.iter().enumerate() {
                            let rect = white_rect(wi);
                            let midi = (base as i16 + semi as i16).clamp(12, 108) as u8;
                            let held = is_midi_held(midi);
                            let pc = (midi + 12 - root_pc) % 12;
                            let in_scale = scale_ivs.contains(&pc);
                            let is_root = pc == 0;
                            let bg = if held {
                                active_col
                            } else if is_root {
                                Color32::from_rgb(236, 242, 255)
                            } else {
                                Color32::from_rgb(212, 220, 234)
                            };
                            painter.rect_filled(rect, Rounding::same(4.0), bg);
                            painter.rect_stroke(
                                rect,
                                Rounding::same(4.0),
                                Stroke::new(
                                    if held { 1.5_f32 } else { 1.0_f32 },
                                    if held {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(48, 56, 74)
                                    },
                                ),
                            );
                            if !held && in_scale {
                                painter.circle_filled(
                                    egui::pos2(rect.center().x, rect.bottom() - 31.0),
                                    if is_root { 3.0 } else { 2.0 },
                                    if is_root {
                                        tc
                                    } else {
                                        tc.gamma_multiply(0.55)
                                    },
                                );
                            }
                            if let Some(klbl) = key_lbl_for(semi) {
                                painter.text(
                                    egui::pos2(rect.center().x, rect.bottom() - 21.0),
                                    egui::Align2::CENTER_CENTER,
                                    klbl,
                                    egui::FontId::proportional(9.5),
                                    Color32::from_rgb(22, 28, 40),
                                );
                            }
                            if white_w >= 28.0 || (midi % 12 == 0) {
                                painter.text(
                                    egui::pos2(rect.center().x, rect.bottom() - 7.5),
                                    egui::Align2::CENTER_CENTER,
                                    note_name(midi),
                                    egui::FontId::proportional(8.0),
                                    if held {
                                        Color32::from_rgb(12, 15, 22)
                                    } else {
                                        Color32::from_rgb(70, 82, 104)
                                    },
                                );
                            }
                        }

                        // 2. Draw 10 Raised Black Keys on top
                        for &(w_idx, semi) in &BLACK_LAYOUT {
                            let rect = black_rect(w_idx);
                            let midi = (base as i16 + semi as i16).clamp(12, 108) as u8;
                            let held = is_midi_held(midi);
                            let pc = (midi + 12 - root_pc) % 12;
                            let in_scale = scale_ivs.contains(&pc);
                            let bg = if held {
                                active_col
                            } else {
                                Color32::from_rgb(24, 28, 40)
                            };
                            painter.rect_filled(rect, Rounding::same(3.0), bg);
                            painter.rect_stroke(
                                rect,
                                Rounding::same(3.0),
                                Stroke::new(
                                    1.0_f32,
                                    if held {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_rgb(70, 80, 104)
                                    },
                                ),
                            );
                            if !held && in_scale {
                                painter.circle_filled(
                                    egui::pos2(rect.center().x, rect.bottom() - 6.0),
                                    2.0,
                                    if pc == 0 {
                                        tc
                                    } else {
                                        tc.gamma_multiply(0.55)
                                    },
                                );
                            }
                            if let Some(klbl) = key_lbl_for(semi) {
                                painter.text(
                                    egui::pos2(rect.center().x, rect.top() + 14.0),
                                    egui::Align2::CENTER_CENTER,
                                    klbl,
                                    egui::FontId::proportional(9.0),
                                    if held {
                                        Color32::from_rgb(12, 15, 22)
                                    } else {
                                        Color32::from_rgb(190, 200, 222)
                                    },
                                );
                            }
                        }
                    }

                    // Mini Oscilloscope + Live Stereo VU & Sidechain Meter + Status
                    ui.vertical(|ui| {
                        ui.set_width(176.0);
                        ui.set_max_width(176.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let scope_rect = ui.allocate_exact_size(Vec2::new(128.0, 46.0), Sense::hover()).0;
                            draw_scope(ui.painter(), scope_rect, &self.scope_buf, accent);
                            let meter_rect = ui.allocate_exact_size(Vec2::new(36.0, 46.0), Sense::hover()).0;
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
                        ui.add(
                            egui::Label::new(RichText::new(status_text).size(10.5).weak()).truncate(),
                        );
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
                    StudioTab::Studio => {
                        self.ui_studio(ui, ctx, &mut proj, &telemetry, playing, cur_step, accent, theme)
                    }
                    StudioTab::Sampler => {
                        let mut sampler_armed =
                            self.armed_record == Some(RecordTarget::Core(TrackId::Sampler));
                        let was_armed = sampler_armed;
                        let step_phase = self.engine.step_phase();
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
                            &mut sampler_armed,
                            &mut self.step_rec_cursor,
                            self.rec_replace,
                            &mut self.rec_take_cleared,
                            step_phase,
                            &mut self.rec_motion,
                            &mut self.test_repeat_on,
                            &mut self.test_repeat_next_t,
                            accent,
                            theme,
                        ) {
                            self.set_toast(ctx, msg);
                        }
                        if sampler_armed != was_armed {
                            self.armed_record = if sampler_armed {
                                Some(RecordTarget::Core(TrackId::Sampler))
                            } else {
                                None
                            };
                        }
                    }
                    StudioTab::ArrangeParty => {
                        self.ui_arrange_party(ui, &mut proj, &telemetry, playing, accent, theme)
                    }
                    StudioTab::SoundLab => {
                        self.ui_sound_lab(ui, ctx, &mut proj, &telemetry, accent, theme)
                    }
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

        if !self.undo_action_this_frame {
            let pointer_down = ctx.input(|i| i.pointer.any_down());
            if pointer_down {
                if self.drag_base_proj.is_none() && proj != frame_start_proj {
                    self.drag_base_proj = Some(frame_start_proj);
                }
            } else {
                let baseline = self.drag_base_proj.take().unwrap_or(frame_start_proj);
                if proj != baseline {
                    if self.undo_stack.len() >= 32 {
                        self.undo_stack.pop_front();
                    }
                    self.undo_stack.push_back(baseline);
                    self.redo_stack.clear();
                }
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
        tel: &EngineTelemetry,
        playing: bool,
        cur_step: usize,
        accent: Color32,
        theme: StudioTheme,
    ) {
        // 1. VIBE PICKER STRIP
        card_frame(ui, theme, |ui| {
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
                    "{}  •  Space = Play/Stop  •  A–' / W–P = Chromatic Keys  •  Z/X = Octave  •  R = Record Track",
                    proj.vibe.tagline()
                ))
                .size(11.5)
                .weak(),
            );
        });

        ui.add_space(8.0);

        // 2. TRACK RACK (LEFT ~38%) + STEP SEQUENCER (RIGHT ~62%), stacked cleanly below 960px
        let total_w = ui.available_width();
        if total_w >= 960.0 {
            let gap = 10.0;
            let left_w = (total_w * 0.385)
                .clamp(470.0, 520.0)
                .min((total_w - gap) * 0.48);
            let right_w = (total_w - left_w - gap).max(460.0);
            let top_left = ui.cursor().min;
            let max_y = ui.max_rect().bottom().max(top_left.y + 420.0);

            let left_rect = egui::Rect::from_min_max(top_left, egui::pos2(top_left.x + left_w, max_y));
            let mut left_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(left_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            left_ui.set_width(left_w);
            self.ui_studio_tracks_card(&mut left_ui, ctx, proj, tel, accent, theme);

            let right_pos = egui::pos2(top_left.x + left_w + gap, top_left.y);
            let right_rect = egui::Rect::from_min_max(right_pos, egui::pos2(right_pos.x + right_w, max_y));
            let mut right_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(right_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            right_ui.set_width(right_w);
            self.ui_studio_sequencer_card(&mut right_ui, ctx, proj, playing, cur_step, accent, theme);

            let max_h = left_ui.min_rect().height().max(right_ui.min_rect().height());
            ui.advance_cursor_after_rect(egui::Rect::from_min_size(top_left, Vec2::new(total_w, max_h)));
        } else {
            self.ui_studio_tracks_card(ui, ctx, proj, tel, accent, theme);
            ui.add_space(8.0);
            self.ui_studio_sequencer_card(ui, ctx, proj, playing, cur_step, accent, theme);
        }

        ui.add_space(8.0);

        // 3. VIBE MACROS & LIVE DJ PERFORMANCE STRIP
        self.ui_vibe_macros_and_dj_card(ui, proj, tel, playing, cur_step, accent, theme);
    }

    fn ui_studio_tracks_card(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        proj: &mut Project,
        tel: &EngineTelemetry,
        accent: Color32,
        theme: StudioTheme,
    ) {
        card_frame(ui, theme, |ui| {
            let active_core = proj.active_tracks.iter().filter(|&&a| a).count();
            let total_tracks = active_core + proj.extra_tracks.len();
            let mut add_extra_preset: Option<usize> = None;
            let mut add_phil_bank: Option<u8> = None;
            let mut add_core = false;

            let show_header_title = |ui: &mut egui::Ui| {
                ui.label(
                    RichText::new(format!("STUDIO TRACKS ({total_tracks}/{})", 6 + MAX_EXTRA_TRACKS))
                        .strong()
                        .size(12.5)
                        .color(accent),
                );
            };
            if ui.available_width() >= 330.0 {
                ui.horizontal(|ui| {
                    show_header_title(ui);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if proj.extra_tracks.len() < MAX_EXTRA_TRACKS {
                            ui.menu_button(
                                RichText::new("+ Add Track")
                                    .strong()
                                    .size(11.5)
                                    .color(Color32::from_rgb(82, 232, 180)),
                                |ui| {
                                    ui_add_track_menu_contents(
                                        ui,
                                        &mut add_extra_preset,
                                        &mut add_phil_bank,
                                    );
                                },
                            );
                        }
                        if active_core < 6
                            && ui
                                .small_button("+ Core")
                                .on_hover_text("Restore all 6 core studio tracks")
                                .clicked()
                        {
                            add_core = true;
                        }
                    });
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    show_header_title(ui);
                    if active_core < 6
                        && ui
                            .small_button("+ Core")
                            .on_hover_text("Restore all 6 core studio tracks")
                            .clicked()
                    {
                        add_core = true;
                    }
                    if proj.extra_tracks.len() < MAX_EXTRA_TRACKS {
                        ui.menu_button(
                            RichText::new("+ Add Track")
                                .strong()
                                .size(11.5)
                                .color(Color32::from_rgb(82, 232, 180)),
                            |ui| {
                                ui_add_track_menu_contents(
                                    ui,
                                    &mut add_extra_preset,
                                    &mut add_phil_bank,
                                );
                            },
                        );
                    }
                });
            }
            if add_core {
                for (idx, &t) in TrackId::STUDIO_TRACKS.iter().enumerate() {
                    if !proj.active_tracks[idx] {
                        proj.active_tracks[idx] = true;
                        let (_, mute, _) = track_mixer_mut(proj, t);
                        *mute = false;
                    }
                }
            }
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
            if let Some(bank_id) = add_phil_bank {
                preload_philharmonia_bank_async(bank_id);
                let palette: [[u8; 3]; 6] = [
                    [245, 206, 112],
                    [130, 214, 255],
                    [255, 148, 118],
                    [204, 162, 255],
                    [110, 236, 192],
                    [255, 152, 210],
                ];
                let n = proj.extra_tracks.len();
                let mut tr = ExtraTrack {
                    pan: if n % 2 == 0 { -0.22 } else { 0.22 },
                    color: palette[n % palette.len()],
                    ..ExtraTrack::default()
                };
                apply_philharmonia_instrument(&mut tr, bank_id);
                let name = tr.name.clone();
                proj.extra_tracks.push(tr);
                let seed = self.next_seed();
                let seq = generate_philharmonia_track_seq(
                    proj.root_note,
                    proj.progression_idx,
                    bank_id,
                    seed,
                );
                for pat in &mut proj.patterns {
                    pat.extra_seqs.push(seq.clone());
                }
                self.selected_extra = Some(n);
                self.set_toast(
                    ctx,
                    format!("Added {name} (Philharmonia) — play QWERTY/MIDI or tweak in Sound Lab"),
                );
            }
            ui.add_space(4.0);

            let mut removed_track_idx: Option<usize> = None;
            for (row_idx, &track) in TrackId::STUDIO_TRACKS.iter().enumerate() {
                if !proj.active_tracks[row_idx] {
                    continue;
                }
                let selected = self.selected_extra.is_none() && self.selected_track == track;
                let armed = self.armed_record == Some(RecordTarget::Core(track));
                let tc = track_color(track);
                let row_bg = if selected {
                    Color32::from_rgb(30, 38, 56)
                } else {
                    Color32::from_rgb(20, 24, 34)
                };
                let mut row_rec = false;
                let mut row_rnd = false;
                let mut row_clr = false;
                let mut row_rm = false;
                egui::Frame::none()
                    .fill(row_bg)
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .stroke(Stroke::new(
                        if armed || selected { 1.5_f32 } else { 1.0_f32 },
                        if armed {
                            Color32::from_rgb(228, 54, 70)
                        } else if selected {
                            tc
                        } else {
                            Color32::from_rgb(34, 40, 56)
                        },
                    ))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
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
                            .min_size(Vec2::new(78.0, 22.0));
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
                                        ui.horizontal_wrapped(|ui| {
                                            if ui.small_button("808").clicked() {
                                                proj.drums.set_all_model(DrumModel::Tr808);
                                            }
                                            if ui.small_button("909").clicked() {
                                                proj.drums.set_all_model(DrumModel::Tr909);
                                            }
                                            if ui.small_button("Acou").clicked() {
                                                proj.drums.set_all_model(DrumModel::Acoustic);
                                            }
                                            if ui.small_button("Linn").clicked() {
                                                proj.drums.set_all_model(DrumModel::Linn80s);
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
                                .min_size(Vec2::new(104.0, 22.0));
                                if ui
                                    .add(vbtn)
                                    .on_hover_text("Open 8-Part Volca Performance Sampler (load MP3/WAV, auto-chop, Isolator EQ)")
                                    .clicked()
                                {
                                    self.tab = StudioTab::Sampler;
                                }
                            } else {
                                let presets = voice_preset_names(track);
                                let raw_idx = if proj.party_mode && !tel.party_palette.is_empty() {
                                    tel.party_voice_presets[row_idx]
                                } else {
                                    proj.voice_presets[row_idx]
                                };
                                let cur_preset = raw_idx % presets.len().max(1);
                                egui::ComboBox::from_id_salt(("vp", row_idx))
                                    .width(104.0)
                                    .selected_text(presets[cur_preset])
                                    .show_ui(ui, |ui| {
                                        for (pi, &pname) in presets.iter().enumerate() {
                                            if ui.selectable_label(cur_preset == pi, pname).clicked() {
                                                apply_voice_preset(proj, track, pi);
                                                if proj.party_mode {
                                                    self.engine.send_live(LiveEvent::SetPartyInstrument {
                                                        track,
                                                        preset_idx: pi,
                                                    });
                                                }
                                            }
                                        }
                                    });
                            }

                            let (vol, mute, solo) = track_mixer_mut(proj, track);
                            let (rec, r, c, rm) = ui_track_mix_strip(
                                ui,
                                armed,
                                vol,
                                mute,
                                solo,
                                "Generate fresh pattern for this track",
                                "Remove track from mix (mutes & hides it; re-add below anytime)",
                            );
                            row_rec = rec;
                            row_rnd = r;
                            row_clr = c;
                            row_rm = rm;
                        });
                    });
                if row_rec {
                    self.toggle_arm_record(ctx, proj, RecordTarget::Core(track));
                }
                if row_rnd {
                    let prev_sc = self.target_step_count(proj, RecordTarget::Core(track));
                    let seed = self.next_seed();
                    if track == TrackId::Drums {
                        let mut drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                        drums.set_all_step_counts(prev_sc, false);
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
                        pat.set_sampler_step_counts(prev_sc);
                        proj.mixer.channels[TrackId::Sampler.index()].mute = false;
                    } else {
                        let (mut bass, mut pad, mut arp, mut acid, mut samp) = generate_melodic_tracks(
                            proj.vibe,
                            proj.root_note,
                            proj.progression_idx,
                            seed,
                        );
                        let pat = proj.current_pattern_mut();
                        match track {
                            TrackId::Moog => {
                                bass.step_count = prev_sc;
                                pat.moog = bass;
                            }
                            TrackId::MoogPad => {
                                pad.step_count = prev_sc;
                                pat.moog_pad = pad;
                            }
                            TrackId::Monologue => {
                                arp.step_count = prev_sc;
                                pat.monologue = arp;
                            }
                            TrackId::Tb303 => {
                                acid.step_count = prev_sc;
                                pat.tb303 = acid;
                            }
                            TrackId::Sampler => {
                                samp.step_count = prev_sc;
                                pat.sampler = samp;
                            }
                            _ => {}
                        }
                    }
                }
                if row_clr {
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
                if row_rm {
                    removed_track_idx = Some(row_idx);
                }
                ui.add_space(4.0);
            }

            if let Some(rm_idx) = removed_track_idx {
                let rm_track = TrackId::STUDIO_TRACKS[rm_idx];
                proj.active_tracks[rm_idx] = false;
                let (_, mute, solo) = track_mixer_mut(proj, rm_track);
                *mute = true;
                *solo = false;
                if self.armed_record == Some(RecordTarget::Core(rm_track)) {
                    self.armed_record = None;
                }
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
                let armed = self.armed_record == Some(RecordTarget::Extra(ei));
                let [cr, cg, cb] = proj.extra_tracks[ei].color;
                let tc = Color32::from_rgb(cr, cg, cb);
                let row_bg = if selected {
                    Color32::from_rgb(30, 38, 56)
                } else {
                    Color32::from_rgb(20, 24, 34)
                };
                let mut ex_rec = false;
                let mut ex_rnd = false;
                let mut ex_clr = false;
                let mut ex_rm = false;
                egui::Frame::none()
                    .fill(row_bg)
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .stroke(Stroke::new(
                        if armed || selected { 1.5_f32 } else { 1.0_f32 },
                        if armed {
                            Color32::from_rgb(228, 54, 70)
                        } else if selected {
                            tc
                        } else {
                            Color32::from_rgb(34, 40, 56)
                        },
                    ))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
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
                            .min_size(Vec2::new(78.0, 22.0));
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
                                .width(104.0)
                                .selected_text(pad_presets[cur_p])
                                .show_ui(ui, |ui| {
                                    for (pi, &pname) in pad_presets.iter().enumerate() {
                                        if ui.selectable_label(cur_p == pi, pname).clicked() {
                                            apply_extra_track_preset(&mut proj.extra_tracks[ei], pi);
                                            proj.extra_tracks[ei].name = pname.to_string();
                                        }
                                    }
                                });

                            let tr = &mut proj.extra_tracks[ei];
                            let (rec, r, c, rm) = ui_track_mix_strip(
                                ui,
                                armed,
                                &mut tr.volume,
                                &mut tr.mute,
                                &mut tr.solo,
                                "Generate fresh chord-locked pattern for this track",
                                "Delete this synth track from the mix",
                            );
                            ex_rec = rec;
                            ex_rnd = r;
                            ex_clr = c;
                            ex_rm = rm;
                        });
                    });
                if ex_rec {
                    self.toggle_arm_record(ctx, proj, RecordTarget::Extra(ei));
                }
                if ex_rnd {
                    let prev_sc = self.target_step_count(proj, RecordTarget::Extra(ei));
                    let seed = self.next_seed();
                    let mut seq = generate_extra_track_seq(
                        proj.root_note,
                        proj.progression_idx,
                        proj.extra_tracks[ei].preset_idx,
                        seed,
                    );
                    seq.step_count = prev_sc;
                    if let Some(slot) = proj.current_pattern_mut().extra_seqs.get_mut(ei) {
                        *slot = seq;
                    }
                }
                if ex_clr {
                    if let Some(slot) = proj.current_pattern_mut().extra_seqs.get_mut(ei) {
                        slot.clear();
                    }
                }
                if ex_rm {
                    remove_extra_idx = Some(ei);
                }
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
                if self.armed_record == Some(RecordTarget::Extra(rm_e)) {
                    self.armed_record = None;
                } else if let Some(RecordTarget::Extra(cur)) = self.armed_record {
                    if cur > rm_e {
                        self.armed_record = Some(RecordTarget::Extra(cur - 1));
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
    }

    fn ui_studio_sequencer_card(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        proj: &mut Project,
        playing: bool,
        cur_step: usize,
        _accent: Color32,
        theme: StudioTheme,
    ) {
        card_frame(ui, theme, |ui| {
            let (seq_title, tc) = if let Some(ei) = self.selected_extra.filter(|&i| i < proj.extra_tracks.len()) {
                let [cr, cg, cb] = proj.extra_tracks[ei].color;
                (
                    format!("{} SEQUENCER", proj.extra_tracks[ei].name.to_uppercase()),
                    Color32::from_rgb(cr, cg, cb),
                )
            } else {
                (
                    format!("{} SEQUENCER", self.selected_track.track_instrument_title(proj)),
                    track_color(self.selected_track),
                )
            };

            // Row 1: Title + Bank A..H + Pattern Actions (cleanly wrapping, zero right_to_left collision)
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(seq_title)
                        .strong()
                        .size(12.5)
                        .color(tc),
                );
                ui.separator();
                ui.label(RichText::new("Bank:").size(11.0).weak());
                for b in 0..8 {
                    let label = format!("{}", (b'A' + b as u8) as char);
                    if ui.selectable_label(proj.active_pattern == b, label).clicked() {
                        proj.active_pattern = b;
                    }
                }
                ui.separator();
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
            });

            let active_target = self.active_target(proj);
            let cur_sc = self.target_step_count(proj, active_target);
            let cur_len = cur_sc.as_usize();
            let cur_pages = cur_sc.pages();
            self.bar_page = self.bar_page.min(cur_pages.saturating_sub(1));
            self.step_rec_cursor = self.step_rec_cursor % cur_len;
            let rec_col = Color32::from_rgb(228, 54, 70);
            let is_armed_here = self.armed_record == Some(active_target);

            // Row 2: Pattern Length by 16s + Bar 1..N + Follow + 16-Step Block Copy
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                ui.label(RichText::new("Len:").size(11.0).weak());
                for &sc in &StepCount::ALL {
                    if ui.selectable_label(cur_sc == sc, sc.label()).clicked() {
                        self.set_target_step_count(proj, active_target, sc);
                        self.bar_page = self.bar_page.min(sc.pages().saturating_sub(1));
                        self.step_rec_cursor = self.step_rec_cursor % sc.as_usize();
                    }
                }
                if ui
                    .small_button("->All")
                    .on_hover_text("Apply this step length (16/32/48/64) to all tracks in the active Bank")
                    .clicked()
                {
                    let len_now = cur_sc.as_usize();
                    proj.current_pattern_mut().set_all_step_counts(cur_sc);
                    self.set_toast(ctx, format!("Set all tracks in Bank to {len_now} steps"));
                }
                ui.separator();
                for b in 0..cur_pages {
                    let label = format!("Bar {}", b + 1);
                    if ui.selectable_label(self.bar_page == b, label).clicked() {
                        self.bar_page = b;
                        self.follow_playhead = false;
                    }
                }
                ui.checkbox(&mut self.follow_playhead, "Follow");
                ui.separator();
                if ui
                    .small_button("Copy->Rest")
                    .on_hover_text("Copy this 16-step Bar to the remaining bars of this track (expands 16 -> 64 automatically)")
                    .clicked()
                {
                    let new_sc = match active_target {
                        RecordTarget::Core(TrackId::Drums) => {
                            proj.current_pattern_mut().drums.copy_page_to_rest(self.bar_page)
                        }
                        RecordTarget::Core(TrackId::Sampler) => {
                            proj.current_pattern_mut().copy_sampler_page_to_rest(self.bar_page)
                        }
                        RecordTarget::Core(tr) => proj
                            .current_pattern_mut()
                            .melodic_mut(tr)
                            .map(|s| s.copy_page_to_rest(self.bar_page))
                            .unwrap_or(cur_sc),
                        RecordTarget::Extra(ei) => proj
                            .current_pattern_mut()
                            .extra_seqs
                            .get_mut(ei)
                            .map(|s| s.copy_page_to_rest(self.bar_page))
                            .unwrap_or(cur_sc),
                    };
                    self.set_toast(
                        ctx,
                        format!(
                            "Copied Bar {} -> rest ({} steps)",
                            self.bar_page + 1,
                            new_sc.as_usize()
                        ),
                    );
                }
                if ui
                    .small_button("->Next")
                    .on_hover_text("Copy this 16-step Bar to the next Bar (expands 16 -> 32) and jump there")
                    .clicked()
                {
                    let dst = match active_target {
                        RecordTarget::Core(TrackId::Drums) => {
                            proj.current_pattern_mut().drums.copy_page_to_next(self.bar_page)
                        }
                        RecordTarget::Core(TrackId::Sampler) => {
                            proj.current_pattern_mut().copy_sampler_page_to_next(self.bar_page)
                        }
                        RecordTarget::Core(tr) => proj
                            .current_pattern_mut()
                            .melodic_mut(tr)
                            .map(|s| s.copy_page_to_next(self.bar_page))
                            .unwrap_or(0),
                        RecordTarget::Extra(ei) => proj
                            .current_pattern_mut()
                            .extra_seqs
                            .get_mut(ei)
                            .map(|s| s.copy_page_to_next(self.bar_page))
                            .unwrap_or(0),
                    };
                    self.bar_page = dst;
                    self.follow_playhead = false;
                    self.set_toast(ctx, format!("Copied Bar -> Bar {}", dst + 1));
                }
                if ui
                    .small_button("All->Rest")
                    .on_hover_text("Copy this 16-step Bar across the rest of the pattern for ALL tracks")
                    .clicked()
                {
                    proj.current_pattern_mut()
                        .copy_all_tracks_page_to_rest(self.bar_page);
                    self.set_toast(
                        ctx,
                        format!("Copied Bar {} to rest across all tracks", self.bar_page + 1),
                    );
                }
                if self.selected_extra.is_some() || !matches!(self.selected_track, TrackId::Drums | TrackId::Sampler) {
                    ui.separator();
                    ui.label(RichText::new("Div:").size(11.0).weak());
                    let cur_div = if let Some(ei) = self.selected_extra {
                        proj.current_pattern()
                            .extra_seqs
                            .get(ei)
                            .map(|s| s.clock_div)
                            .unwrap_or(ClockDiv::Sixteenth)
                    } else {
                        proj.current_pattern()
                            .melodic(self.selected_track)
                            .map(|s| s.clock_div)
                            .unwrap_or(ClockDiv::Sixteenth)
                    };
                    for &dv in &ClockDiv::ALL {
                        if ui
                            .selectable_label(cur_div == dv, dv.label())
                            .on_hover_text("Track step clock rate (1/8 half-time, 1/16 normal, 1/32 double-time)")
                            .clicked()
                        {
                            if let Some(ei) = self.selected_extra {
                                if let Some(seq) = proj.current_pattern_mut().extra_seqs.get_mut(ei) {
                                    seq.clock_div = dv;
                                }
                            } else if let Some(seq) = proj.current_pattern_mut().melodic_mut(self.selected_track) {
                                seq.clock_div = dv;
                            }
                        }
                    }
                }
            });

            // Row 2b: Per-Instrument Record Bar + Track Contextual Controls
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let rec_btn = egui::Button::new(
                    RichText::new("● REC")
                        .strong()
                        .size(11.0)
                        .color(Color32::WHITE),
                )
                .fill(if is_armed_here {
                    rec_col
                } else {
                    Color32::from_rgb(44, 34, 42)
                });
                if ui
                    .add(rec_btn)
                    .on_hover_text("Put this instrument in Record Mode: play when stopped to Step Record, or while playing to Live Record")
                    .clicked()
                {
                    self.toggle_arm_record(ctx, proj, active_target);
                }
                if ui.selectable_label(self.rec_replace, "Replace").clicked() {
                    self.rec_replace = true;
                }
                if ui.selectable_label(!self.rec_replace, "Overdub").clicked() {
                    self.rec_replace = false;
                }

                if self.armed_record == Some(active_target) {
                    if playing {
                        ui.label(
                            RichText::new("LIVE REC")
                                .strong()
                                .size(11.0)
                                .color(Color32::from_rgb(255, 110, 120)),
                        );
                    } else {
                        let cur_disp = (self.step_rec_cursor % cur_len) + 1;
                        ui.label(
                            RichText::new(format!("STEP {cur_disp}/{cur_len}"))
                                .strong()
                                .size(11.0)
                                .color(Color32::from_rgb(255, 110, 120)),
                        );
                        if ui.small_button("Rest").on_hover_text("Insert rest and advance (Tab / .)").clicked() {
                            self.step_rec_rest(proj);
                        }
                        if !matches!(
                            active_target,
                            RecordTarget::Core(TrackId::Drums | TrackId::Sampler)
                        ) {
                            if ui.small_button("Tie").on_hover_text("Extend previous note over this step (-)").clicked() {
                                self.step_rec_tie(proj);
                            }
                            if ui.selectable_label(self.step_rec_accent, "Acc").on_hover_text("Accent next recorded notes (or hold Shift)").clicked() {
                                self.step_rec_accent = !self.step_rec_accent;
                            }
                            if ui.selectable_label(self.step_rec_slide, "Slide").on_hover_text("Slide/tie next recorded notes").clicked() {
                                self.step_rec_slide = !self.step_rec_slide;
                            }
                        }
                        if ui.small_button("←").on_hover_text("Previous step (Backspace / Left)").clicked() {
                            self.step_rec_back(proj);
                        }
                        if ui.small_button("→").on_hover_text("Next step (Right)").clicked() {
                            self.step_rec_forward(proj);
                        }
                    }
                }

                if self.selected_extra.is_none() && self.selected_track == TrackId::Drums {
                    ui.separator();
                    ui.label(RichText::new("Beat Energy:").size(11.5));
                    ui.scope(|ui| {
                        ui.spacing_mut().slider_width = 88.0;
                        if ui
                            .add(
                                egui::Slider::new(&mut proj.beat_energy, 0.1..=1.0)
                                    .show_value(false)
                                    .trailing_fill(true),
                            )
                            .changed()
                            && !proj.auto_beat_mode
                        {
                            let prev_sc = proj.current_pattern().drums.step_count;
                            let seed = proj.party_seed;
                            let mut drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                            drums.set_all_step_counts(prev_sc, false);
                            proj.current_pattern_mut().drums = drums;
                        }
                    });
                    if ui.small_button("New Beat").clicked() {
                        let prev_sc = proj.current_pattern().drums.step_count;
                        let seed = self.next_seed();
                        proj.party_seed = seed;
                        let mut drums = generate_auto_beat(proj.vibe, proj.beat_energy, seed);
                        drums.set_all_step_counts(prev_sc, false);
                        proj.current_pattern_mut().drums = drums;
                    }
                    if ui.small_button("Fill Now").clicked() {
                        self.engine.send_live(LiveEvent::TriggerFill);
                    }
                } else if self.selected_extra.is_none() && self.selected_track == TrackId::Sampler {
                    ui.separator();
                    if ui.small_button("Open Volca Sampler Workbench ->").clicked() {
                        self.tab = StudioTab::Sampler;
                    }
                }
            });

            // Row 2c: Lane ClockDiv / Euclidean / Transform / Humanize Tools
            if self.selected_extra.is_none() && self.selected_track == TrackId::Drums {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let dl = self.selected_drum_lane.min(7);
                    let dl_name = DrumVoiceId::from_index(dl)
                        .map(|d| d.short_name_for_model(proj.drums.voices[dl].model))
                        .unwrap_or("KICK");
                    ui.label(RichText::new(format!("Lane {dl_name}:")).size(11.0).color(tc));
                    let lane_div = proj.current_pattern().drums.lanes[dl].clock_div;
                    for &dv in &ClockDiv::ALL {
                        if ui
                            .selectable_label(lane_div == dv, dv.label())
                            .on_hover_text("Clock division for selected drum lane")
                            .clicked()
                        {
                            proj.auto_beat_mode = false;
                            proj.current_pattern_mut().drums.lanes[dl].clock_div = dv;
                        }
                    }
                    let max_e = proj.current_pattern().drums.lanes[dl].effective_len().min(32).max(1);
                    self.seq_euclid_hits = self.seq_euclid_hits.clamp(1, max_e);
                    self.seq_euclid_rot = self.seq_euclid_rot.min(max_e.saturating_sub(1));
                    ui.add(egui::DragValue::new(&mut self.seq_euclid_hits).range(1..=max_e).prefix("H:"));
                    ui.add(egui::DragValue::new(&mut self.seq_euclid_rot).range(0..=max_e.saturating_sub(1)).prefix("R:"));
                    if ui.small_button("Euclid").on_hover_text("Fill selected drum lane with an even Euclidean rhythm").clicked() {
                        proj.auto_beat_mode = false;
                        proj.current_pattern_mut().drums.lanes[dl]
                            .fill_euclidean(self.seq_euclid_hits, self.seq_euclid_rot);
                    }
                    if ui.small_button("Humanize").on_hover_text("Add subtle organic velocity feel to this drum lane").clicked() {
                        proj.auto_beat_mode = false;
                        let seed = self.next_seed();
                        proj.current_pattern_mut().drums.lanes[dl].humanize_velocities(seed, 0.14);
                    }
                });
            } else if self.selected_extra.is_some()
                || !matches!(self.selected_track, TrackId::Drums | TrackId::Sampler)
            {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    let target = self.selected_track;
                    let extra_sel = self.selected_extra;
                    let root_note = proj.root_note;
                    let scale = proj.scale;
                    let next_seed = self.next_seed();
                    let seq_len = if let Some(ei) = extra_sel {
                        proj.current_pattern()
                            .extra_seqs
                            .get(ei)
                            .map(|s| s.step_count.as_usize())
                            .unwrap_or(16)
                    } else {
                        proj.current_pattern()
                            .melodic(target)
                            .map(|s| s.step_count.as_usize())
                            .unwrap_or(16)
                    }
                    .min(32)
                    .max(1);
                    self.seq_euclid_hits = self.seq_euclid_hits.clamp(1, seq_len);
                    self.seq_euclid_rot = self.seq_euclid_rot.min(seq_len.saturating_sub(1));
                    let base_oct = match target {
                        TrackId::Moog => 2,
                        TrackId::Tb303 => 3,
                        _ => 4,
                    };
                    let mut op: Option<u8> = None;
                    if ui.small_button("◀").on_hover_text("Rotate steps left by 1").clicked() {
                        op = Some(1);
                    }
                    if ui.small_button("▶").on_hover_text("Rotate steps right by 1").clicked() {
                        op = Some(2);
                    }
                    if ui.small_button("-1").on_hover_text("Transpose pattern -1 semitone in scale").clicked() {
                        op = Some(3);
                    }
                    if ui.small_button("+1").on_hover_text("Transpose pattern +1 semitone in scale").clicked() {
                        op = Some(4);
                    }
                    if ui.small_button("-12").on_hover_text("Shift pattern down 1 octave").clicked() {
                        op = Some(5);
                    }
                    if ui.small_button("+12").on_hover_text("Shift pattern up 1 octave").clicked() {
                        op = Some(6);
                    }
                    if ui.small_button("Humanize").on_hover_text("Add natural groove velocity variation").clicked() {
                        op = Some(7);
                    }
                    ui.add(egui::DragValue::new(&mut self.seq_euclid_hits).range(1..=seq_len).prefix("H:"));
                    ui.add(egui::DragValue::new(&mut self.seq_euclid_rot).range(0..=seq_len.saturating_sub(1)).prefix("R:"));
                    if ui.small_button("Euclid").on_hover_text("Generate Euclidean melody in current Key & Scale").clicked() {
                        op = Some(8);
                    }
                    if let Some(code) = op {
                        let pat = proj.current_pattern_mut();
                        let seq_opt = if let Some(ei) = extra_sel {
                            pat.extra_seqs.get_mut(ei)
                        } else {
                            pat.melodic_mut(target)
                        };
                        if let Some(seq) = seq_opt {
                            match code {
                                1 => seq.shift(-1),
                                2 => seq.shift(1),
                                3 => seq.transpose(-1, root_note, scale, true),
                                4 => seq.transpose(1, root_note, scale, true),
                                5 => seq.transpose(-12, root_note, scale, false),
                                6 => seq.transpose(12, root_note, scale, false),
                                7 => seq.humanize_velocities(next_seed, 0.14),
                                8 => seq.fill_euclidean(
                                    self.seq_euclid_hits,
                                    self.seq_euclid_rot,
                                    root_note,
                                    scale,
                                    base_oct,
                                    next_seed,
                                ),
                                _ => {}
                            }
                        }
                    }
                });
            }

            ui.add_space(4.0);
            let f_params = track_filter_mut(proj, self.selected_track, self.selected_extra);
            ui_instrument_filter_bar(ui, f_params, tc, theme);

            ui.add_space(6.0);

            let page_start = self.bar_page * 16;

            if self.selected_extra.is_none() && self.selected_track == TrackId::Drums {
                // 8-lane compact Drum Beat Grid (16 steps per bar page)
                let active_pat = proj.active_pattern;
                let cell_w = ((ui.available_width() - 68.0 - 16.0 * 3.0) / 16.0).clamp(18.0, 48.0);
                let cell_h = 25.0;
                let mut solo_drum_lane: Option<usize> = None;
                let mut unmute_all_drums = false;

                for lane_idx in 0..8 {
                    let voice = DrumVoiceId::from_index(lane_idx).unwrap();
                    let v_model = proj.drums.voices[lane_idx].model;
                    let v_short = voice.short_name_for_model(v_model);
                    let v_full = voice.full_name_for_model(v_model);
                    let is_muted = proj.drums.voices[lane_idx].mute;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;

                        // Clickable drum voice label: Left-click = Mute, Right-click = Full Voice Options & Tuning!
                        let lbl_btn = egui::Button::new(
                            RichText::new(if is_muted {
                                format!("X {v_short}")
                            } else {
                                v_short.to_string()
                            })
                            .size(10.0)
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
                        .min_size(Vec2::new(62.0, cell_h));

                        let lbl_resp = ui
                            .add(lbl_btn)
                            .on_hover_text(format!("{v_full} — Left-click: Mute/Select Lane  •  Right-click: Tune, 808/909 & Fill options"));
                        if lbl_resp.clicked() {
                            self.selected_drum_lane = lane_idx;
                            proj.drums.voices[lane_idx].mute = !proj.drums.voices[lane_idx].mute;
                        }
                        if lbl_resp.secondary_clicked() {
                            self.selected_drum_lane = lane_idx;
                        }
                        lbl_resp.context_menu(|ui| {
                            ui.set_min_width(220.0);
                            ui.label(RichText::new(v_full).strong().color(tc));
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
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Model:");
                                for &m in &DrumModel::ALL {
                                    ui.selectable_value(&mut vp.model, m, m.name());
                                }
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
                                if ui.small_button("Copy Bar -> Rest").clicked() {
                                    lane.copy_page_to_rest(self.bar_page);
                                    ui.close_menu();
                                }
                                if ui.small_button("Clear").clicked() {
                                    lane.clear();
                                    ui.close_menu();
                                }
                            });
                        });

                        let lane = &mut proj.patterns[active_pat].drums.lanes[lane_idx];
                        let d_len = lane.step_count.as_usize();
                        for col in 0..16 {
                            let idx = (page_start + col) % d_len;
                            let st = &mut lane.steps[idx];
                            let is_play = playing && (cur_step % d_len) == (page_start + col);
                            let is_rec_col = is_armed_here
                                && !playing
                                && (self.step_rec_cursor % d_len) == (page_start + col);
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
                                proj.auto_beat_mode = false;
                                st.active = !st.active;
                                if is_armed_here && !playing {
                                    self.step_rec_cursor = idx;
                                }
                                if st.active {
                                    st.velocity = 0.85;
                                    st.ratchet = 1;
                                }
                            }
                            resp.context_menu(|ui| {
                                ui.set_min_width(190.0);
                                ui.label(RichText::new(format!("{v_short} — Step {}", idx + 1)).strong().color(tc));
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
                                ui.checkbox(&mut st.flam, "Flam");
                                ui.add(egui::Slider::new(&mut st.velocity, 0.1..=1.0).text("Vel").max_decimals(2));
                                let mut prob_pct = (st.probability * 100.0).round();
                                if ui.add(egui::Slider::new(&mut prob_pct, 10.0..=100.0).suffix("%").text("Prob").max_decimals(0)).changed() {
                                    st.probability = (prob_pct / 100.0).clamp(0.1, 1.0);
                                    st.active = true;
                                }
                                if ui.small_button("Reset Step FX").clicked() {
                                    st.clear_step_fx();
                                    st.ratchet = 1;
                                    st.flam = false;
                                    st.probability = 1.0;
                                }
                            });
                            let painter = ui.painter();
                            painter.rect_filled(rect, Rounding::same(4.0), fill);
                            if st.active && st.probability < 0.999 {
                                let w = rect.width() * st.probability.clamp(0.1, 1.0);
                                painter.rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(rect.min.x + 1.0, rect.max.y - 3.0),
                                        egui::pos2((rect.min.x + w - 1.0).max(rect.min.x + 3.0), rect.max.y - 1.0),
                                    ),
                                    Rounding::same(1.0),
                                    Color32::from_rgb(76, 218, 255),
                                );
                            }
                            if is_rec_col {
                                painter.rect_stroke(
                                    rect,
                                    Rounding::same(4.0),
                                    Stroke::new(2.0_f32, Color32::from_rgb(245, 72, 86)),
                                );
                            }
                            if st.active && (st.ratchet > 1 || st.has_step_fx() || st.flam) {
                                let b = if st.ratchet > 1 {
                                    format!("{}x", st.ratchet)
                                } else if st.flam {
                                    "f".to_string()
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
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Click drum name = Mute  •  Right-click drum = Tune/Options  •  Right-click step = Ratchet")
                        .size(10.5)
                        .weak(),
                );
            } else if self.selected_extra.is_none() && self.selected_track == TrackId::Sampler {
                // 8-Lane Volca Sampler Grid right inside STUDIO!
                let active_pat = proj.active_pattern;
                let cell_w = ((ui.available_width() - 68.0 - 16.0 * 3.0) / 16.0).clamp(18.0, 48.0);
                let cell_h = 25.0;
                for part_idx in 0..8 {
                    let is_muted = proj.sample_parts[part_idx].mute;
                    let short_name: String = proj.sample_parts[part_idx]
                        .name
                        .chars()
                        .take(7)
                        .collect();
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let lbl = egui::Button::new(
                            RichText::new(if is_muted {
                                format!("X {short_name}")
                            } else {
                                short_name.clone()
                            })
                            .size(10.0)
                            .strong()
                            .color(if is_muted { Color32::GRAY } else { tc }),
                        )
                        .fill(if is_muted {
                            Color32::from_rgb(38, 22, 28)
                        } else {
                            Color32::from_rgb(24, 29, 42)
                        })
                        .min_size(Vec2::new(62.0, cell_h));
                        if ui
                            .add(lbl)
                            .on_hover_text(format!("Pad {} ({}) — Click to Mute/Unmute", part_idx + 1, proj.sample_parts[part_idx].name))
                            .clicked()
                        {
                            proj.sample_parts[part_idx].mute = !proj.sample_parts[part_idx].mute;
                        }
                        let lane = &mut proj.patterns[active_pat].sample_lanes[part_idx];
                        let s_len = lane.step_count.as_usize();
                        for col in 0..16 {
                            let idx = (page_start + col) % s_len;
                            let st = &mut lane.steps[idx];
                            let is_play = playing && (cur_step % s_len) == (page_start + col);
                            let is_rec_col = is_armed_here
                                && !playing
                                && (self.step_rec_cursor % s_len) == (page_start + col);
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
                                if is_armed_here && !playing {
                                    self.step_rec_cursor = idx;
                                }
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
                            if is_rec_col {
                                painter.rect_stroke(
                                    rect,
                                    Rounding::same(4.0),
                                    Stroke::new(2.0_f32, Color32::from_rgb(245, 72, 86)),
                                );
                            }
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
                ui.add_space(4.0);
                ui.label(RichText::new("Click P1–P8 = Mute  •  Right-click step = Ratchet").size(10.5).weak());
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
                    let pad_w = ((ui.available_width() - 7.0 * 6.0) / 8.0).clamp(44.0, 112.0);
                    let pad_h = 46.0;
                    for row in 0..2 {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for col in 0..8 {
                                let step_in_page = row * 8 + col;
                                let idx = (page_start + step_in_page) % len;
                                let st = &mut seq.steps[idx];
                                let is_play = playing && (cur_step % len) == (page_start + step_in_page);
                                let is_sel = self.selected_step == idx;
                                let is_rec_cur =
                                    is_armed_here && !playing && (self.step_rec_cursor % len) == idx;
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
                                    if is_armed_here && !playing {
                                        self.step_rec_cursor = idx;
                                    }
                                }
                                if resp.secondary_clicked() {
                                    self.selected_step = idx;
                                    if is_armed_here && !playing {
                                        self.step_rec_cursor = idx;
                                    }
                                }
                                let painter = ui.painter();
                                painter.rect_filled(rect, Rounding::same(6.0), fill);
                                painter.rect_stroke(
                                    rect,
                                    Rounding::same(6.0),
                                    Stroke::new(
                                        if is_rec_cur || is_sel { 2.0_f32 } else { 1.0_f32 },
                                        if is_rec_cur {
                                            Color32::from_rgb(245, 72, 86)
                                        } else if is_sel {
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
                                        rect.center() + Vec2::new(0.0, 3.0),
                                        egui::Align2::CENTER_CENTER,
                                        label,
                                        egui::FontId::proportional(11.5),
                                        Color32::from_rgb(12, 15, 22),
                                    );
                                    // Velocity bottom fill bar + micro-badges (prob / lock / slide)
                                    let v_w = (rect.width() - 8.0) * st.velocity.clamp(0.1, 1.0);
                                    painter.rect_filled(
                                        egui::Rect::from_min_max(
                                            egui::pos2(rect.min.x + 4.0, rect.max.y - 5.0),
                                            egui::pos2(rect.min.x + 4.0 + v_w, rect.max.y - 2.5),
                                        ),
                                        Rounding::same(1.5),
                                        if st.accent {
                                            Color32::from_rgb(255, 214, 96)
                                        } else {
                                            Color32::from_rgb(18, 22, 32)
                                        },
                                    );
                                    let mut dot_x = rect.max.x - 6.0;
                                    if st.motion.any_locked() {
                                        painter.circle_filled(
                                            egui::pos2(dot_x, rect.min.y + 6.0),
                                            2.5,
                                            Color32::from_rgb(255, 118, 210),
                                        );
                                        dot_x -= 6.5;
                                    }
                                    if st.probability < 0.999 {
                                        painter.circle_filled(
                                            egui::pos2(dot_x, rect.min.y + 6.0),
                                            2.5,
                                            Color32::from_rgb(64, 226, 255),
                                        );
                                        dot_x -= 6.5;
                                    }
                                    if st.slide {
                                        painter.circle_filled(
                                            egui::pos2(dot_x, rect.min.y + 6.0),
                                            2.5,
                                            Color32::from_rgb(14, 18, 28),
                                        );
                                    }
                                }
                            }
                        });
                        ui.add_space(6.0);
                    }

                    // Interactive 16-Step Velocity Paint Lane
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let bar_w = ((ui.available_width() - 15.0 * 3.0) / 16.0).clamp(12.0, 56.0);
                        for col in 0..16 {
                            let idx = (page_start + col) % len;
                            let st = &mut seq.steps[idx];
                            let (vrect, vresp) = ui.allocate_exact_size(
                                Vec2::new(bar_w, 22.0),
                                Sense::click_and_drag(),
                            );
                            if let Some(pos) = vresp.interact_pointer_pos() {
                                let norm = ((vrect.max.y - pos.y) / vrect.height()).clamp(0.10, 1.0);
                                st.velocity = norm;
                                self.selected_step = idx;
                            }
                            let pnt = ui.painter();
                            pnt.rect_filled(vrect, Rounding::same(2.0), Color32::from_rgb(18, 22, 32));
                            let fill_h = (vrect.height() - 2.0) * st.velocity.clamp(0.1, 1.0);
                            let bar_col = if st.active {
                                if idx == self.selected_step % len {
                                    Color32::WHITE
                                } else {
                                    tc.gamma_multiply(0.85)
                                }
                            } else {
                                Color32::from_rgb(48, 56, 74)
                            };
                            pnt.rect_filled(
                                egui::Rect::from_min_max(
                                    egui::pos2(vrect.min.x + 1.0, vrect.max.y - 1.0 - fill_h),
                                    egui::pos2(vrect.max.x - 1.0, vrect.max.y - 1.0),
                                ),
                                Rounding::same(1.5),
                                bar_col,
                            );
                        }
                    });
                    ui.add_space(4.0);

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
                        let mut prob_pct = (st.probability * 100.0).round();
                        if ui.add(egui::Slider::new(&mut prob_pct, 10.0..=100.0).suffix("%").text("Prob").max_decimals(0)).changed() {
                            st.probability = (prob_pct / 100.0).clamp(0.1, 1.0);
                            st.active = true;
                        }
                        ui.checkbox(&mut st.accent, "Acc");
                        ui.checkbox(&mut st.slide, "Slide");
                        // Per-Step P-Locks (Filter / Res / Drive / Shape) for Synths!
                        let mut c_val = st.motion.cutoff.unwrap_or(0.6);
                        if ui.add(egui::Slider::new(&mut c_val, 0.0..=1.0).text("Filt").max_decimals(2)).changed() {
                            st.motion.cutoff = Some(c_val);
                            st.active = true;
                        }
                        let mut r_val = st.motion.resonance.unwrap_or(0.3);
                        if ui.add(egui::Slider::new(&mut r_val, 0.0..=1.0).text("Res").max_decimals(2)).changed() {
                            st.motion.resonance = Some(r_val);
                            st.active = true;
                        }
                        let mut d_val = st.motion.drive.unwrap_or(0.3);
                        if ui.add(egui::Slider::new(&mut d_val, 0.0..=1.0).text("Drive").max_decimals(2)).changed() {
                            st.motion.drive = Some(d_val);
                            st.active = true;
                        }
                        let mut s_val = st.motion.shape.unwrap_or(0.5);
                        if ui.add(egui::Slider::new(&mut s_val, 0.0..=1.0).text("Shape").max_decimals(2)).changed() {
                            st.motion.shape = Some(s_val);
                            st.active = true;
                        }
                        if st.motion.any_locked() && ui.small_button("Clr Lock").clicked() {
                            st.motion.clear();
                        }
                    });
                    ui.label(RichText::new("Left-click = toggle step  •  Right-click = select step  •  Drag velocity strip below pads").size(10.5).weak());
                }
            }
        });
    }

    fn ui_vibe_macros_and_dj_card(
        &mut self,
        ui: &mut egui::Ui,
        proj: &mut Project,
        tel: &EngineTelemetry,
        playing: bool,
        cur_step: usize,
        accent: Color32,
        theme: StudioTheme,
    ) {
        card_frame(ui, theme, |ui| {
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
                ui.label(
                    RichText::new("Sweep any macro with REC MOTION ON to automate")
                        .size(11.0)
                        .weak(),
                );
            });

            ui.add_space(6.0);

            let cols = if ui.available_width() >= 840.0 { 6 } else { 3 };
            let gap = 6.0;
            let cell_w = ((ui.available_width() - gap * (cols - 1) as f32) / cols as f32)
                .floor()
                .max(80.0);

            let mut filt_norm = ((proj.fx.global_filter_cutoff.clamp(120.0, 18_000.0) / 120.0).ln()
                / (18_000.0_f32 / 120.0).ln())
            .clamp(0.0, 1.0);
            let filt_lbl = if proj.fx.global_filter_cutoff >= 1000.0 {
                format!("{:.1} kHz", proj.fx.global_filter_cutoff / 1000.0)
            } else {
                format!("{:.0} Hz", proj.fx.global_filter_cutoff)
            };
            let mut space = ((proj.fx.reverb_mix + proj.fx.delay_mix) * 0.5).clamp(0.0, 1.0);
            let space_lbl = format!("{:.0}%", space * 100.0);
            let sc_lbl = format!("{:.0}%", proj.sidechain_amount * 100.0);
            let warm_lbl = format!("{:.0}%", proj.fx.analog_warmth * 100.0);
            let glitch_lbl = format!("{:.0}%", (proj.fx.glitch / 0.85) * 100.0);
            let master_lbl = format!("{:.0}%", proj.mixer.master_volume * 100.0);

            let mut render_macro = |ui: &mut egui::Ui, idx: usize| match idx {
                0 => {
                    if macro_cell(ui, cell_w, "Filter", &filt_lbl, &mut filt_norm, 0.0..=1.0, accent, theme) {
                        proj.fx.global_filter_cutoff = 120.0 * (18_000.0_f32 / 120.0).powf(filt_norm);
                        if self.rec_motion && playing {
                            proj.current_pattern_mut().motion.filter[cur_step % 64] = Some(filt_norm);
                        }
                    }
                }
                1 => {
                    if macro_cell(ui, cell_w, "Space", &space_lbl, &mut space, 0.0..=1.0, accent, theme) {
                        proj.fx.reverb_mix = space;
                        proj.fx.delay_mix = (space * 0.92).min(0.85);
                        if self.rec_motion && playing {
                            proj.current_pattern_mut().motion.space[cur_step % 64] = Some(space);
                        }
                    }
                }
                2 => {
                    if macro_cell(ui, cell_w, "Sidechain", &sc_lbl, &mut proj.sidechain_amount, 0.0..=1.0, accent, theme)
                        && self.rec_motion
                        && playing
                    {
                        let sc = proj.sidechain_amount;
                        proj.current_pattern_mut().motion.sidechain[cur_step % 64] = Some(sc);
                    }
                }
                3 => {
                    macro_cell(ui, cell_w, "Warmth", &warm_lbl, &mut proj.fx.analog_warmth, 0.0..=1.0, accent, theme);
                }
                4 => {
                    if macro_cell(ui, cell_w, "Glitch", &glitch_lbl, &mut proj.fx.glitch, 0.0..=0.85, accent, theme)
                        && self.rec_motion
                        && playing
                    {
                        let gl = proj.fx.glitch;
                        proj.current_pattern_mut().motion.glitch[cur_step % 64] = Some(gl);
                    }
                }
                _ => {
                    macro_cell(ui, cell_w, "Master", &master_lbl, &mut proj.mixer.master_volume, 0.0..=1.15, accent, theme);
                }
            };

            if cols == 6 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for i in 0..6 {
                        render_macro(ui, i);
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for i in 0..3 {
                        render_macro(ui, i);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for i in 3..6 {
                        render_macro(ui, i);
                    }
                });
            }

            ui.add_space(6.0);

            // GLOBAL FILTER & ENVELOPE STRIP (directly beneath Vibe Macros!)
            egui::Frame::none()
                .fill(theme.control_bg())
                .rounding(Rounding::same(6.0))
                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                .stroke(Stroke::new(1.0_f32, theme.border()))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(RichText::new("GLOBAL FILTER").strong().size(11.5).color(accent));
                        toggle_pill(
                            ui,
                            if proj.fx.global_filter_enabled { "ON" } else { "OFF" },
                            &mut proj.fx.global_filter_enabled,
                            accent,
                            "Enable/bypass global DJ filter",
                        );
                        ui.selectable_value(&mut proj.fx.global_filter_mode, GlobalFilterMode::LowPass, "LP");
                        ui.selectable_value(&mut proj.fx.global_filter_mode, GlobalFilterMode::HighPass, "HP");
                        ui.selectable_value(&mut proj.fx.global_filter_mode, GlobalFilterMode::BandPass, "BP");
                        ui.label(RichText::new("Cut").size(11.0).weak());
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 108.0;
                            if ui
                                .add(
                                    egui::Slider::new(&mut proj.fx.global_filter_cutoff, 45.0..=18_000.0)
                                        .logarithmic(true)
                                        .show_value(false)
                                        .trailing_fill(true),
                                )
                                .changed()
                                && self.rec_motion
                                && playing
                            {
                                let norm = ((proj.fx.global_filter_cutoff.clamp(120.0, 18_000.0) / 120.0).ln()
                                    / (18_000.0_f32 / 120.0).ln())
                                .clamp(0.0, 1.0);
                                proj.current_pattern_mut().motion.filter[cur_step % 64] = Some(norm);
                            }
                        });
                        ui.label(RichText::new(format_hz(proj.fx.global_filter_cutoff)).size(11.0).color(accent));
                        ui.label(RichText::new("Res").size(11.0).weak());
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 72.0;
                            ui.add(
                                egui::Slider::new(&mut proj.fx.global_filter_resonance, 0.0..=0.88)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                        });
                        ui.label(RichText::new(format!("{:.0}%", proj.fx.global_filter_resonance * 100.0)).size(11.0).weak());
                        ui.separator();
                        ui.label(RichText::new("Crisp").size(11.0).color(accent))
                            .on_hover_text("Modern mastering crispness: mono-locked sub, mud carve, silky air highs & punchy transient limiter");
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 68.0;
                            ui.add(
                                egui::Slider::new(&mut proj.fx.master_crisp, 0.0..=1.0)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                        });
                        ui.label(RichText::new(format!("{:.0}%", proj.fx.master_crisp * 100.0)).size(11.0).weak());
                    });

                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let env_col = Color32::from_rgb(138, 198, 255);
                        ui.label(RichText::new("FILTER ENV").strong().size(11.5).color(env_col));
                        ui.label(RichText::new("Trig").size(11.0).weak());
                        egui::ComboBox::from_id_salt("gfilt_env_trig")
                            .width(76.0)
                            .selected_text(proj.fx.global_filter_env_trigger.name())
                            .show_ui(ui, |ui| {
                                for &t in &FilterEnvTrigger::ALL {
                                    ui.selectable_value(&mut proj.fx.global_filter_env_trigger, t, t.name());
                                }
                            });
                        ui.label(RichText::new("Amt").size(11.0).weak());
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 80.0;
                            ui.add(
                                egui::Slider::new(&mut proj.fx.global_filter_env_amount, -1.0..=1.0)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                        });
                        ui.label(RichText::new(format!("{:+.0}%", proj.fx.global_filter_env_amount * 100.0)).size(11.0).color(env_col));
                        ui.label(RichText::new("Atk").size(11.0).weak());
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 64.0;
                            ui.add(
                                egui::Slider::new(&mut proj.fx.global_filter_env_attack, 0.002..=1.5)
                                    .logarithmic(true)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                        });
                        ui.label(RichText::new(format!("{:.0}ms", proj.fx.global_filter_env_attack * 1000.0)).size(10.5).weak());
                        ui.label(RichText::new("Dec").size(11.0).weak());
                        ui.scope(|ui| {
                            ui.spacing_mut().slider_width = 64.0;
                            ui.add(
                                egui::Slider::new(&mut proj.fx.global_filter_env_decay, 0.04..=3.0)
                                    .logarithmic(true)
                                    .show_value(false)
                                    .trailing_fill(true),
                            );
                        });
                        ui.label(RichText::new(format!("{:.0}ms", proj.fx.global_filter_env_decay * 1000.0)).size(10.5).weak());
                        if ui
                            .small_button("Fire Env")
                            .on_hover_text("Trigger the global filter envelope right now")
                            .clicked()
                        {
                            self.engine.send_live(LiveEvent::TriggerGlobalFilterEnv);
                        }
                        let meter_r = ui.allocate_exact_size(Vec2::new(34.0, 12.0), Sense::hover()).0;
                        ui.painter().rect_filled(meter_r, Rounding::same(3.0), Color32::from_rgb(12, 15, 22));
                        let fill_w = (meter_r.width() * tel.global_filter_env_level.clamp(0.0, 1.0)).max(0.0);
                        if fill_w > 1.0 {
                            let fill_r = egui::Rect::from_min_size(meter_r.min, Vec2::new(fill_w, meter_r.height()));
                            ui.painter().rect_filled(fill_r, Rounding::same(3.0), env_col);
                        }
                    });
                });

            ui.add_space(6.0);

            // Live DJ Performance Buttons
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("DJ LIVE:").strong().size(12.0).color(Color32::from_rgb(255, 168, 52)));
                ui.separator();

                let (drop_label, drop_fill, drop_tip) = match tel.drop_phase.as_str() {
                    "BUILD" => (
                        format!("BUILD {:.0}%", tel.drop_progress * 100.0),
                        Color32::from_rgb(245, 148, 36),
                        "Drop macro building! Click again to cancel build.",
                    ),
                    "HOLD..." => (
                        "HOLD...".to_string(),
                        Color32::from_rgb(42, 198, 232),
                        "Pre-drop tension cut! Massive impact incoming...",
                    ),
                    "DROP!" => (
                        "DROP!".to_string(),
                        Color32::from_rgb(235, 58, 138),
                        "Drop impact active! Click to queue another drop.",
                    ),
                    _ => (
                        "BASS DROP".to_string(),
                        Color32::from_rgb(208, 66, 98),
                        "Launches configurable multi-stage Bass Drop Macro: cuts instruments, accelerates drums, sweeps riser -> massive layered impact!",
                    ),
                };
                let drop_btn = egui::Button::new(RichText::new(drop_label).strong().color(Color32::WHITE))
                    .fill(drop_fill)
                    .min_size(Vec2::new(112.0, 28.0));
                if ui.add(drop_btn).on_hover_text(drop_tip).clicked() {
                    self.engine.send_live(LiveEvent::TriggerBassDrop);
                }

                let auto_lbl = match proj.drop_macro.auto_drop_bars {
                    0 => "Off",
                    8 => "8 Bars",
                    16 => "16 Bars",
                    _ => "32 Bars",
                };
                ui.label(RichText::new("Auto:").size(11.0).weak());
                egui::ComboBox::from_id_salt("auto_drop_combo")
                    .width(68.0)
                    .selected_text(auto_lbl)
                    .show_ui(ui, |ui| {
                        for &(bars, lbl) in &[(0u16, "Off"), (8, "8 Bars"), (16, "16 Bars"), (32, "32 Bars")] {
                            ui.selectable_value(&mut proj.drop_macro.auto_drop_bars, bars, lbl);
                        }
                    });

                toggle_pill(
                    ui,
                    "Drop Setup",
                    &mut self.show_drop_setup,
                    Color32::from_rgb(235, 118, 86),
                    "Configure Drop Macro build length, instrument cuts, accelerating drums, riser sweep, pre-drop gap & auto-drop",
                );

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

            if self.show_drop_setup {
                ui.add_space(6.0);
                egui::Frame::none()
                    .fill(theme.control_bg())
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(235, 118, 86)))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let dm = &mut proj.drop_macro;
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(
                                RichText::new("DROP SETUP:")
                                    .strong()
                                    .size(11.5)
                                    .color(Color32::from_rgb(255, 168, 52)),
                            );
                            ui.label(RichText::new("Build:").size(11.0).weak());
                            for &(b, lbl) in &[(1u8, "1B"), (2, "2B"), (4, "4B")] {
                                ui.selectable_value(&mut dm.build_bars, b, lbl);
                            }
                            ui.separator();
                            ui.label(RichText::new("Cut:").size(11.0).weak());
                            ui.checkbox(&mut dm.cut_drums, "Drums");
                            ui.checkbox(&mut dm.cut_bass, "Bass");
                            ui.checkbox(&mut dm.cut_chords, "Chords");
                            ui.checkbox(&mut dm.cut_lead, "Lead/303");
                            ui.separator();
                            ui.checkbox(&mut dm.drum_accel, "Drum Accel");
                            ui.checkbox(&mut dm.riser_sweep, "Riser Sweep");
                            ui.separator();
                            ui.label(RichText::new("Gap:").size(11.0).weak());
                            for &(g, lbl) in &[(0u8, "0"), (2, "1/2B"), (4, "1B")] {
                                ui.selectable_value(&mut dm.pre_drop_gap_steps, g, lbl);
                            }
                            ui.separator();
                            ui.label(RichText::new("Impact").size(11.0).weak());
                            ui.scope(|ui| {
                                ui.spacing_mut().slider_width = 74.0;
                                ui.add(
                                    egui::Slider::new(&mut dm.impact_intensity, 0.2..=1.0)
                                        .show_value(false)
                                        .trailing_fill(true),
                                );
                            });
                            ui.label(RichText::new(format!("{:.0}%", dm.impact_intensity * 100.0)).size(11.0).weak());
                        });
                    });
            }
        });
    }

    fn load_sample_dialog(&mut self, ctx: &egui::Context, proj: &mut Project) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio (MP3, WAV, FLAC, OGG)", &["mp3", "wav", "flac", "ogg"])
            .pick_file()
        {
            match load_audio_to_stereo_f32(&path, 48_000) {
                Ok((pcm_l, pcm_r)) => {
                    let sname = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Custom Sample")
                        .to_string();
                    let secs = pcm_l.len() as f32 / 48_000.0;
                    proj.sample_data = SavedSample::from_stereo_pcm(sname.clone(), 48_000, &pcm_l, &pcm_r, None);
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
        theme: StudioTheme,
    ) {
        // 1. PARTY DJ MODE DIRECTOR CARD
        card_frame(ui, theme, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("PARTY DJ MODE — CONTINUOUS GENRE SONG MIXER")
                        .strong()
                        .size(14.0)
                        .color(Color32::from_rgb(244, 92, 186)),
                );
                ui.separator();
                if toggle_pill(
                    ui,
                    if proj.party_mode { "PARTY DJ ACTIVE" } else { "START PARTY DJ" },
                    &mut proj.party_mode,
                    Color32::from_rgb(244, 92, 186),
                    "Generates full songs in your chosen Vibe (Intro -> Groove -> Build -> Drop -> Break -> Drop 2) and automatically mixes into a brand-new song at the end of each cycle!",
                ) && proj.party_mode
                    && !playing
                {
                    self.engine.playing.store(true, Ordering::Relaxed);
                }
                if ui.button("Next Section").on_hover_text("Jump to the next section of the current song").clicked() {
                    self.engine.send_live(LiveEvent::NextPartySection);
                }
                if ui.button("Drop Now").on_hover_text("Fire a key-locked sub bass drop right now").clicked() {
                    self.engine.send_live(LiveEvent::TriggerBassDrop);
                }
                if ui.button("⏮ Prev").on_hover_text("Jump back to the previous DJ song").clicked() {
                    proj.party_mode = true;
                    self.engine.send_live(LiveEvent::PrevPartySong);
                    self.engine.playing.store(true, Ordering::Relaxed);
                }
                if ui.button("Mix Next Song ⏭").on_hover_text("Generate a brand-new song with fresh voice palette, groove & arrangement and DJ-transition into it right now!").clicked() {
                    proj.party_seed = self.next_seed();
                    proj.party_mode = true;
                    self.engine.send_live(LiveEvent::NextPartySong);
                    self.engine.playing.store(true, Ordering::Relaxed);
                }
                if ui
                    .button("Reload Instruments")
                    .on_hover_text("Pick a fresh set of genre-matched instruments for the current DJ song without skipping the track")
                    .clicked()
                {
                    proj.party_mode = true;
                    self.engine.send_live(LiveEvent::ReloadPartyRig);
                }
                if proj.party_mode {
                    let cap_btn = egui::Button::new(
                        RichText::new("Capture Song → Studio")
                            .strong()
                            .color(Color32::from_rgb(14, 18, 24)),
                    )
                    .fill(Color32::from_rgb(72, 224, 148));
                    if ui
                        .add(cap_btn)
                        .on_hover_text("Save this generated DJ song (synths, drums, FX, patterns & sections) into your editable Studio & Song Arranger!")
                        .clicked()
                    {
                        let title = capture_party_song_to_project(proj, tel);
                        self.set_toast(
                            ui.ctx(),
                            format!("Captured '{title}' to Studio & Song Arranger!"),
                        );
                    }
                }
            });

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Party Energy:");
                ui.add(egui::Slider::new(&mut proj.party_energy, 0.15..=1.0).max_decimals(2));
                ui.checkbox(&mut proj.party_auto_evolve, "Auto-Mix New Songs & Progressions");
                ui.separator();
                ui.label("Chord Speed:");
                for &bpc in &[1u8, 2, 4] {
                    ui.selectable_value(&mut proj.bars_per_chord, bpc, format!("{bpc} bar{}", if bpc > 1 { "s" } else { "" }));
                }
            });

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Crate:");
                for &mode in &PartyMixMode::ALL {
                    ui.selectable_value(&mut proj.party_mix_mode, mode, mode.name());
                }
                ui.separator();
                ui.label("Pace:");
                for &pace in &PartyPace::ALL {
                    ui.selectable_value(&mut proj.party_pace, pace, pace.name());
                }
                ui.separator();
                ui.checkbox(&mut proj.party_sound_morph, "Dynamic Sound Morph")
                    .on_hover_text("Swaps synth/drum/sampler voice palettes between songs and dynamically sweeps filters, space FX, and timbres live!");
            });

            if proj.party_mix_mode == PartyMixMode::CustomCrate {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 3.0);
                    ui.label(RichText::new("Custom Crate:").size(11.0).weak());
                    if ui.small_button("All").clicked() {
                        proj.party_crate_mask = 0x00FF_FFFF;
                    }
                    if ui.small_button("None").clicked() {
                        proj.party_crate_mask = 0;
                    }
                    for (vi, &vibe) in Vibe::ALL.iter().enumerate() {
                        let bit = 1u32 << vi;
                        let mut on = (proj.party_crate_mask & bit) != 0;
                        if ui.selectable_label(on, vibe.name()).clicked() {
                            on = !on;
                            if on {
                                proj.party_crate_mask |= bit;
                            } else {
                                proj.party_crate_mask &= !bit;
                            }
                        }
                    }
                });
            }

            if proj.party_mode {
                let bpm_disp = if tel.party_bpm >= 40.0 {
                    tel.party_bpm.round() as u32
                } else {
                    proj.bpm.round() as u32
                };
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "NOW PLAYING: {}  •  {} BPM  •  Section: {} (Bar {} / {})  •  Active Chord: {}",
                        if tel.party_song_title.is_empty() {
                            proj.vibe.name()
                        } else {
                            &tel.party_song_title
                        },
                        bpm_disp,
                        tel.party_section,
                        tel.party_bar,
                        tel.party_section_bars,
                        tel.active_chord
                    ))
                    .strong()
                    .color(accent),
                );
                ui.add_space(3.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let next_lbl = if tel.party_next_title.is_empty() {
                        "Auto-selecting next track...".to_string()
                    } else {
                        format!(
                            "UP NEXT: {}  •  {:.0} BPM",
                            tel.party_next_title,
                            tel.party_next_bpm
                        )
                    };
                    ui.label(RichText::new(next_lbl).size(11.5).color(Color32::from_rgb(176, 186, 206)));
                    let q_lbl = match tel.party_queued_vibe {
                        Some(v) => format!("Queue: {}", v.name()),
                        None => "Queue Next: Auto".to_string(),
                    };
                    egui::ComboBox::from_id_salt("dj_queue_next_vibe")
                        .width(142.0)
                        .selected_text(q_lbl)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(tel.party_queued_vibe.is_none(), "Auto (Crate)")
                                .clicked()
                            {
                                self.engine.send_live(LiveEvent::QueueNextPartyVibe(None));
                            }
                            for &v in &Vibe::ALL {
                                if ui
                                    .selectable_label(tel.party_queued_vibe == Some(v), v.name())
                                    .clicked()
                                {
                                    self.engine
                                        .send_live(LiveEvent::QueueNextPartyVibe(Some(v)));
                                }
                            }
                        });
                });
                ui.add_space(6.0);
                egui::Frame::none()
                    .fill(theme.control_bg())
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                    .stroke(Stroke::new(1.0_f32, theme.border()))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(
                                RichText::new("DJ INSTRUMENTS")
                                    .strong()
                                    .size(11.5)
                                    .color(accent),
                            );
                            if ui
                                .small_button("Reload All")
                                .on_hover_text("Swap all DJ voices to a fresh genre-matched palette")
                                .clicked()
                            {
                                self.engine.send_live(LiveEvent::ReloadPartyRig);
                            }
                            if !tel.party_palette.is_empty() {
                                ui.separator();
                                ui.label(
                                    RichText::new(&tel.party_palette)
                                        .size(11.0)
                                        .color(Color32::from_rgb(176, 186, 206)),
                                );
                            }
                        });
                        ui.add_space(4.0);
                        const DJ_RIG_ITEMS: [(&str, TrackId, usize); 6] = [
                            ("Bass", TrackId::Moog, 1),
                            ("Chords", TrackId::MoogPad, 2),
                            ("Lead", TrackId::Monologue, 3),
                            ("Drums", TrackId::Drums, 0),
                            ("303", TrackId::Tb303, 4),
                            ("Sampler", TrackId::Sampler, 5),
                        ];
                        let avail_w = ui.available_width();
                        let cols = if avail_w >= 1260.0 {
                            6
                        } else if avail_w >= 580.0 {
                            3
                        } else {
                            2
                        };
                        let col_gap = 8.0;
                        let cell_w = ((avail_w - col_gap * (cols - 1) as f32) / cols as f32)
                            .floor()
                            .max(130.0);
                        let combo_w = (cell_w - 58.0).clamp(88.0, 160.0);
                        for (row_i, chunk) in DJ_RIG_ITEMS.chunks(cols).enumerate() {
                            if row_i > 0 {
                                ui.add_space(4.0);
                            }
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = col_gap;
                                for &(role_lbl, track, slot) in chunk {
                                    let presets = voice_preset_names(track);
                                    let raw_idx = if !tel.party_palette.is_empty() {
                                        tel.party_voice_presets[slot]
                                    } else {
                                        proj.voice_presets[slot]
                                    };
                                    let cur = raw_idx % presets.len().max(1);
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(cell_w, 22.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            ui.label(
                                                RichText::new(role_lbl)
                                                    .strong()
                                                    .size(11.0)
                                                    .color(track_color(track)),
                                            );
                                            egui::ComboBox::from_id_salt(("dj_rig_vp", slot))
                                                .width(combo_w)
                                                .selected_text(presets[cur])
                                                .show_ui(ui, |ui| {
                                                    for (pi, &pname) in presets.iter().enumerate() {
                                                        if ui.selectable_label(cur == pi, pname).clicked() {
                                                            apply_voice_preset(proj, track, pi);
                                                            self.engine.send_live(
                                                                LiveEvent::SetPartyInstrument {
                                                                    track,
                                                                    preset_idx: pi,
                                                                },
                                                            );
                                                        }
                                                    }
                                                });
                                        },
                                    );
                                }
                            });
                        }
                    });
            }
        });

        ui.add_space(10.0);

        // 2. VISUAL SONG ARRANGER
        card_frame(ui, theme, |ui| {
            let total_bars: usize = proj
                .song_sections
                .iter()
                .map(|s| s.bars.max(1) as usize)
                .sum();
            let total_secs = ((total_bars as f32) * 4.0 * 60.0 / proj.bpm.clamp(40.0, 240.0)).round() as usize;
            let mins = total_secs / 60;
            let secs = total_secs % 60;

            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("SONG ARRANGER").strong().size(14.0).color(accent));
                ui.separator();
                if toggle_pill(
                    ui,
                    if proj.song_mode { "SONG MODE ON" } else { "SONG MODE OFF" },
                    &mut proj.song_mode,
                    Color32::from_rgb(72, 224, 148),
                    "Play through the sections below in order (automatically pauses Party DJ Mode)",
                ) && proj.song_mode
                {
                    proj.party_mode = false;
                }
                ui.separator();
                ui.label(
                    RichText::new(format!("Total: {total_bars} bars ({mins}:{secs:02})"))
                        .size(11.5)
                        .weak(),
                );
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let auto_btn = egui::Button::new(
                    RichText::new("Auto-Arrange Loop → Song")
                        .strong()
                        .color(Color32::from_rgb(14, 18, 24)),
                )
                .fill(accent);
                if ui
                    .add(auto_btn)
                    .on_hover_text("Build a full 4-pattern, 7-section song (Intro -> Groove -> Build -> Drop -> Break -> Drop 2 -> Outro) from your active loop!")
                    .clicked()
                {
                    let seed = self.next_seed();
                    auto_arrange_project_song(proj, seed);
                    self.set_toast(
                        ui.ctx(),
                        "Built 7-section Song Arrangement in Patterns P1–P4!",
                    );
                }
                if ui.button("+ Add Section").clicked() {
                    proj.song_sections.push(SongSection {
                        name: format!("Part {}", proj.song_sections.len() + 1),
                        pattern: proj.active_pattern,
                        bars: 4,
                        bass_drop: false,
                        fill: true,
                        track_mask: 0b111111,
                        extra_mask: 0x03FF,
                    });
                }
                if ui
                    .button("Clone Pat → Next")
                    .on_hover_text("Copy the active pattern into the next pattern slot for fast Verse/Chorus variations")
                    .clicked()
                {
                    let n_pats = proj.patterns.len().max(1);
                    let src = proj.active_pattern.min(n_pats - 1);
                    let dst = (src + 1) % n_pats;
                    let cloned = proj.patterns[src].clone();
                    proj.patterns[dst] = cloned;
                    proj.active_pattern = dst;
                    self.set_toast(
                        ui.ctx(),
                        format!("Cloned P{} → P{}", src + 1, dst + 1),
                    );
                }
                if ui.button("Reset Default").clicked() {
                    proj.song_sections = crate::model::default_vibe_sections(proj.active_pattern);
                }
                if ui
                    .button("Export .MID")
                    .on_hover_text("Export arranged sections / active pattern as a multi-track Standard MIDI (.mid) file")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_file_name(format!("{}_song.mid", proj.vibe.name().to_lowercase().replace(' ', "_")))
                        .add_filter("Standard MIDI", &["mid", "midi"])
                        .save_file()
                    {
                        match export_project_midi(proj, &path) {
                            Ok(()) => self.set_toast(ui.ctx(), "Exported Standard MIDI (.mid) tracks!"),
                            Err(e) => self.set_toast(ui.ctx(), format!("MIDI export error: {e}")),
                        }
                    }
                }
            });

            ui.add_space(8.0);

            let mut remove_idx = None;
            let mut move_up_idx = None;
            let mut move_down_idx = None;
            let mut dup_idx = None;
            let n_pats = proj.patterns.len().max(1);
            let extra_count = proj.extra_tracks.len().min(MAX_EXTRA_TRACKS);
            let extra_names: Vec<String> = proj
                .extra_tracks
                .iter()
                .take(extra_count)
                .map(|t| t.name.clone())
                .collect();

            egui::ScrollArea::horizontal()
                .id_salt("song_arr_scroll")
                .show(ui, |ui| {
                    egui::Grid::new("song_arranger_grid")
                        .striped(true)
                        .spacing(Vec2::new(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.strong("Play");
                            ui.strong("Section");
                            ui.strong("Pat");
                            ui.strong("Bars");
                            ui.strong("Drums");
                            ui.strong("Bass");
                            ui.strong("Chords");
                            ui.strong("Arp");
                            ui.strong("303");
                            ui.strong("Samp");
                            for (ei, ename) in extra_names.iter().enumerate() {
                                ui.strong(format!("E{}", ei + 1))
                                    .on_hover_text(ename);
                            }
                            ui.strong("Drop");
                            ui.strong("Fill");
                            ui.strong("Order");
                            ui.end_row();

                            let sec_count = proj.song_sections.len();
                            for (idx, sec) in proj.song_sections.iter_mut().enumerate() {
                                let is_active_sec = proj.song_mode
                                    && !proj.party_mode
                                    && tel.active_song_section == Some(idx);
                                let play_lbl = if is_active_sec {
                                    format!("▶ {}/{}", tel.party_bar, sec.bars.max(1))
                                } else {
                                    "▶".to_string()
                                };
                                let pbtn = egui::Button::new(
                                    RichText::new(play_lbl)
                                        .strong()
                                        .size(11.0)
                                        .color(if is_active_sec {
                                            Color32::from_rgb(12, 14, 20)
                                        } else {
                                            Color32::from_rgb(212, 220, 236)
                                        }),
                                )
                                .fill(if is_active_sec {
                                    accent
                                } else {
                                    theme.control_bg()
                                });
                                if ui
                                    .add(pbtn)
                                    .on_hover_text("Jump to and play this section now")
                                    .clicked()
                                {
                                    proj.party_mode = false;
                                    proj.song_mode = true;
                                    self.engine.send_live(LiveEvent::JumpToSongSection {
                                        section_idx: idx,
                                    });
                                    self.engine.playing.store(true, Ordering::Relaxed);
                                }

                                ui.add(egui::TextEdit::singleline(&mut sec.name).desired_width(88.0));

                                let pat_cur = sec.pattern.min(n_pats - 1);
                                egui::ComboBox::from_id_salt(("sec_pat", idx))
                                    .width(50.0)
                                    .selected_text(format!("P{}", pat_cur + 1))
                                    .show_ui(ui, |ui| {
                                        for pi in 0..n_pats {
                                            ui.selectable_value(
                                                &mut sec.pattern,
                                                pi,
                                                format!("P{}", pi + 1),
                                            );
                                        }
                                    });

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
                                for ei in 0..extra_count {
                                    let mut on = (sec.extra_mask & (1u16 << ei)) != 0;
                                    if ui
                                        .checkbox(&mut on, "")
                                        .on_hover_text(&extra_names[ei])
                                        .changed()
                                    {
                                        if on {
                                            sec.extra_mask |= 1u16 << ei;
                                        } else {
                                            sec.extra_mask &= !(1u16 << ei);
                                        }
                                    }
                                }
                                ui.checkbox(&mut sec.bass_drop, "");
                                ui.checkbox(&mut sec.fill, "");
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    if ui.add_enabled(idx > 0, egui::Button::new("▲").small()).clicked() {
                                        move_up_idx = Some(idx);
                                    }
                                    if ui
                                        .add_enabled(idx + 1 < sec_count, egui::Button::new("▼").small())
                                        .clicked()
                                    {
                                        move_down_idx = Some(idx);
                                    }
                                    if ui
                                        .small_button("Dup")
                                        .on_hover_text("Duplicate section below")
                                        .clicked()
                                    {
                                        dup_idx = Some(idx);
                                    }
                                    if ui
                                        .add_enabled(sec_count > 1, egui::Button::new("X").small())
                                        .clicked()
                                    {
                                        remove_idx = Some(idx);
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });

            if let Some(i) = move_up_idx {
                if i > 0 {
                    proj.song_sections.swap(i, i - 1);
                }
            } else if let Some(i) = move_down_idx {
                if i + 1 < proj.song_sections.len() {
                    proj.song_sections.swap(i, i + 1);
                }
            } else if let Some(i) = dup_idx {
                let copy = proj.song_sections[i].clone();
                proj.song_sections.insert(i + 1, copy);
            } else if let Some(i) = remove_idx {
                if proj.song_sections.len() > 1 {
                    proj.song_sections.remove(i);
                }
            }
        });
    }

    fn ui_sound_lab(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        proj: &mut Project,
        tel: &EngineTelemetry,
        accent: Color32,
        theme: StudioTheme,
    ) {
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
                card_frame(ui, theme, |ui| {
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
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(lab_title)
                                .strong()
                                .size(13.0)
                                .color(tc),
                        );
                        ui.separator();
                        for &(mode, lbl) in &[
                            (TestRepeatMode::Quarter, "1/4"),
                            (TestRepeatMode::Eighth, "1/8"),
                            (TestRepeatMode::Drone, "Drone"),
                        ] {
                            if ui.selectable_label(self.test_repeat_mode == mode, lbl).clicked() {
                                self.test_repeat_mode = mode;
                                self.stop_test_repeat_note();
                                self.test_repeat_next_t = ctx.input(|i| i.time);
                            }
                        }
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
                    });
                    ui.add_space(6.0);

                    // Sleek Preset Browser Bar + Favorites + User Patch Saving inside Sound Lab!
                    let preset_track = if active_extra.is_some() {
                        TrackId::MoogPad
                    } else {
                        self.selected_track
                    };
                    let lab_presets = voice_preset_names(preset_track);
                    if !lab_presets.is_empty() {
                        let cur_p = if let Some(ei) = active_extra {
                            proj.extra_tracks[ei].preset_idx % lab_presets.len()
                        } else {
                            let slot_idx = TrackId::STUDIO_TRACKS
                                .iter()
                                .position(|&t| t == self.selected_track)
                                .unwrap_or(0);
                            let raw_idx = if proj.party_mode && !tel.party_palette.is_empty() {
                                tel.party_voice_presets[slot_idx]
                            } else {
                                proj.voice_presets[slot_idx]
                            };
                            raw_idx % lab_presets.len()
                        };
                        let cur_name = lab_presets[cur_p];
                        let fav_key = format!("{}:{}", preset_track.role_name(), cur_name);
                        let is_fav = self.settings.is_favorite(&fav_key);

                        let apply_lab_preset = |app: &FunKeySynthApp, proj: &mut Project, pi: usize| {
                            if let Some(ei) = active_extra {
                                apply_extra_track_preset(&mut proj.extra_tracks[ei], pi);
                                proj.extra_tracks[ei].name = lab_presets[pi].to_string();
                            } else {
                                apply_voice_preset(proj, app.selected_track, pi);
                                if proj.party_mode {
                                    app.engine.send_live(LiveEvent::SetPartyInstrument {
                                        track: app.selected_track,
                                        preset_idx: pi,
                                    });
                                }
                            }
                        };

                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            ui.label(
                                RichText::new("PRESET:")
                                    .strong()
                                    .size(11.0)
                                    .color(Color32::from_rgb(165, 176, 200)),
                            );
                            if ui.small_button("◀").on_hover_text("Previous preset").clicked() {
                                let prev = if cur_p == 0 { lab_presets.len() - 1 } else { cur_p - 1 };
                                apply_lab_preset(self, proj, prev);
                            }
                            let cur_btn = egui::Button::new(
                                RichText::new(format!(
                                    "{} ({}/{})",
                                    cur_name,
                                    cur_p + 1,
                                    lab_presets.len()
                                ))
                                .strong()
                                .size(11.5)
                                .color(Color32::from_rgb(12, 14, 20)),
                            )
                            .fill(tc);
                            if ui
                                .add(cur_btn)
                                .on_hover_text("Click to open searchable preset browser")
                                .clicked()
                            {
                                self.show_preset_browser = !self.show_preset_browser;
                            }
                            if ui.small_button("▶").on_hover_text("Next preset").clicked() {
                                let next = (cur_p + 1) % lab_presets.len();
                                apply_lab_preset(self, proj, next);
                            }
                            let fav_btn = egui::Button::new(
                                RichText::new(if is_fav { "★" } else { "☆" })
                                    .strong()
                                    .size(12.5)
                                    .color(if is_fav {
                                        Color32::from_rgb(255, 204, 64)
                                    } else {
                                        Color32::from_rgb(180, 190, 210)
                                    }),
                            );
                            if ui.add(fav_btn).on_hover_text("Toggle Favorite preset").clicked() {
                                self.settings.toggle_favorite(&fav_key);
                            }
                            if ui
                                .selectable_label(
                                    self.show_preset_browser,
                                    format!("Browse ({}) ▾", lab_presets.len()),
                                )
                                .clicked()
                            {
                                self.show_preset_browser = !self.show_preset_browser;
                            }
                            if active_extra.is_some() || self.selected_track != TrackId::MidiOut {
                                if ui
                                    .selectable_label(self.show_save_patch, "Save Patch")
                                    .on_hover_text("Save your current tweaked sound as a reusable User Patch")
                                    .clicked()
                                {
                                    self.show_save_patch = !self.show_save_patch;
                                    if self.show_save_patch && self.save_patch_name.trim().is_empty() {
                                        self.save_patch_name = format!("My {}", cur_name);
                                    }
                                }
                            }
                        });

                        if self.show_save_patch {
                            ui.add_space(4.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new("Patch Name:").size(11.0).weak());
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.save_patch_name)
                                        .desired_width(150.0),
                                );
                                if ui.button("Save").clicked() {
                                    let name = self.save_patch_name.trim().to_string();
                                    if !name.is_empty() {
                                        let data = if let Some(ei) = active_extra {
                                            UserPatchData::Moog(proj.extra_tracks[ei].synth.clone())
                                        } else {
                                            match self.selected_track {
                                                TrackId::Drums => UserPatchData::Drums(proj.drums.clone()),
                                                TrackId::Moog => UserPatchData::Moog(proj.moog.clone()),
                                                TrackId::MoogPad => UserPatchData::Moog(proj.moog_pad.clone()),
                                                TrackId::Monologue => UserPatchData::Monologue(proj.monologue.clone()),
                                                TrackId::Tb303 => UserPatchData::Tb303(proj.tb303.clone()),
                                                TrackId::Sampler | TrackId::MidiOut => {
                                                    UserPatchData::Sampler(proj.sampler.clone())
                                                }
                                            }
                                        };
                                        let patch = UserPatch {
                                            name: name.clone(),
                                            track_role: preset_track.role_name().to_string(),
                                            data,
                                        };
                                        match AppSettings::save_user_patch(&patch) {
                                            Ok(()) => {
                                                self.user_patches = AppSettings::load_user_patches();
                                                self.show_save_patch = false;
                                                self.show_preset_browser = true;
                                                self.set_toast(ctx, format!("Saved patch '{name}'!"));
                                            }
                                            Err(e) => self.set_toast(ctx, format!("Save patch error: {e}")),
                                        }
                                    }
                                }
                                if ui.small_button("Cancel").clicked() {
                                    self.show_save_patch = false;
                                }
                            });
                        }

                        if self.show_preset_browser {
                            ui.add_space(4.0);
                            egui::Frame::none()
                                .fill(theme.control_bg())
                                .rounding(Rounding::same(6.0))
                                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                                .stroke(Stroke::new(1.0_f32, theme.border()))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        for &(ci, clbl) in &[
                                            (0usize, "All"),
                                            (1, "★ Favs"),
                                            (2, "Analog"),
                                            (3, "FM & Digital"),
                                            (4, "Orchestral"),
                                            (5, "Acoustic"),
                                            (6, "User"),
                                        ] {
                                            ui.selectable_value(&mut self.preset_cat_filter, ci, clbl);
                                        }
                                        ui.separator();
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.preset_search)
                                                .hint_text("Search presets...")
                                                .desired_width(120.0),
                                        );
                                    });
                                    ui.add_space(4.0);
                                    let q = self.preset_search.trim().to_ascii_lowercase();
                                    let mut del_user_patch: Option<String> = None;
                                    let mut loaded_patch_toast: Option<String> = None;
                                    egui::ScrollArea::vertical()
                                        .id_salt("sound_lab_preset_scroll")
                                        .max_height(150.0)
                                        .show(ui, |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                                                if self.preset_cat_filter != 6 {
                                                    for (pi, &pname) in lab_presets.iter().enumerate() {
                                                        let cat = voice_preset_category(preset_track, pi);
                                                        let pkey = format!("{}:{}", preset_track.role_name(), pname);
                                                        let fav = self.settings.is_favorite(&pkey);
                                                        let cat_ok = match self.preset_cat_filter {
                                                            0 => true,
                                                            1 => fav,
                                                            2 => cat == "Analog" || cat == "Club / 808 / 909" || cat == "Acid" || cat == "Retro / Linn",
                                                            3 => cat == "FM & Digital",
                                                            4 => cat == "Orchestral",
                                                            5 => cat == "Acoustic",
                                                            _ => true,
                                                        };
                                                        if !cat_ok {
                                                            continue;
                                                        }
                                                        if !q.is_empty() && !pname.to_ascii_lowercase().contains(&q) {
                                                            continue;
                                                        }
                                                        let active = cur_p == pi;
                                                        let lbl = if fav {
                                                            format!("★ {pname}")
                                                        } else {
                                                            pname.to_string()
                                                        };
                                                        let pbtn = egui::Button::new(
                                                            RichText::new(lbl)
                                                                .strong()
                                                                .size(11.0)
                                                                .color(if active {
                                                                    Color32::from_rgb(12, 14, 20)
                                                                } else {
                                                                    Color32::from_rgb(212, 220, 236)
                                                                }),
                                                        )
                                                        .fill(if active { tc } else { Color32::from_rgb(24, 29, 42) })
                                                        .stroke(Stroke::new(
                                                            1.0_f32,
                                                            if active { tc } else { Color32::from_rgb(48, 56, 78) },
                                                        ));
                                                        if ui.add(pbtn).on_hover_text(cat).clicked() {
                                                            apply_lab_preset(self, proj, pi);
                                                        }
                                                    }
                                                }
                                                if self.preset_cat_filter == 0 || self.preset_cat_filter == 6 {
                                                    for up in &self.user_patches {
                                                        let matches_track = if active_extra.is_some() {
                                                            matches!(up.data, UserPatchData::Moog(_))
                                                        } else {
                                                            match (self.selected_track, &up.data) {
                                                                (TrackId::Drums, UserPatchData::Drums(_)) => true,
                                                                (TrackId::Moog | TrackId::MoogPad, UserPatchData::Moog(_)) => true,
                                                                (TrackId::Monologue, UserPatchData::Monologue(_)) => true,
                                                                (TrackId::Tb303, UserPatchData::Tb303(_)) => true,
                                                                (TrackId::Sampler, UserPatchData::Sampler(_)) => true,
                                                                _ => false,
                                                            }
                                                        };
                                                        if !matches_track {
                                                            continue;
                                                        }
                                                        if !q.is_empty() && !up.name.to_ascii_lowercase().contains(&q) {
                                                            continue;
                                                        }
                                                        if ui
                                                            .button(
                                                                RichText::new(format!("User: {}", up.name))
                                                                    .size(11.0)
                                                                    .color(Color32::from_rgb(120, 230, 175)),
                                                            )
                                                            .clicked()
                                                        {
                                                            if let Some(ei) = active_extra {
                                                                if let UserPatchData::Moog(ref mp) = up.data {
                                                                    proj.extra_tracks[ei].synth = mp.clone();
                                                                    proj.extra_tracks[ei].name = up.name.clone();
                                                                }
                                                            } else {
                                                                match (&up.data, self.selected_track) {
                                                                    (UserPatchData::Drums(dk), TrackId::Drums) => proj.drums = dk.clone(),
                                                                    (UserPatchData::Moog(mp), TrackId::Moog) => proj.moog = mp.clone(),
                                                                    (UserPatchData::Moog(mp), TrackId::MoogPad) => proj.moog_pad = mp.clone(),
                                                                    (UserPatchData::Monologue(mn), TrackId::Monologue) => proj.monologue = mn.clone(),
                                                                    (UserPatchData::Tb303(tb), TrackId::Tb303) => proj.tb303 = tb.clone(),
                                                                    (UserPatchData::Sampler(sp), TrackId::Sampler) => proj.sampler = sp.clone(),
                                                                    _ => {}
                                                                }
                                                            }
                                                            loaded_patch_toast = Some(up.name.clone());
                                                        }
                                                        if ui
                                                            .small_button("✕")
                                                            .on_hover_text("Delete user patch")
                                                            .clicked()
                                                        {
                                                            del_user_patch = Some(up.name.clone());
                                                        }
                                                    }
                                                }
                                            });
                                        });
                                    if let Some(pname) = loaded_patch_toast {
                                        self.set_toast(ctx, format!("Loaded patch '{pname}'"));
                                    }
                                    if let Some(name) = del_user_patch {
                                        let _ = AppSettings::delete_user_patch(&name);
                                        self.user_patches = AppSettings::load_user_patches();
                                    }
                                });
                        }
                        ui.add_space(6.0);
                        ui.separator();
                    }

                    {
                        let f_params = track_filter_mut(proj, self.selected_track, active_extra);
                        ui_instrument_filter_bar(ui, f_params, tc, theme);
                        ui.add_space(6.0);
                    }

                    {
                        let cur_engine = if let Some(ei) = active_extra {
                            Some(proj.extra_tracks[ei].synth.engine)
                        } else {
                            match self.selected_track {
                                TrackId::Moog => Some(proj.moog.engine),
                                TrackId::MoogPad => Some(proj.moog_pad.engine),
                                TrackId::Monologue => Some(proj.monologue.engine),
                                _ => None,
                            }
                        };
                        if cur_engine == Some(SynthEngine::PhilharmoniaD50) {
                            let bank_names = SynthEngine::PhilharmoniaD50.bank_names();
                            let bank_val = if let Some(ei) = active_extra {
                                proj.extra_tracks[ei].synth.engine_bank
                            } else {
                                match self.selected_track {
                                    TrackId::Moog => proj.moog.engine_bank,
                                    TrackId::MoogPad => proj.moog_pad.engine_bank,
                                    TrackId::Monologue => proj.monologue.engine_bank,
                                    _ => 0,
                                }
                            };
                            let cur_bi = (bank_val as usize) % bank_names.len();
                            preload_philharmonia_bank_async(cur_bi as u8);
                            let (on_disk, zones, cached) = philharmonia_bank_status(cur_bi as u8);
                            if on_disk && !cached {
                                ctx.request_repaint_after(std::time::Duration::from_millis(120));
                            }
                            let (status_txt, status_col) = if cached && on_disk {
                                (
                                    format!("Studio Library • {zones} real sampled zones ready"),
                                    Color32::from_rgb(92, 236, 180),
                                )
                            } else if !cached && on_disk {
                                (
                                    "Loading studio MP3 zones...".to_string(),
                                    Color32::from_rgb(245, 206, 112),
                                )
                            } else {
                                (
                                    "Built-in synth fallback (philharmonia-all-samples not found)".to_string(),
                                    Color32::from_rgb(165, 176, 200),
                                )
                            };
                            ui.group(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new("PHILHARMONIA ORCHESTRA")
                                            .strong()
                                            .size(11.5)
                                            .color(Color32::from_rgb(245, 206, 112)),
                                    );
                                    ui.label(RichText::new(status_txt).small().color(status_col));
                                });
                                ui.horizontal_wrapped(|ui| {
                                    const CATS: [&str; 7] = [
                                        "All",
                                        "Ensembles",
                                        "Strings",
                                        "Plucked",
                                        "Brass",
                                        "Woodwinds",
                                        "Perc",
                                    ];
                                    for (ci, &clbl) in CATS.iter().enumerate() {
                                        if ui.selectable_label(self.phil_cat == ci, clbl).clicked() {
                                            self.phil_cat = ci;
                                        }
                                    }
                                });
                                ui.add_space(2.0);
                                let mut picked_bi: Option<usize> = None;
                                ui.horizontal_wrapped(|ui| {
                                    let iter_ids: Vec<usize> = if self.phil_cat == 0 {
                                        (0..bank_names.len()).collect()
                                    } else {
                                        PHIL_SECTIONS
                                            .get(self.phil_cat - 1)
                                            .map(|s| s.1.to_vec())
                                            .unwrap_or_default()
                                    };
                                    for bi in iter_ids {
                                        let active = cur_bi == bi;
                                        let lbl = bank_names[bi];
                                        let btn = egui::Button::new(
                                            RichText::new(lbl).size(11.0).color(if active {
                                                Color32::from_rgb(12, 14, 20)
                                            } else {
                                                Color32::from_rgb(220, 226, 240)
                                            }),
                                        )
                                        .fill(if active {
                                            Color32::from_rgb(245, 206, 112)
                                        } else {
                                            Color32::from_rgb(28, 34, 48)
                                        });
                                        if ui.add(btn).clicked() {
                                            picked_bi = Some(bi);
                                        }
                                    }
                                });
                                if let Some(bi) = picked_bi {
                                    preload_philharmonia_bank_async(bi as u8);
                                    if let Some(ei) = active_extra {
                                        proj.extra_tracks[ei].synth.engine_bank = bi as u8;
                                        proj.extra_tracks[ei].name = bank_names[bi].to_string();
                                        proj.extra_tracks[ei].synth.engine_morph =
                                            if bi == 3 { 0.48 } else { 0.0 };
                                    } else {
                                        let target_m = match self.selected_track {
                                            TrackId::Moog => Some(&mut proj.moog.engine_bank),
                                            TrackId::MoogPad => Some(&mut proj.moog_pad.engine_bank),
                                            _ => None,
                                        };
                                        if let Some(bref) = target_m {
                                            *bref = bi as u8;
                                        } else if self.selected_track == TrackId::Monologue {
                                            proj.monologue.engine_bank = bi as u8;
                                        }
                                    }
                                }
                                if cur_bi == 25 {
                                    ui.add_space(3.0);
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new("Kit Map (C3–B4):")
                                                .small()
                                                .strong()
                                                .color(Color32::from_rgb(180, 190, 212)),
                                        );
                                        for &pl in &PHILHARMONIA_PERC_LABELS {
                                            ui.label(
                                                RichText::new(pl)
                                                    .small()
                                                    .color(Color32::from_rgb(156, 168, 194)),
                                            );
                                        }
                                    });
                                }
                            });
                            ui.add_space(4.0);
                        }
                        if cur_engine == Some(SynthEngine::SoundFont) {
                            let (sf2_name, sf2_presets) = self.engine.soundfont_meta();
                            ui.horizontal_wrapped(|ui| {
                                if ui.button("Load .sf2...").clicked() {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("SoundFont 2.0", &["sf2", "SF2"])
                                        .pick_file()
                                    {
                                        match self.engine.load_soundfont_file(&path) {
                                            Ok(name) => {
                                                proj.soundfont_path = Some(path.to_string_lossy().into_owned());
                                                self.toast = Some((format!("Loaded SF2: {name}"), 3.5));
                                            }
                                            Err(err) => {
                                                self.toast = Some((format!("SF2 Error: {err}"), 4.0));
                                            }
                                        }
                                    }
                                }
                                let lbl = if sf2_name.is_empty() {
                                    "Built-in EP fallback (no .sf2 loaded)"
                                } else {
                                    sf2_name.as_str()
                                };
                                ui.label(RichText::new(lbl).small().color(tc));
                                if !sf2_presets.is_empty() {
                                    let bank_mut = if let Some(ei) = active_extra {
                                        &mut proj.extra_tracks[ei].synth.engine_bank
                                    } else {
                                        match self.selected_track {
                                            TrackId::Moog => &mut proj.moog.engine_bank,
                                            TrackId::MoogPad => &mut proj.moog_pad.engine_bank,
                                            TrackId::Monologue => &mut proj.monologue.engine_bank,
                                            _ => &mut proj.moog.engine_bank,
                                        }
                                    };
                                    let cur_idx = (*bank_mut as usize) % sf2_presets.len();
                                    let cur_label = sf2_presets
                                        .get(cur_idx)
                                        .map(|s| s.as_str())
                                        .unwrap_or("Preset");
                                    egui::ComboBox::from_id_salt("sf2_preset_sel")
                                        .width(190.0)
                                        .selected_text(cur_label)
                                        .show_ui(ui, |ui| {
                                            for (pi, pname) in sf2_presets.iter().take(255).enumerate() {
                                                let v = pi as u8;
                                                if ui.selectable_value(bank_mut, v, pname).clicked() {
                                                    proj.soundfont_preset = pi;
                                                }
                                            }
                                        });
                                }
                            });
                            ui.add_space(4.0);
                        }
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
                                if ui.button("Acoustic").clicked() {
                                    proj.drums.set_all_model(DrumModel::Acoustic);
                                }
                                if ui.button("80s Linn").clicked() {
                                    proj.drums.set_all_model(DrumModel::Linn80s);
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
                                let name = DrumVoiceId::from_index(i)
                                    .map(|d| d.full_name_for_model(v.model))
                                    .unwrap_or("Drum");
                                egui::CollapsingHeader::new(
                                    RichText::new(format!(
                                        "{}  [{}]{}",
                                        name,
                                        v.model.name(),
                                        if v.mute { "  (MUTED)" } else { "" }
                                    ))
                                    .strong()
                                    .color(if v.mute { Color32::GRAY } else { tc }),
                                )
                                .default_open(i < 2)
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        if ui.small_button("Hit").clicked() {
                                            v.mute = false;
                                            self.engine.send_live(LiveEvent::PlayNote {
                                                track: TrackId::Drums,
                                                note: i as u8,
                                                velocity: 0.92,
                                            });
                                        }
                                        ui.checkbox(&mut v.mute, "Mute");
                                        for &m in &DrumModel::ALL {
                                            ui.selectable_value(&mut v.model, m, m.short_name());
                                        }
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
                            ui.horizontal_wrapped(|ui| {
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
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Grit Mode:");
                                ui.selectable_value(&mut s.mode, SamplerMode::Sk1LoFi, "8-bit SK-1");
                                ui.selectable_value(&mut s.mode, SamplerMode::Vintage12Bit, "12-bit");
                                ui.selectable_value(&mut s.mode, SamplerMode::HiFi, "Hi-Fi");
                            });
                            ui.horizontal_wrapped(|ui| {
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
                            let mut clear_samp_thru = false;
                            ui.horizontal_wrapped(|ui| {
                                ui.label("Filter:");
                                if ui
                                    .selectable_label(!s.filter_enabled, "Thru (Off)")
                                    .on_hover_text("Bypass sampler filter (clean passthrough)")
                                    .clicked()
                                {
                                    s.set_filter_passthrough();
                                }
                                for &(mode, lbl) in &[
                                    (GlobalFilterMode::LowPass, "LP"),
                                    (GlobalFilterMode::BandPass, "BP"),
                                    (GlobalFilterMode::HighPass, "HP"),
                                ] {
                                    if ui
                                        .selectable_label(s.filter_enabled && s.filter_mode == mode, lbl)
                                        .clicked()
                                    {
                                        s.filter_enabled = true;
                                        s.filter_mode = mode;
                                    }
                                }
                                if ui
                                    .small_button("Clear (Thru)")
                                    .on_hover_text("Clear sampler & channel filters to clean passthrough (Hi-Fi, 0 grit)")
                                    .clicked()
                                {
                                    s.set_filter_passthrough();
                                    s.mode = SamplerMode::HiFi;
                                    s.bit_crush = 0.0;
                                    clear_samp_thru = true;
                                }
                            });
                            if ui
                                .add(egui::Slider::new(&mut s.cutoff, 150.0..=18_000.0).logarithmic(true).text("Cutoff Hz"))
                                .changed()
                            {
                                s.filter_enabled = true;
                            }
                            if ui
                                .add(egui::Slider::new(&mut s.resonance, 0.0..=0.88).text("Resonance"))
                                .changed()
                            {
                                s.filter_enabled = true;
                            }
                            ui.add(egui::Slider::new(&mut s.attack, 0.001..=0.5).text("Attack"));
                            ui.add(egui::Slider::new(&mut s.release, 0.02..=2.0).text("Release"));
                            if clear_samp_thru {
                                proj.mixer.channels[TrackId::Sampler.index()].filter =
                                    TrackFilterParams::default();
                            }
                        }
                        TrackId::MidiOut => {}
                    }
                    }
                });
            });

            // Right: Stereo FX Rack + Full Stereo Mixer
            cols[1].vertical(|ui| {
                card_frame(ui, theme, |ui| {
                    ui.label(RichText::new("STEREO FX & MASTER").strong().size(13.0).color(accent));
                    ui.add_space(4.0);
                    let fx = &mut proj.fx;
                    ui.horizontal_wrapped(|ui| {
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
                    ui.add(egui::Slider::new(&mut fx.reverb_shimmer, 0.0..=1.0).text("Shimmer"));
                    ui.separator();

                    ui.horizontal_wrapped(|ui| {
                        ui.checkbox(&mut fx.global_filter_enabled, "Master Filter");
                        ui.selectable_value(&mut fx.global_filter_mode, GlobalFilterMode::LowPass, "LP");
                        ui.selectable_value(&mut fx.global_filter_mode, GlobalFilterMode::HighPass, "HP");
                        ui.selectable_value(&mut fx.global_filter_mode, GlobalFilterMode::BandPass, "BP");
                    });
                    ui.add(egui::Slider::new(&mut fx.global_filter_cutoff, 45.0..=18_000.0).logarithmic(true).text("Cutoff Hz"));
                    ui.add(egui::Slider::new(&mut fx.global_filter_resonance, 0.0..=0.88).text("Resonance"));
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Env Trig:");
                        for &t in &FilterEnvTrigger::ALL {
                            ui.selectable_value(&mut fx.global_filter_env_trigger, t, t.name());
                        }
                    });
                    ui.add(egui::Slider::new(&mut fx.global_filter_env_amount, -1.0..=1.0).text("Env Amount"));
                    ui.add(egui::Slider::new(&mut fx.global_filter_env_attack, 0.002..=1.5).logarithmic(true).text("Env Attack"));
                    ui.add(egui::Slider::new(&mut fx.global_filter_env_decay, 0.04..=3.0).logarithmic(true).text("Env Decay"));
                    ui.add(egui::Slider::new(&mut fx.analog_warmth, 0.0..=1.0).text("Tape Warmth"));
                    ui.add(egui::Slider::new(&mut fx.master_crisp, 0.0..=1.0).text("Modern Crisp"));
                    ui.add(egui::Slider::new(&mut fx.stereo_chorus, 0.0..=1.0).text("Juno Chorus"));
                    ui.add(egui::Slider::new(&mut fx.club_punch, 0.0..=1.0).text("Club Punch"));
                });

                ui.add_space(8.0);

                card_frame(ui, theme, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("STEREO CONSOLE & METERS").strong().size(13.0).color(accent));
                        ui.separator();
                        let mono_btn = egui::Button::new(
                            RichText::new(if proj.mixer.mono_sub { "MONO SUB ON" } else { "Mono Sub <120Hz" })
                                .size(10.5)
                                .strong(),
                        )
                        .fill(if proj.mixer.mono_sub {
                            Color32::from_rgb(64, 188, 255)
                        } else {
                            Color32::from_rgb(36, 42, 58)
                        });
                        if ui
                            .add(mono_btn)
                            .on_hover_text("Sum master frequencies below 120 Hz to pure mono for club/PA tight sub bass")
                            .clicked()
                        {
                            proj.mixer.mono_sub = !proj.mixer.mono_sub;
                        }
                        if ui.small_button("Reset Pan/Width").clicked() {
                            for ch in &mut proj.mixer.channels {
                                ch.pan = 0.0;
                                ch.width = 1.0;
                            }
                            proj.mixer.moog_pad_width = 1.0;
                            for tr in &mut proj.extra_tracks {
                                tr.pan = 0.0;
                                tr.width = 1.0;
                            }
                        }
                    });
                    // Master L/R Peak Readout
                    let pk = tel.peak_l.max(tel.peak_r);
                    let db = if pk > 0.0001 {
                        20.0 * pk.log10()
                    } else {
                        -60.0
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Master Peak: {db:.1} dBFS")).size(10.5).weak());
                        let (mrect, _) = ui.allocate_exact_size(Vec2::new(110.0, 8.0), Sense::hover());
                        let pnt = ui.painter();
                        pnt.rect_filled(mrect, Rounding::same(3.0), Color32::from_rgb(20, 24, 34));
                        let mw = mrect.width() * pk.clamp(0.0, 1.0);
                        let mcol = if pk > 0.92 {
                            Color32::from_rgb(245, 76, 86)
                        } else if pk > 0.72 {
                            Color32::from_rgb(255, 198, 66)
                        } else {
                            Color32::from_rgb(72, 224, 148)
                        };
                        pnt.rect_filled(
                            egui::Rect::from_min_size(mrect.min, Vec2::new(mw, mrect.height())),
                            Rounding::same(3.0),
                            mcol,
                        );
                    });
                    ui.add_space(4.0);
                    ui.spacing_mut().slider_width = 52.0;
                    egui::ScrollArea::horizontal()
                        .id_salt("console_mix_scroll")
                        .show(ui, |ui| {
                            egui::Grid::new("stereo_mix_grid").striped(true).show(ui, |ui| {
                                ui.strong("Track");
                                ui.strong("M/S");
                                ui.strong("Lvl");
                                ui.strong("Vol");
                                ui.strong("Pan");
                                ui.strong("Width");
                                ui.strong("Delay");
                                ui.strong("Reverb");
                                ui.end_row();

                                for &track in &TrackId::STUDIO_TRACKS {
                                    let peak_slot = match track {
                                        TrackId::Moog => 0,
                                        TrackId::Monologue => 1,
                                        TrackId::Tb303 => 2,
                                        TrackId::Drums => 3,
                                        TrackId::Sampler => 4,
                                        TrackId::MoogPad => 5,
                                        TrackId::MidiOut => 0,
                                    };
                                    let tpeak = tel.track_peaks[peak_slot].clamp(0.0, 1.0);
                                    if ui
                                        .selectable_label(
                                            self.selected_extra.is_none() && self.selected_track == track,
                                            RichText::new(track.role_name()).strong().color(track_color(track)),
                                        )
                                        .clicked()
                                    {
                                        self.selected_track = track;
                                        self.selected_extra = None;
                                    }
                                    if track == TrackId::MoogPad {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 2.0;
                                            let m_lbl = RichText::new("M").size(10.0).color(if proj.mixer.moog_pad_mute { Color32::from_rgb(245, 76, 86) } else { Color32::GRAY });
                                            if ui.selectable_label(proj.mixer.moog_pad_mute, m_lbl).clicked() {
                                                proj.mixer.moog_pad_mute = !proj.mixer.moog_pad_mute;
                                            }
                                            let s_lbl = RichText::new("S").size(10.0).color(if proj.mixer.moog_pad_solo { Color32::from_rgb(255, 208, 66) } else { Color32::GRAY });
                                            if ui.selectable_label(proj.mixer.moog_pad_solo, s_lbl).clicked() {
                                                proj.mixer.moog_pad_solo = !proj.mixer.moog_pad_solo;
                                            }
                                        });
                                        let (lrect, _) = ui.allocate_exact_size(Vec2::new(34.0, 8.0), Sense::hover());
                                        ui.painter().rect_filled(lrect, Rounding::same(2.0), Color32::from_rgb(20, 24, 34));
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(lrect.min, Vec2::new(lrect.width() * tpeak, lrect.height())),
                                            Rounding::same(2.0),
                                            track_color(track),
                                        );
                                        ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_volume, 0.0..=1.4).show_value(false));
                                        ui.label(RichText::new("Wide").size(10.5).weak());
                                        ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_width, 0.0..=1.8).show_value(false));
                                        ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_delay_send, 0.0..=1.0).show_value(false));
                                        ui.add(egui::Slider::new(&mut proj.mixer.moog_pad_reverb_send, 0.0..=1.0).show_value(false));
                                    } else {
                                        let ch = &mut proj.mixer.channels[track.index()];
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 2.0;
                                            let m_lbl = RichText::new("M").size(10.0).color(if ch.mute { Color32::from_rgb(245, 76, 86) } else { Color32::GRAY });
                                            if ui.selectable_label(ch.mute, m_lbl).clicked() {
                                                ch.mute = !ch.mute;
                                            }
                                            let s_lbl = RichText::new("S").size(10.0).color(if ch.solo { Color32::from_rgb(255, 208, 66) } else { Color32::GRAY });
                                            if ui.selectable_label(ch.solo, s_lbl).clicked() {
                                                ch.solo = !ch.solo;
                                            }
                                        });
                                        let (lrect, _) = ui.allocate_exact_size(Vec2::new(34.0, 8.0), Sense::hover());
                                        ui.painter().rect_filled(lrect, Rounding::same(2.0), Color32::from_rgb(20, 24, 34));
                                        ui.painter().rect_filled(
                                            egui::Rect::from_min_size(lrect.min, Vec2::new(lrect.width() * tpeak, lrect.height())),
                                            Rounding::same(2.0),
                                            track_color(track),
                                        );
                                        ui.add(egui::Slider::new(&mut ch.volume, 0.0..=1.4).show_value(false));
                                        ui.add(egui::Slider::new(&mut ch.pan, -1.0..=1.0).show_value(false));
                                        ui.add(egui::Slider::new(&mut ch.width, 0.0..=1.8).show_value(false));
                                        ui.add(egui::Slider::new(&mut ch.delay_send, 0.0..=1.0).show_value(false));
                                        ui.add(egui::Slider::new(&mut ch.reverb_send, 0.0..=1.0).show_value(false));
                                    }
                                    ui.end_row();
                                }

                                for (ei, tr) in proj.extra_tracks.iter_mut().take(MAX_EXTRA_TRACKS).enumerate() {
                                    let ec = Color32::from_rgb(tr.color[0], tr.color[1], tr.color[2]);
                                    if ui
                                        .selectable_label(
                                            self.selected_extra == Some(ei),
                                            RichText::new(tr.name.chars().take(10).collect::<String>()).strong().color(ec),
                                        )
                                        .clicked()
                                    {
                                        self.selected_extra = Some(ei);
                                    }
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 2.0;
                                        let m_lbl = RichText::new("M").size(10.0).color(if tr.mute { Color32::from_rgb(245, 76, 86) } else { Color32::GRAY });
                                        if ui.selectable_label(tr.mute, m_lbl).clicked() {
                                            tr.mute = !tr.mute;
                                        }
                                        let s_lbl = RichText::new("S").size(10.0).color(if tr.solo { Color32::from_rgb(255, 208, 66) } else { Color32::GRAY });
                                        if ui.selectable_label(tr.solo, s_lbl).clicked() {
                                            tr.solo = !tr.solo;
                                        }
                                    });
                                    ui.label(RichText::new("Ext").size(10.0).weak());
                                    ui.add(egui::Slider::new(&mut tr.volume, 0.0..=1.4).show_value(false));
                                    ui.add(egui::Slider::new(&mut tr.pan, -1.0..=1.0).show_value(false));
                                    ui.add(egui::Slider::new(&mut tr.width, 0.0..=1.8).show_value(false));
                                    ui.add(egui::Slider::new(&mut tr.delay_send, 0.0..=1.0).show_value(false));
                                    ui.add(egui::Slider::new(&mut tr.reverb_send, 0.0..=1.0).show_value(false));
                                    ui.end_row();
                                }
                            });
                        });
                });
            });
        });
    }
}

fn engine_selector_controls(
    ui: &mut egui::Ui,
    engine: &mut SynthEngine,
    bank: &mut u8,
    morph: &mut f32,
    phaser_mix: &mut f32,
    amp_cab: &mut f32,
    id: &str,
    tc: Color32,
) {
    ui.label(RichText::new("SYNTH ENGINE & CHARACTER").strong().size(11.5).color(tc));
    ui.horizontal_wrapped(|ui| {
        for &e in &SynthEngine::ALL {
            if ui.selectable_label(*engine == e, e.name()).clicked() {
                *engine = e;
                if e == SynthEngine::PhilharmoniaD50 {
                    preload_philharmonia_bank_async(*bank);
                }
            }
        }
    });
    if *engine != SynthEngine::Analog && *engine != SynthEngine::SoundFont {
        let banks = engine.bank_names();
        let cur = (*bank as usize) % banks.len().max(1);
        ui.horizontal_wrapped(|ui| {
            ui.label("Bank:");
            egui::ComboBox::from_id_salt((id, "eng_bank"))
                .width(175.0)
                .selected_text(banks[cur])
                .show_ui(ui, |ui| {
                    for (bi, &bname) in banks.iter().enumerate() {
                        if ui.selectable_value(bank, bi as u8, bname).clicked()
                            && *engine == SynthEngine::PhilharmoniaD50
                        {
                            preload_philharmonia_bank_async(bi as u8);
                        }
                    }
                });
        });
        let morph_lbl = if *engine == SynthEngine::PhilharmoniaD50 {
            "D-50 Synth Blend"
        } else {
            "Morph / Tone"
        };
        ui.add(egui::Slider::new(morph, 0.0..=1.0).text(morph_lbl));
    }
    ui.add(egui::Slider::new(phaser_mix, 0.0..=1.0).text("Stereo Phaser"));
    ui.add(egui::Slider::new(amp_cab, 0.0..=1.0).text("Tube Amp / Cab"));
    ui.separator();
}

fn moog_controls(ui: &mut egui::Ui, m: &mut crate::model::MoogParams, id: &str, tc: Color32) {
    engine_selector_controls(
        ui,
        &mut m.engine,
        &mut m.engine_bank,
        &mut m.engine_morph,
        &mut m.phaser_mix,
        &mut m.amp_cab,
        id,
        tc,
    );
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut m.poly_mode, "Polyphonic");
        ui.add(egui::Slider::new(&mut m.glide, 0.0..=0.6).text("Glide"));
    });

    if m.engine == SynthEngine::Analog {
        ui.separator();
        ui.label(RichText::new("OSCILLATORS & CP3 MIXER").strong().size(11.5).color(tc));
        ui.horizontal_wrapped(|ui| {
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
        ui.horizontal_wrapped(|ui| {
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
        ui.horizontal_wrapped(|ui| {
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
    }
    ui.add(egui::Slider::new(&mut m.drive, 0.0..=1.0).text("Drive"));

    ui.separator();
    ui.label(RichText::new("FILTER & FILTER ENV").strong().size(11.5).color(tc));
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
    engine_selector_controls(
        ui,
        &mut m.engine,
        &mut m.engine_bank,
        &mut m.engine_morph,
        &mut m.phaser_mix,
        &mut m.amp_cab,
        "monologue",
        tc,
    );
    if m.engine == SynthEngine::Analog {
        ui.label(RichText::new("VCO 1 & VCO 2 (SYNC / RING)").strong().size(11.5).color(tc));
        ui.horizontal_wrapped(|ui| {
            ui.label("VCO 1:");
            for &w in &MonologueOscWave::ALL {
                ui.selectable_value(&mut m.vco1_wave, w, w.name());
            }
        });
        ui.add(egui::Slider::new(&mut m.vco1_shape, 0.0..=1.0).text("VCO 1 Shape"));
        ui.add(egui::Slider::new(&mut m.vco1_level, 0.0..=1.0).text("VCO 1 Level"));

        ui.horizontal_wrapped(|ui| {
            ui.label("VCO 2:");
            for &w in &MonologueOscWave::ALL {
                ui.selectable_value(&mut m.vco2_wave, w, w.name());
            }
            ui.add(egui::DragValue::new(&mut m.vco2_octave).range(-2..=2).prefix("Oct "));
        });
        ui.add(egui::Slider::new(&mut m.vco2_pitch, -12.0..=12.0).suffix(" st").text("VCO 2 Pitch"));
        ui.add(egui::Slider::new(&mut m.vco2_shape, 0.0..=1.0).text("VCO 2 Shape"));
        ui.add(egui::Slider::new(&mut m.vco2_level, 0.0..=1.0).text("VCO 2 Level"));
        ui.horizontal_wrapped(|ui| {
            ui.label("VCO 2 Mod:");
            for &mm in &Osc2ModMode::ALL {
                ui.selectable_value(&mut m.mod_mode, mm, mm.name());
            }
        });
    }
    ui.add(egui::Slider::new(&mut m.glide, 0.0..=0.5).text("Glide"));

    ui.separator();
    ui.label(RichText::new("FILTER, DRIVE & ENVELOPE").strong().size(11.5).color(tc));
    ui.add(egui::Slider::new(&mut m.cutoff, 30.0..=16_000.0).logarithmic(true).suffix(" Hz").text("Cutoff"));
    ui.add(egui::Slider::new(&mut m.resonance, 0.0..=0.94).text("Resonance"));
    ui.add(egui::Slider::new(&mut m.drive, 0.0..=1.0).text("Drive"));
    ui.horizontal_wrapped(|ui| {
        ui.label("EG Target:");
        for &t in &EgTarget::ALL {
            ui.selectable_value(&mut m.eg_target, t, t.name());
        }
    });
    ui.add(egui::Slider::new(&mut m.eg_attack, 0.002..=1.5).suffix(" s").text("EG Attack"));
    ui.add(egui::Slider::new(&mut m.eg_decay, 0.02..=2.5).suffix(" s").text("EG Decay"));
    ui.add(egui::Slider::new(&mut m.eg_int, -1.0..=1.0).text("EG Intensity"));
    ui.checkbox(&mut m.amp_sustain, "Amp Gate Sustain (off = punchy AD)");

    if m.engine == SynthEngine::Analog {
        ui.separator();
        ui.label(RichText::new("LFO MODULATION").strong().size(11.5).color(tc));
        ui.horizontal_wrapped(|ui| {
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
        ui.horizontal_wrapped(|ui| {
            ui.label("Target:");
            for &lt in &LfoTarget::ALL {
                ui.selectable_value(&mut m.lfo_target, lt, lt.name());
            }
        });
        ui.add(egui::Slider::new(&mut m.lfo_rate, 0.0..=1.0).text("LFO Rate"));
        ui.add(egui::Slider::new(&mut m.lfo_int, -1.0..=1.0).text("LFO Intensity"));
    }
}

fn format_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.1}k", hz / 1000.0)
    } else {
        format!("{:.0}Hz", hz)
    }
}

fn track_filter_mut(
    proj: &mut Project,
    track: TrackId,
    extra: Option<usize>,
) -> &mut TrackFilterParams {
    if let Some(ei) = extra.filter(|&i| i < proj.extra_tracks.len()) {
        return &mut proj.extra_tracks[ei].filter;
    }
    if track == TrackId::MoogPad {
        &mut proj.mixer.moog_pad_filter
    } else {
        &mut proj.mixer.channels[track.index()].filter
    }
}

fn ui_instrument_filter_bar(
    ui: &mut egui::Ui,
    f: &mut TrackFilterParams,
    tc: Color32,
    theme: StudioTheme,
) {
    egui::Frame::none()
        .fill(theme.control_bg())
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .stroke(Stroke::new(1.0_f32, theme.border()))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                toggle_pill(
                    ui,
                    if f.enabled { "FILTER" } else { "BYP" },
                    &mut f.enabled,
                    tc,
                    "Enable or bypass this instrument's channel filter",
                );
                ui.selectable_value(&mut f.mode, GlobalFilterMode::LowPass, "LP");
                ui.selectable_value(&mut f.mode, GlobalFilterMode::BandPass, "BP");
                ui.selectable_value(&mut f.mode, GlobalFilterMode::HighPass, "HP");
                ui.label(RichText::new("Cut").size(11.0).weak());
                ui.scope(|ui| {
                    ui.spacing_mut().slider_width = 96.0;
                    ui.add(
                        egui::Slider::new(&mut f.cutoff, 40.0..=18_000.0)
                            .logarithmic(true)
                            .show_value(false)
                            .trailing_fill(true),
                    );
                });
                ui.label(RichText::new(format_hz(f.cutoff)).size(11.0).color(tc));
                ui.label(RichText::new("Res").size(11.0).weak());
                ui.scope(|ui| {
                    ui.spacing_mut().slider_width = 62.0;
                    ui.add(
                        egui::Slider::new(&mut f.resonance, 0.0..=0.88)
                            .show_value(false)
                            .trailing_fill(true),
                    );
                });
                ui.label(RichText::new(format!("{:.0}%", f.resonance * 100.0)).size(11.0).weak());
                if (f.enabled || f.cutoff < 17_900.0 || f.resonance > 0.01 || f.mode != GlobalFilterMode::LowPass)
                    && ui
                        .small_button("Clear (Thru)")
                        .on_hover_text("Clear instrument channel filter to clean bypass")
                        .clicked()
                {
                    *f = TrackFilterParams::default();
                }
            });
        });
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

fn ui_track_mix_strip(
    ui: &mut egui::Ui,
    armed: bool,
    vol: &mut f32,
    mute: &mut bool,
    solo: &mut bool,
    rnd_hover: &str,
    rm_hover: &str,
) -> (bool, bool, bool, bool) {
    let r_btn = egui::Button::new(
        RichText::new("R").strong().size(11.0).color(if armed {
            Color32::WHITE
        } else {
            Color32::from_rgb(200, 140, 148)
        }),
    )
    .fill(if armed {
        Color32::from_rgb(228, 54, 70)
    } else {
        Color32::from_rgb(44, 30, 36)
    });
    let rec = ui
        .add(r_btn)
        .on_hover_text("Arm track for Record Mode (stopped = Step Rec, playing = Live Rec)")
        .clicked();

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

    let slider_w = (ui.available_width() - 116.0).clamp(34.0, 150.0);
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = slider_w;
        ui.add(
            egui::Slider::new(vol, 0.0..=1.15)
                .show_value(false)
                .trailing_fill(true),
        );
    });

    let rnd = ui.small_button("RND").on_hover_text(rnd_hover).clicked();
    let clr = ui.small_button("CLR").on_hover_text("Clear track pattern").clicked();
    let sub_btn = egui::Button::new(
        RichText::new("−")
            .strong()
            .size(12.0)
            .color(Color32::from_rgb(235, 130, 140)),
    )
    .fill(Color32::from_rgb(44, 26, 32));
    let rm = ui.add(sub_btn).on_hover_text(rm_hover).clicked();
    (rec, rnd, clr, rm)
}

fn macro_cell(
    ui: &mut egui::Ui,
    cell_w: f32,
    label: &str,
    value_text: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    accent: Color32,
    theme: StudioTheme,
) -> bool {
    let mut changed = false;
    egui::Frame::none()
        .fill(theme.bar_bg())
        .rounding(Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(8.0, 5.0))
        .stroke(Stroke::new(1.0_f32, theme.border()))
        .show(ui, |ui| {
            let inner_w = (cell_w - 16.0).max(64.0);
            ui.vertical(|ui| {
                ui.set_min_width(inner_w);
                ui.set_max_width(inner_w);
                egui::Sides::new().height(16.0).show(
                    ui,
                    |ui| {
                        ui.label(RichText::new(label).strong().size(11.0));
                    },
                    |ui| {
                        ui.label(RichText::new(value_text).size(11.0).color(accent));
                    },
                );
                ui.add_space(2.0);
                ui.spacing_mut().slider_width = (inner_w - 4.0).max(48.0);
                changed = ui
                    .add(
                        egui::Slider::new(value, range)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                    .changed();
            });
        });
    changed
}

fn card_frame(ui: &mut egui::Ui, theme: StudioTheme, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(theme.card_bg())
        .rounding(Rounding::same(10.0))
        .inner_margin(egui::Margin::same(12.0))
        .stroke(Stroke::new(1.0_f32, theme.border()))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui);
        });
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

#[cfg(test)]
mod ui_layout_tests {
    use super::*;
    use crate::engine::EngineTelemetry;
    use std::collections::BTreeMap;

    fn collect_text_boxes(
        shape: &egui::epaint::Shape,
        clip: egui::Rect,
        out: &mut Vec<(String, egui::Rect, egui::Rect)>,
    ) {
        match shape {
            egui::epaint::Shape::Vec(shapes) => {
                for s in shapes {
                    collect_text_boxes(s, clip, out);
                }
            }
            egui::epaint::Shape::Text(ts) => {
                let txt = ts.galley.text().trim().to_string();
                if txt.is_empty() {
                    return;
                }
                let r = ts.visual_bounding_rect();
                if r.width() > 0.0 && r.height() > 0.0 {
                    out.push((txt, r, clip));
                }
            }
            _ => {}
        }
    }

    fn assert_no_overlap_or_overflow(
        ctx: &egui::Context,
        app: &mut FunKeySynthApp,
        scenario: &str,
        w: f32,
        h: f32,
    ) -> Vec<(String, egui::Rect)> {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(w, h));
        let mut final_output = None;
        for frame_i in 0..2 {
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(frame_i as f64 * 0.016),
                    ..Default::default()
                },
                |ctx| app.render_ui(ctx),
            );
            final_output = Some(out);
        }
        let out = final_output.expect("frame output missing");

        type ClipKey = (u32, u32, u32, u32);
        let mut by_clip: BTreeMap<ClipKey, Vec<(String, egui::Rect, egui::Rect)>> = BTreeMap::new();
        let mut all_boxes: Vec<(String, egui::Rect)> = Vec::new();
        for clipped in &out.shapes {
            let c = clipped.clip_rect;
            let key = (
                c.min.x.to_bits(),
                c.min.y.to_bits(),
                c.max.x.to_bits(),
                c.max.y.to_bits(),
            );
            let entry = by_clip.entry(key).or_default();
            collect_text_boxes(&clipped.shape, c, entry);
        }

        for items in by_clip.values() {
            for (txt, r, clip) in items {
                all_boxes.push((txt.clone(), *r));
                assert!(
                    r.max.x <= screen.max.x + 2.0,
                    "[{scenario} @ {w}x{h}] Horizontal overflow off right edge for '{txt}': rect={r:?}, clip={clip:?}, screen={screen:?}"
                );
                assert!(
                    r.min.x >= -2.0,
                    "[{scenario} @ {w}x{h}] Horizontal overflow off left edge for '{txt}': rect={r:?}"
                );
            }

            for i in 0..items.len() {
                let (txt_a, ra, clip_a) = &items[i];
                let vis_a = ra.intersect(*clip_a);
                if vis_a.width() <= 0.0 || vis_a.height() <= 0.0 {
                    continue;
                }
                for (txt_b, rb, clip_b) in items.iter().skip(i + 1) {
                    let vis_b = rb.intersect(*clip_b);
                    if vis_b.width() <= 0.0 || vis_b.height() <= 0.0 {
                        continue;
                    }
                    let inter = vis_a.intersect(vis_b);
                    assert!(
                        !(inter.width() > 3.0 && inter.height() > 3.0),
                        "[{scenario} @ {w}x{h}] Overlapping text widgets: '{txt_a}' ({ra:?}) vs '{txt_b}' ({rb:?}), intersection={inter:?}"
                    );
                }
            }
        }
        all_boxes
    }

    #[test]
    fn ui_layouts_have_no_overlaps_or_horizontal_overflow() {
        let sizes = [
            (1440.0_f32, 820.0_f32),
            (1280.0_f32, 720.0_f32),
            (1180.0_f32, 620.0_f32),
            (920.0_f32, 700.0_f32),
        ];

        for &(w, h) in &sizes {
            let ctx = egui::Context::default();
            let mut app = FunKeySynthApp::new_for_test(&ctx);

            // 1. Studio / Drums / party off
            app.tab = StudioTab::Studio;
            app.selected_track = TrackId::Drums;
            app.selected_extra = None;
            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = false;
            }
            app.engine.set_telemetry_for_test(EngineTelemetry::default());
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Drums-Ready", w, h);

            // 2. Studio / Drums / Party DJ active with realistic long song title
            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = true;
            }
            app.engine.set_telemetry_for_test(EngineTelemetry {
                active_chord: "Em9".into(),
                party_section: "DROP 2".into(),
                party_bar: 2,
                party_section_bars: 4,
                party_song_title: "Neon Velvet Bounce".into(),
                party_palette: "Eurobeat Octave • Festival Supersaw • Eurobeat Brass • 909 Club".into(),
                party_voice_presets: [1, 12, 8, 10, 5, 1],
                party_bpm: 124.0,
                party_vibe_name: "Electro Funk".into(),
                ..Default::default()
            });
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Drums-PartyDJ", w, h);

            // 3. Studio / Melodic (Bass)
            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = false;
            }
            app.selected_track = TrackId::Moog;
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Moog", w, h);

            // 4. Studio / Sampler lane grid
            app.selected_track = TrackId::Sampler;
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Sampler", w, h);

            // 5. Sampler tab (Volca 8-part workbench)
            app.tab = StudioTab::Sampler;
            assert_no_overlap_or_overflow(&ctx, &mut app, "Tab-Sampler", w, h);

            // 6. Arrange & Party tab
            app.tab = StudioTab::ArrangeParty;
            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = true;
            }
            assert_no_overlap_or_overflow(&ctx, &mut app, "Tab-ArrangeParty", w, h);

            // 7. Studio / Drop Setup Open + Active Build + Active Filter
            app.tab = StudioTab::Studio;
            app.selected_track = TrackId::Drums;
            app.selected_extra = None;
            app.show_drop_setup = true;
            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = false;
                g.mixer.channels[TrackId::Drums.index()].filter.cutoff = 1200.0;
                g.fx.global_filter_env_amount = 0.75;
            }
            app.engine.set_telemetry_for_test(EngineTelemetry {
                drop_phase: "BUILD".into(),
                drop_progress: 0.65,
                global_filter_env_level: 0.72,
                ..Default::default()
            });
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-DropSetup-Open", w, h);

            // 8. Studio / Extra Dynamic Track selected (checks ExtraTrack per-instrument filter)
            app.show_drop_setup = false;
            {
                let mut g = app.engine.project.lock().unwrap();
                g.extra_tracks.push(ExtraTrack::default());
                let seq = crate::model::MelodicTrackSeq::with_default_note(
                    60,
                    crate::model::StepCount::Steps64,
                );
                for pat in &mut g.patterns {
                    pat.extra_seqs.push(seq.clone());
                }
            }
            app.selected_extra = Some(0);
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-ExtraTrack-Filter", w, h);
            app.selected_extra = None;

            // 9. Studio / Chromatic Monologue + Step Rec + Live Rec + Drums + Scale + Chords + Sampler Armed
            app.tab = StudioTab::Studio;
            app.selected_track = TrackId::Monologue;
            app.selected_extra = None;
            app.keyboard_mode = KeyboardMode::Chromatic;
            app.armed_record = None;
            app.engine.playing.store(false, Ordering::Relaxed);
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Chromatic-Monologue", w, h);

            app.armed_record = Some(RecordTarget::Core(TrackId::Monologue));
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-StepRec-Armed", w, h);

            app.engine.playing.store(true, Ordering::Relaxed);
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-LiveRec-Armed", w, h);
            app.engine.playing.store(false, Ordering::Relaxed);

            app.selected_track = TrackId::Drums;
            app.armed_record = Some(RecordTarget::Core(TrackId::Drums));
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Drums-Pads-Armed", w, h);

            app.armed_record = None;
            app.selected_track = TrackId::Monologue;
            app.keyboard_mode = KeyboardMode::Scale;
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Scale-Dock", w, h);

            app.selected_track = TrackId::MoogPad;
            app.keyboard_mode = KeyboardMode::Chords;
            assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Chords-Dock", w, h);

            app.keyboard_mode = KeyboardMode::Chromatic;
            app.tab = StudioTab::Sampler;
            app.armed_record = Some(RecordTarget::Core(TrackId::Sampler));
            assert_no_overlap_or_overflow(&ctx, &mut app, "Tab-Sampler-Armed", w, h);
            app.armed_record = None;

            // 10. Sound Lab tab across all track types (including open Preset Browser)
            app.tab = StudioTab::SoundLab;
            app.show_preset_browser = true;
            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = false;
            }
            for &tr in &[
                TrackId::Moog,
                TrackId::Monologue,
                TrackId::Tb303,
                TrackId::Drums,
                TrackId::Sampler,
            ] {
                app.selected_track = tr;
                assert_no_overlap_or_overflow(
                    &ctx,
                    &mut app,
                    &format!("Tab-SoundLab-{:?}", tr),
                    w,
                    h,
                );
            }
            app.show_preset_browser = false;
        }

        // Tall viewport regression check: verify below-the-fold Vibe Macros ("Warmth", "Glitch", "Master")
        // and ArrangeParty DJ instrument picker stay strictly inside screen bounds at both 1440px and 920px widths.
        for &(w, h) in &[(1440.0_f32, 1600.0_f32), (920.0_f32, 1600.0_f32)] {
            let ctx = egui::Context::default();
            let mut app = FunKeySynthApp::new_for_test(&ctx);
            app.tab = StudioTab::Studio;
            app.selected_track = TrackId::Drums;
            app.selected_extra = None;
            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = false;
            }
            app.engine.set_telemetry_for_test(EngineTelemetry::default());
            let boxes_ready =
                assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Drums-Ready-Tall", w, h);
            for macro_lbl in ["Warmth", "Glitch", "Master"] {
                let found = boxes_ready.iter().find(|(t, _)| t == macro_lbl);
                assert!(
                    found.is_some(),
                    "Expected '{macro_lbl}' macro label to be rendered at {w}x{h}"
                );
                let (_, r) = found.unwrap();
                assert!(
                    r.max.x <= w + 2.0,
                    "Expected '{macro_lbl}' inside screen width {w}, got {r:?}"
                );
            }

            {
                let mut g = app.engine.project.lock().unwrap();
                g.party_mode = true;
            }
            app.engine.set_telemetry_for_test(EngineTelemetry {
                active_chord: "Dm".into(),
                party_section: "DROP 1".into(),
                party_bar: 1,
                party_section_bars: 4,
                party_song_title: "Initial Drift".into(),
                party_palette: "Eurobeat Octave • Festival Supersaw • Eurobeat Brass • 909 Club".into(),
                party_voice_presets: [1, 12, 8, 10, 5, 1],
                party_bpm: 158.0,
                party_vibe_name: "Eurobeat".into(),
                ..Default::default()
            });
            let boxes_dj =
                assert_no_overlap_or_overflow(&ctx, &mut app, "Studio-Drums-PartyDJ-Tall", w, h);
            for macro_lbl in ["Warmth", "Glitch", "Master"] {
                assert!(
                    boxes_dj.iter().any(|(t, r)| t == macro_lbl && r.max.x <= w + 2.0),
                    "Expected '{macro_lbl}' macro inside {w}x{h} in Party DJ mode"
                );
            }

            app.tab = StudioTab::ArrangeParty;
            let boxes_arr =
                assert_no_overlap_or_overflow(&ctx, &mut app, "Tab-ArrangeParty-Tall", w, h);
            assert!(
                boxes_arr.iter().any(|(t, _)| t == "DJ INSTRUMENTS"),
                "Expected DJ INSTRUMENTS panel in Arrange & Party at {w}x{h}"
            );
        }
    }

    #[test]
    fn step_recording_monologue_notes_rest_tie_back_saves_to_pattern() {
        let ctx = egui::Context::default();
        let mut app = FunKeySynthApp::new_for_test(&ctx);
        let mut proj = Project::default();
        let mono_target = RecordTarget::Core(TrackId::Monologue);

        app.set_target_step_count(&mut proj, mono_target, StepCount::Steps16);
        app.rec_replace = true;
        app.toggle_arm_record(&ctx, &mut proj, mono_target);
        assert_eq!(app.armed_record, Some(mono_target));
        assert_eq!(app.step_rec_cursor, 0);

        // Step 0: C3 (48), Step 1: E3 (52)
        let h0 = app.play_and_record_note_down(&ctx, &mut proj, mono_target, 48, 0.88, false, 0);
        app.release_held_note(&ctx, &mut proj, h0, false);
        let h1 = app.play_and_record_note_down(&ctx, &mut proj, mono_target, 52, 0.88, false, 0);
        app.release_held_note(&ctx, &mut proj, h1, false);
        assert_eq!(app.step_rec_cursor, 2);

        // Step 2: Rest -> advances to step 3
        app.step_rec_rest(&mut proj);
        assert_eq!(app.step_rec_cursor, 3);

        // Step 3: G3 (55) + Tie over step 4 -> advances to step 5
        let h3 = app.play_and_record_note_down(&ctx, &mut proj, mono_target, 55, 0.90, false, 0);
        app.release_held_note(&ctx, &mut proj, h3, false);
        app.step_rec_tie(&mut proj);
        assert_eq!(app.step_rec_cursor, 5);

        // Step back to 4 and record A3 (57) -> advances to 5
        app.step_rec_back(&proj);
        assert_eq!(app.step_rec_cursor, 4);
        let h4 = app.play_and_record_note_down(&ctx, &mut proj, mono_target, 57, 0.90, false, 0);
        app.release_held_note(&ctx, &mut proj, h4, false);
        assert_eq!(app.step_rec_cursor, 5);

        let seq = &proj.current_pattern().monologue;
        assert!(seq.steps[0].active && seq.steps[0].note == 48);
        assert!(seq.steps[1].active && seq.steps[1].note == 52);
        assert!(!seq.steps[2].active);
        assert!(seq.steps[3].active && seq.steps[3].note == 55);
        assert!((seq.steps[3].gate - 1.85).abs() < 1e-4);
        assert!(seq.steps[4].active && seq.steps[4].note == 57);
    }

    #[test]
    fn step_recording_moog_pad_chord_detects_triad_and_advances_on_release() {
        let ctx = egui::Context::default();
        let mut app = FunKeySynthApp::new_for_test(&ctx);
        let mut proj = Project::default();
        let pad_target = RecordTarget::Core(TrackId::MoogPad);

        app.rec_replace = true;
        app.toggle_arm_record(&ctx, &mut proj, pad_target);

        // Hold C4 (60), Eb4 (63), G4 (67) simultaneously
        let h0 = app.play_and_record_note_down(&ctx, &mut proj, pad_target, 60, 0.88, false, 0);
        app.held_keys.push((Key::A, h0));
        let h1 = app.play_and_record_note_down(&ctx, &mut proj, pad_target, 63, 0.88, false, 0);
        app.held_keys.push((Key::E, h1));
        let h2 = app.play_and_record_note_down(&ctx, &mut proj, pad_target, 67, 0.88, false, 0);
        app.held_keys.push((Key::G, h2));

        // Still held: cursor must remain on step 0
        assert_eq!(app.step_rec_cursor, 0);

        // Release first two keys: still held by third
        app.held_keys.remove(0);
        app.release_held_note(&ctx, &mut proj, h0, false);
        assert_eq!(app.step_rec_cursor, 0);
        app.held_keys.remove(0);
        app.release_held_note(&ctx, &mut proj, h1, false);
        assert_eq!(app.step_rec_cursor, 0);

        // Release final key: chord completes and cursor advances to step 1
        app.held_keys.remove(0);
        app.release_held_note(&ctx, &mut proj, h2, false);
        assert_eq!(app.step_rec_cursor, 1);

        let st0 = &proj.current_pattern().moog_pad.steps[0];
        assert!(st0.active);
        assert_eq!(st0.note, 60);
        assert_eq!(st0.chord, ChordType::Minor);
    }

    #[test]
    fn live_recording_drums_disables_autobeat_and_replaces_once_per_take() {
        let ctx = egui::Context::default();
        let mut app = FunKeySynthApp::new_for_test(&ctx);
        let mut proj = Project::default();
        proj.auto_beat_mode = true;
        proj.current_pattern_mut().drums.lanes[1].steps[15].active = true;

        let drum_target = RecordTarget::Core(TrackId::Drums);
        app.rec_replace = true;
        app.toggle_arm_record(&ctx, &mut proj, drum_target);
        assert!(!proj.auto_beat_mode);

        // Live record Kick at step 0, then Closed Hat at step 2
        let h_kick = app.trigger_drum_pad_down(&ctx, &mut proj, 0, 0.92, true, 0);
        app.release_held_note(&ctx, &mut proj, h_kick, true);
        let h_hat = app.trigger_drum_pad_down(&ctx, &mut proj, 4, 0.85, true, 2);
        app.release_held_note(&ctx, &mut proj, h_hat, true);

        let drums = &proj.current_pattern().drums;
        assert!(!drums.lanes[1].steps[15].active, "Replace should clear old beat once on first hit");
        assert!(drums.lanes[0].steps[0].active, "Kick should be saved on step 0");
        assert!(drums.lanes[4].steps[2].active, "Closed Hat should remain saved on step 2");
    }

    #[test]
    fn recording_extra_track_writes_into_extra_seqs() {
        let ctx = egui::Context::default();
        let mut app = FunKeySynthApp::new_for_test(&ctx);
        let mut proj = Project::default();
        proj.extra_tracks.push(ExtraTrack::default());
        let seq = crate::model::MelodicTrackSeq::with_default_note(60, StepCount::Steps16);
        for pat in &mut proj.patterns {
            pat.extra_seqs.push(seq.clone());
        }

        let ex_target = RecordTarget::Extra(0);
        app.rec_replace = true;
        app.toggle_arm_record(&ctx, &mut proj, ex_target);
        let h = app.play_and_record_note_down(&ctx, &mut proj, ex_target, 64, 0.88, false, 0);
        app.release_held_note(&ctx, &mut proj, h, false);

        assert_eq!(app.step_rec_cursor, 1);
        let st = &proj.current_pattern().extra_seqs[0].steps[0];
        assert!(st.active);
        assert_eq!(st.note, 64);
    }

    #[test]
    fn keyboard_auto_repeat_does_not_retrigger_held_chords_or_arp() {
        let ctx = egui::Context::default();
        let mut app = FunKeySynthApp::new_for_test(&ctx);
        let mut proj = Project::default();
        proj.arp.enabled = true;
        let pads = build_auto_chord_pads(proj.root_note, proj.scale);

        let key_ev = |key: Key, pressed: bool, repeat: bool| -> egui::Event {
            egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat,
                modifiers: egui::Modifiers::NONE,
            }
        };

        // 1) Held Chord key: ignore OS auto-repeat & X11 synthetic Release+Press within <35ms
        app.keyboard_mode = KeyboardMode::Chords;
        app.rec_replace = true;
        app.toggle_arm_record(&ctx, &mut proj, RecordTarget::Core(TrackId::MoogPad));

        let mut raw = egui::RawInput::default();
        raw.time = Some(1.000);
        raw.events = vec![key_ev(Key::A, true, false)];
        let _ = ctx.run(raw, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert_eq!(app.held_chord_key, Some(Key::A));
        assert_eq!(app.step_rec_cursor, 1);

        // Repeat=true frames must not re-trigger
        for t in [1.040, 1.080] {
            let mut r = egui::RawInput::default();
            r.time = Some(t);
            r.events = vec![key_ev(Key::A, true, true)];
            let _ = ctx.run(r, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
            assert_eq!(app.held_chord_key, Some(Key::A));
            assert_eq!(app.step_rec_cursor, 1);
        }

        // X11 synthetic Release followed 15ms later by non-repeat Press must cancel release & NOT re-trigger
        let mut r_rel = egui::RawInput::default();
        r_rel.time = Some(1.100);
        r_rel.events = vec![key_ev(Key::A, false, false)];
        let _ = ctx.run(r_rel, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert_eq!(app.held_chord_key, Some(Key::A));

        let mut r_rep = egui::RawInput::default();
        r_rep.time = Some(1.115);
        r_rep.events = vec![key_ev(Key::A, true, false)];
        let _ = ctx.run(r_rep, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert_eq!(app.held_chord_key, Some(Key::A));
        assert_eq!(app.pending_chord_release, None);
        assert_eq!(app.step_rec_cursor, 1, "Synthetic X11 repeat must not re-trigger chord");

        // Real release > 35ms cleanly releases chord
        let mut r_real_rel = egui::RawInput::default();
        r_real_rel.time = Some(1.200);
        r_real_rel.events = vec![key_ev(Key::A, false, false)];
        let _ = ctx.run(r_real_rel, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert_eq!(app.held_chord_key, Some(Key::A));

        let mut r_done = egui::RawInput::default();
        r_done.time = Some(1.245);
        let _ = ctx.run(r_done, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert_eq!(app.held_chord_key, None);

        // 2) Held Scale / Arp note key: only 1 trigger across repeats + X11 release/press glitch
        app.keyboard_mode = KeyboardMode::Scale;
        app.toggle_arm_record(&ctx, &mut proj, RecordTarget::Core(TrackId::Monologue));
        let start_cur = app.step_rec_cursor;

        let mut r_down = egui::RawInput::default();
        r_down.time = Some(2.000);
        r_down.events = vec![key_ev(Key::S, true, false)];
        let _ = ctx.run(r_down, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert_eq!(app.held_keys.len(), 1);
        assert_eq!(app.step_rec_cursor, (start_cur + 1) % 16);

        for t in [2.040, 2.080, 2.120] {
            let mut r = egui::RawInput::default();
            r.time = Some(t);
            r.events = vec![key_ev(Key::S, true, true)];
            let _ = ctx.run(r, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
            assert_eq!(app.step_rec_cursor, (start_cur + 1) % 16);
        }

        let mut s_rel = egui::RawInput::default();
        s_rel.time = Some(2.140);
        s_rel.events = vec![key_ev(Key::S, false, false)];
        let _ = ctx.run(s_rel, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));

        let mut s_rep = egui::RawInput::default();
        s_rep.time = Some(2.155);
        s_rep.events = vec![key_ev(Key::S, true, false)];
        let _ = ctx.run(s_rep, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert_eq!(app.held_keys.len(), 1);
        assert!(app.pending_key_releases.is_empty());
        assert_eq!(app.step_rec_cursor, (start_cur + 1) % 16);

        let mut s_real_rel = egui::RawInput::default();
        s_real_rel.time = Some(2.200);
        s_real_rel.events = vec![key_ev(Key::S, false, false)];
        let _ = ctx.run(s_real_rel, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));

        let mut s_done = egui::RawInput::default();
        s_done.time = Some(2.245);
        let _ = ctx.run(s_done, |c| app.handle_keyboard(c, &mut proj, &pads, false, 0));
        assert!(app.held_keys.is_empty());
    }

    #[test]
    fn undo_and_redo_restore_project_and_bump_sample_generation() {
        let ctx = egui::Context::default();
        let mut app = FunKeySynthApp::new_for_test(&ctx);

        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1280.0, 720.0));
        let orig_proj = Project::default();
        let orig_bpm = orig_proj.bpm;
        let orig_gen = orig_proj.sample_generation;
        app.undo_stack.push_back(orig_proj);

        let mut edited = app.engine.project.lock().unwrap().clone();
        edited.bpm = 148.0;
        edited.sample_generation = orig_gen + 5;
        assert!(app.perform_undo(&ctx, &mut edited));
        assert!((edited.bpm - orig_bpm).abs() < 0.01);
        assert!(edited.sample_generation > orig_gen + 5);
        assert_eq!(app.redo_stack.len(), 1);

        assert!(app.perform_redo(&ctx, &mut edited));
        assert!((edited.bpm - 148.0).abs() < 0.01);
        assert_eq!(app.undo_stack.len(), 1);

        *app.engine.project.lock().unwrap() = edited;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                time: Some(0.1),
                ..Default::default()
            },
            |c| app.render_ui(c),
        );
    }

    #[test]
    fn midi_cc_learn_and_expression_update_project_targets() {
        let ctx = egui::Context::default();
        let mut app = FunKeySynthApp::new_for_test(&ctx);
        let mut proj = Project::default();

        // Simulate MIDI Learn binding CC 42 -> TrackCutoff
        app.selected_track = TrackId::Moog;
        app.midi_learn_target = Some(MidiCcTarget::TrackCutoff);
        app.midi.push_test_msg(MidiMsg::Cc { cc: 42, value: 0.75 });
        let pads = build_auto_chord_pads(proj.root_note, proj.scale);
        app.handle_midi(&ctx, &mut proj, &pads, false, 0);

        assert!(app.midi_learn_target.is_none());
        assert!(proj
            .midi
            .cc_mappings
            .iter()
            .any(|m| m.cc == 42 && m.target == MidiCcTarget::TrackCutoff));
        assert!(proj.moog.cutoff > 1500.0);

        // Pitch Bend & Aftertouch update app expression values
        app.midi.push_test_msg(MidiMsg::PitchBend { bend: 0.5 });
        app.midi.push_test_msg(MidiMsg::Aftertouch { pressure: 0.8 });
        app.handle_midi(&ctx, &mut proj, &pads, false, 0);
        assert!((app.midi_bend_val - 0.5).abs() < 1e-4);
        assert!((app.midi_aftertouch_val - 0.8).abs() < 1e-4);
    }
}




