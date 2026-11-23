//! Soft liquid-glass dictation cues (synthesized — no asset files).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

#[derive(Clone, Copy)]
pub enum CueKind {
    /// Warm ascending glass ping when listening starts.
    Start,
    /// Soft descending settle when dictation stops.
    Stop,
}

/// Map the saved volume (0-100, or the old soft/medium/loud labels) to linear gain.
pub fn volume_from_setting(raw: Option<&str>) -> f32 {
    let raw = raw.unwrap_or("medium").trim().to_ascii_lowercase();
    if let Ok(n) = raw.parse::<f32>() {
        // 100 = the old "loud" gain; the chime clips well before 1.0.
        return (n.clamp(0.0, 100.0) / 100.0) * 0.72;
    }
    match raw.as_str() {
        "soft" | "low" | "quiet" => 0.22,
        "loud" | "high" => 0.72,
        _ => 0.45,
    }
}

/// The louder the PC is turned up, the quieter the chime: full gain up to 35 % system volume,
/// then it falls off so the perceived level stays close to what it is at low volume.
pub fn loudness_compensation(system_volume: f32) -> f32 {
    let v = system_volume.clamp(0.0, 1.0);
    if v <= 0.35 {
        1.0
    } else {
        (0.35 / v).powf(1.4)
    }
}

/// `follow_system`: scale by [loudness_compensation] of the current Windows master volume.
pub fn play_cue(kind: CueKind, volume: f32, follow_system: bool) {
    let vol = volume.clamp(0.0, 1.0);
    if vol <= 0.001 {
        return;
    }
    thread::spawn(move || {
        let vol = if follow_system {
            system_volume().map_or(vol, |v| vol * loudness_compensation(v))
        } else {
            vol
        };
        if let Err(e) = play_cue_blocking(kind, vol) {
            log::debug!("sound cue skipped: {e}");
        }
    });
}

/// Default output device master volume (0-1), 0 when muted.
#[cfg(windows)]
fn system_volume() -> Option<f32> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    unsafe {
        let inited = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let read = || -> windows::core::Result<f32> {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let ep: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
            if ep.GetMute()?.as_bool() {
                return Ok(0.0);
            }
            ep.GetMasterVolumeLevelScalar()
        };
        let out = read();
        if inited {
            CoUninitialize();
        }
        out.map_err(|e| log::debug!("system volume unavailable: {e}")).ok()
    }
}

#[cfg(not(windows))]
fn system_volume() -> Option<f32> {
    None
}

