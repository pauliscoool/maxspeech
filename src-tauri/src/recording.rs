//! Local Remake cache: up to 10 recent dictation WAVs on disk (never a cloud archive).

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAX_REMAKE_RECORDINGS: usize = 10;
pub const WAV_SAMPLE_RATE: u32 = 16_000;

pub fn recordings_dir() -> PathBuf {
    PathBuf::from(crate::store::data_dir()).join("recordings")
}

pub fn wav_path_for_history_id(id: i64) -> PathBuf {
    recordings_dir().join(format!("{id}.wav"))
}

/// Write mono PCM i16 as a standard WAV (16-bit, little-endian).
pub fn write_wav_i16(path: &Path, samples: &[i16], sample_rate: u32) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = File::create(path)?;
    let data_bytes = samples.len() * 2;
    let file_size = 36 + data_bytes as u32;
    // RIFF header
    f.write_all(b"RIFF")?;
    f.write_all(&file_size.to_le_bytes())?;
    f.write_all(b"WAVE")?;
    // fmt chunk
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // PCM chunk size
    f.write_all(&1u16.to_le_bytes())?; // audio format = PCM
    f.write_all(&1u16.to_le_bytes())?; // channels
    f.write_all(&sample_rate.to_le_bytes())?;
    let byte_rate = sample_rate * 2;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits per sample
    // data chunk
    f.write_all(b"data")?;
    f.write_all(&(data_bytes as u32).to_le_bytes())?;
    for &s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}

pub fn delete_recording_file(path: &str) {
    if path.is_empty() {
        return;
    }
    let _ = std::fs::remove_file(path);
}
