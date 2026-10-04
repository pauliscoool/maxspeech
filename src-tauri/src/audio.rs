use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SupportedStreamConfig};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
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
const STT_GATE_REL_CLOSE: f32 = 0.042;
const STT_GATE_PEAK_ATTACK: f32 = 0.28;
const STT_GATE_PEAK_DECAY_OPEN: f32 = 0.996;
const STT_GATE_PEAK_DECAY_CLOSED: f32 = 0.97;
const STT_GATE_HANGOVER_MS: u32 = 350;
const STT_GATE_START_PEAK: f32 = 0.012;

struct SttGate {
    open: bool,
    peak: f32,
    hangover_samples: u32,
    /// This frame is speech now. Unlike `open`, it ignores the hangover.
    voiced: bool,
}

impl SttGate {
    fn new() -> Self {
        Self {
            open: false,
            peak: STT_GATE_START_PEAK,
            hangover_samples: 0,
            voiced: false,
        }
    }
}

/// Empty / "default" means follow the OS default input device.
pub const MIC_DEVICE_DEFAULT: &str = "default";

static FRAME: AtomicU64 = AtomicU64::new(0);

/// Ms since `VOICE_EPOCH` (+1, so 0 = never) of the last frame the gate heard as speech.
static LAST_VOICE_MS: AtomicU64 = AtomicU64::new(0);
static VOICE_EPOCH: OnceLock<Instant> = OnceLock::new();

