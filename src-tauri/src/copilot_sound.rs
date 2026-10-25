//! aion-copilot: the alert sounds, made here rather than shipped as files —
//! nothing to license, nothing to lose on disk. Each preset is a few sine
//! partials under an envelope, rendered to a 16-bit mono WAV at one of eleven
//! volume steps and kept for good: Windows plays the bytes after the call
//! returns (`platform::copilot::play_wav`), so they must never be freed.

use std::sync::OnceLock;

pub const SAMPLE_RATE: u32 = 22_050;
/// Volume steps 0..=10 (0 is silence).
const STEPS: usize = 11;
/// The loudest sample, as a share of full scale: headroom for the mixer.
const PEAK: f32 = 0.9;
/// Fades at both ends, so no sound starts or stops with a click.
const FADE_IN_MS: f32 = 5.0;
const FADE_OUT_MS: f32 = 20.0;

struct Preset {
    name: &'static str,
    duration_ms: u32,
    /// The wave at `t` seconds, any scale: it is normalised after.
    wave: fn(f32) -> f32,
}

const PRESETS: [Preset; 4] = [
    Preset { name: "sino", duration_ms: 1400, wave: sino },
    Preset { name: "ping", duration_ms: 400, wave: ping },
    Preset { name: "alerta", duration_ms: 600, wave: alerta },
    Preset { name: "suave", duration_ms: 1100, wave: suave },
];

/// The preset names, for the settings page.
pub fn presets() -> impl Iterator<Item = &'static str> {
    PRESETS.iter().map(|p| p.name)
}

fn tone(freq: f32, t: f32) -> f32 {
    (std::f32::consts::TAU * freq * t).sin()
}

/// A bell: inharmonic partials, the high ones dying first.
fn sino(t: f32) -> f32 {
    const F: f32 = 880.0;
    // (ratio to the fundamental, amplitude, decay per second)
    const PARTIALS: [(f32, f32, f32); 4] = [(1.0, 1.0, 1.6), (2.0, 0.5, 2.4), (2.76, 0.35, 3.2), (5.4, 0.2, 5.0)];
    PARTIALS.iter().map(|&(r, a, k)| a * (-k * t).exp() * tone(F * r, t)).sum()
}

/// A short, bright tick.
fn ping(t: f32) -> f32 {
    const F: f32 = 1568.0;
    (-9.0 * t).exp() * tone(F, t) + 0.3 * (-14.0 * t).exp() * tone(2.0 * F, t)
}

/// Four quick beeps on two alternating notes, hard to miss.
fn alerta(t: f32) -> f32 {
    const BEEP: f32 = 0.11;
    const GAP: f32 = 0.04;
    const NOTES: [f32; 4] = [988.0, 1319.0, 988.0, 1319.0];
    let slot = (t / (BEEP + GAP)) as usize;
    let at = t - slot as f32 * (BEEP + GAP);
    let Some(&f) = NOTES.get(slot) else { return 0.0 };
    if at >= BEEP {
        return 0.0;
    }
    // Each beep ramps 5 ms in and out.
    let ramp = (at / 0.005).min((BEEP - at) / 0.005).min(1.0);
    ramp * (tone(f, t) + 0.3 * tone(3.0 * f, t))
}

/// A soft major chord that swells in and fades.
fn suave(t: f32) -> f32 {
    const CHORD: [f32; 3] = [523.25, 659.25, 783.99];
    let swell = (t / 0.12).min(1.0) * (-3.0 * t).exp();
    swell * CHORD.iter().map(|&f| tone(f, t)).sum::<f32>()
}

/// The volume step (0..=10) of a volume, clamped to 0..1 (NaN is silence).
fn step(volume: f64) -> usize {
    let v = if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.0 };
    (v * (STEPS - 1) as f64).round() as usize
}

fn preset_index(name: &str) -> Result<usize, String> {
    PRESETS.iter().position(|p| p.name == name).ok_or_else(|| format!("unknown sound: {name}"))
}

fn sample_count(p: &Preset) -> usize {
    (SAMPLE_RATE as u64 * p.duration_ms as u64 / 1000) as usize
}

fn samples(p: &Preset, gain: f32) -> Vec<i16> {
    let n = sample_count(p);
    let rate = SAMPLE_RATE as f32;
    let raw: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / rate;
            let fade = ((i as f32) / (FADE_IN_MS * rate / 1000.0)).min(((n - i) as f32) / (FADE_OUT_MS * rate / 1000.0)).min(1.0);
            (p.wave)(t) * fade
        })
        .collect();
    let peak = raw.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let scale = if peak > 0.0 { PEAK * gain / peak } else { 0.0 };
    raw.iter().map(|s| (s * scale * i16::MAX as f32).round() as i16).collect()
}

/// A RIFF/WAVE file: PCM, mono, 16-bit, `SAMPLE_RATE`.
fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn render_step(preset: usize, step: usize) -> Vec<u8> {
    wav(&samples(&PRESETS[preset], step as f32 / (STEPS - 1) as f32))
}

