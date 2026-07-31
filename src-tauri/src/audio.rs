use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::Emitter;

const TARGET_RATE: u32 = 16000;
const BAR_COUNT: usize = 29;

// Visual meter only — never drops frames sent to STT.
// Lowered so quiet / far-field speech still moves the bars after soft AGC.
const NOISE_GATE: f32 = 0.003;
const LEVEL_GAIN: f32 = 7.0;

// Soft AGC for the STT path: lift quiet speech toward a usable RMS, cap peaks.
const AGC_TARGET_RMS: f32 = 0.10;
const AGC_MAX_GAIN: f32 = 6.0;
const AGC_NOISE_FLOOR: f32 = 0.0012;
const AGC_ATTACK: f32 = 0.22;
const AGC_RELEASE: f32 = 0.06;

static FRAME: AtomicU64 = AtomicU64::new(0);

pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    pub sample_rate: u32,
}

unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            stream: None,
            sample_rate: TARGET_RATE,
        }
    }

    pub fn start(
        &mut self,
        sender: tokio::sync::mpsc::UnboundedSender<Vec<i16>>,
        app: tauri::AppHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No input device available")?;

        let supported = device.default_input_config()?;
        let native_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let sample_format = supported.sample_format();

        let config: cpal::StreamConfig = supported.clone().into();
        self.sample_rate = native_rate;
        FRAME.store(0, Ordering::Relaxed);

        log::info!(
            "Audio: {:?} @ {}Hz {}ch {:?}",
            device.name().unwrap_or_default(),
            native_rate,
            channels,
            sample_format
        );

        let chunk_size = (TARGET_RATE as usize) / 10; // 100ms at 16kHz
        let buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(chunk_size)));
        let buffer_clone = buffer.clone();
        let resample_state = Arc::new(Mutex::new(0.0f64));
        let resample_state_clone = resample_state.clone();
        let agc_gain = Arc::new(Mutex::new(1.0f32));
        let agc_gain_clone = agc_gain.clone();
        let ratio = native_rate as f64 / TARGET_RATE as f64;

        let err_fn = |err| log::error!("Audio stream error: {err}");

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let app2 = app.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        process_f32(
                            data,
                            channels,
                            ratio,
                            &resample_state_clone,
                            &agc_gain_clone,
                            &buffer_clone,
                            chunk_size,
                            &sender,
                            &app2,
                        );
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::I16 => {
                let app2 = app.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> =
                            data.iter().map(|&s| s as f32 / 32768.0).collect();
                        process_f32(
                            &f32_data,
                            channels,
                            ratio,
                            &resample_state_clone,
                            &agc_gain_clone,
                            &buffer_clone,
                            chunk_size,
                            &sender,
                            &app2,
                        );
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let app2 = app.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data
                            .iter()
                            .map(|&s| (s as f32 / 32768.0) - 1.0)
                            .collect();
                        process_f32(
                            &f32_data,
                            channels,
                            ratio,
                            &resample_state_clone,
                            &agc_gain_clone,
                            &buffer_clone,
                            chunk_size,
                            &sender,
                            &app2,
                        );
                    },
                    err_fn,
                    None,
                )?
            }
            other => return Err(format!("Unsupported sample format: {other:?}").into()),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream = None;
    }
}