fn play_cue_blocking(kind: CueKind, volume: f32) -> Result<(), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "no output device".to_string())?;
    let config = device
        .default_output_config()
        .map_err(|e| e.to_string())?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let samples = synthesize(kind, sample_rate, volume);

    let mut cursor = 0usize;
    let done = Arc::new(AtomicBool::new(false));
    let done_flag = done.clone();

    let err_fn = |e| log::debug!("sound cue stream error: {e}");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let conf: cpal::StreamConfig = config.clone().into();
            device
                .build_output_stream(
                    &conf,
                    move |data: &mut [f32], _| {
                        write_interleaved(data, channels, &samples, &mut cursor, &done_flag);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        cpal::SampleFormat::I16 => {
            let conf: cpal::StreamConfig = config.clone().into();
            device
                .build_output_stream(
                    &conf,
                    move |data: &mut [i16], _| {
                        write_interleaved_i16(data, channels, &samples, &mut cursor, &done_flag);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        cpal::SampleFormat::U16 => {
            let conf: cpal::StreamConfig = config.into();
            device
                .build_output_stream(
                    &conf,
                    move |data: &mut [u16], _| {
                        write_interleaved_u16(data, channels, &samples, &mut cursor, &done_flag);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        _ => return Err("unsupported output format".into()),
    };

    stream.play().map_err(|e| e.to_string())?;
    // Wait until buffer drained (+ tiny pad).
    while !done.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(8));
    }
    thread::sleep(Duration::from_millis(40));
    Ok(())
}

fn write_interleaved(
    data: &mut [f32],
    channels: usize,
    samples: &[f32],
    cursor: &mut usize,
    done: &AtomicBool,
) {
    for frame in data.chunks_mut(channels) {
        let s = if *cursor < samples.len() {
            let v = samples[*cursor];
            *cursor += 1;
            v
        } else {
            done.store(true, Ordering::SeqCst);
            0.0
        };
        for ch in frame.iter_mut() {
            *ch = s;
        }
    }
    if *cursor >= samples.len() {
        done.store(true, Ordering::SeqCst);
    }
}

fn write_interleaved_i16(
    data: &mut [i16],
    channels: usize,
    samples: &[f32],
    cursor: &mut usize,
    done: &AtomicBool,
) {
    for frame in data.chunks_mut(channels) {
        let s = if *cursor < samples.len() {
            let v = samples[*cursor];
            *cursor += 1;
            (v.clamp(-1.0, 1.0) * 32767.0) as i16
        } else {
            done.store(true, Ordering::SeqCst);
            0
        };
        for ch in frame.iter_mut() {
            *ch = s;
        }
    }
    if *cursor >= samples.len() {
        done.store(true, Ordering::SeqCst);
    }
}

fn write_interleaved_u16(
    data: &mut [u16],
    channels: usize,
    samples: &[f32],
    cursor: &mut usize,
    done: &AtomicBool,
) {
    for frame in data.chunks_mut(channels) {
        let s = if *cursor < samples.len() {
            let v = samples[*cursor];
            *cursor += 1;
            ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
        } else {
            done.store(true, Ordering::SeqCst);
            u16::MAX / 2
        };
        for ch in frame.iter_mut() {
            *ch = s;
        }
    }
    if *cursor >= samples.len() {
        done.store(true, Ordering::SeqCst);
    }
}

/// Handmade liquid-glass chime — soft attack, harmonic shimmer, no stock beep.
fn synthesize(kind: CueKind, sample_rate: u32, volume: f32) -> Vec<f32> {
    let sr = sample_rate as f32;
    let (dur_s, tones): (f32, &[(f32, f32, f32)]) = match kind {
        CueKind::Start => (
            0.34,
            &[
                // Warm turquoise-ish glass: E5 + shimmering fifth + airy overtone
                (659.25, 1.0, 0.38),
                (987.77, 1.08, 0.22),
                (1318.5, 1.2, 0.08),
                // Soft body undercurrent
                (329.63, 0.95, 0.12),
            ],
        ),
        CueKind::Stop => (
            0.24,
            &[
                // Gentle settle — descending feel
                (523.25, 1.0, 0.28),
                (392.0, 1.15, 0.16),
                (261.63, 1.3, 0.08),
            ],
        ),
    };

    let n = (dur_s * sr) as usize;
    let mut out = vec![0.0f32; n];
    let attack = (0.006 * sr) as usize;

    for i in 0..n {
        let t = i as f32 / sr;
        let mut s = 0.0f32;
        for &(freq, decay_scale, amp) in tones {
            let env = (-t * (6.2 * decay_scale)).exp();
            // Tiny FM shimmer — feels glassier than a pure sine.
            let mod_hz = 4.5 + freq * 0.002;
            let phase = std::f32::consts::TAU * (freq * t + 0.012 * (mod_hz * t).sin());
            s += (phase.sin() * env * amp)
                + (phase * 2.0).sin() * env * amp * 0.08;
        }
        // Soft attack envelope
        let a = if i < attack {
            i as f32 / attack as f32
        } else {
            1.0
        };
        // Gentle high shelf roll via one-pole feel (approx with blend)
        out[i] = (s * a * volume * 0.55).clamp(-0.95, 0.95);
    }

    // Very light fade-out tail so the stream ends cleanly.
    let fade = (0.03 * sr) as usize;
    for k in 0..fade.min(n) {
        let i = n - 1 - k;
        let g = k as f32 / fade.max(1) as f32;
        out[i] *= g;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn louder_pc_means_quieter_cue() {
        assert_eq!(loudness_compensation(0.2), 1.0);
        let half = loudness_compensation(0.5);
        let full = loudness_compensation(1.0);
        assert!(half < 1.0 && full < half && full > 0.15, "{half} {full}");
    }

    #[test]
    fn volume_accepts_numbers_and_old_labels() {
        assert!((volume_from_setting(Some("100")) - 0.72).abs() < 1e-6);
        assert_eq!(volume_from_setting(Some("0")), 0.0);
        assert_eq!(volume_from_setting(Some("soft")), 0.22);
        assert_eq!(volume_from_setting(None), 0.45);
    }
}