/// Preset `name` as a WAV at `volume` (0..1, clamped), rendered fresh.
pub fn render(name: &str, volume: f64) -> Result<Vec<u8>, String> {
    Ok(render_step(preset_index(name)?, step(volume)))
}

/// Every rendering, made on first use and never freed: at most 44, ~60 KB
/// each at the longest.
static CACHE: [[OnceLock<Vec<u8>>; STEPS]; PRESETS.len()] = [const { [const { OnceLock::new() }; STEPS] }; PRESETS.len()];

/// Preset `name` at `volume`, as bytes that live as long as the app.
pub fn cached(name: &str, volume: f64) -> Result<&'static [u8], String> {
    let (p, s) = (preset_index(name)?, step(volume));
    Ok(CACHE[p][s].get_or_init(|| render_step(p, s)).as_slice())
}

/// aion-copilot: play an alert sound ("sino", "ping", "alerta", "suave") at
/// `volume` (0..1). Returns at once; the sound plays on.
#[tauri::command]
pub fn copilot_play_sound(preset: String, volume: f64) -> Result<(), String> {
    preset_index(&preset)?;
    if step(volume) == 0 {
        return Ok(());
    }
    crate::platform::copilot::play_wav(cached(&preset, volume)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes([b[i], b[i + 1]])
    }
    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }
    fn pcm(b: &[u8]) -> Vec<i16> {
        b[44..].as_chunks::<2>().0.iter().map(|&c| i16::from_le_bytes(c)).collect()
    }

    #[test]
    fn every_preset_is_a_valid_mono_16_bit_wav_of_its_length() {
        for p in &PRESETS {
            let b = render(p.name, 1.0).unwrap();
            assert_eq!(&b[0..4], b"RIFF");
            assert_eq!(u32_at(&b, 4) as usize, b.len() - 8);
            assert_eq!(&b[8..16], b"WAVEfmt ");
            assert_eq!(u32_at(&b, 16), 16);
            assert_eq!((u16_at(&b, 20), u16_at(&b, 22)), (1, 1), "PCM, mono");
            assert_eq!(u32_at(&b, 24), SAMPLE_RATE);
            assert_eq!(u32_at(&b, 28), SAMPLE_RATE * 2);
            assert_eq!((u16_at(&b, 32), u16_at(&b, 34)), (2, 16));
            assert_eq!(&b[36..40], b"data");
            let samples = u32_at(&b, 40) as usize / 2;
            assert_eq!(samples, SAMPLE_RATE as usize * p.duration_ms as usize / 1000, "{}", p.name);
            assert_eq!(b.len(), 44 + samples * 2);
            assert!(p.duration_ms < 1500, "{} is short", p.name);
        }
    }

    #[test]
    fn full_volume_peaks_near_the_ceiling_and_never_clips() {
        for p in &PRESETS {
            let peak = pcm(&render(p.name, 1.0).unwrap()).iter().map(|s| s.unsigned_abs()).max().unwrap();
            let ceiling = (PEAK * i16::MAX as f32) as u16;
            assert!(peak > ceiling - 50 && peak <= ceiling + 1, "{}: {peak}", p.name);
        }
    }

    #[test]
    fn each_sound_starts_and_ends_at_silence() {
        for p in &PRESETS {
            let s = pcm(&render(p.name, 1.0).unwrap());
            assert_eq!(s[0], 0, "{}", p.name);
            assert!(s[s.len() - 1].unsigned_abs() < 200, "{}", p.name);
        }
    }

    #[test]
    fn volume_is_clamped_to_zero_one() {
        assert_eq!(step(1.0), 10);
        assert_eq!(step(0.0), 0);
        assert_eq!(step(0.5), 5);
        assert_eq!(step(0.04), 0);
        assert_eq!(step(7.0), 10);
        assert_eq!(step(-1.0), 0);
        assert_eq!(step(f64::NAN), 0);
        assert_eq!(step(f64::INFINITY), 0);
        assert_eq!(render("ping", 3.0).unwrap(), render("ping", 1.0).unwrap());
        assert!(pcm(&render("ping", -2.0).unwrap()).iter().all(|&s| s == 0));
        let half = pcm(&render("ping", 0.5).unwrap()).iter().map(|s| s.unsigned_abs()).max().unwrap();
        let full = pcm(&render("ping", 1.0).unwrap()).iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!((half as f32 / full as f32 - 0.5).abs() < 0.01);
    }

    #[test]
    fn an_unknown_preset_is_an_error() {
        assert!(render("buzina", 1.0).is_err());
        assert!(cached("", 0.5).is_err());
        assert!(copilot_play_sound("Sino".into(), 1.0).is_err(), "names are exact");
    }

    #[test]
    fn the_cache_hands_out_the_same_bytes_for_good() {
        let a = cached("sino", 0.7).unwrap();
        let b = cached("sino", 0.71).unwrap();
        assert!(std::ptr::eq(a, b), "same step, same buffer");
        assert_eq!(a, render("sino", 0.7).unwrap().as_slice());
        assert_eq!(presets().collect::<Vec<_>>(), ["sino", "ping", "alerta", "suave"]);
    }
}
