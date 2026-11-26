use crate::model::{voice_led_chord, ChordSlot, ChordType, ClockDiv, Project};
use midir::{Ignore, MidiInput, MidiInputConnection};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy)]
pub enum MidiMsg {
    NoteOn { note: u8, velocity: f32 },
    NoteOff { note: u8 },
    Cc { cc: u8, value: f32 },
    PitchBend { bend: f32 },
    Aftertouch { pressure: f32 },
}

pub struct MidiManager {
    conn: Option<MidiInputConnection<()>>,
    queue: Arc<Mutex<VecDeque<MidiMsg>>>,
    pub ports: Vec<String>,
    pub connected_name: Option<String>,
    pub status: String,
}

impl Default for MidiManager {
    fn default() -> Self {
        let mut mgr = Self {
            conn: None,
            queue: Arc::new(Mutex::new(VecDeque::new())),
            ports: Vec::new(),
            connected_name: None,
            status: "No MIDI".into(),
        };
        mgr.refresh_ports();
        // Auto-connect first available hardware MIDI port if present
        if !mgr.ports.is_empty() {
            let _ = mgr.connect(0);
        }
        mgr
    }
}

impl MidiManager {
    pub fn refresh_ports(&mut self) {
        self.ports.clear();
        if let Ok(mut midi_in) = MidiInput::new("fun-key-synth-scan") {
            midi_in.ignore(Ignore::None);
            for p in midi_in.ports() {
                if let Ok(name) = midi_in.port_name(&p) {
                    self.ports.push(name);
                }
            }
        }
    }

    pub fn disconnect(&mut self) {
        self.conn = None;
        self.connected_name = None;
        self.status = "MIDI Off".into();
    }

    pub fn connect(&mut self, port_idx: usize) -> Result<(), String> {
        self.disconnect();
        let mut midi_in =
            MidiInput::new("fun-key-synth-in").map_err(|e| format!("MIDI init error: {e}"))?;
        midi_in.ignore(Ignore::None);
        let ports = midi_in.ports();
        let port = ports
            .get(port_idx)
            .ok_or_else(|| "MIDI port index out of range".to_string())?;
        let name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| format!("MIDI Port {port_idx}"));
        let q = self.queue.clone();
        let conn = midi_in
            .connect(
                port,
                "fun-key-synth-midi",
                move |_stamp, msg, _| {
                    if msg.len() < 2 {
                        return;
                    }
                    let status = msg[0] & 0xF0;
                    let ev = match status {
                        0x90 if msg.len() >= 3 && msg[2] > 0 => Some(MidiMsg::NoteOn {
                            note: msg[1],
                            velocity: (msg[2] as f32 / 127.0).clamp(0.05, 1.0),
                        }),
                        0x80 | 0x90 => Some(MidiMsg::NoteOff { note: msg[1] }),
                        0xB0 if msg.len() >= 3 => Some(MidiMsg::Cc {
                            cc: msg[1],
                            value: (msg[2] as f32 / 127.0).clamp(0.0, 1.0),
                        }),
                        0xD0 => Some(MidiMsg::Aftertouch {
                            pressure: (msg[1] as f32 / 127.0).clamp(0.0, 1.0),
                        }),
                        0xE0 if msg.len() >= 3 => {
                            let val = ((msg[2] as u16) << 7) | (msg[1] as u16 & 0x7F);
                            let norm = ((val as f32 - 8192.0) / 8192.0).clamp(-1.0, 1.0);
                            Some(MidiMsg::PitchBend { bend: norm })
                        }
                        _ => None,
                    };
                    if let Some(m) = ev {
                        if let Ok(mut lock) = q.lock() {
                            if lock.len() < 128 {
                                lock.push_back(m);
                            }
                        }
                    }
                },
                (),
            )
            .map_err(|e| format!("MIDI connect error: {e}"))?;
        self.conn = Some(conn);
        self.connected_name = Some(name.clone());
        self.status = format!("MIDI: {name}");
        Ok(())
    }

    pub fn drain(&self) -> Vec<MidiMsg> {
        let mut out = Vec::new();
        if let Ok(mut q) = self.queue.lock() {
            while let Some(m) = q.pop_front() {
                out.push(m);
            }
        }
        out
    }

    pub fn push_test_msg(&self, msg: MidiMsg) {
        if let Ok(mut q) = self.queue.lock() {
            q.push_back(msg);
        }
    }
}

fn push_vlq(out: &mut Vec<u8>, mut val: u32) {
    let mut buf = [0u8; 4];
    let mut count = 0usize;
    loop {
        buf[count] = (val & 0x7F) as u8;
        count += 1;
        val >>= 7;
        if val == 0 {
            break;
        }
    }
    for i in (0..count).rev() {
        let b = if i > 0 { buf[i] | 0x80 } else { buf[i] };
        out.push(b);
    }
}

