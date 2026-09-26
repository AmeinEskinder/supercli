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
//! 2. fps measured over 60 s WHILE THE SCREEN ANIMATES: scrcpy only emits
//!    frames when the picture changes, so a static home screen reads ~3 fps
//!    and the number means nothing. We launch Settings, verify it is the
//!    resumed activity, and scroll it continuously with INJECT_SCROLL_EVENT
//!    during the window, and report the animated fps.
//! 3. Tap-to-next-frame latency p50/p95: native control channel vs
//!    `adb shell input` (20 trials each). Each trial taps something that
//!    visibly toggles (swipe-up opens the app drawer, HOME closes it); a
//!    trial with no frame within 1 s is a "miss" counted in metrics, NOT a
//!    fatal error (a tap that changes nothing produces no frames).
//! 4. A two-pointer pinch completes without error and the stream survives.
//!
//! Methodology note (honest): "tap-to-frame" here = time from tap injection
//! to the arrival of the next encoded H.264 packet.
//!
//! At ~60 fps a packet arrives roughly every 16 ms regardless, so this
//! metric = injection cost + time-to-next-frame-boundary. The *comparison*
//! between the two injection paths is the point, not the absolute number.

#![cfg(feature = "device")]

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use supercli_device::scrcpy_native::{
    swipe_control, AndroidKeycode, ScrcpyNative, TouchAction, VideoStreamReader,
};
use supercli_device::DeviceError;

const PACKET_TARGET: usize = 100;
const FPS_WINDOW: Duration = Duration::from_secs(60);
const TAP_TRIALS: usize = 20;
/// Per-trial frame deadline. No frame in this long after a gesture = the
/// gesture produced no visible change: count a "miss", do not fail.
const FRAME_DEADLINE: Duration = Duration::from_secs(1);
/// Miss budget: at most 10% of trials may miss (Amein: CI correctness gate).
const MAX_MISS_FRACTION: f64 = 0.10;

fn percentile(mut xs: Vec<f64>, p: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((p / 100.0) * (xs.len() as f64 - 1.0)).round() as usize;
    xs[idx.min(xs.len() - 1)]
}

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// Run `adb -s <serial> shell <args>`. Used for the deliberate adb-path
/// comparison trials and for launching Settings (not a fallback here).
fn adb_shell(serial: &str, args: &[&str]) -> Result<(), DeviceError> {
    use std::process::Command;
    let status = Command::new("adb")
        .arg("-s")
        .arg(serial)
        .arg("shell")
        .args(args)
        .status()
        .map_err(DeviceError::Io)?;
    if status.success() {
        Ok(())
    } else {
        Err(DeviceError::Parse(format!(
            "adb shell {} exited {status}",
            args.join(" ")
        )))
    }
}

/// Capture `adb shell <args>` stdout as a String.
fn adb_shell_output(serial: &str, args: &[&str]) -> Result<String, DeviceError> {
    use std::process::Command;
    let out = Command::new("adb")
        .arg("-s")
        .arg(serial)
        .arg("shell")
        .args(args)
        .output()
        .map_err(DeviceError::Io)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Emulator render rate via `dumpsys gfxinfo`.
///
/// The CI runner has no GPU: the emulator renders with SwiftShader and
/// scrcpy software-encodes on 2-4 vCPUs, so CI fps is encoder-bound, not
/// a product signal. Recording the emulator's OWN render rate during the
/// scroll window tells us whether the renderer or the encoder is the
/// bottleneck: if gfxinfo shows ~60 fps rendered but scrcpy delivers 2,
/// the encoder is the bottleneck.
///
/// Method: reset gfxinfo stats for the package, run the caller closure
/// (the scroll window), then read "Total frames rendered" and divide by
/// elapsed seconds. Returns (closure_result, render_fps, frames_rendered).
///
/// A 0 frame count is meaningful, not a parse failure: it means the app
/// did not render anything during the window (e.g. it wasn't resumed),
/// so the fps number must not be trusted — see the packet-gate logic.
fn measure_render_rate<F, T>(
    serial: &str,
    package: &str,
    f: F,
) -> Result<(T, f64, u64), DeviceError>
where
    F: FnOnce() -> Result<T, DeviceError>,
{
    // Reset stats so the count covers only our window. Documented arg
    // order is `dumpsys gfxinfo reset <package>`.
    let _ = adb_shell_output(serial, &["dumpsys", "gfxinfo", "reset", package]);
    let start = Instant::now();
    let result = f()?;
    let elapsed_s = start.elapsed().as_secs_f64();
    let dump = adb_shell_output(serial, &["dumpsys", "gfxinfo", package])?;
    let mut frames: u64 = 0;
    let mut parsed = false;
    for line in dump.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Total frames rendered:") {
            if let Ok(n) = rest.trim().parse::<u64>() {
                frames = n;
                parsed = true;
                break;
            }
        }
    }
    let render_fps = frames as f64 / elapsed_s.max(0.001);
    if parsed {
        eprintln!("e2e: gfxinfo {package}: {frames} frames in {elapsed_s:.1}s = {render_fps:.1} render fps");
    } else {
        eprintln!("e2e: gfxinfo {package}: could not parse 'Total frames rendered' (package may not have rendered)");
    }
    Ok((result, render_fps, frames))
}

