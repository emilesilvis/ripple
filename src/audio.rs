//! Offline WAV rendering and measurement. Live playback lives in tui.rs.

use crate::world::World;

/// Check samples before PCM conversion can conceal a NaN or saturation.
pub fn render_wav(
    mut world: World,
    sr: u32,
    path: &str,
    seconds: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    if sr == 0 || !seconds.is_finite() || seconds <= 0.0 {
        return Err("sample rate and duration must be finite and positive".into());
    }
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    let total = (seconds * sr as f32) as usize;
    let mut peak = 0.0f32;
    let mut sum_sq = 0.0f64;
    for sample in 0..total {
        let (l, r) = world.next_sample();
        if !l.is_finite() || !r.is_finite() {
            return Err(format!("non-finite audio at sample {sample} in {}", path).into());
        }
        peak = peak.max(l.abs()).max(r.abs());
        sum_sq += (l as f64 * l as f64 + r as f64 * r as f64) / 2.0;
        writer.write_sample((l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        writer.write_sample((r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    let rms = (sum_sq / total.max(1) as f64).sqrt();
    println!(
        "rendered {seconds}s to {}  |  peak: {peak:.3}  rms: {rms:.3} ({:.1} dBFS)",
        path,
        20.0 * rms.max(1e-9).log10()
    );
    if peak > 1.0 {
        return Err(format!("audio exceeded PCM range (peak {peak:.3}) in {}", path).into());
    }
    Ok(())
}