fn voice_clock_ms() -> u64 {
    VOICE_EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// True once the mic has been quiet for `ms` (or no speech was heard this take).
/// Lets the hotkey-release trail end as soon as the speaker has actually stopped.
pub fn silent_for(ms: u64) -> bool {
    let last = LAST_VOICE_MS.load(Ordering::Relaxed);
    last == 0 || voice_clock_ms().saturating_sub(last) >= ms
}

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
        LAST_VOICE_MS.store(0, Ordering::Relaxed);

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
        let resampler = Arc::new(Mutex::new(Resampler::new(
            native_rate as f64 / TARGET_RATE as f64,
        )));
        let resampler_clone = resampler.clone();
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
                            &resampler_clone,
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
                            &resampler_clone,
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
                            &resampler_clone,
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
                            &resampler_clone,
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
    resampler: &Arc<Mutex<Resampler>>,
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
        resampler.lock().unwrap().process(&gained)
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
    let out = apply_near_field_gate_inner(samples, &mut g, sample_rate);
    if g.voiced {
        LAST_VOICE_MS.store(voice_clock_ms(), Ordering::Relaxed);
    }
    out
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
    gate.voiced = pass && level >= close_thresh;
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

const RESAMPLE_ZERO_CROSSINGS: f64 = 14.0;
/// Fraction of the lower Nyquist kept flat; the rest is the filter's transition band.
const RESAMPLE_CUTOFF: f64 = 0.95;
const RESAMPLE_OVERSAMPLE: usize = 64;

/// Streaming windowed-sinc resampler to `TARGET_RATE`. Linear interpolation folds
/// everything above 8 kHz (fan noise, sibilant harmonics) back into the speech
/// band; low-passing at the kernel stage removes it before decimation.
struct Resampler {
    ratio: f64,
    half: usize,
    kernel: Vec<f32>,
    buf: Vec<f32>,
    pos: f64,
}

impl Resampler {
    fn new(ratio: f64) -> Self {
        let g = RESAMPLE_CUTOFF * ratio.recip().min(1.0);
        let half = (RESAMPLE_ZERO_CROSSINGS / g).ceil() as usize;
        let steps = half * RESAMPLE_OVERSAMPLE;
        let kernel = (0..=steps + 1)
            .map(|i| {
                let x = i as f64 / RESAMPLE_OVERSAMPLE as f64;
                let z = std::f64::consts::PI * g * x;
                let sinc = if z.abs() < 1e-9 { 1.0 } else { z.sin() / z };
                let t = (x / half as f64).min(1.0);
                let w = 0.35875
                    + 0.48829 * (std::f64::consts::PI * t).cos()
                    + 0.14128 * (2.0 * std::f64::consts::PI * t).cos()
                    + 0.01168 * (3.0 * std::f64::consts::PI * t).cos();
                (g * sinc * w) as f32
            })
            .collect();
        Self {
            ratio,
            half,
            kernel,
            // Leading zeros stand in for the signal before the first sample.
            buf: vec![0.0; half],
            pos: half as f64,
        }
    }

    fn process(&mut self, input: &[f32]) -> Vec<i16> {
        self.buf.extend_from_slice(input);
        let half = self.half as f64;
        let len = self.buf.len() as f64;
        let os = RESAMPLE_OVERSAMPLE;
        let mut out = Vec::with_capacity((input.len() as f64 / self.ratio) as usize + 2);
        while self.pos + half < len {
            // The sub-sample phase is the same for every tap on one side of
            // the output point, so the kernel interpolation weight is hoisted.
            let i0 = self.pos.floor();
            let pf = self.pos - i0;
            let i0 = i0 as usize;
            let mut acc = 0.0f32;

            let a = pf * os as f64;
            let (ai, af) = (a as usize, (a - a.floor()) as f32);
            for k in 0..=(half - pf).floor() as usize {
                let i = ai + k * os;
                acc += self.buf[i0 - k] * (self.kernel[i] + (self.kernel[i + 1] - self.kernel[i]) * af);
            }
            let b = (1.0 - pf) * os as f64;
            let (bi, bf) = (b as usize, (b - b.floor()) as f32);
            for k in 0..=(half - 1.0 + pf).floor() as usize {
                let i = bi + k * os;
                acc += self.buf[i0 + 1 + k] * (self.kernel[i] + (self.kernel[i + 1] - self.kernel[i]) * bf);
            }

            out.push((acc.clamp(-1.0, 1.0) * 32767.0).round() as i16);
            self.pos += self.ratio;
        }
        let drop = (self.pos - half).floor().max(0.0) as usize;
        self.buf.drain(..drop);
        self.pos -= drop as f64;
        out
    }
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
    fn voiced_drops_at_once_when_speech_stops_even_inside_the_hangover() {
        let mut gate = SttGate::new();
        let speech = tone(160, 0.3);
        let _ = apply_near_field_gate_inner(&speech, &mut gate, 16000);
        assert!(gate.voiced, "loud speech frame is voiced");
        let silence = vec![0.0f32; 160];
        let out = apply_near_field_gate_inner(&silence, &mut gate, 16000);
        assert!(gate.open, "hangover keeps the gate open");
        assert!(!passed(&out) || !gate.voiced);
        assert!(!gate.voiced, "but the frame is no longer speech");
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
    fn quiet_tail_stays_open_through_hangover() {
        let mut gate = SttGate::new();
        let speech = tone(160, 0.22);
        let quiet = tone(160, 0.004);
        for _ in 0..20 {
            assert!(passed(&apply_near_field_gate_inner(&speech, &mut gate, 16000)));
        }
        // ~250ms of trailing-off speech — shorter than hangover, must still pass.
        for i in 0..25 {
            let out = apply_near_field_gate_inner(&quiet, &mut gate, 16000);
            assert!(
                passed(&out),
                "quiet phrase ending muted at frame {i} (peak={:.3})",
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
        // Longer than hangover so the gate actually closes.
        for _ in 0..50 {
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
        for _ in 0..50 {
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

#[cfg(test)]
mod resample_tests {
    use super::*;
    use std::time::Instant;

    /// The previous linear-interpolation path, kept only as the comparison baseline.
    fn linear_baseline(input: &[f32], ratio: f64, pos: &mut f64) -> Vec<i16> {
        let mut out = Vec::new();
        while *pos < input.len() as f64 - 1.0 {
            let i = *pos as usize;
            let frac = (*pos - i as f64) as f32;
            let s = input[i] * (1.0 - frac) + input[i + 1] * frac;
            out.push((s.clamp(-1.0, 1.0) * 32767.0) as i16);
            *pos += ratio;
        }
        *pos -= input.len() as f64;
        if *pos < 0.0 {
            *pos = 0.0;
        }
        out
    }

    fn sine(rate: u32, hz: f64, secs: f64, amp: f32) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize)
            .map(|i| amp * (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin() as f32)
            .collect()
    }

    fn rms_db(s: &[i16], skip: usize) -> f64 {
        let body = &s[skip..s.len() - skip];
        let ms = body.iter().map(|&v| (v as f64 / 32768.0).powi(2)).sum::<f64>() / body.len() as f64;
        10.0 * ms.max(1e-20).log10()
    }

    fn run_new(rate: u32, x: &[f32]) -> Vec<i16> {
        Resampler::new(rate as f64 / TARGET_RATE as f64).process(x)
    }

    fn run_old(rate: u32, x: &[f32]) -> Vec<i16> {
        linear_baseline(x, rate as f64 / TARGET_RATE as f64, &mut 0.0)
    }

    #[test]
    fn passband_is_flat() {
        for rate in [48_000u32, 44_100, 32_000] {
            for hz in [200.0, 1000.0, 3000.0, 5000.0, 6500.0] {
                let x = sine(rate, hz, 1.0, 0.5);
                let want = 20.0 * (0.5f64 / 2f64.sqrt()).log10();
                let got = rms_db(&run_new(rate, &x), 400);
                assert!((got - want).abs() < 0.3, "{rate}Hz {hz}Hz: {got:.2} dB vs {want:.2} dB");
            }
        }
    }

    #[test]
    fn out_of_band_tones_do_not_alias() {
        for rate in [48_000u32, 44_100] {
            for hz in [10_000.0, 12_000.0, 15_000.0, 20_000.0, 22_000.0] {
                let x = sine(rate, hz, 1.0, 0.5);
                let new = rms_db(&run_new(rate, &x), 400);
                let old = rms_db(&run_old(rate, &x), 400);
                println!("{rate}Hz in, {hz:>7.0}Hz tone: linear {old:>7.1} dB -> sinc {new:>7.1} dB");
                assert!(new < -70.0, "{rate}Hz {hz}Hz leaked at {new:.1} dB");
            }
        }
    }

    #[test]
    fn chunked_stream_matches_one_shot() {
        let x: Vec<f32> = (0..48_000)
            .map(|i| 0.3 * ((i as f32 * 0.07).sin() + (i as f32 * 0.91).sin()))
            .collect();
        for rate_ratio in [3.0, 2.75625] {
            let whole = Resampler::new(rate_ratio).process(&x);
            let mut r = Resampler::new(rate_ratio);
            let mut chunked = Vec::new();
            let mut i = 0;
            let mut n = 1;
            while i < x.len() {
                let end = (i + n).min(x.len());
                chunked.extend(r.process(&x[i..end]));
                i = end;
                n = (n * 7 + 3) % 1500 + 1;
            }
            // Trim points move the f64 position by an ulp, so allow 1 LSB.
            assert_eq!(whole.len(), chunked.len(), "ratio {rate_ratio}");
            let worst = whole.iter().zip(&chunked).map(|(a, b)| (*a as i32 - *b as i32).abs()).max();
            assert!(worst <= Some(1), "ratio {rate_ratio}: worst diff {worst:?}");
        }
    }

    #[test]
    fn output_length_tracks_input() {
        let out = run_new(48_000, &vec![0.1; 48_000]);
        assert!((out.len() as i64 - 16_000).abs() <= 20, "{}", out.len());
    }

    /// cargo test --release resample_bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn resample_bench() {
        for rate in [48_000u32, 44_100] {
            let secs = 60.0;
            let mut x = sine(rate, 440.0, secs, 0.3);
            for (i, v) in x.iter_mut().enumerate() {
                *v += 0.05 * ((i as f32 * 12.9898).sin() * 43758.545).fract();
            }
            let chunk = (rate / 100) as usize; // 10 ms audio callback
            let ratio = rate as f64 / TARGET_RATE as f64;

            let mut pos = 0.0;
            let t = Instant::now();
            let mut n_old = 0;
            for c in x.chunks(chunk) {
                n_old += linear_baseline(c, ratio, &mut pos).len();
            }
            let old = t.elapsed();

            let mut r = Resampler::new(ratio);
            let t = Instant::now();
            let mut n_new = 0;
            let mut worst = std::time::Duration::ZERO;
            for c in x.chunks(chunk) {
                let s = Instant::now();
                n_new += r.process(c).len();
                worst = worst.max(s.elapsed());
            }
            let new = t.elapsed();
            println!(
                "{rate}Hz, {secs}s audio: linear {old:?} ({:.4}% of one core) | sinc {new:?} ({:.4}% of one core), worst 10ms callback {worst:?} | out {n_old}/{n_new} samples",
                old.as_secs_f64() / secs * 100.0,
                new.as_secs_f64() / secs * 100.0,
            );
        }
    }
}