/// Verify Settings is the resumed (foreground) activity after `am start`.
/// Without this check a scroll loop can run against a non-resumed app,
/// producing a meaningless 0-frame gfxinfo window.
///
/// Amein (run #18): on Android 14 the dumpsys field is `topResumedActivity`
/// (older versions use `mResumedActivity`), so match either; poll for up
/// to 10 s instead of checking once because the launch can be slow.
/// Poll for Settings being the resumed activity.
/// 
/// Returns `Ok(true)` if Settings resumed, `Ok(false)` if it did not resume
/// after retries (caller should fall back to launcher-swipe animation rather
/// than failing — the fps metric is not gated, and CI emulator slowness can
/// leave Settings at INITIALIZING).
/// 
/// Amein (run #24): poll for up to 20 s; the Settings launch exists only to
/// animate the screen for the (ungated) fps metric.
fn wait_settings_resumed(serial: &str) -> Result<bool, DeviceError> {
    // Lines mentioning resume from the last poll, logged on failure so the
    // CI annotations show what the emulator actually reports.
    let mut last_resum_lines: Vec<String> = Vec::new();
    for poll in 0..40 {
        let out = adb_shell_output(serial, &["dumpsys", "activity", "activities"])?;
        let mut field_seen = false;
        last_resum_lines.clear();
        for line in out.lines() {
            let t = line.trim();
            if t.to_lowercase().contains("resum") && last_resum_lines.len() < 20 {
                last_resum_lines.push(t.to_string());
            }
            // Separator-agnostic match: Android 14 emits
            // `topResumedActivity=ActivityRecord{...}` (with `=`), older
            // dumps use `topResumedActivity:`/`mResumedActivity:`.
            // Scan ALL lines: the field can appear once per display/section
            // and the first occurrence is not necessarily Settings.
            if t.contains("mResumedActivity") || t.contains("topResumedActivity") {
                eprintln!("e2e: resumed-activity field: {t}");
                if t.contains("com.android.settings") {
                    return Ok(true);
                }
                field_seen = true;
            }
        }
        if !field_seen {
            eprintln!("e2e: poll {poll}: no resumed-activity field in dumpsys output");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    eprintln!("e2e: Settings not resumed after 20 s; lines mentioning 'resum' on final poll:");
    for l in &last_resum_lines {
        eprintln!("e2e:   {l}");
    }
    // Not an error: caller falls back to launcher-swipe animation.
    Ok(false)
}

/// Gestures used by the latency trials. Each one visibly toggles the
/// screen: swipe-up opens the app drawer, HOME returns to the launcher.
#[derive(Clone, Copy)]
enum Gesture {
    SwipeUp,
    Home,
}

impl Gesture {
    fn name(self) -> &'static str {
        match self {
            Gesture::SwipeUp => "swipe-up(drawer)",
            Gesture::Home => "HOME",
        }
    }
}

/// Wait up to [`FRAME_DEADLINE`] for the next H.264 packet after a gesture.
/// `Ok(Some(latency))` — a frame arrived; `Ok(None)` — "miss", no frame in
/// time (the gesture changed nothing on screen); counted, never fatal.
///
/// The 1 s socket timeout means a single `next()` either returns a whole
/// packet or times out cleanly: on loopback a packet's bytes arrive
/// together, so a timeout implies an idle screen, never a torn header.
fn wait_for_frame(client: &mut ScrcpyNative, t0: Instant) -> Result<Option<Duration>, DeviceError> {
    client.set_video_read_timeout(Some(FRAME_DEADLINE))?;
    // Scope ends the `&mut client` borrow from the stream before the
    // timeout is restored below.
    let out = {
        let mut iter = client.start_video_stream();
        match iter.next() {
            Some(Ok(_)) => Ok(Some(t0.elapsed())),
            Some(Err(DeviceError::Io(e)))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Ok(None)
            }
            Some(Err(e)) => Err(e),
            None => Err(DeviceError::Parse(
                "video stream ended during latency trial".to_string(),
            )),
        }
    };
    // Restore the streaming default.
    client.set_video_read_timeout(Some(Duration::from_secs(10)))?;
    out
}

