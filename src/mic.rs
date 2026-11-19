use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

/// Simple live microphone recorder using the system's default CPAL audio input device.
/// Records up to 15 seconds of mono f32 audio, resamples to 48 kHz, trims silence, and normalizes.
pub struct MicRecorder {
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    src_sr: u32,
    pub recording: bool,
    pub status: String,
}

impl Default for MicRecorder {
    fn default() -> Self {
        Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            src_sr: 48_000,
            recording: false,
            status: String::new(),
        }
    }
}

impl MicRecorder {
    pub fn start(&mut self) -> Result<(), String> {
        self.stop_discard();
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| "No microphone / audio input device found".to_string())?;
        let cfg = device
            .default_input_config()
            .map_err(|e| format!("Mic config error: {e}"))?;
        let channels = (cfg.channels() as usize).max(1);
        let sr = cfg.sample_rate().0.max(8_000);
        let max_samples = (sr as usize) * 15;
        let buf = Arc::new(Mutex::new(Vec::with_capacity(sr as usize * 4)));
        let buf_ref = buf.clone();

        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &cfg.into(),
                move |data: &[f32], _| {
                    if let Ok(mut b) = buf_ref.lock() {
                        if b.len() >= max_samples {
                            return;
                        }
                        for frame in data.chunks(channels) {
                            let sum: f32 = frame.iter().copied().sum();
                            b.push((sum / frame.len() as f32).clamp(-1.0, 1.0));
                        }
                    }
                },
                |_| {},
                None,
            ),
            cpal::SampleFormat::I16 => device.build_input_stream(
                &cfg.into(),
                move |data: &[i16], _| {
                    if let Ok(mut b) = buf_ref.lock() {
                        if b.len() >= max_samples {
                            return;
                        }
                        for frame in data.chunks(channels) {
                            let sum: f32 = frame.iter().map(|&s| s as f32 / 32768.0).sum();
                            b.push((sum / frame.len() as f32).clamp(-1.0, 1.0));
                        }
                    }
                },
                |_| {},
                None,
            ),
            _ => return Err("Unsupported microphone sample format".to_string()),
        }
        .map_err(|e| format!("Mic stream error: {e}"))?;

        stream.play().map_err(|e| format!("Mic play error: {e}"))?;
        self.stream = Some(stream);
        self.buffer = buf;
        self.src_sr = sr;
        self.recording = true;
        self.status = format!("Recording Mic @ {} Hz...", sr);
        Ok(())
    }

    pub fn stop_discard(&mut self) {
        self.stream = None;
        self.recording = false;
        self.status.clear();
    }

    /// Stops recording and returns trimmed, normalized 48 kHz mono PCM.
    pub fn stop_and_finish(&mut self) -> Result<Vec<f32>, String> {
        self.stream = None;
        self.recording = false;
        self.status.clear();
        let raw = self
            .buffer
            .lock()
            .map(|b| b.clone())
            .unwrap_or_default();
        if raw.len() < 256 {
            return Err("Recording too short — speak or play into mic while recording".to_string());
        }
        let resampled = crate::dsp::sampler::resample_mono_linear(&raw, self.src_sr, 48_000);
        Ok(crate::dsp::sampler::trim_silence_and_normalize(&resampled, 48_000))
    }

    pub fn recorded_secs(&self) -> f32 {
        if !self.recording {
            return 0.0;
        }
        let n = self.buffer.lock().map(|b| b.len()).unwrap_or(0);
        n as f32 / self.src_sr.max(1) as f32
    }
}
