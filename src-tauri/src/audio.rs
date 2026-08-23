use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SupportedStreamConfig};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

const TARGET_RATE: u32 = 16000;
const BAR_COUNT: usize = 20;

// Visual meter — tuned so quiet PC / USB mics still drive a lively swell
// (laptop built-ins are louder; metering post-AGC equalizes them).
const NOISE_GATE: f32 = 0.00035;
const LEVEL_GAIN: f32 = 12.0;

// AGC: lift quiet mics for STT without blasting room noise into speech.
const AGC_PREAMP: f32 = 1.95;
const AGC_TARGET_RMS: f32 = 0.14;
const AGC_MAX_GAIN: f32 = 5.2;
const AGC_NOISE_FLOOR: f32 = 0.0006;
const AGC_ATTACK: f32 = 0.32;
const AGC_RELEASE: f32 = 0.08;

/// Near-field gate: open for normal close-mic speech; stay closed for quieter
/// room / behind-you talk once a speech peak is established.
/// Relative open/close are vs an RMS-heavy envelope (not raw plosive peak) so a
/// loud start cannot mute the rest of a long hold.
const STT_GATE_FLOOR_OPEN: f32 = 0.0032;
const STT_GATE_FLOOR_CLOSE: f32 = 0.0015;
const STT_GATE_REL_OPEN: f32 = 0.12;
const STT_GATE_REL_CLOSE: f32 = 0.055;
const STT_GATE_PEAK_ATTACK: f32 = 0.28;
const STT_GATE_PEAK_DECAY_OPEN: f32 = 0.996;
const STT_GATE_PEAK_DECAY_CLOSED: f32 = 0.97;
const STT_GATE_HANGOVER_MS: u32 = 220;
const STT_GATE_START_PEAK: f32 = 0.012;

struct SttGate {
    open: bool,
    peak: f32,
    hangover_samples: u32,
}

impl SttGate {
    fn new() -> Self {
        Self {
            open: false,
            peak: STT_GATE_START_PEAK,
            hangover_samples: 0,
        }
    }
}

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
    /// Shared PCM buffer so `stop()` can flush a partial trailing chunk.
    pcm_buffer: Option<Arc<Mutex<Vec<i16>>>>,
    pcm_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<i16>>>,
}

unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            stream: None,
            sample_rate: TARGET_RATE,
            pcm_buffer: None,
            pcm_tx: None,
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

        let chunk_size = (TARGET_RATE as usize) / 20; // 50ms at 16kHz — snappier start/end
        let buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(chunk_size)));
        let buffer_clone = buffer.clone();
        self.pcm_buffer = Some(buffer);
        self.pcm_tx = Some(sender.clone());
        let resample_state = Arc::new(Mutex::new(0.0f64));
        let resample_state_clone = resample_state.clone();
        let agc_gain = Arc::new(Mutex::new(1.0f32));
        let agc_gain_clone = agc_gain.clone();
        // Low starter peak so the *first* utterance opens on the absolute floor
        // (easy for the user). After they speak, an RMS envelope + hangover
        // keeps the same talker open through pauses / quieter continuation.
        let gate = Arc::new(Mutex::new(SttGate::new()));
        let gate_clone = gate.clone();
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
                            &gate_clone,
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
                            &gate_clone,
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
                            &gate_clone,
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
                            &gate_clone,
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
        // Stop the callback first, then flush any leftover < chunk_size samples
        // so the last ~50ms of speech still reaches Deepgram.
        self.stream = None;
        if let (Some(buf), Some(tx)) = (self.pcm_buffer.take(), self.pcm_tx.take()) {
            let leftover = {
                let mut b = buf.lock().unwrap();
                std::mem::take(&mut *b)
            };
            if !leftover.is_empty() {
                let _ = tx.send(leftover);
            }
        }
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
    gate: &Arc<Mutex<SttGate>>,
    buffer: &Arc<Mutex<Vec<i16>>>,
    chunk_size: usize,
    sender: &tokio::sync::mpsc::UnboundedSender<Vec<i16>>,
    app: &tauri::AppHandle,
) {
    let mono = to_mono(data, channels);

    if mono.is_empty() {
        return;
    }

    // Gate quieter room / call voices before AGC so gain can't lift them up.
    let native_rate = (TARGET_RATE as f64 * ratio).round().max(1.0) as u32;
    let gated = apply_near_field_gate(&mono, gate, native_rate);
    let gained = apply_soft_agc(&gated, agc_gain);

    let frame = FRAME.fetch_add(1, Ordering::Relaxed);
    // Meter the AGC'd signal so quiet PC/USB mics animate like loud laptop mics.
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

/// Mute frames that are soft relative to the user's recent speech envelope.
/// Close talkers stay open through pauses; quieter background (call speakers, room) stays out.
fn apply_near_field_gate(
    samples: &[f32],
    gate: &Arc<Mutex<SttGate>>,
    sample_rate: u32,
) -> Vec<f32> {
    let mut g = gate.lock().unwrap();
    apply_near_field_gate_inner(samples, &mut g, sample_rate)
}

fn apply_near_field_gate_inner(
    samples: &[f32],
    gate: &mut SttGate,
    sample_rate: u32,
) -> Vec<f32> {
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
    let peak = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0f32, f32::max);
    // Instantaneous decision uses some peak so plosives still open quickly;
    // the *stored* envelope is RMS-heavy so one loud consonant cannot set an
    // unreachable reopen bar for the rest of the hold.
    let level = rms * 0.45 + peak * 0.55;
    let env = rms * 0.8 + peak * 0.2;

    if env > gate.peak {
        gate.peak += (env - gate.peak) * STT_GATE_PEAK_ATTACK;
    } else {
        let decay = if gate.open {
            STT_GATE_PEAK_DECAY_OPEN
        } else {
            STT_GATE_PEAK_DECAY_CLOSED
        };
        gate.peak *= decay;
        if gate.peak < STT_GATE_START_PEAK {
            gate.peak = STT_GATE_START_PEAK;
        }
    }

    let open_thresh = STT_GATE_FLOOR_OPEN.max(gate.peak * STT_GATE_REL_OPEN);
    let close_thresh = STT_GATE_FLOOR_CLOSE.max(gate.peak * STT_GATE_REL_CLOSE);
    let hangover_max = sample_rate.saturating_mul(STT_GATE_HANGOVER_MS) / 1000;

    if gate.open {
        if level < close_thresh {
            if gate.hangover_samples > samples.len() as u32 {
                gate.hangover_samples -= samples.len() as u32;
            } else {
                gate.hangover_samples = 0;
                gate.open = false;
            }
        } else {
            gate.hangover_samples = hangover_max;
        }
    } else if level >= open_thresh {
        gate.open = true;
        gate.hangover_samples = hangover_max;
    }

    let pass = gate.open;
    if pass {
        samples.to_vec()
    } else {
        // Hard mute for STT — do not leak quiet background as "speech".
        vec![0.0; samples.len()]
    }
}

/// Downmix to mono.
/// - Stereo headsets: pick the louder channel (avg washes speech with a dead side).
/// - Mic arrays (3+ ch): average — loudest-channel often picks noise on Intel SST arrays.
fn to_mono(data: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return data.to_vec();
    }
    let frames = data.len() / ch;
    if frames == 0 {
        return Vec::new();
    }

    if ch >= 3 {
        return (0..frames)
            .map(|frame| {
                let mut sum = 0.0f32;
                for c in 0..ch {
                    sum += data[frame * ch + c];
                }
                sum / ch as f32
            })
            .collect();
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

    (0..frames).map(|frame| data[frame * ch + best]).collect()
}