/// One control-channel trial: inject the gesture, then wait for a frame.
fn trial_control(
    client: &mut ScrcpyNative,
    gesture: Gesture,
) -> Result<Option<Duration>, DeviceError> {
    let t0 = Instant::now();
    match gesture {
        Gesture::SwipeUp => {
            let (w, h) = client.video_size();
            client.swipe(w / 2, h * 3 / 4, w / 2, h / 4, Duration::from_millis(300))?;
        }
        Gesture::Home => {
            client.press_key(AndroidKeycode::Home)?;
        }
    }
    wait_for_frame(client, t0)
}

/// One adb-path trial: same gestures via `adb shell input`, frame wait on
/// the client's own video stream. Injection cost includes the adb spawn.
fn trial_adb(
    serial: &str,
    client: &mut ScrcpyNative,
    gesture: Gesture,
) -> Result<Option<Duration>, DeviceError> {
    let t0 = Instant::now();
    let (w, h) = client.video_size();
    match gesture {
        Gesture::SwipeUp => {
            adb_shell(
                serial,
                &[
                    "input",
                    "swipe",
                    &(w / 2).to_string(),
                    &(h * 3 / 4).to_string(),
                    &(w / 2).to_string(),
                    &(h / 4).to_string(),
                    "300",
                ],
            )?;
        }
        Gesture::Home => {
            adb_shell(serial, &["input", "keyevent", "3"])?;
        }
    }
    wait_for_frame(client, t0)
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

struct Metrics {
    kvm_present: bool,
    device: String,
    video_w: u32,
    video_h: u32,
    max_size: Option<String>,
    packets_read: usize,
    keyframes: usize,
    fps_animating: f64,
    /// How the screen was animated during the fps window: "settings-scroll"
    /// or "launcher-swipe" (fallback when Settings didn't resume; Amein run
    /// #24). The fps metric is recorded, not gated, either way.
    fps_source: String,
    /// The emulator's OWN render rate (dumpsys gfxinfo) during the scroll
    /// window. If this is ~60 but scrcpy fps is 2, the encoder (not the
    /// renderer) is the bottleneck.
    render_fps: f64,
    /// Raw "Total frames rendered" count behind `render_fps`. A 0 here
    /// means the app did not render during the window (not resumed, not
    /// scrollable) — the packet gate is skipped, because the failure is
    /// in the test's own animation driver, not the stream.
    gfxinfo_frames: u64,
    control_p50_ms: f64,
    control_p95_ms: f64,
    control_mean_ms: f64,
    control_n: usize,
    control_misses: usize,
    adb_p50_ms: f64,
    adb_p95_ms: f64,
    adb_mean_ms: f64,
    adb_n: usize,
    adb_misses: usize,
    pinch_ok: bool,
    home_key_ok: bool,
}

impl Metrics {
    fn to_json(&self) -> String {
        let max_size_json = match &self.max_size {
            Some(v) => format!("\"{v}\""),
            None => "null".to_string(),
        };
        format!(
            concat!(
                "{{\n",
                "  \"kvm_present\": {},\n",
                "  \"device\": \"{}\",\n",
                "  \"video_width\": {},\n",
                "  \"video_height\": {},\n",
                "  \"max_size\": {},\n",
                "  \"packets_read\": {},\n",
                "  \"keyframes\": {},\n",
                "  \"fps_animating_60s\": {:.2},\n",
                "  \"fps_source\": \"{}\",\n",
                "  \"render_fps_gfxinfo\": {:.2},\n",
                "  \"gfxinfo_frames_rendered\": {},\n",
                "  \"control_tap_to_frame_ms\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"mean\": {:.2}, \"n\": {}, \"misses\": {}}},\n",
                "  \"adb_tap_to_frame_ms\": {{\"p50\": {:.2}, \"p95\": {:.2}, \"mean\": {:.2}, \"n\": {}, \"misses\": {}}},\n",
                "  \"pinch_ok\": {},\n",
                "  \"home_key_ok\": {}\n",
                "}}"
            ),
            self.kvm_present,
            self.device,
            self.video_w,
            self.video_h,
            max_size_json,
            self.packets_read,
            self.keyframes,
            self.fps_animating,
            self.fps_source,
            self.render_fps,
            self.gfxinfo_frames,
            self.control_p50_ms,
            self.control_p95_ms,
            self.control_mean_ms,
            self.control_n,
            self.control_misses,
            self.adb_p50_ms,
            self.adb_p95_ms,
            self.adb_mean_ms,
            self.adb_n,
            self.adb_misses,
            self.pinch_ok,
            self.home_key_ok,
        )
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

    // --- Phase 1: fps WHILE ANIMATING (60 s) --------------------------------
    // scrcpy only emits frames when the picture changes, so fps on a static
    // screen is ~3 and meaningless. Launch Settings, verify it is the
    // resumed (foreground) activity, then scroll it continuously with
    // INJECT_SCROLL_EVENT while reading packets; the split borrow lets us
    // drive input and read video simultaneously.
    //
    // The scroll window is wrapped in measure_render_rate: dumpsys gfxinfo
    // records the emulator's OWN render rate, so we can tell whether the
    // renderer or the (software) encoder is the fps bottleneck on CI. A
    // gfxinfo count of 0 means the app never rendered (not resumed, not
    // scrollable) — the packet gate is skipped in that case, because the
    // failure is in the test's own animation driver, not the stream.
    eprintln!("e2e: launching Settings for the animated fps window ...");
    // Retry once: on the second run of a two-run job the emulator can be
    // briefly unresponsive while the previous scrcpy server tears down.
    // Amein (run #18): use -W so am waits until the launch completes,
    // then poll for the resumed activity (warm starts just bring the task
    // to front, which can race the resumed check).
    let mut launched = false;
    for attempt in 1..=2 {
        match adb_shell(
            serial,
            &["am", "start", "-W", "-n", "com.android.settings/.Settings"],
        ) {
            Ok(()) => {
                launched = true;
                break;
            }
            Err(e) if attempt == 1 => {
                eprintln!("e2e: am start attempt 1 failed ({e:?}), retrying in 5 s ...");
                std::thread::sleep(Duration::from_secs(5));
            }
            Err(e) => return Err(e),
        }
    }
    assert!(launched);
    // Amein (run #16): verify the app is actually resumed before trusting
    // any fps number — a 0-frame gfxinfo window means the animation driver
    // missed, not that the stream is broken.
    // Amein (run #24): if Settings doesn't resume (CI emulator slowness),
    // don't fail — fall back to launcher-swipe animation. The fps metric
    // is not gated; the stream/packet/tap/pinch gates stay.
    let settings_resumed = wait_settings_resumed(serial)?;
    let fps_source: String;
    let gfx_package: &str;
    if settings_resumed {
        eprintln!("e2e: Settings resumed, starting 60 s scroll window ...");
        fps_source = "settings-scroll".to_string();
        gfx_package = "com.android.settings";
    } else {
        eprintln!("e2e: Settings not resumed; falling back to launcher-swipe animation ...");
        fps_source = "launcher-swipe".to_string();
        gfx_package = "com.google.android.apps.nexuslauncher";
    }

    eprintln!("e2e: reading H.264 packets for 60 s while animating (fps_source={fps_source}) ...");
    let ((packets_read, keyframes, fps_animating), render_fps, gfxinfo_frames) =
        measure_render_rate(serial, gfx_package, || {
            let (video, control) = client.split();
            video
                .set_read_timeout(Some(Duration::from_millis(200)))
                .map_err(DeviceError::Io)?;
            let mut reader = VideoStreamReader::new(video);
            let mut packets_read = 0usize;
            let mut keyframes = 0usize;
            let start = Instant::now();
            // Animate the screen so scrcpy emits frames (static screens yield
            // ~3 fps, which is meaningless).
            //
            // Settings path: scroll immediately, then every 250 ms,
            // alternating direction — a single-direction scroll stalls at the
            // list end, while alternating keeps the list animating.
            //
            // Launcher fallback (Amein run #24): open/close the app drawer
            // every 300 ms via control-channel swipes. Swipe up from the
            // bottom opens the drawer; swipe down closes it.
            let mut last_anim = Instant::now() - Duration::from_secs(60);
            let mut scroll_down = true;
            let mut drawer_open = false;
            while start.elapsed() < FPS_WINDOW {
                if settings_resumed {
                    if last_anim.elapsed() >= Duration::from_millis(250) {
                        let vscroll = if scroll_down { 1.0 } else { -1.0 };
                        control
                            .inject_scroll(vw / 2, vh / 2, 0.0, vscroll)
                            .map_err(DeviceError::Io)?;
                        scroll_down = !scroll_down;
                        last_anim = Instant::now();
                    }
                } else if last_anim.elapsed() >= Duration::from_millis(300) {
                    // Alternate drawer open/close swipes.
                    let (x0, y0, x1, y1) = if drawer_open {
                        // Close: swipe down from upper-middle to lower-middle.
                        (vw / 2, vh * 3 / 10, vw / 2, vh * 8 / 10)
                    } else {
                        // Open: swipe up from bottom edge to middle.
                        (vw / 2, vh * 9 / 10, vw / 2, vh * 4 / 10)
                    };
                    // No ControlChannel::swipe method; use the free swipe_control.
                    // (control is &mut from split(); reborrow it.)
                    swipe_control(&mut *control, x0, y0, x1, y1, Duration::from_millis(200))
                        .map_err(DeviceError::Io)?;
                    drawer_open = !drawer_open;
                    last_anim = Instant::now();
                }
                match reader.next_packet() {
                    Ok(Some(pkt)) => {
                        packets_read += 1;
                        if packets_read == 1 {
                            eprintln!("e2e: stage=first_packet");
                        }
                        if packets_read.is_multiple_of(200) {
                            eprintln!("e2e: stage=packets n={packets_read}");
                        }
                        if pkt.is_keyframe {
                            keyframes += 1;
                        }
                    }
                    Ok(None) => {
                        return Err(DeviceError::Parse(
                            "video stream ended during animated fps window".to_string(),
                        ))
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => return Err(DeviceError::Io(e)),
                }
            }
            let elapsed_s = start.elapsed().as_secs_f64();
            let fps = packets_read as f64 / elapsed_s;
            video
                .set_read_timeout(Some(Duration::from_secs(10)))
                .map_err(DeviceError::Io)?;
            Ok((packets_read, keyframes, fps))
        })?;
    eprintln!(
        "e2e: {packets_read} packets ({keyframes} keyframes) in 60 s while animating = {fps_animating:.1} fps"
    );

    // Let the encoder flush the animation tail; deterministic state: Home.
    std::thread::sleep(Duration::from_secs(1));
    client.press_key(AndroidKeycode::Home)?;
    std::thread::sleep(Duration::from_millis(500));

    // --- Phase 2: tap-to-frame latency, control channel --------------------
    // Alternate swipe-up (opens the app drawer) and HOME (closes it): every
    // trial visibly toggles the screen. A trial with no frame within 1 s is
    // a miss (gesture changed nothing), counted — never fatal.
    eprintln!("e2e: {TAP_TRIALS} control-channel trials (swipe-up/HOME alternating) ...");
    let mut control_ms: Vec<f64> = Vec::with_capacity(TAP_TRIALS);
    let mut control_misses = 0usize;
    for i in 0..TAP_TRIALS {
        let gesture = if i % 2 == 0 {
            Gesture::SwipeUp
        } else {
            Gesture::Home
        };
        match trial_control(&mut client, gesture)? {
            Some(d) => {
                let ms = d.as_secs_f64() * 1000.0;
                eprintln!("e2e: control trial {i} ({}): {ms:.1} ms", gesture.name());
                control_ms.push(ms);
            }
            None => {
                eprintln!(
                    "e2e: control trial {i} ({}): MISS (no frame within 1 s)",
                    gesture.name()
                );
                control_misses += 1;
            }
        }
    }

    // --- Phase 3: tap-to-frame latency, adb shell input --------------------
    // Same alternating gestures via `adb shell input`; frame wait on the
    // client's own video stream. The comparison between the two injection
    // paths is the point of the metric.
    eprintln!("e2e: {TAP_TRIALS} adb-shell-input trials (swipe-up/HOME alternating) ...");
    let mut adb_ms: Vec<f64> = Vec::with_capacity(TAP_TRIALS);
    let mut adb_misses = 0usize;
    for i in 0..TAP_TRIALS {
        let gesture = if i % 2 == 0 {
            Gesture::SwipeUp
        } else {
            Gesture::Home
        };
        match trial_adb(serial, &mut client, gesture)? {
            Some(d) => {
                let ms = d.as_secs_f64() * 1000.0;
                eprintln!("e2e: adb trial {i} ({}): {ms:.1} ms", gesture.name());
                adb_ms.push(ms);
            }
            None => {
                eprintln!(
                    "e2e: adb trial {i} ({}): MISS (no frame within 1 s)",
                    gesture.name()
                );
                adb_misses += 1;
            }
        }
    }

    // --- Phase 4: pinch (two pointers) ------------------------------------
    eprintln!("e2e: pinch gesture (two pointers) ...");
    let cx = vw / 2;
    let cy = vh / 2;
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
        max_size: std::env::var("SCRCPY_MAX_SIZE")
            .ok()
            .filter(|v| !v.trim().is_empty()),
        packets_read,
        keyframes,
        fps_animating,
        fps_source,
        render_fps,
        gfxinfo_frames,
        control_p50_ms: percentile(control_ms.clone(), 50.0),
        control_p95_ms: percentile(control_ms.clone(), 95.0),
        control_mean_ms: mean(&control_ms),
        control_n: control_ms.len(),
        control_misses,
        adb_p50_ms: percentile(adb_ms.clone(), 50.0),
        adb_p95_ms: percentile(adb_ms.clone(), 95.0),
        adb_mean_ms: mean(&adb_ms),
        adb_n: adb_ms.len(),
        adb_misses,
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
    let serial = std::env::var("ANDROID_SERIAL").unwrap_or_else(|_| "emulator-5554".to_string());
    let metrics_out = std::env::var("METRICS_OUT").unwrap_or_else(|_| "metrics.json".to_string());

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

    // Packet gate (Amein, run #16): only meaningful if the app actually
    // rendered during the scroll window. gfxinfo == 0 frames means the
    // animation driver missed (app not resumed / not scrollable) — the
    // stream itself is not proven broken, so the gate is skipped with a
    // loud warning instead of failing the run.
    if m.gfxinfo_frames > 0 {
        assert!(
            m.packets_read >= PACKET_TARGET,
            "expected >= {PACKET_TARGET} packets while animating, got {}",
            m.packets_read
        );
    } else {
        eprintln!(
            "e2e: WARNING: gfxinfo reports 0 frames rendered for com.android.settings — \
             the app was not animating during the scroll window; SKIPPING the packet gate \
             (packets_read = {})",
            m.packets_read
        );
    }
    // fps is recorded as a NUMBER in metrics.json, not a gate: on CI the
    // runner has no GPU (SwiftShader + software encode on 2-4 vCPUs), so
    // the encoder is the bottleneck and 60 fps is not achievable there.
    // See render_fps_gfxinfo in metrics.json for the renderer-vs-encoder
    // breakdown.
    eprintln!(
        "e2e: fps_animating = {:.1} (recorded, not gated); render_fps = {:.1}",
        m.fps_animating, m.render_fps
    );
    // Correctness gates (Amein): misses <= 10% on each injection path.
    let max_misses = (TAP_TRIALS as f64 * MAX_MISS_FRACTION).ceil() as usize;
    assert!(
        m.control_misses <= max_misses,
        "too many control misses: {}/{} (budget {})",
        m.control_misses,
        TAP_TRIALS,
        max_misses
    );
    assert!(
        m.adb_misses <= max_misses,
        "too many adb misses: {}/{} (budget {})",
        m.adb_misses,
        TAP_TRIALS,
        max_misses
    );
    // The control channel must be faster than adb shell input (the point
    // of the native path). Compare medians; both are tap-to-frame.
    assert!(
        m.control_p50_ms < m.adb_p50_ms,
        "control channel not faster than adb: control p50 = {:.1} ms, adb p50 = {:.1} ms",
        m.control_p50_ms,
        m.adb_p50_ms
    );
    assert!(m.pinch_ok, "pinch gesture failed");
    assert!(m.home_key_ok, "HOME key failed");
}
