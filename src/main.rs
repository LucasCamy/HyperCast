#![cfg_attr(
    all(windows, feature = "desktop", not(debug_assertions)),
    windows_subsystem = "windows"
)]
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(about = "HyperCast: GPU capture, NVENC publishing and selective receive diagnostics")]
struct Args {
    #[cfg(feature = "desktop")]
    #[arg(long)]
    gui: bool,
    #[arg(long)]
    list: bool,
    #[cfg(feature = "livekit-network")]
    #[arg(long)]
    connect_url: Option<String>,
    #[cfg(feature = "livekit-network")]
    #[arg(long, conflicts_with_all = ["connect_url", "receive_url"])]
    admission_probe_url: Option<String>,
    #[cfg(feature = "livekit-network")]
    #[arg(long, requires = "admission_probe_url")]
    reject_first: bool,
    #[cfg(feature = "livekit-network")]
    #[arg(long, conflicts_with = "connect_url")]
    receive_url: Option<String>,
    #[cfg(feature = "livekit-network")]
    #[arg(long, value_enum, default_value = "spotlight")]
    view_mode: hypercast::subscription::ViewMode,
    #[cfg(feature = "livekit-network")]
    #[arg(long, requires = "receive_url")]
    focus: Option<String>,
    #[cfg(feature = "livekit-network")]
    #[arg(long, requires = "receive_url", value_parser = clap::value_parser!(u32).range(1..=3600))]
    pause_after: Option<u32>,
    #[cfg(all(windows, feature = "nvenc"))]
    #[arg(long)]
    encode: bool,
    #[cfg(all(windows, feature = "nvenc"))]
    #[arg(long, value_enum, default_value = "h265")]
    codec: hypercast::encoder::Codec,
    #[cfg(all(windows, feature = "nvenc"))]
    #[arg(long, default_value_t = 40_000_000, value_parser = clap::value_parser!(u32).range(1_000_000..=45_000_000))]
    bitrate: u32,
    #[cfg(all(windows, feature = "nvenc"))]
    #[arg(long, requires = "encode")]
    dump: Option<std::path::PathBuf>,
    #[cfg(all(windows, feature = "nvenc", feature = "livekit-network"))]
    #[arg(long, requires = "encode", conflicts_with_all = ["connect_url", "receive_url", "admission_probe_url"])]
    publish_url: Option<String>,
    #[arg(long, default_value_t = 0)]
    adapter: u32,
    #[arg(long, default_value_t = 0)]
    output: u32,
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u32).range(1..=240))]
    fps: u32,
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=3600))]
    seconds: u32,
}

