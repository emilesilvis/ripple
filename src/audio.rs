//! Turning the world into sound you can hear or save: live playback through the
//! default output device, or an offline render to a WAV file.

use crate::world::World;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub fn render_wav(mut world: World, sr: u32, path: &str, seconds: f32) -> Result<(), Box<dyn std::error::Error>> {
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
    for _ in 0..total {
        let (l, r) = world.next_sample();
        peak = peak.max(l.abs()).max(r.abs());
        sum_sq += (l as f64 * l as f64 + r as f64 * r as f64) / 2.0;
        writer.write_sample((l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        writer.write_sample((r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    let rms = (sum_sq / total.max(1) as f64).sqrt();
    println!(
        "rendered {seconds}s to {path}  |  peak: {peak:.3}  rms: {rms:.3} ({:.1} dBFS)",
        20.0 * rms.max(1e-9).log10()
    );
    Ok(())
}

pub fn run_live(build: impl Fn(f32) -> World, desc: &str) -> Result<(), Box<dyn std::error::Error>> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no default audio output device found")?;
    let config = device.default_output_config()?;
    let sr = config.sample_rate().0 as f32;
    let channels = config.channels() as usize;

    println!(
        "ripple — a world you can hear\n  device: {}\n  sample rate: {} Hz\n\n  {}\n\npress Ctrl+C to stop.",
        device.name().unwrap_or_else(|_| "unknown".into()),
        sr as u32,
        desc,
    );

    let mut world = build(sr);
    let err_fn = |err| eprintln!("audio stream error: {err}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _| {
                for frame in data.chunks_mut(channels) {
                    let (l, r) = world.next_sample();
                    frame[0] = l;
                    if channels > 1 {
                        frame[1] = r;
                    }
                    for extra in frame.iter_mut().skip(2) {
                        *extra = 0.0;
                    }
                }
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            &config.into(),
            move |data: &mut [i16], _| {
                for frame in data.chunks_mut(channels) {
                    let (l, r) = world.next_sample();
                    frame[0] = (l * i16::MAX as f32) as i16;
                    if channels > 1 {
                        frame[1] = (r * i16::MAX as f32) as i16;
                    }
                    for extra in frame.iter_mut().skip(2) {
                        *extra = 0;
                    }
                }
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_output_stream(
            &config.into(),
            move |data: &mut [u16], _| {
                for frame in data.chunks_mut(channels) {
                    let (l, r) = world.next_sample();
                    let conv = |s: f32| ((s * 0.5 + 0.5) * u16::MAX as f32) as u16;
                    frame[0] = conv(l);
                    if channels > 1 {
                        frame[1] = conv(r);
                    }
                    for extra in frame.iter_mut().skip(2) {
                        *extra = conv(0.0);
                    }
                }
            },
            err_fn,
            None,
        )?,
        other => return Err(format!("unsupported sample format: {other}").into()),
    };

    stream.play()?;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