fn encode_mtrk(events: &mut [(u32, u8, Vec<u8>)], name: &str) -> Vec<u8> {
    // Sort chronologically, placing NoteOff (priority 0) before NoteOn (priority 1) at matching ticks.
    events.sort_by_key(|e| (e.0, e.1));
    let mut data = Vec::with_capacity(events.len() * 6 + 32);
    // Track Name Meta Event (delta = 0)
    data.push(0);
    data.extend_from_slice(&[0xFF, 0x03]);
    let name_bytes = name.as_bytes();
    push_vlq(&mut data, name_bytes.len().min(64) as u32);
    data.extend_from_slice(&name_bytes[..name_bytes.len().min(64)]);

    let mut prev_tick = 0u32;
    for (tick, _, msg) in events.iter() {
        let delta = tick.saturating_sub(prev_tick);
        push_vlq(&mut data, delta);
        data.extend_from_slice(msg);
        prev_tick = *tick;
    }
    // End of Track
    data.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

    let mut track = Vec::with_capacity(data.len() + 8);
    track.extend_from_slice(b"MTrk");
    track.extend_from_slice(&(data.len() as u32).to_be_bytes());
    track.extend_from_slice(&data);
    track
}

fn div_ticks(clock_div: ClockDiv) -> u32 {
    match clock_div {
        ClockDiv::Eighth => 240,
        ClockDiv::Sixteenth => 120,
        ClockDiv::ThirtySecond => 60,
    }
}

