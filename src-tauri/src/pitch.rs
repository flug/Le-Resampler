use rodio::{Decoder, Source};
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

// ── YIN pitch detection ────────────────────────────────────────────────────────

/// Detects the musical root note via YIN pitch detection (monophonic, fast).
/// Returns None for unpitched content, silent files, or formats rodio cannot decode.
pub fn detect_key_yin(path: &Path) -> Option<String> {
    let (mono, sr) = decode_mono(path)?;
    let freq = yin_pitch(&mono, sr, 0.12)?;
    Some(frequency_to_note(freq))
}

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
    // Descend to the bottom of that valley to avoid stopping on the slope.
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

// ── Krumhansl-Schmuckler key detection ────────────────────────────────────────

// Key profiles from Krumhansl & Schmuckler (1990), starting at C.
const MAJOR_PROFILE: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR_PROFILE: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Detects the musical key via Krumhansl-Schmuckler (harmonic analysis, polyphonic).
/// Returns note names like "C", "Am", "F#m" etc.
/// Returns None for silent, too-short, or undecodable files.
pub fn detect_key_ks(path: &Path) -> Option<String> {
    let (mono, sr) = decode_mono(path)?;
    let chroma = compute_chroma(&mono, sr);
    ks_key(&chroma)
}

/// Builds a 12-bin chroma vector from the audio via STFT.
fn compute_chroma(samples: &[f32], sample_rate: u32) -> [f32; 12] {
    const WINDOW: usize = 4096;
    const HOP: usize = WINDOW / 2;

    let hann: Vec<f32> = (0..WINDOW)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / (WINDOW - 1) as f32).cos()))
        .collect();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(WINDOW);
    let mut buf = vec![Complex::new(0.0f32, 0.0f32); WINDOW];

    let mut chroma = [0.0f32; 12];
    let mut frame_count = 0usize;
    let mut pos = 0;

    while pos + WINDOW <= samples.len() {
        for i in 0..WINDOW {
            buf[i] = Complex::new(samples[pos + i] * hann[i], 0.0);
        }
        fft.process(&mut buf);

        for i in 1..=WINDOW / 2 {
            let freq = i as f32 * sample_rate as f32 / WINDOW as f32;
            if freq < 27.5 || freq > 4186.0 {
                continue;
            }
            let midi = 12.0 * (freq / 440.0).log2() + 69.0;
            let pc = (midi.round() as i32).rem_euclid(12) as usize;
            chroma[pc] += buf[i].norm_sqr();
        }

        frame_count += 1;
        pos += HOP;
    }

    if frame_count > 0 {
        for c in &mut chroma {
            *c /= frame_count as f32;
        }
    }

    let sum: f32 = chroma.iter().sum();
    if sum > 1e-10 {
        for c in &mut chroma {
            *c /= sum;
        }
    }

    chroma
}

/// Returns the key with the highest Pearson correlation against the 24 K-S profiles.
pub(crate) fn ks_key(chroma: &[f32; 12]) -> Option<String> {
    if chroma.iter().all(|&x| x < 1e-10) {
        return None;
    }

    const NOTES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];

    let mut best_r = f32::NEG_INFINITY;
    let mut best_key = String::new();

    for root in 0..12usize {
        let mut major = [0.0f32; 12];
        let mut minor = [0.0f32; 12];
        for i in 0..12 {
            major[i] = MAJOR_PROFILE[(i + 12 - root) % 12];
            minor[i] = MINOR_PROFILE[(i + 12 - root) % 12];
        }

        let r_maj = pearson(chroma, &major);
        let r_min = pearson(chroma, &minor);

        if r_maj > best_r {
            best_r = r_maj;
            best_key = NOTES[root].to_string();
        }
        if r_min > best_r {
            best_r = r_min;
            best_key = format!("{}m", NOTES[root]);
        }
    }

    Some(best_key)
}

