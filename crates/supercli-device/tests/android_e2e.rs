// crates/supercli-device/tests/android_e2e.rs
//! Headless Android emulator end-to-end proof for the native scrcpy client.
//!
//! Runs ONLY when `SUPERCLI_ANDROID_E2E=1` is set (requires a real emulator
//! with `adb` on PATH). Without the env var the test passes trivially with
//! a skip note, so plain `cargo test` stays green.
//!
//! What it proves:
//! 1. `ScrcpyNative::connect` against a real emulator (jar deploy + SHA-256,
//!    port forward, server start, H.264 header parse).
//! 2. >= 600 real H.264 packets read; fps measured over 60 s.
//! 3. Tap-to-next-frame latency p50/p95: native control channel vs
//!    `adb shell input` (20 trials each).
//! 4. A two-pointer pinch completes without error and the stream survives.
//!
//! Methodology note (honest): "tap-to-frame" here = time from tap injection
//! to the arrival of the next encoded H.264 packet. At ~60 fps a packet
//! arrives roughly every 16 ms regardless, so this metric = injection cost
//! + time-to-next-frame-boundary. The *comparison* between the two injection
//! paths is the point, not the absolute number.

#![cfg(feature = "device")]

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use supercli_device::scrcpy_native::{
    AndroidKeycode, ScrcpyNative, TouchAction, fallback_adb_input,
};
use supercli_device::{DeviceError, DeviceId};

const PACKET_TARGET: usize = 600;
const FPS_WINDOW: Duration = Duration::from_secs(60);
const TAP_TRIALS: usize = 20;

fn percentile(mut xs: Vec<f64>, p: f64) -> f64 {
    assert!(!xs.is_empty(), "percentile of empty sample");
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((p / 100.0) * (xs.len() as f64 - 1.0)).round() as usize;
    xs[idx.min(xs.len() - 1)]
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

struct Metrics {
    kvm_present: bool,
    device: String,
    video_w: u32,
    video_h: u32,
    packets_read: usize,
    keyframes: usize,
    fps: f64,
    control_p50_ms: f64,
    control_p95_ms: f64,
    control_mean_ms: f64,
    adb_p50_ms: f64,
    adb_p95_ms: f64,
    adb_mean_ms: f64,
    pinch_ok: bool,
    home_key_ok: bool,
}

impl Metrics {
    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"kvm_present\": {},\n",
                "  \"device\": \"{}\",\n",
                "  \"video_width\": {},\n",
                "  \"video_height\": {},\n",
                "  \"packets_read\": {},\n",
                "  \"keyframes\": {},\n",
                "  \"fps_60s\": {:.2},\n",
                "  \"control_tap_to_frame_ms\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"mean\": {:.2}, \"n\": {}}},\n",
                "  \"adb_tap_to_frame_ms\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"mean\": {:.2}, \"n\": {}}},\n",
                "  \"pinch_ok\": {},\n",
                "  \"home_key_ok\": {}\n",
                "}}"
            ),
            self.kvm_present,
            self.device,
            self.video_w,
            self.video_h,
            self.packets_read,
            self.keyframes,
            self.fps,
            self.control_p50_ms,
            self.control_p95_ms,
            self.control_mean_ms,
            TAP_TRIALS,
            self.adb_p50_ms,
            self.adb_p95_ms,
            self.adb_mean_ms,
            TAP_TRIALS,
            self.pinch_ok,
            self.home_key_ok,
        )
    }
}

/// Time from tap injection to the arrival of the next H.264 packet.
fn tap_latency_control(
    client: &mut ScrcpyNative,
    x: u32,
    y: u32,
) -> Result<Duration, DeviceError> {
    let t0 = Instant::now();
    client.tap(x, y)?;
    // Fresh reader: the previous one consumed whole packets only, so we are
    // at a packet boundary (VideoStreamReader does no buffering).
    let mut iter = client.start_video_stream();
    match iter.next() {
        Some(Ok(_pkt)) => Ok(t0.elapsed()),
        Some(Err(e)) => Err(e),
        None => Err(DeviceError::Parse(
            "video stream ended during latency trial".to_string(),
        )),
    }
}

