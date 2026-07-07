use rodio::{Decoder, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Detects the musical root note of a tonal one-shot via YIN pitch detection.
/// Returns None for unpitched content, silent files, or formats rodio cannot decode.
pub fn detect_key(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let decoder = Decoder::new(BufReader::new(file)).ok()?;
    let sample_rate = decoder.sample_rate().get();
    let channels = decoder.channels().get() as usize;

    let all: Vec<f32> = decoder.take(sample_rate as usize * 2 * channels).collect();

    let mono: Vec<f32> = all
        .chunks(channels)
        .map(|ch| ch.iter().sum::<f32>() / channels as f32)
        .collect();

    let freq = yin_pitch(&mono, sample_rate, 0.12)?;
    Some(frequency_to_note(freq))
}

/// YIN pitch detection algorithm. Returns the fundamental frequency in Hz.
/// Analysis window capped at 4096 samples for performance (~93 ms at 44100 Hz).
pub(crate) fn yin_pitch(samples: &[f32], sample_rate: u32, threshold: f32) -> Option<f32> {
    const WINDOW: usize = 2048;
    let n = samples.len().min(WINDOW * 2);
    if n < WINDOW {
        return None;
    }
    let half = n / 2;
    let samples = &samples[..n];

    // Difference function d(τ)
    let mut d = vec![0.0f32; half];
    for tau in 1..half {
        for j in 0..half {
            let diff = samples[j] - samples[j + tau];
            d[tau] += diff * diff;
        }
    }

    // Cumulative mean normalised difference d'(τ)
    let mut d_prime = vec![0.0f32; half];
    d_prime[0] = 1.0;
    let mut running_sum = 0.0f32;
    for tau in 1..half {
        running_sum += d[tau];
        d_prime[tau] = if running_sum > 1e-10 {
            d[tau] * tau as f32 / running_sum
        } else {
            1.0
        };
    }

    // First valley below threshold in the musically valid range (~40–1200 Hz).
    // Once a τ below the threshold is found, we descend to the bottom of that
    // local valley rather than stopping immediately.  This avoids stopping on the
    // slope of a valley when the true minimum is a few samples further on.
    let tau_min = (sample_rate as f32 / 1200.0).ceil() as usize;
    let tau_max = ((sample_rate as f32 / 40.0).ceil() as usize).min(half - 2);

    for tau in tau_min..=tau_max {
        if d_prime[tau] < threshold {
            let mut t = tau;
            while t + 1 <= tau_max && d_prime[t + 1] < d_prime[t] {
                t += 1;
            }
            return Some(sample_rate as f32 / t as f32);
        }
    }

    None
}

/// Maps a frequency in Hz to the nearest chromatic note name.
pub(crate) fn frequency_to_note(freq: f32) -> String {
    const NOTES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let midi = 12.0 * (freq / 440.0).log2() + 69.0;
    NOTES[(midi.round() as i32).rem_euclid(12) as usize].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// WAV with a pure sine wave. `channels` = 1 (mono) or 2 (stereo).
    fn make_sine_wav(freq: f32, sample_rate: u32, duration_ms: u32, channels: u16) -> Vec<u8> {
        let num_frames = (sample_rate as f32 * duration_ms as f32 / 1000.0) as u32;
        let data_size = num_frames * channels as u32 * 2;
        let mut pcm = Vec::new();
        for frame in 0..num_frames {
            let t = frame as f32 / sample_rate as f32;
            let sample = ((2.0 * std::f32::consts::PI * freq * t).sin() * 32767.0) as i16;
            for _ in 0..channels {
                pcm.extend_from_slice(&sample.to_le_bytes());
            }
        }
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&sample_rate.to_le_bytes());
        v.extend_from_slice(&(sample_rate * 2 * channels as u32).to_le_bytes());
        v.extend_from_slice(&(2u16 * channels).to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend_from_slice(&pcm);
        v
    }

    // --- frequency_to_note ---

    #[test]
    fn note_a4() {
        assert_eq!(frequency_to_note(440.0), "A");
    }

    #[test]
    fn note_c4() {
        assert_eq!(frequency_to_note(261.63), "C");
    }

    #[test]
    fn note_a_sharp() {
        assert_eq!(frequency_to_note(466.16), "A#");
    }

    // --- yin_pitch ---

    #[test]
    fn yin_too_short_returns_none() {
        assert!(yin_pitch(&vec![0.0f32; 100], 44100, 0.12).is_none());
    }

    #[test]
    fn yin_silence_returns_none() {
        // Silent signal: all d[τ]=0, running_sum=0, d_prime set to 1.0 → no valley below threshold
        assert!(yin_pitch(&vec![0.0f32; 4096], 44100, 0.12).is_none());
    }

    #[test]
    fn yin_440hz_sine_detected() {
        let sr = 44100u32;
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr as f32).sin())
            .collect();
        let freq = yin_pitch(&samples, sr, 0.12).unwrap();
        // Accept ±30 Hz tolerance (< 1 semitone at 440 Hz)
        assert!(
            (freq - 440.0).abs() < 30.0,
            "got {freq} Hz, expected ~440 Hz"
        );
    }

    // --- detect_key ---

    #[test]
    fn detect_key_nonexistent_returns_none() {
        assert!(detect_key(Path::new("/nonexistent/sample.wav")).is_none());
    }

    #[test]
    fn detect_key_invalid_format_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("garbage.wav");
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(detect_key(&path).is_none());
    }

    #[test]
    fn detect_key_silence_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("silent.wav");
        let n = 22050u32; // 500 ms at 44100 Hz
        let data_size = n * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&44100u32.to_le_bytes());
        wav.extend_from_slice(&88200u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        wav.extend(std::iter::repeat_n(0u8, data_size as usize));
        std::fs::write(&path, wav).unwrap();
        assert!(detect_key(&path).is_none());
    }

    #[test]
    fn detect_key_a440_mono() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a440_mono.wav");
        std::fs::write(&path, make_sine_wav(440.0, 44100, 500, 1)).unwrap();
        assert_eq!(detect_key(&path), Some("A".to_string()));
    }

    #[test]
    fn detect_key_a440_stereo() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a440_stereo.wav");
        std::fs::write(&path, make_sine_wav(440.0, 44100, 500, 2)).unwrap();
        assert_eq!(detect_key(&path), Some("A".to_string()));
    }
}
