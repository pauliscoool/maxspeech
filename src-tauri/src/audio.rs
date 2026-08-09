use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SupportedStreamConfig};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

const TARGET_RATE: u32 = 16000;
const BAR_COUNT: usize = 29;

// Visual meter only.
const NOISE_GATE: f32 = 0.003;
const LEVEL_GAIN: f32 = 7.0;

// Soft AGC for quiet mics. Keep boost modest — over-boosting makes Deepgram
// hear different words ("build"→"blood") and pulls up distant background talk.
const AGC_TARGET_RMS: f32 = 0.09;
const AGC_MAX_GAIN: f32 = 1.7;
const AGC_NOISE_FLOOR: f32 = 0.01;
const AGC_ATTACK: f32 = 0.14;
const AGC_RELEASE: f32 = 0.1;

// STT noise gate: mute frames well below the current close-talker level so
// background conversations aren't transcribed. Uses absolute floor + relative
// threshold vs a slow peak hold (closest / loudest speaker wins).
const STT_GATE_FLOOR_OPEN: f32 = 0.01;
const STT_GATE_FLOOR_CLOSE: f32 = 0.006;
const STT_GATE_REL_OPEN: f32 = 0.38; // open at ≥38% of recent peak
const STT_GATE_REL_CLOSE: f32 = 0.22;
const STT_GATE_CLOSED_GAIN: f32 = 0.0;
const STT_PEAK_DECAY: f32 = 0.995; // ~slow forget of prior loudness

/// Empty / "default" means follow the OS default input device.
pub const MIC_DEVICE_DEFAULT: &str = "default";

static FRAME: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
pub struct MicDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MicTestResult {
    pub name: String,
    pub peak: f32,
    pub ok: bool,
    pub message: String,
}

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
        preferred_device: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (device, label) = resolve_input_device(preferred_device)?;
        let supported = pick_input_config(&device)?;
        let native_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let sample_format = supported.sample_format();

        let config: cpal::StreamConfig = supported.clone().into();
        self.sample_rate = native_rate;
        FRAME.store(0, Ordering::Relaxed);

        log::info!(
            "Audio: {:?} @ {}Hz {}ch {:?} (preferred={:?})",
            label,
            native_rate,
            channels,
            sample_format,
            preferred_device
        );

        let chunk_size = (TARGET_RATE as usize) / 10; // 100ms at 16kHz
        let buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(chunk_size)));
        let buffer_clone = buffer.clone();
        let resample_state = Arc::new(Mutex::new(0.0f64));
        let resample_state_clone = resample_state.clone();
        let agc_gain = Arc::new(Mutex::new(1.0f32));
        let agc_gain_clone = agc_gain.clone();
        let gate_open = Arc::new(Mutex::new(false));
        let gate_open_clone = gate_open.clone();
        let gate_peak = Arc::new(Mutex::new(0.02f32));
        let gate_peak_clone = gate_peak.clone();
        let ratio = native_rate as f64 / TARGET_RATE as f64;

        let err_fn = |err| log::error!("Audio stream error: {err}");

        let stream = match sample_format {
            SampleFormat::F32 => {
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
                            &gate_open_clone,
                            &gate_peak_clone,
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
            SampleFormat::I16 => {
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
                            &gate_open_clone,
                            &gate_peak_clone,
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
            SampleFormat::U16 => {
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
                            &gate_open_clone,
                            &gate_peak_clone,
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
            SampleFormat::I32 => {
                let app2 = app.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i32], _: &cpal::InputCallbackInfo| {
                        let f32_data: Vec<f32> = data
                            .iter()
                            .map(|&s| s as f32 / 2147483648.0)
                            .collect();
                        process_f32(
                            &f32_data,
                            channels,
                            ratio,
                            &resample_state_clone,
                            &agc_gain_clone,
                            &gate_open_clone,
                            &gate_peak_clone,
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

pub fn list_microphones() -> Result<Vec<MicDeviceInfo>, Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());

    let mut out = Vec::new();
    for device in host.input_devices()? {
        let Ok(name) = device.name() else { continue };
        // Skip devices that cannot open an input config at all.
        if device.default_input_config().is_err() {
            continue;
        }
        let is_default = default_name
            .as_ref()
            .is_some_and(|d| names_match(d, &name));
        out.push(MicDeviceInfo { name, is_default });
    }
    // Stable order: default first, then alphabetical.
    out.sort_by(|a, b| match (a.is_default, b.is_default) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(out)
}

fn normalize_mic_pref(preferred: Option<&str>) -> Option<String> {
    preferred
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case(MIC_DEVICE_DEFAULT))
        .map(|s| s.to_string())
}

/// Collapse trademark glyphs / whitespace so "Intel® SST" matches across IPC/cloud.
fn normalize_device_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        let c = match ch {
            '®' | '™' | '©' => ' ',
            _ => ch.to_ascii_lowercase(),
        };
        if c.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
            continue;
        }
        prev_space = false;
        out.push(c);
    }
    out.trim().to_string()
}

fn names_match(a: &str, b: &str) -> bool {
    a == b || normalize_device_name(a) == normalize_device_name(b)
}

/// Resolve a saved preference to a currently attached device name (exact or fuzzy).
pub fn resolve_saved_mic_name(preferred: &str, devices: &[MicDeviceInfo]) -> Option<String> {
    let pref = preferred.trim();
    if pref.is_empty() || pref.eq_ignore_ascii_case(MIC_DEVICE_DEFAULT) {
        return Some(MIC_DEVICE_DEFAULT.to_string());
    }
    devices
        .iter()
        .find(|d| names_match(&d.name, pref))
        .map(|d| d.name.clone())
}

fn resolve_input_device(
    preferred: Option<&str>,
) -> Result<(Device, String), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let pref = normalize_mic_pref(preferred);

    if let Some(want) = pref.as_deref() {
        let mut fuzzy: Option<(Device, String)> = None;
        for device in host.input_devices()? {
            if let Ok(name) = device.name() {
                if name == want {
                    return Ok((device, name));
                }
                if fuzzy.is_none() && names_match(&name, want) {
                    fuzzy = Some((device, name));
                }
            }
        }
        if let Some(hit) = fuzzy {
            log::info!("Microphone fuzzy-matched {want:?} → {:?}", hit.1);
            return Ok(hit);
        }
        log::warn!("Saved microphone {want:?} not found — falling back to system default");
    }

    let device = host
        .default_input_device()
        .ok_or("No input device available")?;
    let label = device
        .name()
        .unwrap_or_else(|_| "System default".into());
    Ok((device, label))
}