/// Pearson correlation coefficient between two 12-element arrays.
pub(crate) fn pearson(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let mean_a = a.iter().sum::<f32>() / 12.0;
    let mean_b = b.iter().sum::<f32>() / 12.0;
    let mut num = 0.0f32;
    let mut den_a = 0.0f32;
    let mut den_b = 0.0f32;
    for i in 0..12 {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        num += da * db;
        den_a += da * da;
        den_b += db * db;
    }
    let den = (den_a * den_b).sqrt();
    if den < 1e-10 {
        0.0
    } else {
        num / den
    }
}

// ── Shared helpers ─────────────────────────────────────────────────────────────

/// Maps a frequency in Hz to the nearest chromatic note name.
pub(crate) fn frequency_to_note(freq: f32) -> String {
    const NOTES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let midi = 12.0 * (freq / 440.0).log2() + 69.0;
    NOTES[(midi.round() as i32).rem_euclid(12) as usize].to_string()
}

/// Decodes audio at `path` to a mono f32 vector (up to 30 s).
fn decode_mono(path: &Path) -> Option<(Vec<f32>, u32)> {
    let file = File::open(path).ok()?;
    let decoder = Decoder::new(BufReader::new(file)).ok()?;
    let sr = decoder.sample_rate().get();
    let channels = decoder.channels().get() as usize;
    let all: Vec<f32> = decoder.take(sr as usize * 30 * channels).collect();
    let mono: Vec<f32> = all
        .chunks(channels)
        .map(|ch| ch.iter().sum::<f32>() / channels as f32)
        .collect();
    Some((mono, sr))
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_sine_wav(freq: f32, sample_rate: u32, duration_ms: u32, channels: u16) -> Vec<u8> {
        let num_frames = (sample_rate as f32 * duration_ms as f32 / 1000.0) as u32;
        let data_size = num_frames * channels as u32 * 2;
        let mut pcm = Vec::new();
        for frame in 0..num_frames {
            let t = frame as f32 / sample_rate as f32;
            let sample = ((2.0 * PI * freq * t).sin() * 32767.0) as i16;
            for _ in 0..channels {
                pcm.extend_from_slice(&sample.to_le_bytes());
            }
        }
        build_wav(pcm, sample_rate, channels, data_size)
    }

    /// Sum of sine waves (for chord tests).
    fn make_chord_wav(freqs: &[f32], sample_rate: u32, duration_ms: u32) -> Vec<u8> {
        let num_frames = (sample_rate as f32 * duration_ms as f32 / 1000.0) as u32;
        let data_size = num_frames * 2;
        let mut pcm = Vec::new();
        for frame in 0..num_frames {
            let t = frame as f32 / sample_rate as f32;
            let s: f32 =
                freqs.iter().map(|&f| (2.0 * PI * f * t).sin()).sum::<f32>() / freqs.len() as f32;
            pcm.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
        }
        build_wav(pcm, sample_rate, 1, data_size)
    }

    fn build_wav(pcm: Vec<u8>, sr: u32, ch: u16, data_size: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&ch.to_le_bytes());
        v.extend_from_slice(&sr.to_le_bytes());
        v.extend_from_slice(&(sr * 2 * ch as u32).to_le_bytes());
        v.extend_from_slice(&(2u16 * ch).to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend_from_slice(&pcm);
        v
    }

    fn silent_wav(frames: u32) -> Vec<u8> {
        let data_size = frames * 2;
        let pcm = vec![0u8; data_size as usize];
        build_wav(pcm, 44100, 1, data_size)
    }

    // ── frequency_to_note ──────────────────────────────────────────────────────

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

    // ── yin_pitch ──────────────────────────────────────────────────────────────

    #[test]
    fn yin_too_short_returns_none() {
        assert!(yin_pitch(&vec![0.0f32; 100], 44100, 0.12).is_none());
    }

    #[test]
    fn yin_silence_returns_none() {
        assert!(yin_pitch(&vec![0.0f32; 4096], 44100, 0.12).is_none());
    }

    #[test]
    fn yin_440hz_sine_detected() {
        let sr = 44100u32;
        let samples: Vec<f32> = (0..4096)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / sr as f32).sin())
            .collect();
        let freq = yin_pitch(&samples, sr, 0.12).unwrap();
        assert!(
            (freq - 440.0).abs() < 30.0,
            "got {freq} Hz, expected ~440 Hz"
        );
    }

    // ── detect_key_yin ─────────────────────────────────────────────────────────

    #[test]
    fn detect_key_yin_nonexistent_returns_none() {
        assert!(detect_key_yin(Path::new("/nonexistent/sample.wav")).is_none());
    }

    #[test]
    fn detect_key_yin_invalid_format_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("garbage.wav");
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(detect_key_yin(&path).is_none());
    }

    #[test]
    fn detect_key_yin_silence_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("silent.wav");
        std::fs::write(&path, silent_wav(22050)).unwrap();
        assert!(detect_key_yin(&path).is_none());
    }

    #[test]
    fn detect_key_yin_a440_mono() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a440_mono.wav");
        std::fs::write(&path, make_sine_wav(440.0, 44100, 500, 1)).unwrap();
        assert_eq!(detect_key_yin(&path), Some("A".to_string()));
    }

    #[test]
    fn detect_key_yin_a440_stereo() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a440_stereo.wav");
        std::fs::write(&path, make_sine_wav(440.0, 44100, 500, 2)).unwrap();
        assert_eq!(detect_key_yin(&path), Some("A".to_string()));
    }

    // ── pearson ────────────────────────────────────────────────────────────────

    #[test]
    fn pearson_identical_returns_one() {
        let a = MAJOR_PROFILE;
        assert!((pearson(&a, &a) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn pearson_constant_chroma_returns_zero() {
        // Zero variance in a → den < 1e-10 → returns 0
        let a = [1.0f32; 12];
        assert_eq!(pearson(&a, &MAJOR_PROFILE), 0.0);
    }

    // ── ks_key ─────────────────────────────────────────────────────────────────

    #[test]
    fn ks_key_silent_chroma_returns_none() {
        assert!(ks_key(&[0.0f32; 12]).is_none());
    }

    #[test]
    fn ks_key_c_major_triad_returns_c() {
        // C, E, G dominant → C major
        let mut chroma = [0.0f32; 12];
        chroma[0] = 1.0; // C
        chroma[4] = 0.8; // E
        chroma[7] = 0.9; // G
        assert_eq!(ks_key(&chroma), Some("C".to_string()));
    }

    #[test]
    fn ks_key_a_minor_triad_returns_am() {
        // A, C, E dominant → A minor
        let mut chroma = [0.0f32; 12];
        chroma[9] = 1.0; // A
        chroma[0] = 0.8; // C
        chroma[4] = 0.7; // E
        assert_eq!(ks_key(&chroma), Some("Am".to_string()));
    }

    // ── detect_key_ks ──────────────────────────────────────────────────────────

    #[test]
    fn detect_key_ks_nonexistent_returns_none() {
        assert!(detect_key_ks(Path::new("/nonexistent/sample.wav")).is_none());
    }

    #[test]
    fn detect_key_ks_invalid_format_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("garbage.wav");
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(detect_key_ks(&path).is_none());
    }

    #[test]
    fn detect_key_ks_silence_returns_none() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("silent.wav");
        std::fs::write(&path, silent_wav(22050)).unwrap();
        assert!(detect_key_ks(&path).is_none());
    }

    #[test]
    fn detect_key_ks_too_short_returns_none() {
        // 100 frames < FFT window (4096) → empty chroma → None
        let dir = tempdir().unwrap();
        let path = dir.path().join("short.wav");
        std::fs::write(&path, silent_wav(100)).unwrap();
        assert!(detect_key_ks(&path).is_none());
    }

    #[test]
    fn detect_key_ks_c_major_chord() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("c_major.wav");
        // C4 + E4 + G4
        std::fs::write(&path, make_chord_wav(&[261.63, 329.63, 392.0], 44100, 2000)).unwrap();
        assert_eq!(detect_key_ks(&path), Some("C".to_string()));
    }

    #[test]
    fn detect_key_ks_a_minor_chord() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a_minor.wav");
        // A4 + C5 + E5
        std::fs::write(&path, make_chord_wav(&[440.0, 523.25, 659.25], 44100, 2000)).unwrap();
        assert_eq!(detect_key_ks(&path), Some("Am".to_string()));
    }
}