fn tap_latency_adb(serial: &str, x: u32, y: u32) -> Result<Duration, DeviceError> {
    let xs = x.to_string();
    let ys = y.to_string();
    let t0 = Instant::now();
    fallback_adb_input(&DeviceId::new(serial), &["tap", &xs, &ys])?;
    // NOTE: the adb path has no handle on the video socket; the caller
    // measures against its own stream. This helper only times the injection.
    Ok(t0.elapsed())
}

/// Wait for the emulator to finish booting, polling
/// `sys.boot_completed`. The CI script also waits, but the test must not
/// trust its invoker: a panic here used to be indistinguishable from a
/// scrcpy failure (CI run #2 died in ~2 min total, i.e. suspiciously fast
/// for boot + 60 s of video).
fn wait_for_boot(serial: &str) -> Result<(), DeviceError> {
    use std::process::Command;
    eprintln!("e2e: stage=boot_wait_start");
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let out = Command::new("adb")
            .args(["-s", serial, "shell", "getprop", "sys.boot_completed"])
            .output()
            .map_err(DeviceError::Io)?;
        let prop = String::from_utf8_lossy(&out.stdout);
        if prop.trim() == "1" {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(DeviceError::Timeout {
                tool: format!("adb shell getprop sys.boot_completed on {serial}"),
            });
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn run_e2e(serial: &str) -> Result<Metrics, DeviceError> {
    let kvm_present = Path::new("/dev/kvm").exists();
    eprintln!("e2e: /dev/kvm present = {kvm_present}");

    wait_for_boot(serial)?;
    eprintln!("e2e: stage=boot_completed serial={serial}");

    eprintln!("e2e: connecting to {serial} ...");
    let mut client = ScrcpyNative::connect(serial)?;
    let (vw, vh) = client.video_size();
    eprintln!(
        "e2e: connected to '{}' ({}x{})",
        client.device_name(),
        vw,
        vh
    );

    // Deterministic starting point: Home.
    let home_key_ok = client.press_key(AndroidKeycode::Home).is_ok();
    eprintln!("e2e: HOME key ok = {home_key_ok}");

    let cx = vw / 2;
    let cy = vh / 2;

    // --- Phase 1: fps over 60 s, >= 600 packets ---------------------------
    eprintln!("e2e: reading H.264 packets for 60 s ...");
    let mut packets_read = 0usize;
    let mut keyframes = 0usize;
    let start = Instant::now();
    {
        let mut iter = client.start_video_stream();
        while start.elapsed() < FPS_WINDOW {
            match iter.next() {
                Some(Ok(pkt)) => {
                    packets_read += 1;
                    if packets_read == 1 {
                        eprintln!("e2e: stage=first_packet");
                    }
                    if packets_read.is_multiple_of(100) {
                        eprintln!("e2e: stage=packets n={packets_read}");
                    }
                    if pkt.is_keyframe {
                        keyframes += 1;
                    }
                }
                Some(Err(e)) => return Err(e),
                None => {
                    return Err(DeviceError::Parse(
                        "video stream ended during fps window".to_string(),
                    ))
                }
            }
        }
    }
    let elapsed_s = start.elapsed().as_secs_f64();
    let fps = packets_read as f64 / elapsed_s;
    eprintln!("e2e: {packets_read} packets ({keyframes} keyframes) in {elapsed_s:.1}s = {fps:.1} fps");

    // --- Phase 2: tap-to-frame latency, control channel --------------------
    eprintln!("e2e: {TAP_TRIALS} control-channel tap trials ...");
    let mut control_ms = Vec::with_capacity(TAP_TRIALS);
    for i in 0..TAP_TRIALS {
        // Vary the tap point slightly so each tap is a distinct input.
        let x = cx.saturating_sub(40) + (i as u32 * 4 % 80);
        let d = tap_latency_control(&mut client, x, cy)?;
        control_ms.push(d.as_secs_f64() * 1000.0);
        eprintln!("e2e: control trial {i}: {:.1} ms", control_ms[i]);
    }

    // --- Phase 3: tap-to-frame latency, adb shell input --------------------
    // The adb path cannot share the client's video socket (borrow rules), so
    // we time injection here and add the frame-boundary wait measured once
    // via the stream below. Honest split: injection vs frame wait.
    eprintln!("e2e: {TAP_TRIALS} adb-shell-input tap trials ...");
    let mut adb_ms = Vec::with_capacity(TAP_TRIALS);
    for i in 0..TAP_TRIALS {
        let x = cx.saturating_sub(40) + (i as u32 * 4 % 80);
        let t0 = Instant::now();
        tap_latency_adb(serial, x, cy)?;
        // Now wait for the next frame on our own stream, same as control.
        let mut iter = client.start_video_stream();
        match iter.next() {
            Some(Ok(_)) => {}
            Some(Err(e)) => return Err(e),
            None => {
                return Err(DeviceError::Parse(
                    "video stream ended during adb trial".to_string(),
                ))
            }
        }
        let d = t0.elapsed();
        adb_ms.push(d.as_secs_f64() * 1000.0);
        eprintln!("e2e: adb trial {i}: {:.1} ms", adb_ms[i]);
    }

    // --- Phase 4: pinch (two pointers) ------------------------------------
    eprintln!("e2e: pinch gesture (two pointers) ...");
    let pinch_ok = (|| -> Result<(), DeviceError> {
        let x0a = cx.saturating_sub(120);
        let x1a = cx + 120;
        let x0b = cx.saturating_sub(220);
        let x1b = cx + 220;
        client.inject_touch(0, x0a, cy, TouchAction::Down)?;
        client.inject_touch(1, x1a, cy, TouchAction::PointerDown)?;
        for step in 1..=10u32 {
            let t = step as f64 / 10.0;
            let xa = (x0a as f64 + (x0b as f64 - x0a as f64) * t) as u32;
            let xb = (x1a as f64 + (x1b as f64 - x1a as f64) * t) as u32;
            client.inject_touch(0, xa, cy, TouchAction::Move)?;
            client.inject_touch(1, xb, cy, TouchAction::Move)?;
            std::thread::sleep(Duration::from_millis(16));
        }
        client.inject_touch(1, x1b, cy, TouchAction::PointerUp)?;
        client.inject_touch(0, x0b, cy, TouchAction::Up)?;
        // Stream must still be alive afterwards.
        let mut iter = client.start_video_stream();
        match iter.next() {
            Some(Ok(_)) => Ok(()),
            Some(Err(e)) => Err(e),
            None => Err(DeviceError::Parse(
                "video stream ended after pinch".to_string(),
            )),
        }
    })()
    .is_ok();
    eprintln!("e2e: pinch ok = {pinch_ok}");

    Ok(Metrics {
        kvm_present,
        device: serial.to_string(),
        video_w: vw,
        video_h: vh,
        packets_read,
        keyframes,
        fps,
        control_p50_ms: percentile(control_ms.clone(), 50.0),
        control_p95_ms: percentile(control_ms.clone(), 95.0),
        control_mean_ms: mean(&control_ms),
        adb_p50_ms: percentile(adb_ms.clone(), 50.0),
        adb_p95_ms: percentile(adb_ms.clone(), 95.0),
        adb_mean_ms: mean(&adb_ms),
        pinch_ok,
        home_key_ok,
    })
}

#[test]
fn android_headless_e2e() {
    if std::env::var("SUPERCLI_ANDROID_E2E").as_deref() != Ok("1") {
        eprintln!("SKIP android_headless_e2e: set SUPERCLI_ANDROID_E2E=1 for a real emulator");
        return;
    }
    let serial =
        std::env::var("ANDROID_SERIAL").unwrap_or_else(|_| "emulator-5554".to_string());
    let metrics_out =
        std::env::var("METRICS_OUT").unwrap_or_else(|_| "metrics.json".to_string());

    let m = match run_e2e(&serial) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("e2e: FATAL stage dump above; error = {e:?}");
            panic!("android e2e failed: {e:?}");
        }
    };
    let json = m.to_json();
    eprintln!("e2e metrics:\n{json}");
    fs::write(&metrics_out, &json).expect("write metrics.json");

    assert!(
        m.packets_read >= PACKET_TARGET,
        "expected >= {PACKET_TARGET} packets, got {}",
        m.packets_read
    );
    assert!(m.fps > 5.0, "fps suspiciously low: {:.1}", m.fps);
    assert!(m.pinch_ok, "pinch gesture failed");
    assert!(m.home_key_ok, "HOME key failed");
}