#[cfg(windows)]
fn main() -> Result<()> {
    use hypercast::{
        pacing::FramePacer,
        screen_capture::{CaptureConfig, ScreenCapture},
    };
    use std::time::{Duration, Instant};
    let args = Args::parse();
    #[cfg(feature = "desktop")]
    if args.gui || std::env::args().len() == 1 {
        return hypercast::desktop::run();
    }
    #[cfg(feature = "livekit-network")]
    let runtime = tokio::runtime::Runtime::new()?;
    #[cfg(feature = "livekit-network")]
    if let Some(address) = &args.admission_probe_url {
        return runtime.block_on(hypercast::network::admission_probe(
            address,
            &hypercast::network::token_from_env()?,
            args.reject_first,
        ));
    }
    #[cfg(feature = "livekit-network")]
    if let Some(address) = &args.receive_url {
        let frames = runtime.block_on(hypercast::receiver::receive(
            address,
            &hypercast::network::token_from_env()?,
            hypercast::receiver::ReceiveOptions {
                mode: args.view_mode,
                focus_identity: args.focus,
                pause_after: args
                    .pause_after
                    .map(|seconds| Duration::from_secs(seconds as u64)),
                duration: Duration::from_secs(args.seconds as u64),
            },
        ))?;
        anyhow::ensure!(
            frames > 0,
            "No screen frames received during the diagnostic window"
        );
        return Ok(());
    }
    #[cfg(feature = "livekit-network")]
    if let Some(address) = &args.connect_url {
        return runtime.block_on(hypercast::network::diagnostic(
            address,
            &hypercast::network::token_from_env()?,
            args.seconds,
        ));
    }
    if args.list {
        for display in hypercast::screen_capture::displays()? {
            println!(
                "GPU {} / output {}: {} — {} ({}x{})",
                display.adapter_index,
                display.output_index,
                display.gpu_name,
                display.display_name,
                display.width,
                display.height
            );
        }
        return Ok(());
    }
    let mut capture = ScreenCapture::new(CaptureConfig {
        adapter_index: args.adapter,
        output_index: args.output,
    })?;
    let mut pacer = FramePacer::new(args.fps)?;
    #[cfg(feature = "nvenc")]
    let device = capture.device().clone();
    #[cfg(feature = "nvenc")]
    let (capture_width, capture_height) = capture.dimensions();
    #[cfg(feature = "nvenc")]
    let mut encoder = if args.encode {
        Some(hypercast::encoder::HardwareEncoder::new(
            &device,
            capture_width,
            capture_height,
            args.fps,
            args.bitrate,
            args.codec,
        )?)
    } else {
        None
    };
    #[cfg(feature = "nvenc")]
    let mut dump = args
        .dump
        .as_ref()
        .map(|path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
        })
        .transpose()?;
    #[cfg(feature = "nvenc")]
    let (mut encoded_frames, mut encoded_bytes, mut encode_timings) = (0u64, 0u64, Vec::new());
    #[cfg(all(feature = "nvenc", feature = "livekit-network"))]
    let mut publisher = if let Some(address) = &args.publish_url {
        Some(runtime.block_on(hypercast::network::EncodedPublisher::open(
            address,
            &hypercast::network::token_from_env()?,
            capture_width,
            capture_height,
            args.fps,
            args.bitrate,
            matches!(args.codec, hypercast::encoder::Codec::Av1),
        ))?)
    } else {
        None
    };
    #[cfg(all(feature = "nvenc", feature = "livekit-network"))]
    let mut network_paused = false;
    let start = Instant::now();
    let mut timings = Vec::new();
    let mut updates = 0u64;
    let mut idle = 0u64;
    let mut dimensions = None;
    while start.elapsed() < Duration::from_secs(args.seconds as u64) {
        pacer.wait()?;
        let frame = capture.acquire()?;
        if let Some(frame) = &frame {
            let mut desc = Default::default();
            // SAFETY: texture lives until frame.release() below.
            unsafe {
                frame.texture().GetDesc(&mut desc);
            }
            dimensions = Some((desc.Width, desc.Height));
            timings.push(frame.acquire_duration.as_secs_f64() * 1000.0);
            if frame.info.LastPresentTime != 0 {
                updates += 1;
            }
        } else {
            idle += 1;
        }
        #[cfg(feature = "nvenc")]
        if args.encode && encoded_frames == 0 && frame.is_none() {
            continue;
        }
        #[cfg(feature = "nvenc")]
        if let Some(encoder) = &mut encoder {
            let force_keyframe = encoded_frames == 0;
            let encode_now = true;
            #[cfg(feature = "livekit-network")]
            let (force_keyframe, encode_now) = {
                let mut force_keyframe = force_keyframe;
                let mut encode_now = encode_now;
                if let Some(publisher) = &publisher {
                    force_keyframe |= publisher.take_keyframe_request();
                    if let Some(target) = publisher.take_rate_control_request() {
                        network_paused = target.target_bitrate_bps == 0;
                        if !network_paused {
                            let fps = (target.framerate_fps as u32).clamp(1, args.fps);
                            let bitrate =
                                target.target_bitrate_bps.clamp(64_000, args.bitrate as u64) as u32;
                            encoder.reconfigure(bitrate, fps)?;
                            pacer.set_fps(fps)?;
                            force_keyframe = true;
                        }
                    }
                    encode_now = !network_paused;
                }
                (force_keyframe, encode_now)
            };
            if encode_now {
                let encode_start = Instant::now();
                // None reuses the last OWN GPU texture, never a released DXGI surface.
                let packet = encoder.encode(
                    frame.as_ref(),
                    start.elapsed().as_micros() as i64,
                    force_keyframe,
                )?;
                encode_timings.push(encode_start.elapsed().as_secs_f64() * 1000.0);
                encoded_frames += 1;
                encoded_bytes += packet.bytes.len() as u64;
                if let Some(dump) = &mut dump {
                    use std::io::Write;
                    dump.write_all(packet.bytes)?;
                }
                #[cfg(feature = "livekit-network")]
                if let Some(publisher) = &mut publisher {
                    if !publisher.push(packet.bytes, packet.timestamp_us, packet.keyframe)? {
                        anyhow::bail!(
                            "WebRTC rejected an encoded frame; reconnect and force a keyframe"
                        );
                    }
                }
            }
        }
        if let Some(frame) = frame {
            frame.release()?;
        }
    }
    timings.sort_by(f64::total_cmp);
    let percentile = |p: f64| {
        timings
            .get(((timings.len().saturating_sub(1)) as f64 * p).round() as usize)
            .copied()
            .unwrap_or(0.0)
    };
    println!(
        "Capture: {:?}, target {} Hz, elapsed {:.2}s",
        dimensions,
        args.fps,
        start.elapsed().as_secs_f64()
    );
    println!(
        "Acquired {}, desktop updates {}, idle polls {}",
        timings.len(),
        updates,
        idle
    );
    println!(
        "AcquireNextFrame p50 {:.3} ms / p95 {:.3} ms / p99 {:.3} ms",
        percentile(0.5),
        percentile(0.95),
        percentile(0.99)
    );
    println!("DXGI emits updates only; static desktops do not generate 120 new images/s.");
    #[cfg(feature = "nvenc")]
    if encoded_frames > 0 {
        println!(
            "NVENC measurement excludes encoder/room setup; includes pauses and congestion adaptation."
        );
        encode_timings.sort_by(f64::total_cmp);
        println!(
            "NVENC: {} frames, {:.2} FPS, {:.2} Mbps, copy+encode p95 {:.3} ms",
            encoded_frames,
            encoded_frames as f64 / start.elapsed().as_secs_f64(),
            encoded_bytes as f64 * 8.0 / start.elapsed().as_secs_f64() / 1_000_000.0,
            encode_timings[((encode_timings.len() - 1) as f64 * 0.95).round() as usize]
        );
    }
    #[cfg(all(feature = "nvenc", feature = "livekit-network"))]
    if let Some(publisher) = publisher {
        runtime.block_on(publisher.close())?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() -> Result<()> {
    let _ = Args::parse();
    anyhow::bail!("DXGI capture requires Windows 10/11 and a hardware D3D11 adapter")
}