fn process_f32(
    data: &[f32],
    channels: u16,
    ratio: f64,
    resample_pos: &Arc<Mutex<f64>>,
    agc_gain: &Arc<Mutex<f32>>,
    buffer: &Arc<Mutex<Vec<i16>>>,
    chunk_size: usize,
    sender: &tokio::sync::mpsc::UnboundedSender<Vec<i16>>,
    app: &tauri::AppHandle,
) {
    let mono: Vec<f32> = if channels == 1 {
        data.to_vec()
    } else {
        data.chunks(channels as usize)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    if mono.is_empty() {
        return;
    }

    // Soft AGC on the STT path — never hard-gate silence; lift quiet speech, soft-cap peaks.
    let gained = apply_soft_agc(&mono, agc_gain);

    let frame = FRAME.fetch_add(1, Ordering::Relaxed);
    // Bars reflect what Deepgram hears (post-AGC), not a separate gated stream.
    let bars = compute_bars(&gained, BAR_COUNT, frame);
    let _ = app.emit("audio-level", bars);

    let resampled = if (ratio - 1.0).abs() < 0.001 {
        gained
            .iter()
            .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
            .collect()
    } else {
        resample_linear(&gained, ratio, resample_pos)
    };

    let mut buf = buffer.lock().unwrap();
    buf.extend_from_slice(&resampled);
    while buf.len() >= chunk_size {
        let chunk: Vec<i16> = buf.drain(..chunk_size).collect();
        let _ = sender.send(chunk);
    }
}

/// Soft automatic gain for far-field / quiet input.
/// Below the noise floor we ease toward unity so hiss isn't chased; otherwise we
/// gently pull RMS toward AGC_TARGET_RMS with a hard max boost and soft clip.
fn apply_soft_agc(samples: &[f32], gain_state: &Arc<Mutex<f32>>) -> Vec<f32> {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let peak = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0f32, f32::max);

    let desired = if rms < AGC_NOISE_FLOOR {
        // True near-silence: don't amplify room noise; drift back to 1×.
        1.0
    } else {
        let from_rms = (AGC_TARGET_RMS / rms).clamp(1.0, AGC_MAX_GAIN);
        // Also respect peaks so a single spike doesn't force under-gain forever.
        let peak_cap = if peak > 1e-6 {
            (0.95 / peak).clamp(1.0, AGC_MAX_GAIN)
        } else {
            AGC_MAX_GAIN
        };
        from_rms.min(peak_cap)
    };

    let mut g = gain_state.lock().unwrap();
    let alpha = if desired > *g { AGC_ATTACK } else { AGC_RELEASE };
    *g = *g + (desired - *g) * alpha;
    let gain = *g;
    drop(g);

    samples
        .iter()
        .map(|&s| {
            let x = s * gain;
            // Soft clip: keep peaks usable without hard digital clipping.
            if x.abs() <= 0.9 {
                x
            } else {
                x.signum() * (0.9 + 0.1 * ((x.abs() - 0.9) / 0.1).tanh())
            }
        })
        .collect()
}

fn resample_linear(input: &[f32], ratio: f64, pos: &Arc<Mutex<f64>>) -> Vec<i16> {
    let mut p = pos.lock().unwrap();
    let mut out = Vec::new();
    while *p < input.len() as f64 - 1.0 {
        let i = *p as usize;
        let frac = (*p - i as f64) as f32;
        let sample = input[i] * (1.0 - frac) + input[i + 1] * frac;
        out.push((sample.clamp(-1.0, 1.0) * 32767.0) as i16);
        *p += ratio;
    }
    *p -= input.len() as f64;
    if *p < 0.0 {
        *p = 0.0;
    }
    out
}

fn compute_bars(samples: &[f32], n: usize, frame: u64) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.08; n];
    }

    let overall_rms =
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let peak = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0f32, f32::max);
    let energy = (overall_rms * 0.65 + peak * 0.35).max(0.0);

    // Soft floor so bars still breathe while mic is open
    let t = frame as f32 * 0.18;
    let mut bars = Vec::with_capacity(n);

    if energy < NOISE_GATE {
        for i in 0..n {
            let phase = i as f32 * 0.55;
            let breathe = 0.08 + 0.06 * ((t + phase).sin() * 0.5 + 0.5);
            bars.push(breathe);
        }
        return bars;
    }

    let chunk = (samples.len() / n).max(1);
    for i in 0..n {
        let start = i * chunk;
        let end = ((i + 1) * chunk).min(samples.len());
        let slice = if start < samples.len() {
            &samples[start..end]
        } else {
            &samples[..]
        };
        let rms = (slice.iter().map(|s| s * s).sum::<f32>() / slice.len().max(1) as f32).sqrt();
        let local_peak = slice
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0f32, f32::max);
        let mixed = rms * 0.55 + local_peak * 0.45;

        // Center-weighted envelope + per-bar phase so the meter looks alive
        let mid = 1.0 - (i as f32 - (n as f32 - 1.0) / 2.0).abs() / ((n as f32 - 1.0) / 2.0) * 0.25;
        let phase = i as f32 * 0.7 + t;
        let jitter = 0.12 * (phase.sin() * 0.5 + 0.5);

        let gated = ((mixed - NOISE_GATE * 0.5).max(0.0) * LEVEL_GAIN).clamp(0.0, 1.0);
        let level = (gated.powf(0.72) * mid + jitter * gated).clamp(0.08, 1.0);
        bars.push(level);
    }
    bars
}

pub fn test_microphone(_app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No input device available")?;
    let supported = device.default_input_config()?;
    let config: cpal::StreamConfig = supported.into();

    let received = Arc::new(Mutex::new(false));
    let received_clone = received.clone();

    let stream = device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if !data.is_empty() {
                *received_clone.lock().unwrap() = true;
            }
        },
        |err| log::error!("Mic test error: {err}"),
        None,
    )?;
    stream.play()?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    drop(stream);
    if *received.lock().unwrap() {
        Ok(())
    } else {
        Err("No audio data received from microphone".into())
    }
}
