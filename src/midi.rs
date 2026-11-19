use midir::{Ignore, MidiInput, MidiInputConnection};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy)]
pub enum MidiMsg {
    NoteOn { note: u8, velocity: f32 },
    NoteOff { note: u8 },
    Cc { cc: u8, value: f32 },
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
}
