use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

pub struct AudioCapture {
    stream: Option<cpal::Stream>,
    pub sample_rate: u32,
}

unsafe impl Send for AudioCapture {}
unsafe impl Sync for AudioCapture {}

impl AudioCapture {
    pub fn new() -> Self {
        Self { stream: None, sample_rate: 16000 }
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
        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(16000),
            buffer_size: cpal::BufferSize::Default,
        };
        log::info!("Audio: using device {:?}", device.name());

        let chunk_size = 1600; // 100ms at 16kHz
        let buffer: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(chunk_size)));
        let buffer_clone = buffer.clone();

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let samples: Vec<i16> = data.iter().map(|&s| (s * 32767.0) as i16).collect();
                let rms: f32 = (samples.iter().map(|&s| (s as f32).powi(2)).sum::<f32>()
                    / samples.len() as f32)
                    .sqrt()
                    / 32767.0;
                let _ = app.emit("audio-level", vec![rms; 16]);

                let mut buf = buffer_clone.lock().unwrap();
                buf.extend_from_slice(&samples);
                if buf.len() >= chunk_size {
                    let chunk: Vec<i16> = buf.drain(..).collect();
                    let _ = sender.send(chunk);
                }
            },
            |err| log::error!("Audio stream error: {err}"),
            None,
        )?;
        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.stream = None;
    }
}

pub fn test_microphone(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No input device available")?;
    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(16000),
        buffer_size: cpal::BufferSize::Default,
    };
    let received = Arc::new(Mutex::new(false));
    let received_clone = received.clone();
    let _app = app.clone();
    let stream = device.build_input_stream(
        &config,
        move |_data: &[f32], _: &cpal::InputCallbackInfo| {
            *received_clone.lock().unwrap() = true;
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
        Err("No audio data received".into())
    }
}
