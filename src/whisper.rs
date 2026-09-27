//! Audio transcription using Whisper (whisper.cpp via whisper-rs).
//!
//! Models can be located in the local application data directory,
//! next to the binary, or in the sibling `cyber_transcript` project.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

/// Information about a supported or downloaded Whisper model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelInfo {
    pub name: &'static str,
    pub filename: &'static str,
    pub size_str: &'static str,
}

pub const AVAILABLE_MODELS: [ModelInfo; 5] = [
    ModelInfo {
        name: "Whisper Tiny",
        filename: "ggml-tiny.bin",
        size_str: "75 MB",
    },
    ModelInfo {
        name: "Whisper Base",
        filename: "ggml-base.bin",
        size_str: "142 MB",
    },
    ModelInfo {
        name: "Whisper Small (Recommended)",
        filename: "ggml-small.bin",
        size_str: "466 MB",
    },
    ModelInfo {
        name: "Whisper Medium",
        filename: "ggml-medium.bin",
        size_str: "1.5 GB",
    },
    ModelInfo {
        name: "Whisper Large v3",
        filename: "ggml-large-v3.bin",
        size_str: "3.1 GB",
    },
];

/// Collects directories to inspect for whisper ggml models.
pub fn model_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1. App standard directories
    let app_dirs = crate::paths::AppDirs::discover();
    dirs.push(app_dirs.state.join("models"));
    dirs.push(app_dirs.config.join("models"));
    dirs.push(app_dirs.cache.join("models"));

    if let Some(proj_dirs) = directories::ProjectDirs::from("me", "paolino", "zapfast") {
        dirs.push(proj_dirs.data_dir().join("models"));
        dirs.push(proj_dirs.config_dir().join("models"));
    }
    if let Some(proj_dirs) = directories::ProjectDirs::from("rocks", "zapfast", "ZapFast") {
        dirs.push(proj_dirs.data_dir().join("models"));
        dirs.push(proj_dirs.config_dir().join("models"));
    }

    // 2. Relative to current working dir / executable (portable packages)
    dirs.push(PathBuf::from("models"));
    dirs.push(PathBuf::from("bin/whisper"));

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("models"));
            dirs.push(parent.join("bin/whisper"));
            dirs.push(parent.join("../models"));
            dirs.push(parent.join("../../models"));
        }
    }

    // 3. User standard cache / whisper folders
    if let Some(home) = directories::UserDirs::new() {
        let home_dir = home.home_dir();
        dirs.push(home_dir.join(".cache").join("whisper"));
        dirs.push(home_dir.join(".local").join("share").join("whisper"));
        let codes = home_dir.join("Documents").join("codes");
        dirs.push(codes.join("cyber_transcript/src-tauri/bin/whisper"));
        dirs.push(codes.join("cyber_transcript/bin/whisper"));
        dirs.push(codes.join("cyber_transcript/cyber_transcript_egui/bin/whisper"));
    }

    // 4. Sibling cyber_transcript project directories
    dirs.push(PathBuf::from("../cyber_transcript/src-tauri/bin/whisper"));
    dirs.push(PathBuf::from("../cyber_transcript/bin/whisper"));
    dirs.push(PathBuf::from("../../cyber_transcript/src-tauri/bin/whisper"));
    dirs.push(PathBuf::from("../../cyber_transcript/bin/whisper"));

    // Deduplicate preserving order
    let mut unique = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        if let Ok(canonical) = dir.canonicalize() {
            if seen.insert(canonical.clone()) {
                unique.push(canonical);
            }
        } else if seen.insert(dir.clone()) {
            unique.push(dir);
        }
    }
    unique
}