/// Encodes the active Pattern (or full arranged Song when `project.song_mode` is enabled)
/// as a Standard Format-1 MIDI file (480 PPQ) with separate labeled tracks for Drums (Ch 10)
/// and each Melodic/Chord/Extra instrument.
pub fn export_project_midi_bytes(project: &Project) -> Result<Vec<u8>, String> {
    const PPQ: u16 = 480;
    const TICKS_PER_16TH: u32 = 120;
    let bpm = project.bpm.clamp(40.0, 240.0);
    let us_per_quarter = ((60_000_000.0 / bpm).round() as u32).clamp(200_000, 2_000_000);

    // Conductor track (Track 0)
    let mut conductor = vec![
        (
            0u32,
            0u8,
            vec![
                0xFF,
                0x51,
                0x03,
                ((us_per_quarter >> 16) & 0xFF) as u8,
                ((us_per_quarter >> 8) & 0xFF) as u8,
                (us_per_quarter & 0xFF) as u8,
            ],
        ),
        (0u32, 0u8, vec![0xFF, 0x58, 0x04, 0x04, 0x02, 24, 8]),
    ];

    let mut drum_evts: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    let mut moog_evts: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    let mut pad_evts: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    let mut mono_evts: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    let mut acid_evts: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    let mut samp_evts: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    let mut extra_evts: Vec<Vec<(u32, u8, Vec<u8>)>> =
        vec![Vec::new(); project.extra_tracks.len()];

    let sections: Vec<(usize, u16, u8, u16)> = if project.song_mode && !project.song_sections.is_empty()
    {
        project
            .song_sections
            .iter()
            .map(|s| (s.pattern, s.bars.max(1) as u16, s.track_mask, s.extra_mask))
            .collect()
    } else {
        let len_bars = (project.current_pattern().drums.step_count.as_usize() / 16).max(1) as u16;
        vec![(project.active_pattern, len_bars, 0b111111, 0x03FF)]
    };

    const GM_DRUM_NOTES: [u8; 8] = [36, 38, 39, 56, 42, 46, 45, 49];
    let mut cursor_tick = 0u32;

    for (pat_idx, bars, track_mask, extra_mask) in sections {
        let pat = project
            .patterns
            .get(pat_idx)
            .unwrap_or_else(|| project.current_pattern());
        let section_16ths = (bars as u32) * 16;
        let section_ticks = section_16ths * TICKS_PER_16TH;

        // 1. Drums (MIDI Ch 10 -> status 0x99 / 0x89)
        if (track_mask & 0b000001) != 0 && !project.mixer.channels[3].mute {
            for (lane_i, lane) in pat.drums.lanes.iter().enumerate() {
                if project.drums.voices[lane_i].mute {
                    continue;
                }
                let step_ticks = div_ticks(lane.clock_div);
                let len = lane.effective_len().max(1);
                let gm_note = GM_DRUM_NOTES[lane_i];
                let mut t = 0u32;
                let mut idx = 0usize;
                while t < section_ticks {
                    let st = &lane.steps[idx % len];
                    if st.active {
                        let vel = ((st.velocity * 127.0).round() as u8).clamp(12, 127);
                        let on_t = cursor_tick + t;
                        let off_t = on_t + (step_ticks / 2).max(20);
                        drum_evts.push((on_t, 1, vec![0x99, gm_note, vel]));
                        drum_evts.push((off_t, 0, vec![0x89, gm_note, 0]));
                    }
                    t += step_ticks;
                    idx += 1;
                }
            }
        }

        // Helper closure to emit a melodic sequence on `midi_ch` (0..15)
        let emit_melodic = |seq: &crate::model::MelodicTrackSeq,
                            midi_ch: u8,
                            out: &mut Vec<(u32, u8, Vec<u8>)>| {
            let step_ticks = div_ticks(seq.clock_div);
            let len = seq.step_count.as_usize().max(1);
            let mut t = 0u32;
            let mut idx = 0usize;
            while t < section_ticks {
                let st = &seq.steps[idx % len];
                if st.active {
                    let vel = ((st.velocity * 127.0).round() as u8).clamp(12, 127);
                    let gate_ticks = ((step_ticks as f32 * st.gate.clamp(0.15, 16.0)).round()
                        as u32)
                        .clamp(15, section_ticks.saturating_sub(t).max(15));
                    let on_t = cursor_tick + t;
                    let off_t = on_t + gate_ticks;
                    let on_st = 0x90 | (midi_ch & 0x0F);
                    let off_st = 0x80 | (midi_ch & 0x0F);
                    if st.chord != ChordType::Single {
                        let slot = ChordSlot {
                            offset: (st.note + 12 - (project.root_note % 12)) % 12,
                            chord: st.chord,
                        };
                        for n in voice_led_chord(project.root_note, slot, 60) {
                            let note = n.clamp(12, 120);
                            out.push((on_t, 1, vec![on_st, note, vel]));
                            out.push((off_t, 0, vec![off_st, note, 0]));
                        }
                    } else {
                        let note = st.note.clamp(12, 120);
                        out.push((on_t, 1, vec![on_st, note, vel]));
                        out.push((off_t, 0, vec![off_st, note, 0]));
                    }
                }
                t += step_ticks;
                idx += 1;
            }
        };

        if (track_mask & 0b000010) != 0 && !project.mixer.channels[0].mute {
            emit_melodic(&pat.moog, 0, &mut moog_evts);
        }
        if (track_mask & 0b000100) != 0 && !project.mixer.moog_pad_mute {
            emit_melodic(&pat.moog_pad, 1, &mut pad_evts);
        }
        if (track_mask & 0b001000) != 0 && !project.mixer.channels[1].mute {
            emit_melodic(&pat.monologue, 2, &mut mono_evts);
        }
        if (track_mask & 0b010000) != 0 && !project.mixer.channels[2].mute {
            emit_melodic(&pat.tb303, 3, &mut acid_evts);
        }
        if (track_mask & 0b100000) != 0 && !project.mixer.channels[4].mute {
            emit_melodic(&pat.sampler, 4, &mut samp_evts);
        }
        for (ei, tr) in project.extra_tracks.iter().enumerate() {
            if !tr.mute && (extra_mask & (1u16 << ei)) != 0 {
                if let Some(seq) = pat.extra_seqs.get(ei) {
                    let raw_ch = (5 + ei) as u8;
                    let ch = if raw_ch >= 9 { raw_ch + 1 } else { raw_ch }.min(15);
                    emit_melodic(seq, ch, &mut extra_evts[ei]);
                }
            }
        }

        cursor_tick += section_ticks;
    }

    let mut tracks = vec![encode_mtrk(&mut conductor, "Tempo & Time")];
    if !drum_evts.is_empty() {
        tracks.push(encode_mtrk(&mut drum_evts, "Drums"));
    }
    if !moog_evts.is_empty() {
        tracks.push(encode_mtrk(&mut moog_evts, "Bass (Moog)"));
    }
    if !pad_evts.is_empty() {
        tracks.push(encode_mtrk(&mut pad_evts, "Chords (Pad)"));
    }
    if !mono_evts.is_empty() {
        tracks.push(encode_mtrk(&mut mono_evts, "Lead (Monologue)"));
    }
    if !acid_evts.is_empty() {
        tracks.push(encode_mtrk(&mut acid_evts, "Acid (TB-303)"));
    }
    if !samp_evts.is_empty() {
        tracks.push(encode_mtrk(&mut samp_evts, "Sampler"));
    }
    for (ei, evts) in extra_evts.iter_mut().enumerate() {
        if !evts.is_empty() {
            tracks.push(encode_mtrk(evts, &project.extra_tracks[ei].name));
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // Format 1 (multi-track synchronous)
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&PPQ.to_be_bytes());
    for t in tracks {
        out.extend_from_slice(&t);
    }
    Ok(out)
}

pub fn export_project_midi(project: &Project, path: &Path) -> Result<(), String> {
    let bytes = export_project_midi_bytes(project)?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}