fn apply_soft_agc(samples: &[f32], gain_state: &Arc<Mutex<f32>>) -> Vec<f32> {
    // Mild fixed preamp so quiet / distant laptop mics still reach Deepgram.
    let boosted: Vec<f32> = samples.iter().map(|&s| s * AGC_PREAMP).collect();
    let rms = (boosted.iter().map(|s| s * s).sum::<f32>() / boosted.len() as f32).sqrt();
    let peak = boosted
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0f32, f32::max);

    let desired = if rms < AGC_NOISE_FLOOR {
        1.0
    } else {
        let from_rms = (AGC_TARGET_RMS / rms).clamp(1.0, AGC_MAX_GAIN);
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

    boosted
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
        // Flat idle — do not fabricate a jumping "hearing you" breathe from
        // silence or a closed near-field gate. Overlay connecting vs live
        // depends on real levels from this session.
        return vec![0.08; n];
    }

    // One global loudness for the whole chunk — no per-slice chaos. A gentle
    // symmetrical wave shaped by that level; peaks read as a smooth swell.
    // Softer curve (0.58) so quiet devices jump earlier without clipping loud ones.
    let level = ((energy - NOISE_GATE * 0.35).max(0.0) * LEVEL_GAIN)
        .powf(0.58)
        .clamp(0.12, 0.98);
    for i in 0..n {
        // Bell envelope: tallest in the middle, tapering to the capsule ends.
        let center_dist = (i as f32 - (n as f32 - 1.0) / 2.0) / ((n as f32 - 1.0) / 2.0);
        let bell = 1.0 - center_dist * center_dist * 0.40;
        // Slow wave so neighboring bars differ slightly — calm, not jittery.
        let wave = 0.82 + 0.18 * ((t * 1.6 + i as f32 * 0.5).sin() * 0.5 + 0.5);
        bars.push((level * bell * wave).clamp(0.12, 0.98));
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

#[cfg(test)]
mod gate_tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (i as f32 * 0.31).sin())
            .collect()
    }

    fn passed(out: &[f32]) -> bool {
        out.iter().any(|s| s.abs() > 1e-6)
    }

    #[test]
    fn quieter_continuation_stays_open_after_loud_start() {
        let mut gate = SttGate::new();
        let loud = tone(160, 0.55);
        let cont = tone(160, 0.07);
        for _ in 0..12 {
            assert!(passed(&apply_near_field_gate_inner(&loud, &mut gate, 16000)));
        }
        for i in 0..80 {
            let out = apply_near_field_gate_inner(&cont, &mut gate, 16000);
            assert!(
                passed(&out),
                "gate muted continued speech at frame {i} (peak={:.3})",
                gate.peak
            );
        }
    }

    #[test]
    fn reopens_after_mid_hold_pause() {
        let mut gate = SttGate::new();
        let speech = tone(160, 0.22);
        let silence = vec![0.0f32; 160];
        for _ in 0..20 {
            assert!(passed(&apply_near_field_gate_inner(&speech, &mut gate, 16000)));
        }
        // ~400ms of silence — longer than hangover, gate should close.
        for _ in 0..40 {
            let _ = apply_near_field_gate_inner(&silence, &mut gate, 16000);
        }
        assert!(!gate.open, "expected gate to close after a pause");
        // Same talker resumes at a bit below the original level.
        let resume = tone(160, 0.12);
        let mut reopened = false;
        for _ in 0..8 {
            if passed(&apply_near_field_gate_inner(&resume, &mut gate, 16000)) {
                reopened = true;
                break;
            }
        }
        assert!(reopened, "gate did not reopen for continued close-mic speech");
    }

    #[test]
    fn behind_you_stays_closed_after_speech_peak() {
        let mut gate = SttGate::new();
        let close = tone(160, 0.28);
        let behind = tone(160, 0.006);
        for _ in 0..15 {
            assert!(passed(&apply_near_field_gate_inner(&close, &mut gate, 16000)));
        }
        for _ in 0..40 {
            let _ = apply_near_field_gate_inner(&vec![0.0; 160], &mut gate, 16000);
        }
        for _ in 0..20 {
            let out = apply_near_field_gate_inner(&behind, &mut gate, 16000);
            assert!(!passed(&out), "behind-you talk leaked through after peak");
        }
    }

    #[test]
    fn silence_meter_is_flat_not_a_fake_breathe() {
        let quiet = vec![0.0f32; 320];
        let a = compute_bars(&quiet, 20, 0);
        let b = compute_bars(&quiet, 20, 17);
        assert_eq!(a, b, "silence must not fabricate jumping bars across frames");
        assert!(a.iter().all(|&v| (v - 0.08).abs() < 1e-5));
    }

    #[test]
    fn speech_meter_rises_above_idle() {
        let speech = tone(320, 0.22);
        let bars = compute_bars(&speech, 20, 0);
        assert!(
            bars.iter().any(|&v| v > 0.2),
            "close-mic speech should drive a live waveform, got {bars:?}"
        );
    }
}