/// Discovers an available whisper ggml model file on the local machine.
pub fn find_whisper_model(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(custom) = override_path {
        if custom.is_file() {
            return Some(custom.to_path_buf());
        }
    }

    let candidate_names = [
        "ggml-small.bin",
        "ggml-base.bin",
        "ggml-tiny.bin",
        "ggml-medium.bin",
        "ggml-large-v3.bin",
        "ggml-large.bin",
    ];

    let dirs = model_search_dirs();

    // First check for preferred known models
    for name in &candidate_names {
        for dir in &dirs {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // Otherwise check for any .bin file in the model search directories
    for dir in &dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"))
                {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if stem.starts_with("ggml") || stem.contains("whisper") {
                            return Some(path);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Returns a human-friendly name of the active Whisper model, if found.
pub fn active_model_display_name(override_path: Option<&Path>) -> Option<String> {
    let path = find_whisper_model(override_path)?;
    let filename = path.file_name()?.to_string_lossy();
    let is_custom = override_path.is_some();
    for info in &AVAILABLE_MODELS {
        if filename.eq_ignore_ascii_case(info.filename) {
            if is_custom {
                return Some(format!("Custom: {} ({})", info.name, info.size_str));
            } else {
                return Some(format!("{} ({})", info.name, info.size_str));
            }
        }
    }
    if is_custom {
        Some(format!("Custom: {filename}"))
    } else {
        Some(filename.to_string())
    }
}

/// Converts audio from any supported format to 16,000 Hz mono f32 samples.
///
/// Tries system `ffmpeg` first if available, and falls back to in-process
/// decoding (OGG/Opus via `crate::voice` or rodio decoders) followed by
/// resampling.
pub fn decode_and_resample_to_16k(audio_path: &Path) -> Result<Vec<f32>, String> {
    // 1. Try ffmpeg conversion to temporary 16kHz WAV
    if let Ok(samples) = try_ffmpeg_conversion(audio_path) {
        if !samples.is_empty() {
            return Ok(samples);
        }
    }

    // 2. In-process fallback
    decode_in_process(audio_path)
}

fn try_ffmpeg_conversion(audio_path: &Path) -> Result<Vec<f32>, String> {
    let unique_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let converted_wav = std::env::temp_dir().join(format!("zapfast_whisper_{unique_id}.wav"));

    let mut cmd = Command::new("ffmpeg");
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let status = cmd
        .args([
            "-y",
            "-i",
            &audio_path.to_string_lossy(),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            &converted_wav.to_string_lossy(),
        ])
        .status()
        .map_err(|e| e.to_string())?;

    if !status.success() || !converted_wav.exists() {
        let _ = fs::remove_file(&converted_wav);
        return Err("ffmpeg exited with error".to_string());
    }

    let read_result = (|| {
        let mut reader = hound::WavReader::open(&converted_wav).map_err(|e| e.to_string())?;
        let i16_samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap_or(0)).collect();
        if i16_samples.is_empty() {
            return Err("Empty samples from WAV".to_string());
        }

        let mut audio_f32 = vec![0.0f32; i16_samples.len()];
        whisper_rs::convert_integer_to_float_audio(&i16_samples, &mut audio_f32)
            .map_err(|e| e.to_string())?;

        Ok(audio_f32)
    })();

    let _ = fs::remove_file(&converted_wav);
    read_result
}

fn decode_in_process(audio_path: &Path) -> Result<Vec<f32>, String> {
    let bytes = fs::read(audio_path).map_err(|e| format!("Could not read audio file: {e}"))?;

    // WhatsApp voice notes are mono 48 kHz Opus in OGG
    if bytes.starts_with(b"OggS") {
        if let Ok(samples_48k) = crate::voice::decode(&bytes) {
            return Ok(resample_linear(&samples_48k, crate::voice::RATE, 16_000));
        }
    }

    // Try rodio decoder for MP3, WAV, AAC, etc.
    let file = fs::File::open(audio_path).map_err(|e| format!("Could not open audio: {e}"))?;
    let decoder = rodio::Decoder::new(std::io::BufReader::new(file))
        .map_err(|e| format!("Could not decode audio: {e}"))?;
    use rodio::Source;
    let channels = decoder.channels().get();
    let sample_rate = decoder.sample_rate().get();
    let interleaved: Vec<f32> = decoder.collect();

    let mono: Vec<f32> = if channels > 1 {
        interleaved
            .chunks_exact(usize::from(channels))
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        interleaved
    };

    Ok(resample_linear(&mono, sample_rate, 16_000))
}

/// Linear interpolation resampler from `from_rate` to `to_rate`.
pub fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || from_rate == 0 || to_rate == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = f64::from(from_rate) / f64::from(to_rate);
    let count = (samples.len() as f64 / ratio).floor() as usize;
    (0..count)
        .map(|index| {
            let pos = index as f64 * ratio;
            let left = pos.floor() as usize;
            let t = (pos - left as f64) as f32;
            let a = samples[left.min(samples.len() - 1)];
            let b = samples.get(left + 1).copied().unwrap_or(a);
            a + (b - a) * t
        })
        .collect()
}

/// Transcribes an audio file on disk using the detected Whisper model.
pub fn transcribe_file(
    audio_path: &Path,
    language: &str,
    model_override: Option<&Path>,
) -> Result<String, String> {
    let model_path = find_whisper_model(model_override).ok_or_else(|| {
        "No Whisper model found. Place a ggml model in the models folder or choose one in Settings.".to_string()
    })?;

    let audio_f32 = decode_and_resample_to_16k(audio_path)?;
    if audio_f32.is_empty() {
        return Err("Audio file contains no samples".to_string());
    }

    let ctx = whisper_rs::WhisperContext::new_with_params(
        &model_path,
        whisper_rs::WhisperContextParameters::default(),
    )
    .map_err(|e| format!("Failed to load Whisper model: {e}"))?;

    let mut state = ctx.create_state().map_err(|e| e.to_string())?;

    let mut params =
        whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    params.set_print_progress(false);
    params.set_print_special(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    let threads = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);
    params.set_n_threads(threads.clamp(1, 8));

    let lower_lang;
    if !language.is_empty() && language != "auto" {
        lower_lang = language.to_lowercase();
        let mapped = match lower_lang.as_str() {
            "portuguese" | "pt" | "pt-br" | "pt-pt" => "pt",
            "spanish" | "es" => "es",
            "english" | "en" => "en",
            "french" | "fr" => "fr",
            "german" | "de" => "de",
            "italian" | "it" => "it",
            other => other,
        };
        params.set_language(Some(mapped));
    }

    state.full(params, &audio_f32).map_err(|e| e.to_string())?;

    let mut text = String::new();
    for segment in state.as_iter() {
        text.push_str(&segment.to_string());
        text.push(' ');
    }

    clean_transcript(&text)
}

fn clean_transcript(raw: &str) -> Result<String, String> {
    let hallucinations = [
        "o que é isso",
        "o que e isso",
        "o que é isso?",
        "o que e isso?",
        "[silence]",
        "(silence)",
        "[music]",
        "(music)",
        "obrigado por assistir",
        "[música]",
        "(música)",
        "[musica]",
        "(musica)",
        "thank you for watching",
    ];

    let mut final_text = raw.trim().to_string();
    let lower_text = final_text.to_lowercase();

    for h in hallucinations.iter() {
        let stripped = lower_text
            .replace(h, "")
            .replace(' ', "")
            .replace('.', "")
            .replace(',', "")
            .replace('-', "");
        if stripped.is_empty() || (lower_text.starts_with(h) && stripped.len() < 5) {
            final_text = String::new();
            break;
        }
    }

    let tags = [
        "[música]", "(música)", "[musica]", "(musica)", "[music]", "(music)", "[silence]",
        "(silence)", "♪",
    ];
    for tag in tags {
        final_text = final_text.replace(tag, "");
    }

    let trimmed = final_text.trim();
    if trimmed.is_empty() {
        return Err("No speech detected in audio".to_string());
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_transcript_valid() {
        let raw = "  [music] Olá, tudo bem? ♪  ";
        let cleaned = clean_transcript(raw).expect("should clean successfully");
        assert_eq!(cleaned, "Olá, tudo bem?");
    }

    #[test]
    fn test_clean_transcript_hallucination() {
        let raw = "[silence]";
        assert!(clean_transcript(raw).is_err());

        let raw2 = "obrigado por assistir.";
        assert!(clean_transcript(raw2).is_err());
    }

    #[test]
    fn test_resample_linear_downsample() {
        let samples = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let resampled = resample_linear(&samples, 48_000, 16_000);
        assert_eq!(resampled.len(), 2);
        assert_eq!(resampled[0], 0.0);
        assert_eq!(resampled[1], 3.0);
    }

    #[test]
    fn test_find_whisper_model() {
        let model = find_whisper_model(None);
        assert!(model.is_some(), "Expected to discover ggml model in sibling directory");
    }
}