/// Prefer the device's native shared-mode config (WASAPI often only supports one rate).
fn pick_input_config(device: &Device) -> Result<SupportedStreamConfig, Box<dyn std::error::Error>> {
    if let Ok(default) = device.default_input_config() {
        return Ok(default);
    }
    // Fallback: first supported config at its max rate.
    let mut ranges = device.supported_input_configs()?;
    let range = ranges
        .next()
        .ok_or("No supported input configs for microphone")?;
    Ok(range.with_max_sample_rate())
}

fn process_f32(
    data: &[f32],
    channels: u16,
    ratio: f64,
    resample_pos: &Arc<Mutex<f64>>,
    agc_gain: &Arc<Mutex<f32>>,
    gate_open: &Arc<Mutex<bool>>,
    gate_peak: &Arc<Mutex<f32>>,
    buffer: &Arc<Mutex<Vec<i16>>>,
    chunk_size: usize,
    sender: &tokio::sync::mpsc::UnboundedSender<Vec<i16>>,
    app: &tauri::AppHandle,
) {
    let mono = to_mono(data, channels);

    if mono.is_empty() {
        return;
    }

    // Gate first (drop distant/background), then gentle AGC on close speech only.
    let gated = apply_stt_noise_gate(&mono, gate_open, gate_peak);
    let open = *gate_open.lock().unwrap();
    let gained = if open {
        apply_soft_agc(&gated, agc_gain)
    } else {
        // Don't let AGC climb while the room is quiet / distant talkers are speaking.
        if let Ok(mut g) = agc_gain.lock() {
            *g = (*g * 0.92).max(1.0);
        }
        gated
    };

    let frame = FRAME.fetch_add(1, Ordering::Relaxed);
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

/// Downmix to mono, preferring the loudest channel when it clearly dominates
/// (closest talker on stereo / array mics). Otherwise average to reject noise spikes.
fn to_mono(data: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return data.to_vec();
    }
    let frames = data.len() / ch;
    if frames == 0 {
        return Vec::new();
    }

    let mut energies = vec![0.0f32; ch];
    for frame in 0..frames {
        for c in 0..ch {
            let s = data[frame * ch + c];
            energies[c] += s * s;
        }
    }
    let best = energies
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0);
    let best_e = energies[best];
    let mean_e = energies.iter().sum::<f32>() / ch as f32;

    // Clear winner (~closest / on-axis) → take that channel. Otherwise average.
    let use_best = best_e > mean_e * 1.55 && best_e > 1e-8;
    if use_best || ch == 2 {
        return (0..frames).map(|frame| data[frame * ch + best]).collect();
    }

    (0..frames)
        .map(|frame| {
            let mut sum = 0.0f32;
            for c in 0..ch {
                sum += data[frame * ch + c];
            }
            sum / ch as f32
        })
        .collect()
}

/// Hysteresis gate vs absolute floor + recent peak (closest/loudest speaker).
fn apply_stt_noise_gate(
    samples: &[f32],
    gate_open: &Arc<Mutex<bool>>,
    gate_peak: &Arc<Mutex<f32>>,
) -> Vec<f32> {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();

    let mut peak = gate_peak.lock().unwrap();
    if rms > *peak {
        *peak = rms;
    } else {
        *peak = (*peak * STT_PEAK_DECAY).max(STT_GATE_FLOOR_OPEN);
    }
    let peak_now = *peak;
    drop(peak);

    let open_thr = STT_GATE_FLOOR_OPEN.max(peak_now * STT_GATE_REL_OPEN);
    let close_thr = STT_GATE_FLOOR_CLOSE.max(peak_now * STT_GATE_REL_CLOSE);

    let mut open = gate_open.lock().unwrap();
    if *open {
        if rms < close_thr {
            *open = false;
        }
    } else if rms >= open_thr {
        *open = true;
    }
    let pass = *open;
    drop(open);

    if pass {
        samples.to_vec()
    } else {
        samples.iter().map(|&s| s * STT_GATE_CLOSED_GAIN).collect()
    }
}

fn apply_soft_agc(samples: &[f32], gain_state: &Arc<Mutex<f32>>) -> Vec<f32> {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    let peak = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0f32, f32::max);

    let desired = if rms < AGC_NOISE_FLOOR {
        1.0
    } else {
        let from_rms = (AGC_TARGET_RMS / rms).clamp(1.0, AGC_MAX_GAIN);
        let peak_cap = if peak > 1e-6 {
            (0.92 / peak).clamp(1.0, AGC_MAX_GAIN)
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

        let mid = 1.0 - (i as f32 - (n as f32 - 1.0) / 2.0).abs() / ((n as f32 - 1.0) / 2.0) * 0.25;
        let phase = i as f32 * 0.7 + t;
        let jitter = 0.12 * (phase.sin() * 0.5 + 0.5);

        let gated = ((mixed - NOISE_GATE * 0.5).max(0.0) * LEVEL_GAIN).clamp(0.0, 1.0);
        let level = (gated.powf(0.72) * mid + jitter * gated).clamp(0.08, 1.0);
        bars.push(level);
    }
    bars
}

pub fn test_microphone(
    preferred_device: Option<&str>,
    app: Option<&tauri::AppHandle>,
) -> Result<MicTestResult, Box<dyn std::error::Error>> {
    let (device, label) = resolve_input_device(preferred_device)?;
    let supported = pick_input_config(&device)?;
    let channels = supported.channels();
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();

    let peak = Arc::new(Mutex::new(0.0f32));
    let peak_cb = peak.clone();
    let app_cb = app.cloned();

    let err_fn = |err| log::error!("Mic test error: {err}");

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                mic_test_callback(data, channels, &peak_cb, app_cb.as_ref());
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let f: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                mic_test_callback(&f, channels, &peak_cb, app_cb.as_ref());
            },
            err_fn,
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                let f: Vec<f32> = data
                    .iter()
                    .map(|&s| (s as f32 / 32768.0) - 1.0)
                    .collect();
                mic_test_callback(&f, channels, &peak_cb, app_cb.as_ref());
            },
            err_fn,
            None,
        )?,
        SampleFormat::I32 => device.build_input_stream(
            &config,
            move |data: &[i32], _: &cpal::InputCallbackInfo| {
                let f: Vec<f32> = data
                    .iter()
                    .map(|&s| s as f32 / 2147483648.0)
                    .collect();
                mic_test_callback(&f, channels, &peak_cb, app_cb.as_ref());
            },
            err_fn,
            None,
        )?,
        other => {
            return Err(format!("Unsupported sample format for mic test: {other:?}").into());
        }
    };

    stream.play()?;
    // Longer window — Bluetooth headsets often need a moment to wake the mic path.
    std::thread::sleep(std::time::Duration::from_millis(1200));
    drop(stream);

    let peak_v = *peak.lock().unwrap();
    let ok = peak_v > 0.002;
    let message = if ok {
        format!("Hearing you on {label}")
    } else {
        format!(
            "Opened “{label}” but got silence. Speak, unmute it in Windows Sound settings, or pick another mic (Bluetooth headsets are often silent until active)."
        )
    };
    Ok(MicTestResult {
        name: label,
        peak: peak_v,
        ok,
        message,
    })
}

fn mic_test_callback(
    data: &[f32],
    channels: u16,
    peak: &Arc<Mutex<f32>>,
    app: Option<&tauri::AppHandle>,
) {
    let mono = to_mono(data, channels);
    let level = mono
        .iter()
        .map(|s| s.abs())
        .fold(0.0f32, f32::max);
    {
        let mut p = peak.lock().unwrap();
        if level > *p {
            *p = level;
        }
    }
    if let Some(app) = app {
        let meter = (level * 8.0).clamp(0.0, 1.0);
        let _ = app.emit("mic-test-level", meter);
    }
}
