//! Unpeel mobile client: pairing, session list, live terminal detail.
//!
//! Architectural rule: the iOS session detail is a **live terminal, never
//! semantic chat** (see repo AGENTS.md). Tapping a session opens a
//! [`TerminalView`] fed by long-polling `GET /mobile/output` — the same raw
//! PTY byte stream the Swift client renders through Ghostty. Keystrokes go
//! back through `POST /mobile/write`.
//!
//! Multi-Host: every connected Host remains connected at once in a shared
//! [`HostRegistry`]. Switching Hosts (the "‹ Hosts" list) is a view change,
//! never a teardown — each Host keeps its client, its cached bootstrap
//! snapshot, and its terminal poll threads while another Host is in view.
//! Per-Host UI state (open session, terminal grid, viewport bookkeeping)
//! lives in [`HostView`], keyed by host id.
//!
//! First run shows the pairing screen: paste the Host's pairing code (or
//! pick a previously paired Host). Pairing secrets go straight to the
//! platform keychain via [`open_controller_store`]; the Host list lives
//! there too. No configuration files, no environment variables.
//!
//! Mobile packaging (Xcode/Gradle projects, signing, push-notification
//! shims, QR camera scanning) is tracked separately; this binary is the
//! portable core that those launchers embed.
//!
//! Renderer note: Dioxus mobile renders through the OS webview today. The
//! genuinely native GPU renderer (Blitz, currently beta) is evaluated
//! behind the `native-ui` feature before any commitment.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dioxus::html::geometry::WheelDelta;
use dioxus::html::input_data::MouseButton;
use dioxus::html::InteractionLocation;
use dioxus::prelude::*;
use unpeel_client::dto::{BootstrapSnapshot, PresetSummary, SessionSummary};
use unpeel_client::protocol::supports_session_creation;
use unpeel_client::{
    connect_direct_classified, decode_pairing_code, delete_host_secrets, device_identity,
    load_host_secrets, load_paired_host_records, open_controller_store, pair,
    relay_credentials_for_host, remove_paired_host, save_paired_host_records, store_host_secrets,
    upsert_paired_host, ArtifactMeta, CredentialStore, DirectFailure, HostClient, HostRegistry,
    HostSecrets, PairedHostRecord, RelayConnection, RemoteDeviceIdentity, TransportKind,
};
use unpeel_ui::{
    detect_scroll_shift, fit_grid, flatten_annotation_png, flatten_spec, launchable_presets,
    matches_in_row, method_label, parse_push_bridge_message, row_text, share_entry_js,
    should_resize_remote, viewport_text, word_anchor_at, AnnotationMode, AnnotationResult,
    AppLockManager, AppLockOverlay, ApprovalCard, ArchiveAction, ArchivedSessionsSheet,
    BrowserGalleryPanel, ConnectionBar, DictationSettings, DictationView, FindBar, FindState,
    GalleryDetailView, GalleryEntry, KeystrokePredictionEngine, PairingStatus, PairingView,
    PathClickRequest, PredictionOverlay, PresetDrawer, PushBridgeEvent, PushManager, QrScannerView,
    ScrollPredictionEngine, SelectionRequest, SessionList, SessionOrganizePatch,
    SessionOrganizeSheet, SessionSheetAction, ShellBiometricBackend, TerminalModel,
    TerminalSnapshot, TerminalView, APP_CSS, DEFAULT_COLS, DEFAULT_ROWS, PUSH_BRIDGE_JS,
    PUSH_TOKEN_PROBE_JS,
};

/// Bytes per `output` chunk; the Host truncates at a safe boundary.
const OUTPUT_LIMIT: usize = 64 * 1024;
/// Long-poll window per `output` request (ms). Keeps the radio quiet while
/// staying responsive to session switches.
const OUTPUT_WAIT_MS: u64 = 5_000;

/// Per-Host view state: everything the mobile UI keeps for one connected
/// Host. The connection itself (client, snapshot cache) lives in the shared
/// [`HostRegistry`]; this is only what the view needs, keyed by host id so
/// a Host's terminal keeps polling while another Host is in view.
#[derive(Clone)]
struct HostView {
    selected_session: Option<String>,
    terminal_snapshot: Option<TerminalSnapshot>,
    terminal_error: Option<String>,
    /// Ordered key writer for the open session: `on_key` sends into this
    /// channel; one writer thread drains it into `HostClient::write`.
    /// Spawning a thread per keystroke would waste resources and could
    /// deliver keys out of order.
    terminal_writer: Option<std::sync::mpsc::Sender<String>>,
    /// Bumped every time a session is opened/closed; the poll thread exits
    /// when its generation no longer matches.
    poll_generation: u64,
    /// Current remote PTY grid size in (columns, rows). Viewport changes
    /// move it through [`apply_viewport_size`], never through
    /// keyboard-driven resizes.
    pty_size: (u16, u16),
    /// Bumped on every viewport resize claim so stale resize threads can
    /// tell they were superseded and skip their (now outdated) resize.
    pty_generation: u64,
    /// Set when a remote resize succeeded; the poll loop rebuilds the local
    /// [`TerminalModel`] at the new size on its next iteration.
    pty_remodel: bool,
    /// Measured cell size in CSS px, probed once per terminal open from the
    /// rendered terminal font. Used to convert viewport px into grid cells.
    cell_px: Option<(f64, f64)>,
    /// Mosh-style predictive echo for the open session: provisional
    /// keystrokes render immediately as an overlay and reconcile against
    /// the authoritative grid as server bytes arrive. Reset on every
    /// terminal open/remodel/teardown.
    prediction: KeystrokePredictionEngine,
    /// Mosh-style predictive scroll for the open session: wheel gestures
    /// are forwarded to the host and tracked in the engine, which
    /// reconciles them against observed viewport shifts and measures the
    /// wheel→pixels path latency. The view-layer translation stays behind
    /// [`SCROLL_PREDICTION_DISPLAY_ENABLED`], dark to match the Swift
    /// client. Reset on every terminal open/remodel/teardown.
    scroll: ScrollPredictionEngine,
    /// Accumulated fractional wheel rows for the current burst; the
    /// remainder past the per-event host cap waits for the next event.
    scroll_accum: f64,
    /// True between the first wheel of a burst and the expiry probe, so
    /// the engine latches its display decision once per gesture.
    scroll_gesture_active: bool,
    /// Bumped on every wheel event so a stale expiry probe can't end a
    /// newer burst.
    scroll_gesture_gen: u64,
    /// Press-and-hold tracking for long-press text selection: the pointer
    /// position (CSS px, viewport-relative) at press time, or None when
    /// no press is being tracked.
    press_point: Option<(f64, f64)>,
    /// Terminal view bounding rect (CSS px, viewport-relative) measured
    /// at press time, for mapping the press point to a grid cell.
    press_rect: Option<(f64, f64)>,
    /// Bumped on every press start/cancel so a stale timer can't fire for
    /// a newer gesture.
    press_gen: u64,
    /// Monotonic counter for selection requests; TerminalView opens the
    /// sheet when it sees a nonce it hasn't applied yet.
    selection_nonce: u64,
    /// Pending long-press selection request for TerminalView.
    selection_request: Option<SelectionRequest>,
    /// Terminal find bar state (`None` = closed). The poll loop refreshes
    /// the matches against every new snapshot while open.
    find: Option<FindState>,
    /// Browser gallery for the selected session (artifacts, detail,
    /// annotation editors, flatten → re-upload).
    gallery: GalleryUi,
    /// Session id with the organize sheet open (`None` = closed).
    organize_session_id: Option<String>,
    /// Archive-library sheet (`None` = closed).
    archive_sheet: Option<ArchiveSheetState>,
    /// Preset drawer open (`false` = closed). The drawer's project comes
    /// from each preset's own `project_id` — the flat mobile session list
    /// has no project tree, so there is no per-project drawer like iOS.
    preset_drawer_open: bool,
    /// Preset id currently launching (row shows the spinner, like Swift's
    /// `launchingPresetID`).
    launching_preset_id: Option<String>,
}

impl Default for HostView {
    fn default() -> Self {
        Self {
            selected_session: None,
            terminal_snapshot: None,
            terminal_error: None,
            terminal_writer: None,
            poll_generation: 0,
            pty_size: (DEFAULT_COLS, DEFAULT_ROWS),
            pty_generation: 0,
            pty_remodel: false,
            cell_px: None,
            prediction: KeystrokePredictionEngine::new(),
            scroll: ScrollPredictionEngine::new(),
            scroll_accum: 0.0,
            scroll_gesture_active: false,
            scroll_gesture_gen: 0,
            press_point: None,
            press_rect: None,
            press_gen: 0,
            selection_nonce: 0,
            selection_request: None,
            find: None,
            gallery: GalleryUi::default(),
            organize_session_id: None,
            archive_sheet: None,
            preset_drawer_open: false,
            launching_preset_id: None,
        }
    }
}

impl HostView {
    /// Reset terminal state when the user navigates away: the poll thread
    /// exits on the generation bump, the writer thread on channel close,
    /// and stale resize threads skip via the pty generation bump.
    fn close_terminal(&mut self) {
        self.poll_generation += 1;
        self.selected_session = None;
        self.terminal_snapshot = None;
        self.terminal_error = None;
        self.terminal_writer = None;
        self.pty_size = (DEFAULT_COLS, DEFAULT_ROWS);
        self.pty_generation += 1;
        self.pty_remodel = false;
        self.cell_px = None;
        self.prediction.reset();
        self.scroll.reset_confidence();
        self.scroll_accum = 0.0;
        self.scroll_gesture_active = false;
        self.scroll_gesture_gen += 1;
        self.press_point = None;
        self.press_rect = None;
        self.press_gen += 1;
        self.selection_request = None;
        // The gallery belongs to the session; leaving the terminal
        // closes it (its loader threads exit on the generation bump).
        self.gallery.generation += 1;
        self.gallery = GalleryUi {
            generation: self.gallery.generation,
            ..Default::default()
        };
    }
}

/// Record a key sequence in the view's prediction engine: printable
/// characters become provisional echo predictions anchored at the caret,
/// backspace drops the last one, and anything the server owns the cursor
/// for (submit, arrows, escapes) clears the provisional text while keeping
/// the earned confidence gate.
fn record_keystroke(view: &mut HostView, seq: &str) {
    if seq == "\x7f" {
        view.prediction.backspace();
        return;
    }
    let now = Instant::now();
    let cols = view.pty_size.0 as i32;
    let mut cursor = view
        .terminal_snapshot
        .as_ref()
        .map(|s| (s.cursor.0 as i32, s.cursor.1 as i32));
    let mut any = false;
    for ch in seq.chars() {
        if ch.is_control() {
            continue;
        }
        // The first char anchors at the caret; the rest chain off the
        // previous prediction.
        view.prediction.keystroke(ch, cursor.take(), cols, now);
        any = true;
    }
    if !any {
        view.prediction.clear_pending();
    }
}

/// Master switch for the predictive scroll TRANSLATION. Dark to match the
/// Swift client (`scrollPredictionDisplayEnabled = false`, disabled after
/// 2026-08-30 live verification): translating the whole canvas fights any
/// TUI with pinned chrome — the composer/footer slide while the transcript
/// scrolls in place — messy on the relay where the confidence gate opens,
/// pure noise on fast links. The engine still tracks and measures
/// (confidence + wheel→pixels path latency), so re-enabling is one flag —
/// but only justified by a region-aware shift that translates the scrolled
/// rows instead of the whole surface.
const SCROLL_PREDICTION_DISPLAY_ENABLED: bool = false;
/// Expiry probe delay after the last wheel of a gesture (ms). Restarted on
/// every send, so it only ever evaluates after the last wheel — a live but
/// slow TUI mid-glide is never punished for ack lag.
const SCROLL_EXPIRY_MS: u64 = 550;
/// Wheel steps forwarded to the host per wheel event, mirroring the Swift
/// client's `maximumMouseWheelEventsPerGestureTick`: one high-resolution
/// tick can't flood the PTY. The engine's own 20-row cap bounds tracking.
const MAX_WHEEL_STEPS_PER_EVENT: i32 = 8;

/// Forward a wheel event over the open terminal and track it in the view's
/// scroll prediction engine.
///
/// Vertical wheel steps go to the host as alternate-scroll cursor keys
/// (one `ESC [ B` per row down, `ESC [ A` per row up) — the
/// provider-preference wheel path the Swift client uses for Claude & co
/// when no mouse tracking is seen. The steps are also recorded in the
/// engine, which the poll thread reconciles against observed viewport
/// shifts. The first wheel of a burst latches the engine's display
/// decision and drops keystroke predictions anchored to rows the scroll
/// is about to redraw.
fn record_wheel(mut state: SyncSignal<MobileState>, host_id: String, delta: WheelDelta) {
    let now = Instant::now();
    let mut s = state.write();
    let Some(v) = s.views.get_mut(&host_id) else {
        return;
    };
    let (Some(writer), Some((_, cell_h))) = (v.terminal_writer.clone(), v.cell_px) else {
        return;
    };
    // Wheel deltas arrive in px, lines, or pages depending on the device;
    // normalize to px so one accumulator serves all three.
    let dy_px = match delta {
        WheelDelta::Pixels(d) => d.y,
        WheelDelta::Lines(d) => d.y * cell_h,
        WheelDelta::Pages(d) => d.y * cell_h * v.pty_size.1 as f64,
    };
    if dy_px == 0.0 {
        return;
    }
    if !v.scroll_gesture_active {
        v.scroll_gesture_active = true;
        v.scroll.begin_gesture();
        // Scrolling redraws the rows predictions were anchored to.
        v.prediction.clear_pending();
    }
    // Finger travel per wheel step: a full cell height means the content
    // tracks 1:1, matching the Swift client after its trackpad-multiplier
    // fix — a fraction of a cell over-scrolls every swipe.
    v.scroll_accum += dy_px / cell_h.max(7.0);
    let steps = v.scroll_accum.trunc() as i32;
    if steps == 0 {
        return;
    }
    // Positive = wheel down (content moves up) = cursor Down.
    let capped = steps.clamp(-MAX_WHEEL_STEPS_PER_EVENT, MAX_WHEEL_STEPS_PER_EVENT);
    v.scroll_accum -= capped as f64;
    let seq = if capped > 0 {
        "\x1b[B".repeat(capped as usize)
    } else {
        "\x1b[A".repeat(capped.unsigned_abs() as usize)
    };
    let _ = writer.send(seq);
    v.scroll.wheel_sent(capped, now);
    // One expiry probe per gesture tail: restarted on every send.
    v.scroll_gesture_gen += 1;
    let gen = v.scroll_gesture_gen;
    drop(s);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(SCROLL_EXPIRY_MS));
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        if v.scroll_gesture_gen != gen {
            return; // a newer burst took over; its probe owns the tail
        }
        v.scroll_gesture_active = false;
        v.scroll_accum = 0.0;
        // True when the oldest prediction expired unanswered: the
        // translation eases home and the confidence gate closes only if
        // the whole gesture went unanswered.
        v.scroll.expire_if_unanswered(Instant::now());
    });
}

/// Press-and-hold duration (ms) that opens word-anchored text selection,
/// matching the iOS long-press feel.
const LONG_PRESS_MS: u64 = 500;
/// Pointer travel (CSS px) that cancels a pending long-press — that's a
/// scroll or drag, not a press-and-hold.
const LONG_PRESS_SLOP_PX: f64 = 12.0;
/// Vendored jsQR decoder (Apache-2.0; see `src/vendor/NOTICE.md`), eval'd
/// once at startup so the QR bridge below can decode camera frames.
const JSQR_LIB: &str = include_str!("vendor/jsqr-1.4.0.js");

/// App-lock lifecycle bridge: the webview has no scene-phase API, so a
/// `visibilitychange` listener reports hidden/visible into Rust via
/// `dioxus.send`. Hidden maps to `lock_if_enabled()` (cover the app);
/// visible maps to the single automatic foreground unlock prompt —
/// mirroring Swift's `.background` / `.active` scene-phase handling.
const APP_LOCK_VISIBILITY_JS: &str = r#"(function() {
  if (window.__unpeelAppLockInstalled) return true;
  window.__unpeelAppLockInstalled = true;
  var report = function() {
    dioxus.send(document.hidden ? "app-lock:hidden" : "app-lock:visible");
  };
  document.addEventListener("visibilitychange", report);
  report();
  return true;
})()"#;

/// Installs `window.__unpeelQrStart` / `window.__unpeelQrStop` for the
/// shared [`unpeel_ui::QrScannerView`]. The camera loop runs entirely in
/// JS — `getUserMedia` → `<video>` → frame canvas → jsQR — and only
/// decoded strings cross into Rust via `dioxus.send`. Pausing stops the
/// camera (and its status indicator), it doesn't just gate the decode.
const QR_BRIDGE_INSTALL_JS: &str = r#"(function() {
  if (window.__unpeelQrInstalled) return true;
  window.__unpeelQrInstalled = true;
  var st = window.__unpeelQr = { stream: null, raf: 0, video: null, canvas: null, ctx: null };
  function loop() {
    if (!st.stream) return;
    var video = st.video;
    if (video.readyState === video.HAVE_ENOUGH_DATA) {
      var w = video.videoWidth, h = video.videoHeight;
      if (w && h) {
        st.canvas.width = w; st.canvas.height = h;
        st.ctx.drawImage(video, 0, 0, w, h);
        try {
          var code = jsQR(st.ctx.getImageData(0, 0, w, h).data, w, h);
          if (code && code.data) dioxus.send(code.data);
        } catch (e) {}
      }
    }
    st.raf = requestAnimationFrame(loop);
  }
  window.__unpeelQrStart = async function() {
    if (st.stream) { loop(); return; }
    var stream;
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: 'environment' }, audio: false
      });
    } catch (e) {
      var name = (e && e.name) || 'error';
      throw new Error(name === 'NotAllowedError'
        ? 'permission denied'
        : 'camera unavailable (' + name + ')');
    }
    st.stream = stream;
    var preview = document.querySelector('.qr-preview');
    var video = document.createElement('video');
    video.setAttribute('playsinline', '');
    video.muted = true;
    video.srcObject = stream;
    await video.play();
    st.video = video;
    if (preview) { preview.innerHTML = ''; preview.appendChild(video); }
    var canvas = document.createElement('canvas');
    st.canvas = canvas;
    st.ctx = canvas.getContext('2d', { willReadFrequently: true });
    loop();
  };
  window.__unpeelQrStop = function() {
    if (st.raf) cancelAnimationFrame(st.raf);
    st.raf = 0;
    if (st.stream) { st.stream.getTracks().forEach(function(t) { t.stop(); }); st.stream = null; }
    st.video = null;
    var preview = document.querySelector('.qr-preview');
    if (preview) preview.innerHTML = '';
  };
  return true;
})()"#;

/// JS measuring the terminal view's bounding rect (CSS px,
/// viewport-relative) at press time, so the press point maps to a grid
/// cell. Returns `[left, top]`, `[0, 0]` when the terminal isn't mounted.
const TERM_RECT_JS: &str = r#"(function() {
  var term = document.querySelector('.terminal-view');
  if (!term) return [0, 0];
  var r = term.getBoundingClientRect();
  return [r.left, r.top];
})()"#;

/// Begin press-and-hold tracking for long-press text selection: records
/// the press point, measures the terminal rect for cell mapping, and arms
/// a `LONG_PRESS_MS` timer that fires the selection if the pointer is
/// still down and hasn't moved past the slop.
fn press_started(mut state: SyncSignal<MobileState>, host_id: String, x: f64, y: f64) {
    let gen = {
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        v.press_gen += 1;
        v.press_point = Some((x, y));
        v.press_rect = None;
        v.press_gen
    };
    // Measure the rect now, in component context: the timer thread can't
    // spawn the async eval.
    let hid = host_id.clone();
    spawn(async move {
        let mut ev = dioxus::document::eval(TERM_RECT_JS);
        if let Ok((left, top)) = ev.recv::<(f64, f64)>().await {
            let mut s = state.write();
            if let Some(v) = s.views.get_mut(&hid) {
                if v.press_gen == gen && v.press_point.is_some() {
                    v.press_rect = Some((left, top));
                }
            }
        }
    });
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(LONG_PRESS_MS));
        fire_long_press(state, host_id, gen);
    });
}

/// Cancel the pending long-press when the pointer travels past the slop.
fn press_moved(mut state: SyncSignal<MobileState>, host_id: &str, x: f64, y: f64) {
    let mut s = state.write();
    let Some(v) = s.views.get_mut(host_id) else {
        return;
    };
    if let Some((px, py)) = v.press_point {
        if (x - px).abs() > LONG_PRESS_SLOP_PX || (y - py).abs() > LONG_PRESS_SLOP_PX {
            v.press_point = None;
        }
    }
}

/// The pointer lifted or the gesture was cancelled: drop the press.
fn press_ended(mut state: SyncSignal<MobileState>, host_id: &str) {
    if let Some(v) = state.write().views.get_mut(host_id) {
        v.press_point = None;
    }
}

/// The press survived `LONG_PRESS_MS` without moving or lifting: resolve
/// the press point to a grid cell, anchor the word under it (`None` ⇒
/// blank space ⇒ the sheet selects everything, the Swift
/// `anchorRange == nil` case), and hand TerminalView a selection request.
fn fire_long_press(mut state: SyncSignal<MobileState>, host_id: String, gen: u64) {
    let mut s = state.write();
    let Some(v) = s.views.get_mut(&host_id) else {
        return;
    };
    if v.press_gen != gen || v.press_point.is_none() {
        return; // moved, lifted, or superseded by a newer press
    }
    let (px, py) = v.press_point.take().unwrap();
    let Some((cell_w, cell_h)) = v.cell_px else {
        return;
    };
    let Some(snap) = v.terminal_snapshot.clone() else {
        return;
    };
    if cell_w <= 0.0 || cell_h <= 0.0 {
        return;
    }
    let text = viewport_text(&snap);
    let anchor = match v.press_rect {
        Some((left, top)) => {
            let col = ((px - left) / cell_w).floor().max(0.0) as usize;
            let row = ((py - top) / cell_h).floor().max(0.0) as usize;
            word_anchor_at(&text, row, col)
        }
        // The rect wasn't measured in time: the long-press still
        // happened, so open the sheet without an anchor (select all)
        // rather than dropping the gesture.
        None => None,
    };
    v.selection_nonce += 1;
    v.selection_request = Some(SelectionRequest {
        anchor,
        nonce: v.selection_nonce,
    });
}

#[derive(Clone)]
struct MobileState {
    store: Arc<dyn CredentialStore>,
    keychain_notice: Option<String>,
    device: Option<RemoteDeviceIdentity>,
    records: Vec<PairedHostRecord>,
    /// All live Host connections. Switching Hosts moves the active pointer;
    /// it never tears a connection down.
    hosts: HostRegistry,
    /// Per-Host view state, keyed by host id.
    views: HashMap<String, HostView>,
    /// Show the Host list (pairing screen) over the active Host.
    show_host_list: bool,
    /// Show the QR camera scanner sheet over the pairing screen.
    show_qr_scanner: bool,
    /// APNs push registration. The token itself comes from the native shell
    /// (the OS hands it to the app delegate, like the Swift client's
    /// `PushAppDelegate`); `UNPEEL_APNS_TOKEN` seeds one for simulator/dev
    /// builds. Uploads to the Host go through `on_token_change`, wired in
    /// the mount effect once the signal exists.
    push: PushManager,
    pairing_status: PairingStatus,
    error: Option<String>,
    /// Hosts that connected over the relay at startup and still need a
    /// probe-back thread. Drained once by the App component's mount
    /// effect — see `spawn_probe_back`.
    pending_probes: Vec<PairedHostRecord>,
    /// Optional biometric app lock. Armed from the pairing screen's
    /// Security row; the actual prompt is a native-shell capability
    /// (`ShellBiometricBackend`), so on plain webview builds the toggle
    /// stays disabled. Cold launch starts covered when armed.
    app_lock: AppLockManager,
}

/// Keychain account for the app-lock armed flag (the Swift client's
/// `unpeel.ios.appLockEnabled` UserDefaults key, kept in the credential
/// store so it follows the same keychain/memory-fallback path as the
/// pairing records).
const APP_LOCK_ENABLED_ACCOUNT: &str = "unpeel.app_lock_enabled";

/// Load the persisted app-lock armed flag. A missing or unreadable entry
/// means disarmed — fail-open on the preference, never on the lock itself.
fn load_app_lock_enabled(store: &Arc<dyn CredentialStore>) -> bool {
    store
        .get_secret(APP_LOCK_ENABLED_ACCOUNT)
        .ok()
        .flatten()
        .is_some_and(|v| v == b"1")
}

/// Persist the app-lock armed flag.
fn save_app_lock_enabled(store: &Arc<dyn CredentialStore>, enabled: bool) {
    if enabled {
        let _ = store.set_secret(APP_LOCK_ENABLED_ACCOUNT, b"1");
    } else {
        let _ = store.delete_secret(APP_LOCK_ENABLED_ACCOUNT);
    }
}

impl MobileState {
    fn initial() -> Self {
        let (store, keychain_notice) = open_controller_store();
        let device = device_identity(&*store).ok();
        let records = load_paired_host_records(&*store).unwrap_or_default();
        let app_lock = AppLockManager::new(
            Arc::new(ShellBiometricBackend),
            load_app_lock_enabled(&store),
        );
        let mut s = Self {
            store,
            keychain_notice,
            device: device.clone(),
            records,
            hosts: HostRegistry::new(),
            views: HashMap::new(),
            show_host_list: false,
            show_qr_scanner: false,
            push: PushManager::new(PUSH_ENVIRONMENT),
            pairing_status: PairingStatus::Idle,
            error: None,
            pending_probes: Vec::new(),
            app_lock,
        };
        if device.is_none() {
            s.error = Some("Could not set up this device's identity.".to_string());
        } else if let Some(record) = s.records.last().cloned() {
            // Auto-connect to the most recently paired Host. Other paired
            // Hosts connect on demand from the Host list.
            match connect_result(&s.store, &record, device.as_ref()) {
                Ok((client, snapshot, kind)) => {
                    s.hosts.connect(record.clone(), client, Some(snapshot));
                    if kind == TransportKind::Relay {
                        // The App's mount effect picks this up and arms
                        // the Direct probe-back (no signal exists yet here).
                        s.pending_probes.push(record);
                    }
                }
                Err(e) => s.error = Some(e),
            }
        }
        s
    }
}

/// Blocking Direct connect: the LAN route to the Host's `/mobile` endpoint.
///
/// Failures are classified ([`DirectFailure`]) so the caller can apply the
/// relay fallback policy: relay only on reachability failures, never on
/// TLS/pin mismatches or local setup problems.
fn connect_direct(
    store: &Arc<dyn CredentialStore>,
    record: &PairedHostRecord,
) -> Result<(HostClient, BootstrapSnapshot), DirectFailure> {
    let secrets = load_host_secrets(&**store, &record.host_id)
        .map_err(|e| DirectFailure::Setup(format!("credential store: {e}")))?
        .ok_or_else(|| {
            DirectFailure::Setup("No credentials stored for this host — pair again.".to_string())
        })?;
    connect_direct_classified(record, &secrets)
}

/// Blocking relay (`Link`) connect: E2E-sealed tunnel through the relay
/// when the Direct route is unreachable. Same `/mobile` semantics, same
/// bearer token — the token rides inside the sealed tunnel, never the
/// relay wire.
fn connect_relay(
    store: &Arc<dyn CredentialStore>,
    record: &PairedHostRecord,
    device: &RemoteDeviceIdentity,
) -> Result<(HostClient, BootstrapSnapshot), String> {
    let secrets = load_host_secrets(&**store, &record.host_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No credentials stored for this host — pair again.".to_string())?;
    let creds = relay_credentials_for_host(record, &secrets).ok_or_else(|| {
        "This host was paired before relay fallback existed — pair again to enable it.".to_string()
    })?;
    let conn =
        RelayConnection::connect(creds, &device.id).map_err(|e| format!("Relay connect: {e}"))?;
    let client = HostClient::via_relay(conn, &secrets.auth_token);
    let snapshot = client
        .bootstrap()
        .map_err(|e| format!("Relay bootstrap: {e}"))?;
    Ok((client, snapshot))
}

/// Blocking connect used by `initial` and the background connect thread.
///
/// Direct first; only on Direct reachability failure does the client fall
/// back to the relay — mirroring the native clients. The returned
/// [`TransportKind`] tells the caller which path won so the UI can show
/// **Direct** or **Via Link** and the caller can arm the Direct
/// probe-back.
fn connect_result(
    store: &Arc<dyn CredentialStore>,
    record: &PairedHostRecord,
    device: Option<&RemoteDeviceIdentity>,
) -> Result<(HostClient, BootstrapSnapshot, TransportKind), String> {
    match connect_direct(store, record) {
        Ok((client, snapshot)) => Ok((client, snapshot, TransportKind::Direct)),
        Err(direct_err) => {
            // Relay fallback ONLY on classified reachability failures, and
            // only when the Host record has Link enabled. A TLS/pin
            // failure, HTTP status, decode error, or local setup failure
            // surfaces as a hard error — falling back on those would
            // silently route around a security decision.
            if !direct_err.relay_eligible() {
                return Err(format!(
                    "Direct failed ({direct_err}); not a reachability failure, relay not attempted."
                ));
            }
            if !record.is_link_enabled() {
                return Err(format!(
                    "Direct failed ({direct_err}); Link is disabled for this host."
                ));
            }
            let Some(device) = device else {
                return Err(format!(
                    "Direct failed ({direct_err}); no device identity for relay."
                ));
            };
            match connect_relay(store, record, device) {
                Ok((client, snapshot)) => Ok((client, snapshot, TransportKind::Relay)),
                Err(relay_err) => Err(format!(
                    "Direct failed ({direct_err}); relay failed ({relay_err})"
                )),
            }
        }
    }
}

/// Background Direct probe-back for a Host living on the relay: every 30 s
/// the thread tries the Direct route, and on the first success swaps the
/// registry's client back to Direct. Open terminals keep their captured
/// (relay) client until they're reopened — no mid-session teardown.
///
/// The thread exits when the Host disconnects or is already back on
/// Direct.
fn spawn_probe_back(
    mut state: SyncSignal<MobileState>,
    store: Arc<dyn CredentialStore>,
    record: PairedHostRecord,
    _device: RemoteDeviceIdentity,
) {
    let host_id = record.host_id.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let on_relay = state
            .read()
            .hosts
            .get(&host_id)
            .is_some_and(|h| h.client.transport_kind() == TransportKind::Relay);
        if !on_relay {
            return;
        }
        // On success swap the registry's client back to Direct; on failure
        // stay on the relay and probe again later.
        if let Ok((client, snapshot)) = connect_direct(&store, &record) {
            let mut s = state.write();
            let Some(h) = s.hosts.get_mut(&host_id) else {
                return;
            };
            // Re-check under the lock: the Host may have been
            // forgotten or probed back while we dialed.
            if h.client.transport_kind() != TransportKind::Relay {
                return;
            }
            h.client = client;
            h.snapshot = Some(snapshot);
            h.last_error = None;
            return;
        }
    });
}

/// The active Host's id and client, if any Host is connected.
fn active_client(state: &SyncSignal<MobileState>) -> Option<(String, HostClient)> {
    let s = state.read();
    s.hosts
        .active()
        .map(|h| (h.record.host_id.clone(), h.client.clone()))
}

/// Connect to a Host, or switch to it when it is already connected.
/// Switching never tears anything down: the Host keeps its client, its
/// cached snapshot (rendered instantly), and its terminal threads.
/// APNs environment the push token belongs to: sandbox for debug builds,
/// production for release — the Host forwards this so the relay hits the
/// matching push host. Mirrors the Swift `PushManager.environment`.
#[cfg(debug_assertions)]
const PUSH_ENVIRONMENT: &str = "sandbox";
#[cfg(not(debug_assertions))]
const PUSH_ENVIRONMENT: &str = "production";

/// Runs `f` against the push manager with `on_token_change` detached, then
/// reattaches it. The callback reads launcher state, so it must never fire
/// while a `state.write()` guard is held (that would deadlock the signal).
fn with_push_detached<T>(
    mut state: SyncSignal<MobileState>,
    f: impl FnOnce(&mut PushManager) -> T,
) -> T {
    let cb = state.write().push.on_token_change.take();
    let result = f(&mut state.write().push);
    state.write().push.on_token_change = cb;
    result
}

/// Upload `(hex, env)` to the active Host's `/mobile/push-token` in a
/// background thread. No active Host: the token stays cached and is
/// re-handed on the next connect/pair via [`push_rehand_cached_token`].
fn push_upload_token(mut state: SyncSignal<MobileState>, hex: String, env: String) {
    let client = {
        let s = state.read();
        s.hosts
            .active_id()
            .and_then(|id| s.hosts.get(id))
            .map(|h| h.client.clone())
    };
    let Some(client) = client else { return };
    std::thread::spawn(move || {
        if let Err(e) = client.register_push_token(&hex, &env) {
            state
                .write()
                .push
                .did_fail_to_register(format!("upload failed: {e}"));
        }
    });
}

/// Re-hand the cached push token to the (newly) active Host. Called after
/// every successful connect/pair, mirroring the Swift client's re-upload
/// on (re)pairing. No-op until a token exists.
fn push_rehand_cached_token(state: SyncSignal<MobileState>) {
    let cached = {
        let s = state.read();
        s.push
            .token_hex()
            .map(|hex| (hex.to_string(), s.push.environment.clone()))
    };
    if let Some((hex, env)) = cached {
        push_upload_token(state, hex, env);
    }
}

/// A tapped notification named a session: select it in the active Host's
/// view and drop back to the terminal. Entry point for the native shell's
/// notification-response callback (mirrors Swift's `onOpenSession`).
fn push_open_session(mut state: SyncSignal<MobileState>, session_id: String) {
    let mut s = state.write();
    let Some(active) = s.hosts.active_id().map(|id| id.to_string()) else {
        return;
    };
    s.show_host_list = false;
    if let Some(view) = s.views.get_mut(&active) {
        view.selected_session = Some(session_id);
    }
}

/// Ingest a hex APNs token from the native shell bridge, the pre-pump
/// probe, or the `UNPEEL_APNS_TOKEN` dev seam. Validates via
/// `PushManager::did_register_hex`, uploads only when the token actually
/// changed (the callback stays detached: it reads launcher state and must
/// never fire while a `state.write()` guard is held).
fn push_ingest_hex_token(mut state: SyncSignal<MobileState>, hex: &str, source: &str) {
    let hex = hex.trim();
    if hex.is_empty() {
        return;
    }
    let (accepted, changed, token, env) = with_push_detached(state, |push| {
        let before = push.token_hex().map(|t| t.to_string());
        let accepted = push.did_register_hex(hex);
        let after = push.token_hex().map(|t| t.to_string());
        let changed = accepted && before != after;
        (accepted, changed, after, push.environment.clone())
    });
    if !accepted {
        state.write().error = Some(format!(
            "Push token from {source} was not valid hex; registration skipped."
        ));
        return;
    }
    if changed {
        if let Some(hex) = token {
            push_upload_token(state, hex, env);
        }
    }
}

/// Seed a push token from the `UNPEEL_APNS_TOKEN` hex string (simulator /
/// dev seam). The real token arrives from the native shell via the push
/// bridge — this just makes the upload path testable without one.
fn push_ingest_env_token(state: SyncSignal<MobileState>) {
    let Ok(hex) = std::env::var("UNPEEL_APNS_TOKEN") else {
        return;
    };
    push_ingest_hex_token(state, &hex, "UNPEEL_APNS_TOKEN");
}

fn connect_host(mut state: SyncSignal<MobileState>, host_id: String) {
    if state.read().hosts.contains(&host_id) {
        let mut s = state.write();
        s.hosts.switch(&host_id);
        s.show_host_list = false;
        return;
    }
    let (store, record, device) = {
        let s = state.read();
        (
            s.store.clone(),
            s.records.iter().find(|r| r.host_id == host_id).cloned(),
            s.device.clone(),
        )
    };
    let Some(record) = record else { return };
    state.write().error = None;
    std::thread::spawn(
        move || match connect_result(&store, &record, device.as_ref()) {
            Ok((client, snapshot, kind)) => {
                if kind == TransportKind::Relay {
                    if let Some(device) = device.clone() {
                        spawn_probe_back(state, store.clone(), record.clone(), device);
                    }
                }
                let mut s = state.write();
                s.hosts.connect(record, client, Some(snapshot));
                s.show_host_list = false;
                s.error = None;
                s.pairing_status = PairingStatus::Idle;
                drop(s);
                // The new Host needs the push token, if we have one.
                push_rehand_cached_token(state);
            }
            Err(e) => {
                state.write().error = Some(e);
            }
        },
    );
}

fn submit_pairing_code(mut state: SyncSignal<MobileState>, code: String) {
    let (store, device) = {
        let s = state.read();
        (s.store.clone(), s.device.clone())
    };
    let Some(device) = device else {
        state.write().pairing_status =
            PairingStatus::Failed("Device identity unavailable.".to_string());
        return;
    };
    state.write().pairing_status = PairingStatus::Working;
    state.write().error = None;
    std::thread::spawn(move || {
        let result: Result<PairedHostRecord, String> = (|| {
            let payload = decode_pairing_code(&code)
                .ok_or_else(|| "That doesn't look like a pairing code.".to_string())?;
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis() as u64;
            let response = pair(&payload, &device, now_ms).map_err(|e| e.to_string())?;
            let record = PairedHostRecord::from_pairing_response(
                &response,
                payload.certificate_fingerprint.clone(),
            );
            let secrets = HostSecrets {
                auth_token: response.auth_token.clone(),
                relay_token: response.relay_credentials.relay_token.clone(),
                e2e_key_b64: response.relay_credentials.e2e_key_b64.clone(),
                // Stored so the Direct→relay fallback can rebuild the
                // relay credentials without re-pairing.
                relay_url: Some(response.relay_credentials.relay_url.clone()),
            };
            store_host_secrets(&*store, &record.host_id, &secrets).map_err(|e| e.to_string())?;
            let records = upsert_paired_host(
                load_paired_host_records(&*store).map_err(|e| e.to_string())?,
                record.clone(),
            );
            save_paired_host_records(&*store, &records).map_err(|e| e.to_string())?;
            Ok(record)
        })();
        match result {
            Ok(record) => {
                state.write().records = load_paired_host_records(&*store).unwrap_or_default();
                let id = record.host_id.clone();
                connect_host(state, id);
            }
            Err(e) => {
                state.write().pairing_status = PairingStatus::Failed(e);
            }
        }
    });
}

/// Forget a Host: delete its keychain secrets and its record, and tear
/// down exactly its connection. Its terminal threads exit when their view
/// state disappears; every other Host keeps running untouched.
fn forget_host(mut state: SyncSignal<MobileState>, host_id: String) {
    let store = state.read().store.clone();
    let _ = delete_host_secrets(&*store, &host_id);
    let records = remove_paired_host(state.read().records.clone(), &host_id);
    let _ = save_paired_host_records(&*store, &records);
    let mut s = state.write();
    s.records = records;
    s.hosts.disconnect(&host_id);
    s.views.remove(&host_id);
    if s.hosts.is_empty() {
        s.show_host_list = false;
    }
}

/// Re-fetch the active Host's snapshot (sessions, pending approvals).
/// Approvals only arrive via bootstrap, so the user (or the answer flow
/// below) triggers it explicitly.
fn refresh(mut state: SyncSignal<MobileState>) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    {
        let mut s = state.write();
        if let Some(h) = s.hosts.get_mut(&host_id) {
            h.last_error = None;
        }
    }
    std::thread::spawn(move || match client.bootstrap() {
        Ok(snap) => {
            state.write().hosts.set_snapshot(&host_id, snap);
        }
        Err(e) => {
            state
                .write()
                .hosts
                .set_error(&host_id, format!("Refresh failed: {e}"));
        }
    });
}

/// Answer a pending approval, then re-bootstrap so the approval list and
/// session states reflect the decision.
fn answer_approval(mut state: SyncSignal<MobileState>, approval_id: String, approved: bool) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    std::thread::spawn(
        move || match client.answer_approval(&approval_id, approved) {
            Ok(_) => match client.bootstrap() {
                Ok(snap) => {
                    state.write().hosts.set_snapshot(&host_id, snap);
                }
                Err(e) => {
                    state
                        .write()
                        .hosts
                        .set_error(&host_id, format!("Approval failed: {e}"));
                }
            },
            Err(e) => {
                state
                    .write()
                    .hosts
                    .set_error(&host_id, format!("Approval failed: {e}"));
            }
        },
    );
}

/// Archive or restore a session via the Host's session-organization patch
/// (not the session-action verbs — mirroring the native clients), then
/// re-bootstrap so the session list reflects the change.
fn organize_session(mut state: SyncSignal<MobileState>, session_id: String, archived: bool) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    let verb = if archived { "Archive" } else { "Restore" };
    std::thread::spawn(move || {
        match client.update_session_organization(&session_id, None, None, Some(archived)) {
            Ok(_) => match client.bootstrap() {
                Ok(snap) => {
                    state.write().hosts.set_snapshot(&host_id, snap);
                }
                Err(e) => {
                    state
                        .write()
                        .hosts
                        .set_error(&host_id, format!("{verb} failed: {e}"));
                }
            },
            Err(e) => {
                state
                    .write()
                    .hosts
                    .set_error(&host_id, format!("{verb} failed: {e}"));
            }
        }
    });
}

/// Re-bootstrap the active Host and refresh its cached snapshot, closing
/// the organize sheet on success. Shared tail of the organize flows.
fn refresh_after_organize(
    state: &mut SyncSignal<MobileState>,
    host_id: &str,
    client: &HostClient,
    what: &str,
) {
    match client.bootstrap() {
        Ok(snap) => {
            let mut s = state.write();
            s.hosts.set_snapshot(host_id, snap);
            if let Some(v) = s.views.get_mut(host_id) {
                v.organize_session_id = None;
            }
        }
        Err(e) => {
            state
                .write()
                .hosts
                .set_error(host_id, format!("{what} failed: {e}"));
        }
    }
}

/// Open the session organize sheet (rename/pin/notify/verbs) for
/// `session_id`.
fn open_organize_sheet(mut state: SyncSignal<MobileState>, session_id: String) {
    let Some((host_id, _)) = active_client(&state) else {
        return;
    };
    if let Some(v) = state.write().views.get_mut(&host_id) {
        v.organize_session_id = Some(session_id);
        v.archive_sheet = None;
    }
}

/// Close any organize / archive sheet on the active Host view.
fn close_sheets(mut state: SyncSignal<MobileState>) {
    let Some((host_id, _)) = active_client(&state) else {
        return;
    };
    if let Some(v) = state.write().views.get_mut(&host_id) {
        v.organize_session_id = None;
        v.archive_sheet = None;
    }
}

/// Open the preset drawer ("New session" bottom sheet), mirroring iOS's
/// `showPresetDrawer`. The flat mobile session list has no project tree,
/// so each drawer row launches with its preset's own `project_id`.
fn open_preset_drawer(mut state: SyncSignal<MobileState>) {
    let Some((host_id, _)) = active_client(&state) else {
        return;
    };
    if let Some(v) = state.write().views.get_mut(&host_id) {
        v.preset_drawer_open = true;
        v.organize_session_id = None;
        v.archive_sheet = None;
    }
}

/// Close the preset drawer, clearing any in-flight launch marker.
fn close_preset_drawer(mut state: SyncSignal<MobileState>) {
    let Some((host_id, _)) = active_client(&state) else {
        return;
    };
    if let Some(v) = state.write().views.get_mut(&host_id) {
        v.preset_drawer_open = false;
        v.launching_preset_id = None;
    }
}

/// Arm the app lock from the Security toggle. `enable()` authenticates
/// first so the toggle only arms when the method actually works; the
/// blocking prompt runs on a background thread and the armed flag
/// persists only on success. The signal write guard is held across the
/// prompt — acceptable because the OS sheet covers the app meanwhile, and
/// it serializes the one-prompt invariant.
fn enable_app_lock(mut state: SyncSignal<MobileState>) {
    std::thread::spawn(move || {
        let store = state.read().store.clone();
        let armed = state.write().app_lock.enable();
        if armed {
            save_app_lock_enabled(&store, true);
        }
    });
}

/// Disarm the app lock. No auth gate: reaching the toggle required an
/// unlock.
fn disable_app_lock(mut state: SyncSignal<MobileState>) {
    let store = state.read().store.clone();
    state.write().app_lock.disable();
    save_app_lock_enabled(&store, false);
}

/// Fire the overlay's manual unlock (or the foreground auto-prompt) on a
/// background thread.
fn unlock_app(mut state: SyncSignal<MobileState>) {
    std::thread::spawn(move || {
        state.write().app_lock.unlock();
    });
}

/// The page became visible again: fire the single automatic unlock prompt
/// for this foreground, if the app is locked.
fn foreground_auto_unlock(mut state: SyncSignal<MobileState>) {
    let should_prompt = state.write().app_lock.begin_foreground_unlock();
    if should_prompt {
        unlock_app(state);
    }
}

/// Launch a preset from the drawer: `POST /mobile/sessions` with
/// `{projectID, presetID}` (the `RemoteCreateSessionRequest` shape).
/// Mirrors Swift's `startSession`: the row shows the launching spinner,
/// the drawer closes immediately, and on success the new session is
/// selected after a bounded bootstrap convergence (8 × 250 ms, like
/// Swift) so a headless Host's manifest has time to catch up. The create
/// mutation itself is never retried.
fn launch_preset(mut state: SyncSignal<MobileState>, preset_id: String) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    let project_id = {
        let s = state.read();
        s.hosts
            .get(&host_id)
            .and_then(|h| h.snapshot.clone())
            .and_then(|snap| {
                snap.presets
                    .iter()
                    .find(|p| p.id == preset_id)
                    .and_then(|p| p.project_id.clone())
            })
    };
    let Some(project_id) = project_id else {
        state.write().hosts.set_error(
            &host_id,
            "Could not start session: the preset has no project.".to_string(),
        );
        return;
    };
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.launching_preset_id = Some(preset_id.clone());
            v.preset_drawer_open = false;
        }
    }
    std::thread::spawn(move || {
        let created: Option<String> =
            match client.create_session_with_preset(&project_id, &preset_id) {
                Ok(body) => HostClient::created_session_id(&body),
                Err(e) => {
                    let mut s = state.write();
                    s.hosts
                        .set_error(&host_id, format!("Could not start session: {e}"));
                    if let Some(v) = s.views.get_mut(&host_id) {
                        v.launching_preset_id = None;
                    }
                    return;
                }
            };
        // Bounded convergence: the session id from the create response is
        // authoritative; refresh bootstrap until it appears (or attempts
        // run out), then select it regardless — never fall back to an
        // unrelated existing session.
        if let Some(new_id) = created {
            for _ in 0..8 {
                match client.bootstrap() {
                    Ok(snap) => {
                        let found = snap.sessions.iter().any(|s| s.id == new_id);
                        {
                            let mut s = state.write();
                            s.hosts.set_snapshot(&host_id, snap);
                            if let Some(v) = s.views.get_mut(&host_id) {
                                v.launching_preset_id = None;
                            }
                        }
                        if found {
                            break;
                        }
                    }
                    Err(e) => {
                        state
                            .write()
                            .hosts
                            .set_error(&host_id, format!("Could not start session: {e}"));
                        break;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            open_terminal(state, new_id);
        } else {
            let mut s = state.write();
            s.hosts.set_error(
                &host_id,
                "Could not start session: the Host returned no session id.".to_string(),
            );
            if let Some(v) = s.views.get_mut(&host_id) {
                v.launching_preset_id = None;
            }
        }
    });
}

/// Save the organize sheet's patch via the full session-organization
/// endpoint (`title`/`pinned`/`notifyWhenDone`; archived is untouched),
/// mirroring the iOS sheet's Save. `None` patch fields are left untouched
/// on the Host. A `Some` project id files the session under that project
/// through the portable move verb.
fn save_organize_session(
    mut state: SyncSignal<MobileState>,
    session_id: String,
    patch: SessionOrganizePatch,
) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    std::thread::spawn(move || {
        let result = client.update_session_organization_full(
            &session_id,
            patch.title.as_deref(),
            patch.pinned,
            None,
            patch.notify_when_done,
        );
        let result = result.and_then(|_| match patch.project_id.as_deref() {
            Some(project_id) => client.move_session_to_project(&session_id, project_id),
            None => Ok(serde_json::json!({"ok": true})),
        });
        match result {
            Ok(_) => refresh_after_organize(&mut state, &host_id, &client, "Save"),
            Err(e) => {
                state
                    .write()
                    .hosts
                    .set_error(&host_id, format!("Save failed: {e}"));
            }
        }
    });
}

/// Fire one of the organize sheet's session verbs. `remote_verb()` maps to
/// `/mobile/session-action`; Archive/Restore travel on the organization
/// patch instead — mirroring Swift's `SessionOrganizeSheet.remoteAction`.
fn fire_organize_action(
    mut state: SyncSignal<MobileState>,
    session_id: String,
    action: SessionSheetAction,
) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    let what = format!("{action:?}");
    std::thread::spawn(move || {
        let result = match action.remote_verb() {
            Some(verb) => client.session_action(verb, &session_id).map(|_| ()),
            None => {
                let archived = matches!(action, SessionSheetAction::Archive);
                client
                    .update_session_organization(&session_id, None, None, Some(archived))
                    .map(|_| ())
            }
        };
        match result {
            Ok(_) => refresh_after_organize(&mut state, &host_id, &client, &what),
            Err(e) => {
                state
                    .write()
                    .hosts
                    .set_error(&host_id, format!("{what} failed: {e}"));
            }
        }
    });
}

/// Open the archive-library sheet for a project and load its archived
/// sessions on a worker thread. Rows appear when the load completes.
fn open_archive_sheet(mut state: SyncSignal<MobileState>, project_id: String) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.organize_session_id = None;
            v.archive_sheet = Some(ArchiveSheetState {
                project_id: project_id.clone(),
                sessions: None,
                load_error: None,
            });
        }
    }
    std::thread::spawn(move || match client.archived_sessions(&project_id) {
        Ok(rows) => {
            let mut s = state.write();
            if let Some(sheet) = s
                .views
                .get_mut(&host_id)
                .and_then(|v| v.archive_sheet.as_mut())
            {
                if sheet.project_id == project_id {
                    sheet.sessions = Some(rows);
                }
            }
        }
        Err(e) => {
            let mut s = state.write();
            if let Some(sheet) = s
                .views
                .get_mut(&host_id)
                .and_then(|v| v.archive_sheet.as_mut())
            {
                if sheet.project_id == project_id {
                    sheet.load_error = Some(e.to_string());
                }
            }
        }
    });
}

/// Restore one archived row (and optionally resume it via the restart
/// verb), then reload the archive list and re-bootstrap the live list.
fn archive_restore(mut state: SyncSignal<MobileState>, action: ArchiveAction) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    let project_id = state
        .read()
        .views
        .get(&host_id)
        .and_then(|v| v.archive_sheet.as_ref())
        .map(|sheet| sheet.project_id.clone());
    let session_id = action.session_id.clone();
    let and_resume = action.and_resume;
    std::thread::spawn(move || {
        let result = client
            .update_session_organization(&session_id, None, None, Some(false))
            .and_then(|_| {
                if and_resume {
                    client.session_action("restart", &session_id).map(|_| ())
                } else {
                    Ok(())
                }
            });
        let what = if and_resume {
            "Restore & Resume"
        } else {
            "Restore"
        };
        match result {
            Ok(_) => {
                // Reload the archive list (row should be gone) and the
                // live sessions in one pass.
                if let Some(pid) = project_id {
                    let archive = client.archived_sessions(&pid);
                    let mut s = state.write();
                    if let Some(sheet) = s
                        .views
                        .get_mut(&host_id)
                        .and_then(|v| v.archive_sheet.as_mut())
                    {
                        if sheet.project_id == pid {
                            match archive {
                                Ok(rows) => sheet.sessions = Some(rows),
                                Err(e) => sheet.load_error = Some(e.to_string()),
                            }
                        }
                    }
                }
                match client.bootstrap() {
                    Ok(snap) => {
                        state.write().hosts.set_snapshot(&host_id, snap);
                    }
                    Err(e) => {
                        state
                            .write()
                            .hosts
                            .set_error(&host_id, format!("{what} failed: {e}"));
                    }
                }
            }
            Err(e) => {
                state
                    .write()
                    .hosts
                    .set_error(&host_id, format!("{what} failed: {e}"));
            }
        }
    });
}

/// Archive-library sheet state for one Host view: the project whose
/// archive is shown, plus the loaded rows (`None` = still loading).
#[derive(Clone, Default)]
struct ArchiveSheetState {
    project_id: String,
    sessions: Option<Vec<SessionSummary>>,
    load_error: Option<String>,
}

/// Browser gallery state for one Host view. Artifacts are per-session, so
/// the gallery is opened for the selected session; closing the terminal
/// closes the gallery too.
#[derive(Clone, Default)]
struct GalleryUi {
    /// Session whose artifacts are shown; `None` = gallery closed.
    session_id: Option<String>,
    /// Bumped on every open/close so stale loader threads can't publish
    /// into a newer gallery state.
    generation: u64,
    loading: bool,
    entries: Vec<ArtifactMeta>,
    error: Option<String>,
    /// Grid thumbnails: "kind/name" → data URL, loaded lazily through the
    /// Host's `max_dim` thumbnail transport. Detail always uses the full
    /// bytes; the grid never does.
    thumbs: HashMap<String, String>,
    /// Thumbnail keys with a loader thread in flight, so one grid render
    /// can't spawn duplicate loaders.
    thumbs_inflight: std::collections::HashSet<String>,
    /// Detail: the open entry's meta + downloaded bytes as a data URL.
    detail_meta: Option<ArtifactMeta>,
    detail_data_url: Option<String>,
    detail_bytes: Option<Vec<u8>>,
    detail_mime: Option<String>,
    detail_loading: bool,
    /// Which annotation editor is open over the detail image.
    editor: Option<AnnotationMode>,
    /// In-progress flatten/upload step, shown as a status line.
    busy: Option<String>,
}

/// Load the artifact list for the gallery's session, unless a newer
/// gallery generation superseded this call.
fn gallery_load_entries(
    mut state: SyncSignal<MobileState>,
    host_id: String,
    client: HostClient,
    session_id: String,
    generation: u64,
) {
    std::thread::spawn(move || {
        let result = client.browser_artifacts(&session_id);
        let loaded: Option<Vec<ArtifactMeta>> = {
            let mut s = state.write();
            let Some(v) = s.views.get_mut(&host_id) else {
                return;
            };
            if v.gallery.generation != generation
                || v.gallery.session_id.as_deref() != Some(&session_id)
            {
                return;
            }
            v.gallery.loading = false;
            match result {
                Ok(entries) => {
                    v.gallery.entries = entries.clone();
                    v.gallery.error = None;
                    Some(entries)
                }
                Err(e) => {
                    v.gallery.error = Some(format!("Gallery load failed: {e}"));
                    None
                }
            }
        };
        // The write guard is dropped above; thumbnail loaders take their
        // own guards with the same generation check.
        if let Some(entries) = loaded {
            gallery_ensure_thumbs(state, host_id, client, session_id, entries, generation);
        }
    });
}

/// Cache key for one grid thumbnail.
fn gallery_thumb_key(kind: &str, name: &str) -> String {
    format!("{kind}/{name}")
}

/// Whether an entry deserves a grid thumbnail: images and screenshots,
/// by kind or by file extension (mirrors the shared `GalleryEntry`).
fn gallery_entry_is_image(meta: &ArtifactMeta) -> bool {
    if meta.kind == "image" || meta.kind == "screenshot" {
        return true;
    }
    let lower = meta.name.to_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// Load grid thumbnails for image entries through the Host's `max_dim`
/// thumbnail transport (512 px, like the Swift client). One loader thread
/// per entry, generation-guarded like every other gallery loader: a stale
/// thread never publishes into a newer gallery.
fn gallery_ensure_thumbs(
    mut state: SyncSignal<MobileState>,
    host_id: String,
    client: HostClient,
    session_id: String,
    entries: Vec<ArtifactMeta>,
    generation: u64,
) {
    // Mark in-flight first so concurrent renders can't double-spawn.
    let pending: Vec<(String, String, String)> = {
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        entries
            .iter()
            .filter(|m| gallery_entry_is_image(m))
            .map(|m| {
                (
                    gallery_thumb_key(&m.kind, &m.name),
                    m.kind.clone(),
                    m.name.clone(),
                )
            })
            .filter(|(key, _, _)| {
                !v.gallery.thumbs.contains_key(key) && v.gallery.thumbs_inflight.insert(key.clone())
            })
            .collect()
    };
    for (key, kind, name) in pending {
        let mut state = state;
        let host_id = host_id.clone();
        let client = client.clone();
        let session_id = session_id.clone();
        std::thread::spawn(move || {
            let result = client.artifact_thumbnail_bytes(&session_id, &kind, &name, 512);
            let mut s = state.write();
            let Some(v) = s.views.get_mut(&host_id) else {
                return;
            };
            v.gallery.thumbs_inflight.remove(&key);
            if v.gallery.generation != generation
                || v.gallery.session_id.as_deref() != Some(&session_id)
            {
                return;
            }
            if let Ok((mime, bytes)) = result {
                use base64::Engine as _;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                v.gallery
                    .thumbs
                    .insert(key, format!("data:{mime};base64,{b64}"));
            }
            // A failed thumbnail just leaves the kind placeholder; the
            // full image still loads in detail.
        });
    }
}

/// Reload the gallery grid for the open session.
fn gallery_refresh(mut state: SyncSignal<MobileState>, host_id: String) {
    let s = state.read();
    let (client, session_id, generation) = match (s.hosts.get(&host_id), s.views.get(&host_id)) {
        (Some(h), Some(v)) => match v.gallery.session_id.clone() {
            Some(sid) => (h.client.clone(), sid, v.gallery.generation),
            None => return,
        },
        _ => return,
    };
    drop(s);
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.gallery.loading = true;
            v.gallery.error = None;
        }
    }
    gallery_load_entries(state, host_id, client, session_id, generation);
}

/// Open the gallery for the currently selected session.
fn gallery_open(mut state: SyncSignal<MobileState>) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    let session_id = state
        .read()
        .views
        .get(&host_id)
        .and_then(|v| v.selected_session.clone());
    let Some(session_id) = session_id else { return };
    let generation = {
        let mut s = state.write();
        let v = s.views.entry(host_id.clone()).or_default();
        v.gallery = GalleryUi {
            session_id: Some(session_id.clone()),
            generation: v.gallery.generation + 1,
            loading: true,
            ..Default::default()
        };
        v.gallery.generation
    };
    gallery_load_entries(state, host_id, client, session_id, generation);
}

/// Close the gallery (back to the terminal).
fn gallery_close(mut state: SyncSignal<MobileState>, host_id: String) {
    let mut s = state.write();
    if let Some(v) = s.views.get_mut(&host_id) {
        v.gallery.generation += 1;
        v.gallery = GalleryUi {
            generation: v.gallery.generation,
            ..Default::default()
        };
    }
}

/// Open one artifact in the detail view: downloads the full bytes and
/// serves them to the `<img>` as a data URL (the Host's artifact route
/// needs the client's auth, which a bare `<img src>` can't provide).
fn gallery_open_entry(mut state: SyncSignal<MobileState>, host_id: String, meta: ArtifactMeta) {
    let s = state.read();
    let (client, session_id) = match (
        s.hosts.get(&host_id),
        s.views
            .get(&host_id)
            .and_then(|v| v.gallery.session_id.clone()),
    ) {
        (Some(h), Some(sid)) => (h.client.clone(), sid),
        _ => return,
    };
    drop(s);
    let generation = {
        let mut s = state.write();
        let v = s.views.get_mut(&host_id).expect("view exists");
        v.gallery.detail_meta = Some(meta.clone());
        v.gallery.detail_data_url = None;
        v.gallery.detail_bytes = None;
        v.gallery.detail_mime = None;
        v.gallery.detail_loading = true;
        v.gallery.editor = None;
        v.gallery.error = None;
        v.gallery.generation
    };
    let kind = meta.kind.clone();
    let name = meta.name.clone();
    std::thread::spawn(move || {
        let result = client.artifact_bytes(&session_id, &kind, &name);
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        if v.gallery.generation != generation
            || v.gallery
                .detail_meta
                .as_ref()
                .is_some_and(|m| m.name != name)
        {
            return;
        }
        v.gallery.detail_loading = false;
        match result {
            Ok((mime, bytes)) => {
                use base64::Engine as _;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                v.gallery.detail_data_url = Some(format!("data:{mime};base64,{b64}"));
                v.gallery.detail_bytes = Some(bytes);
                v.gallery.detail_mime = Some(mime);
            }
            Err(e) => v.gallery.error = Some(format!("Couldn't load {name}: {e}")),
        }
    });
}

/// Delete the open detail entry (two-tap confirm lives in the view).
/// Delete one gallery entry (two-tap confirm lives in the shared detail
/// view / grid). `meta` is the entry to delete — the detail view passes
/// the open entry, the grid passes the tapped one.
fn gallery_delete_entry(mut state: SyncSignal<MobileState>, host_id: String, meta: ArtifactMeta) {
    let s = state.read();
    let (client, session_id, generation) = match (s.hosts.get(&host_id), s.views.get(&host_id)) {
        (Some(h), Some(v)) => match v.gallery.session_id.clone() {
            Some(sid) => (h.client.clone(), sid, v.gallery.generation),
            None => return,
        },
        _ => return,
    };
    drop(s);
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.gallery.busy = Some("Deleting…".to_string());
        }
    }
    let kind = meta.kind.clone();
    let name = meta.name.clone();
    std::thread::spawn(move || {
        let result = client.delete_artifact(&session_id, &kind, &name);
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        if v.gallery.generation != generation {
            return;
        }
        v.gallery.busy = None;
        match result {
            Ok(_) => {
                // Back to the grid; the entry is gone.
                v.gallery.detail_meta = None;
                v.gallery.detail_data_url = None;
                v.gallery.detail_bytes = None;
                v.gallery.detail_mime = None;
                v.gallery.editor = None;
                v.gallery.thumbs.remove(&gallery_thumb_key(&kind, &name));
                v.gallery.loading = true;
            }
            Err(e) => {
                v.gallery.error = Some(format!("Delete failed: {e}"));
                return;
            }
        }
        drop(s);
        gallery_load_entries(state, host_id, client, session_id, generation);
    });
}

/// Ask the Host to capture a screenshot into this session's gallery.
fn gallery_screenshot(mut state: SyncSignal<MobileState>, host_id: String) {
    let s = state.read();
    let (client, session_id, generation) = match (s.hosts.get(&host_id), s.views.get(&host_id)) {
        (Some(h), Some(v)) => match v.gallery.session_id.clone() {
            Some(sid) => (h.client.clone(), sid, v.gallery.generation),
            None => return,
        },
        _ => return,
    };
    drop(s);
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.gallery.busy = Some("Capturing screenshot…".to_string());
        }
    }
    std::thread::spawn(move || {
        let result = client.request_screenshot(&session_id);
        let asked_ok = result.is_ok();
        {
            let mut s = state.write();
            let Some(v) = s.views.get_mut(&host_id) else {
                return;
            };
            if v.gallery.generation != generation {
                return;
            }
            v.gallery.busy = None;
            match result {
                Ok(_) => v.gallery.loading = true,
                Err(e) => v.gallery.error = Some(format!("Screenshot failed: {e}")),
            }
        }
        if asked_ok {
            // Give the Host a beat to write the file, then reload.
            std::thread::sleep(std::time::Duration::from_millis(1500));
            gallery_load_entries(state, host_id, client, session_id, generation);
        }
    });
}

/// Bracketed-paste text into the selected session's terminal through the
/// ordered terminal writer. Mirrors the Swift client's bracketed-paste
/// commit paths (dictation insert, gallery add-to-message): pending
/// keystroke predictions are cleared because a paste is not a keystroke
/// the engine can echo.
fn paste_into_terminal(mut state: SyncSignal<MobileState>, host_id: &str, text: &str) {
    let framed = format!("\x1b[200~{text}\x1b[201~");
    let mut s = state.write();
    let Some(v) = s.views.get_mut(host_id) else {
        return;
    };
    v.prediction.clear_pending();
    if let Some(tx) = v.terminal_writer.clone() {
        let _ = tx.send(framed);
    }
}

/// "Add to message": upload the open entry's bytes and bracketed-paste
/// the Host's returned absolute path into the terminal, exactly like the
/// Swift client's `attachImage` (upload → quoted `'path' ` with trailing
/// space → bracketed paste). The artifact is re-uploaded rather than
/// referenced by name because the name alone is not a path the agent can
/// resolve — only the upload completion's `path` is.
///
/// To attach annotated markup, annotate first: that uploads a new entry,
/// which can then be added to the message.
fn gallery_add_to_message(mut state: SyncSignal<MobileState>, host_id: String) {
    let s = state.read();
    let (client, session_id, bytes, mime, generation) =
        match (s.hosts.get(&host_id), s.views.get(&host_id)) {
            (Some(h), Some(v)) => match (
                v.gallery.session_id.clone(),
                v.gallery.detail_bytes.clone(),
                v.gallery.detail_mime.clone(),
            ) {
                (Some(sid), Some(b), Some(m)) => {
                    (h.client.clone(), sid, b, m, v.gallery.generation)
                }
                _ => return,
            },
            _ => return,
        };
    drop(s);
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.gallery.busy = Some("Attaching…".to_string());
            v.gallery.error = None;
        }
    }
    std::thread::spawn(move || {
        let result = client.upload_artifact(&session_id, &mime, &bytes);
        // Paste and gallery-close both need the write guard; do them together.
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        if v.gallery.generation != generation {
            return;
        }
        v.gallery.busy = None;
        match result {
            Ok(path) => {
                // Swift quoting: single-quote, `'` → `'\''`, trailing space
                // so the path stays separate from what the user types next.
                let quoted = format!("'{}' ", path.replace('\'', "'\\''"));
                drop(s);
                paste_into_terminal(state, &host_id, &quoted);
                gallery_close(state, host_id);
            }
            Err(e) => {
                v.gallery.error = Some(format!("Couldn't attach the image: {e}"));
            }
        }
    });
}

/// One-shot file picker: opens the OS picker, reads the chosen image as a
/// data URL, and sends `pick:data:<data-url>` back over its own eval
/// channel (`pick:cancelled` on cancel, `pick:error` on read failure).
/// Per-eval `dioxus` channels mean this never collides with the QR or
/// dictation pumps.
const GALLERY_PICKER_JS: &str = r##"
(async () => {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = 'image/*';
  input.onchange = () => {
    const f = input.files && input.files[0];
    if (!f) { dioxus.send('pick:cancelled'); return; }
    const fr = new FileReader();
    fr.onload = () => dioxus.send('pick:data:' + fr.result);
    fr.onerror = () => dioxus.send('pick:error');
    fr.readAsDataURL(f);
  };
  input.oncancel = () => dioxus.send('pick:cancelled');
  input.click();
})()
"##;

/// Upload a photo from the device photo library into the session gallery
/// (Swift `PhotosPicker` parity). After a successful upload the grid
/// reloads and the newest image entry opens, matching the Host's
/// upload-completion contract.
fn gallery_upload_pick(mut state: SyncSignal<MobileState>, host_id: String) {
    let s = state.read();
    let (client, session_id, generation) = match (s.hosts.get(&host_id), s.views.get(&host_id)) {
        (Some(h), Some(v)) => match v.gallery.session_id.clone() {
            Some(sid) => (h.client.clone(), sid, v.gallery.generation),
            None => return,
        },
        _ => return,
    };
    drop(s);
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.gallery.busy = Some("Choose a photo…".to_string());
            v.gallery.error = None;
        }
    }
    spawn(async move {
        let mut ev = dioxus::document::eval(GALLERY_PICKER_JS);
        let msg = ev.recv::<String>().await.ok();
        let picked: Option<(String, Vec<u8>)> = match msg.as_deref() {
            Some(m) if m.starts_with("pick:data:") => {
                let url = &m["pick:data:".len()..];
                // data:<mime>;base64,<payload>
                let (head, b64) = url.split_once(',').unwrap_or(("", ""));
                let mime = head
                    .strip_prefix("data:")
                    .and_then(|h| h.split(';').next())
                    .filter(|m| !m.is_empty())
                    .unwrap_or("image/png")
                    .to_string();
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .decode(b64)
                    .ok()
                    .map(|bytes| (mime, bytes))
            }
            _ => None,
        };
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        if v.gallery.generation != generation {
            return;
        }
        let Some((mime, bytes)) = picked else {
            v.gallery.busy = None;
            return;
        };
        v.gallery.busy = Some("Uploading…".to_string());
        drop(s);
        std::thread::spawn(move || {
            let result = client.upload_artifact(&session_id, &mime, &bytes);
            let mut s = state.write();
            let Some(v) = s.views.get_mut(&host_id) else {
                return;
            };
            if v.gallery.generation != generation {
                return;
            }
            match result {
                Ok(_path) => {
                    v.gallery.busy = None;
                    v.gallery.loading = true;
                    drop(s);
                    gallery_load_entries(state, host_id, client, session_id, generation);
                }
                Err(e) => {
                    v.gallery.busy = None;
                    v.gallery.error = Some(format!("Upload failed: {e}"));
                }
            }
        });
    });
}

/// Share the open detail entry (Swift `ShareLink` parity, as far as a
/// webview build goes). The script is built by the shared
/// [`share_entry_js`]: the whole argument payload is one JSON object
/// substituted exactly once, so hostile filenames can't corrupt it.
fn gallery_share_entry(mut state: SyncSignal<MobileState>, host_id: String) {
    let (name, mime, bytes) = match state.read().views.get(&host_id) {
        Some(v) => match (
            v.gallery.detail_meta.clone(),
            v.gallery.detail_mime.clone(),
            v.gallery.detail_bytes.clone(),
        ) {
            (Some(m), Some(mime), Some(b)) => (m.name, mime, b),
            _ => return,
        },
        None => return,
    };
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.gallery.busy = Some("Sharing…".to_string());
            v.gallery.error = None;
        }
    }
    let js = share_entry_js(&name, &mime, &bytes);
    spawn(async move {
        let mut ev = dioxus::document::eval(&js);
        let outcome = ev.recv::<String>().await.ok();
        let mut s = state.write();
        let Some(v) = s.views.get_mut(&host_id) else {
            return;
        };
        v.gallery.busy = None;
        match outcome.as_deref() {
            Some("share:shared") => {}
            Some("share:downloaded") => {
                v.gallery.busy = Some("Downloaded — sharing isn't available here".to_string());
            }
            Some(m) if m.starts_with("share:error:") => {
                v.gallery.error = Some(format!(
                    "Share failed: {}",
                    m.strip_prefix("share:error:").unwrap_or("unknown")
                ));
            }
            _ => {}
        }
    });
}

/// Handle a finished annotation session: flatten at native resolution in
/// the webview, upload the PNG as a new artifact, and reload the grid.
/// The original artifact is never modified — the annotated copy is a new
/// gallery entry, matching the Swift client's non-destructive editing.
fn gallery_annotation_done(
    mut state: SyncSignal<MobileState>,
    host_id: String,
    result: AnnotationResult,
) {
    let s = state.read();
    let (session_id, bytes, mime, generation) = match s.views.get(&host_id) {
        Some(v) => match (
            v.gallery.session_id.clone(),
            v.gallery.detail_bytes.clone(),
            v.gallery.detail_mime.clone(),
        ) {
            (Some(sid), Some(b), Some(m)) => (sid, b, m, v.gallery.generation),
            _ => return,
        },
        None => return,
    };
    let client = s.hosts.get(&host_id).map(|h| h.client.clone());
    drop(s);
    let Some(client) = client else { return };
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
            v.gallery.editor = None;
            v.gallery.busy = Some("Flattening annotation…".to_string());
        }
    }
    let (arrows, strokes, crop) = match result {
        AnnotationResult::Arrows(a) => (a, Vec::new(), None),
        AnnotationResult::Freehand(s) => (Vec::new(), s, None),
        AnnotationResult::Crop(c) => (Vec::new(), Vec::new(), Some(c)),
    };
    spawn(async move {
        let spec = flatten_spec(&bytes, &mime, &arrows, &strokes, crop);
        let png = match flatten_annotation_png(&spec).await {
            Ok(png) => png,
            Err(e) => {
                let mut s = state.write();
                if let Some(v) = s.views.get_mut(&host_id) {
                    v.gallery.busy = None;
                    v.gallery.error = Some(format!("Flatten failed: {e}"));
                }
                return;
            }
        };
        {
            let mut s = state.write();
            if let Some(v) = s.views.get_mut(&host_id) {
                v.gallery.busy = Some("Uploading annotated image…".to_string());
            }
        }
        // The upload is blocking: keep it off the async runtime.
        let (mut state2, host_id2, client2) = (state, host_id.clone(), client.clone());
        std::thread::spawn(move || {
            let result = client2.upload_artifact(&session_id, "image/png", &png);
            let mut s = state2.write();
            let Some(v) = s.views.get_mut(&host_id) else {
                return;
            };
            if v.gallery.generation != generation {
                return;
            }
            v.gallery.busy = None;
            match result {
                Ok(_) => {
                    // Show the grid with the new annotated entry.
                    v.gallery.detail_meta = None;
                    v.gallery.detail_data_url = None;
                    v.gallery.detail_bytes = None;
                    v.gallery.detail_mime = None;
                    v.gallery.loading = true;
                }
                Err(e) => {
                    v.gallery.error = Some(format!("Upload failed: {e}"));
                    return;
                }
            }
            drop(s);
            gallery_load_entries(state2, host_id2, client, session_id, generation);
        });
    });
}

fn main() {
    dioxus::launch(MobileApp);
}

/// Whether the poll/writer thread for (`host_id`, `generation`) should keep
/// running: the Host is still connected and its view still expects this
/// generation. Forgetting the Host removes its view, which stops the
/// threads without touching any other Host.
fn terminal_alive(state: &SyncSignal<MobileState>, host_id: &str, generation: u64) -> bool {
    let s = state.read();
    s.hosts.contains(host_id)
        && s.views
            .get(host_id)
            .is_some_and(|v| v.poll_generation == generation)
}

/// Open a session's live terminal on the active Host: size the PTY, then
/// long-poll its output in a background thread, publishing snapshots to
/// the UI.
///
/// The state signal is a [`SyncSignal`], so the poll thread can safely
/// publish snapshots while the UI thread renders them.
fn open_terminal(mut state: SyncSignal<MobileState>, session_id: String) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    let (writer_tx, writer_rx) = std::sync::mpsc::channel::<String>();
    let generation = {
        let mut s = state.write();
        let view = s.views.entry(host_id.clone()).or_default();
        let generation = view.poll_generation + 1;
        view.poll_generation = generation;
        view.selected_session = Some(session_id.clone());
        view.terminal_error = None;
        view.terminal_writer = Some(writer_tx);
        // Fresh terminal: default grid, no pending remodel, cell metrics
        // re-probed once the view renders (see the effect in MobileApp).
        view.pty_size = (DEFAULT_COLS, DEFAULT_ROWS);
        view.pty_generation += 1; // stale resize threads skip their resize
        view.pty_remodel = false;
        view.cell_px = None;
        // Show an empty grid immediately; the poll loop fills it in.
        view.terminal_snapshot = Some(TerminalModel::new(DEFAULT_COLS, DEFAULT_ROWS).snapshot());
        // Fresh predictions: the new session's grid is unrelated to the old.
        view.prediction.reset();
        view.scroll.reset_confidence();
        view.scroll_accum = 0.0;
        view.scroll_gesture_active = false;
        view.scroll_gesture_gen += 1;
        view.press_point = None;
        view.press_rect = None;
        view.press_gen += 1;
        view.selection_request = None;
        generation
    };

    // One ordered writer per open session: keystrokes queue here and go
    // out in order, no matter how fast they arrive.
    {
        let sid = session_id.clone();
        let hid = host_id.clone();
        let writer_client = client.clone();
        std::thread::spawn(move || {
            while let Ok(seq) = writer_rx.recv() {
                if !terminal_alive(&state, &hid, generation) {
                    break;
                }
                let _ = writer_client.write(&sid, &seq);
            }
        });
    }

    std::thread::spawn(move || {
        // Size the PTY to the viewport before streaming.
        let _ = client.resize(&session_id, DEFAULT_COLS as i64, DEFAULT_ROWS as i64);
        let mut model = TerminalModel::new(DEFAULT_COLS, DEFAULT_ROWS);
        let mut offset: Option<u64> = None;
        loop {
            if !terminal_alive(&state, &host_id, generation) {
                break; // user navigated away or opened another session
            }
            // The app-lock cover is up: stop streaming (reconnect + replay)
            // until the user unlocks — mirroring Swift's `shouldStream`
            // gate on `AppLockManager.shared.isLocked`. Sleep instead of
            // busy-spinning; the generation check above still exits.
            if state.read().app_lock.is_locked() {
                std::thread::sleep(std::time::Duration::from_millis(250));
                continue;
            }
            // A viewport resize succeeded remotely: rebuild the local grid
            // at the new size. Full-screen apps repaint on the SIGWINCH the
            // Host sent with the resize, same as any terminal emulator.
            let remodel = {
                let s = state.read();
                s.views.get(&host_id).map(|v| (v.pty_remodel, v.pty_size))
            };
            if let Some((true, (cols, rows))) = remodel {
                model = TerminalModel::new(cols, rows);
                let mut s = state.write();
                if let Some(v) = s.views.get_mut(&host_id) {
                    v.pty_remodel = false;
                    // The grid coordinates changed: predictions anchored to
                    // the old grid are meaningless.
                    v.prediction.reset();
                    v.scroll.reset_confidence();
                    v.scroll_accum = 0.0;
                    v.scroll_gesture_active = false;
                    let snap = model.snapshot();
                    if let Some(f) = v.find.as_mut() {
                        f.refresh(&snap);
                    }
                    v.terminal_snapshot = Some(snap);
                }
            }
            match client.output_chunk(&session_id, offset, OUTPUT_LIMIT, OUTPUT_WAIT_MS) {
                Ok(chunk) => {
                    if chunk.truncated {
                        // Host rotated the output log; restart from the tail.
                        // A replay/rebase repainted the world: predictions
                        // and their earned confidence belong to the previous
                        // picture.
                        model.reset();
                        offset = None;
                        if let Some(v) = state.write().views.get_mut(&host_id) {
                            v.prediction.reset();
                            v.scroll.reset_confidence();
                        }
                        continue;
                    }
                    offset = Some(chunk.next_offset);
                    // Capture the viewport before feeding while scroll
                    // predictions are outstanding: the shift detector
                    // reconciles wheel predictions by observed content
                    // movement, not per-chunk counting.
                    let shift_before = {
                        let s = state.read();
                        let tracking = s
                            .views
                            .get(&host_id)
                            .is_some_and(|v| v.scroll.pending_rows() != 0);
                        tracking.then(|| viewport_text(&model.snapshot()))
                    };
                    if !chunk.data.is_empty() {
                        model.feed(&chunk.data);
                    }
                    if !terminal_alive(&state, &host_id, generation) {
                        break;
                    }
                    if let Some(v) = state.write().views.get_mut(&host_id) {
                        let snap = model.snapshot();
                        // Reconcile provisional keystrokes against the
                        // authoritative grid: confirmations open the
                        // display gate, contradictions and expiry close it.
                        let text = viewport_text(&snap);
                        let rows: Vec<&str> = text.split('\n').collect();
                        v.prediction.reconcile(&rows, Instant::now());
                        if let Some(before) = shift_before {
                            // Drain scroll predictions by the rows this chunk
                            // actually moved the content — one chunk can
                            // carry several coalesced redraws, and a
                            // streaming chunk may have scrolled nothing — in
                            // the same pass the moved content lands.
                            let max_shift = ScrollPredictionEngine::MAXIMUM_PENDING_ROWS
                                .min(4.max(2 * v.scroll.pending_rows().abs()))
                                as usize;
                            let shifted = detect_scroll_shift(&before, &text, max_shift);
                            v.scroll.content_shifted(shifted, Instant::now());
                        }
                        // Keep open find matches in sync with the new grid.
                        if let Some(f) = v.find.as_mut() {
                            f.refresh(&snap);
                        }
                        v.terminal_snapshot = Some(snap);
                    }
                }
                Err(e) => {
                    if !terminal_alive(&state, &host_id, generation) {
                        break;
                    }
                    if let Some(v) = state.write().views.get_mut(&host_id) {
                        v.terminal_error = Some(e.to_string());
                    }
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        }
    });
}

/// Debounce window for viewport resize bursts (rotation animations fire
/// a stream of resize events; only the last one in the burst resizes).
const VIEWPORT_DEBOUNCE_MS: u64 = 400;

/// JS probe that measures one terminal cell in CSS px from the rendered
/// font: a 10ch-wide probe gives the cell width, a rendered terminal row
/// gives the line height. Runs inside `.terminal-view` so it inherits the
/// exact font the grid renders in. Returns `[cell_w, cell_h]`, `[0, 0]`
/// when the terminal is not mounted yet.
const CELL_PROBE_JS: &str = r#"(function() {
  var term = document.querySelector('.terminal-view');
  if (!term) return [0, 0];
  var probe = document.createElement('span');
  probe.style.cssText = 'display:inline-block;width:10ch;visibility:hidden;position:absolute;top:0;left:0;';
  probe.textContent = 'x';
  term.appendChild(probe);
  var r = probe.getBoundingClientRect();
  var row = term.querySelector('.terminal-row');
  var h = row ? row.getBoundingClientRect().height : 0;
  var out = [r.width / 10, h];
  probe.remove();
  return out;
})()"#;

/// Copy text to the OS clipboard through the webview.
///
/// The argument is JSON-encoded into a JS string literal so newlines,
/// quotes, and backslashes survive the eval boundary. Failures (e.g.
/// clipboard permission denied) are silent — the selection sheet stays
/// open and the user can still copy from the textarea manually.
/// Modifier-click (Cmd/Ctrl) on a terminal row: match the row's text for
/// path-like tokens (ClickablePath.swift parity). Exactly one candidate
/// copies the raw path to the clipboard; zero or several are ignored, so
/// the click is never stolen by a guess. The phone is a remote Controller,
/// so "opening" the path on the phone's own filesystem would be wrong —
/// copy is the safe action.
fn handle_path_click(state: SyncSignal<MobileState>, host_id: String, req: PathClickRequest) {
    let text = {
        let s = state.read();
        let Some(view) = s.views.get(&host_id) else {
            return;
        };
        let Some(snap) = view.terminal_snapshot.as_ref() else {
            return;
        };
        let Some(row) = snap.rows.get(req.row) else {
            return;
        };
        row_text(row)
    };
    let mut matches = matches_in_row(&text);
    if matches.len() == 1 {
        let (_, _, m) = matches.pop().unwrap();
        copy_to_clipboard(&m.path);
    }
}

fn copy_to_clipboard(text: &str) {
    let quoted = serde_json::to_string(text).unwrap_or_default();
    let js = format!("navigator.clipboard.writeText({quoted}).catch(()=>{{}})");
    let _ = dioxus::document::eval(&js);
}

/// Apply a viewport-driven grid size to the open session's remote PTY.
///
/// Called from the terminal container's `onresize` (which fires for both
/// rotation and the virtual keyboard). The [`should_resize_remote`] policy
/// drops keyboard-driven changes — same width, height shrank — so the
/// remote grid never follows the keyboard; that matches the platform
/// invariant "keyboard focus must never resize the remote grid".
///
/// Rapid bursts (rotation animations) are debounced: the claim is updated
/// immediately so events converge, but the actual resize is sent only if
/// no newer claim arrived within the debounce window, and stale threads
/// skip their resize via the generation check.
fn apply_viewport_size(mut state: SyncSignal<MobileState>, host_id: String, cols: u16, rows: u16) {
    let (session_id, last) = {
        let s = state.read();
        let view = match s.views.get(&host_id) {
            Some(v) => v,
            None => return,
        };
        if !s.hosts.contains(&host_id) {
            return;
        }
        (view.selected_session.clone(), view.pty_size)
    };
    let next = (cols, rows);
    if !should_resize_remote(last, next) {
        return;
    }
    let Some(sid) = session_id else {
        return; // no terminal open on this Host
    };
    let generation = {
        let mut s = state.write();
        let generation = s
            .views
            .get(&host_id)
            .map(|v| v.pty_generation + 1)
            .unwrap_or(1);
        if let Some(v) = s.views.get_mut(&host_id) {
            v.pty_size = next; // claim it; the resize thread confirms remotely
            v.pty_generation = generation;
        }
        generation
    };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(VIEWPORT_DEBOUNCE_MS));
        // A newer claim arrived while we slept, or the Host went away:
        // this thread no longer owns the resize.
        {
            let s = state.read();
            let current = s.views.get(&host_id).map(|v| v.pty_generation);
            if current != Some(generation) {
                return;
            }
        }
        let client = {
            let s = state.read();
            s.hosts.get(&host_id).map(|h| h.client.clone())
        };
        let Some(client) = client else { return };
        match client.resize(&sid, cols as i64, rows as i64) {
            Ok(_) => {
                // Remote grid moved: have the poll loop rebuild the local
                // model to match on its next iteration.
                let mut s = state.write();
                if let Some(v) = s.views.get_mut(&host_id) {
                    if v.pty_generation == generation {
                        v.pty_remodel = true;
                    }
                }
            }
            Err(_) => {
                // Resize failed: revert the claim so the next viewport
                // change retries instead of assuming the new size.
                let mut s = state.write();
                if let Some(v) = s.views.get_mut(&host_id) {
                    if v.pty_generation == generation && v.pty_size == next {
                        v.pty_size = last;
                    }
                }
            }
        }
    });
}

#[component]
fn MobileApp() -> Element {
    let mut state: SyncSignal<MobileState> = use_signal_sync(MobileState::initial);

    // Typed session-event consumption (Phase 7 C3): per-session turn
    // running state driven by the Host's event stream, on its own OS
    // thread with a blocking sleep — never parks the Dioxus async
    // runtime. Mirrors the desktop launcher's poll loop. `turn.cancelled`
    // also ends the turn (the Host interrupted it); `needs_review`,
    // `tool.ambiguous`, and the lease events are consumed for future UI
    // surfacing (currently they only feed the turn state, keeping the
    // stream drained so the cursor advances).
    let event_turn_running: SyncSignal<std::collections::HashMap<String, bool>> =
        use_signal_sync(std::collections::HashMap::<String, bool>::new);
    {
        let state = state;
        let mut event_turn_running = event_turn_running;
        std::thread::spawn(move || {
            let mut cursors: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(2000));
                let (host_id, client) = match active_client(&state) {
                    Some(pair) => pair,
                    None => continue,
                };
                let session_id = {
                    let s = state.read();
                    s.views
                        .get(&host_id)
                        .and_then(|v| v.selected_session.clone())
                        .unwrap_or_default()
                };
                if session_id.is_empty() {
                    continue;
                }
                let after_seq = cursors.get(&session_id).copied().unwrap_or(0);
                let response = match client.events(&session_id, after_seq, 128) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if response.resync {
                    cursors.insert(session_id.clone(), 0);
                    event_turn_running.write().remove(&session_id);
                    continue;
                }
                cursors.insert(session_id.clone(), response.next_seq);
                for event in &response.events {
                    use unpeel_client::events::SessionEventWire;
                    match event {
                        SessionEventWire::TurnStarted { .. } => {
                            event_turn_running.write().insert(session_id.clone(), true);
                        }
                        SessionEventWire::TurnFinished { .. }
                        | SessionEventWire::TurnCancelled { .. } => {
                            event_turn_running.write().insert(session_id.clone(), false);
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    // Mount effect: install the QR camera bridge (vendored jsQR + the
    // `__unpeelQrStart/Stop` functions the shared QrScannerView drives),
    // and arm Direct probe-back for Hosts that connected over the relay
    // at startup. `pending_probes` is drained here because no signal
    // existed when `initial()` ran.
    use_effect(move || {
        // Wire the push manager's callbacks now that the signal exists,
        // then seed a dev token if the environment provides one.
        {
            let mut st = state;
            let st2 = st;
            let st3 = st;
            let mut s = st.write();
            s.push.on_token_change =
                Some(Box::new(move |hex, env| push_upload_token(st2, hex, env)));
            s.push.on_open_session = Some(Box::new(move |session_id| {
                push_open_session(st3, session_id)
            }));
        }
        push_ingest_env_token(state);
        // Native-shell push bridge: install `window.__unpeelPush`, pick up
        // a token that arrived before this pump existed (the shell stashes
        // it at `window.__unpeelPushToken`), then pump `push:` messages for
        // the app's lifetime. Each message is a short signal write; the
        // token upload runs on its own background thread.
        spawn(async move {
            let _ = dioxus::document::eval(PUSH_BRIDGE_JS).join::<()>().await;
            if let Ok(hex) = dioxus::document::eval(PUSH_TOKEN_PROBE_JS)
                .join::<String>()
                .await
            {
                push_ingest_hex_token(state, &hex, "the native shell");
            }
            let mut ev = dioxus::document::eval(PUSH_TOKEN_PROBE_JS);
            while let Ok(msg) = ev.recv::<String>().await {
                let Some(body) = msg.strip_prefix("push:") else {
                    continue;
                };
                match parse_push_bridge_message(body) {
                    Some(PushBridgeEvent::Token(hex)) => {
                        push_ingest_hex_token(state, &hex, "the native shell")
                    }
                    Some(PushBridgeEvent::Error(message)) => {
                        with_push_detached(state, |push| push.did_fail_to_register(message));
                    }
                    Some(PushBridgeEvent::Open(session_id)) => {
                        with_push_detached(state, |push| push.did_open_notification(&session_id));
                    }
                    None => {}
                }
            }
        });
        spawn(async move {
            // jsQR first (it defines the global `jsQR`), then the bridge.
            let _ = dioxus::document::eval(JSQR_LIB).join::<()>().await;
            let _ = dioxus::document::eval(QR_BRIDGE_INSTALL_JS)
                .join::<bool>()
                .await;
        });
        // App-lock lifecycle: `visibilitychange` hidden → cover the app,
        // visible → the one automatic foreground unlock prompt. The eval
        // channel stays open for the app's lifetime; each message is a
        // short signal write (the blocking auth prompt runs on its own
        // background thread via `foreground_auto_unlock`).
        spawn(async move {
            let mut ev = dioxus::document::eval(APP_LOCK_VISIBILITY_JS);
            while let Ok(msg) = ev.recv::<String>().await {
                match msg.as_str() {
                    "app-lock:hidden" => state.write().app_lock.lock_if_enabled(),
                    "app-lock:visible" => foreground_auto_unlock(state),
                    _ => {}
                }
            }
        });
        let (store, device, probes) = {
            let mut s = state.write();
            (
                s.store.clone(),
                s.device.clone(),
                std::mem::take(&mut s.pending_probes),
            )
        };
        if let Some(device) = device {
            for record in probes {
                spawn_probe_back(state, store.clone(), record, device.clone());
            }
        }
    });

    let pairing_status = state.read().pairing_status.clone();
    let keychain_notice = state.read().keychain_notice.clone();
    let records = state.read().records.clone();
    let global_error = state.read().error.clone();
    // App-lock cover state, read once per render like the other view flags.
    // Both the pairing screen and the main screen render the overlay.
    let app_locked = state.read().app_lock.is_locked();
    let app_lock_label = method_label(&state.read().app_lock.capability()).to_string();
    let app_lock_error = state.read().app_lock.last_error().map(|s| s.to_string());
    let app_lock_busy = state.read().app_lock.auth_in_flight();
    let (connected, show_host_list, connected_ids, active_id) = {
        let s = state.read();
        (
            !s.hosts.is_empty(),
            s.show_host_list,
            s.hosts.host_ids(),
            s.hosts.active_id().map(|id| id.to_string()),
        )
    };

    if !connected || show_host_list {
        let show_qr = state.read().show_qr_scanner;
        // Push diagnostics for the notifications row. Retry only appears
        // when there is a token to re-hand — without one, the native shell
        // hasn't delivered a token yet and there is nothing to retry.
        let (push_label, push_can_retry) = {
            let s = state.read();
            let st = s.push.state();
            (
                st.diagnostic_label(),
                st.can_retry() && s.push.token_hex().is_some(),
            )
        };
        // App-lock Security row: mirrors Swift's PairingView securitySection
        // ("Require <method>", "Locks Unpeel when you leave the app."),
        // disabled when the device cannot authenticate at all.
        let (lock_capability, lock_enabled) = {
            let s = state.read();
            (s.app_lock.capability(), s.app_lock.is_enabled())
        };
        let lock_method = method_label(&lock_capability).to_string();
        return rsx! {
            div { class: "mobile-app",
                style { "{APP_CSS}" }
                if connected {
                    // Back to the active Host. Nothing was torn down while
                    // the list was up: switching is a view change.
                    button {
                        class: "back",
                        onclick: move |_| state.write().show_host_list = false,
                        "‹ Back"
                    }
                }
                PairingView {
                    status: pairing_status,
                    keychain_notice,
                    hosts: records,
                    connected_ids,
                    active_id,
                    on_submit: move |code: String| submit_pairing_code(state, code),
                    on_connect: move |id: String| connect_host(state, id),
                    on_forget: move |id: String| forget_host(state, id),
                }
                button {
                    class: "qr-open",
                    onclick: move |_| state.write().show_qr_scanner = true,
                    "Scan QR code"
                }
                div { class: "push-row",
                    span { class: "push-label", "Notifications: {push_label}" }
                    if push_can_retry {
                        button {
                            class: "push-retry",
                            onclick: move |_| push_rehand_cached_token(state),
                            "Retry"
                        }
                    }
                }
                div { class: "security-row",
                    span { class: "security-text",
                        span { "Require {lock_method}" }
                        span { class: "security-sub", "Locks Unpeel when you leave the app." }
                    }
                    button {
                        class: "security-toggle",
                        aria_checked: if lock_enabled { "true" } else { "false" },
                        disabled: !lock_capability.available,
                        onclick: move |_| {
                            if lock_enabled {
                                disable_app_lock(state)
                            } else {
                                enable_app_lock(state)
                            }
                        },
                    }
                }
                if show_qr {
                    div { class: "sheet",
                        div { class: "sheet-header",
                            span { "Scan pairing code" }
                            button {
                                onclick: move |_| state.write().show_qr_scanner = false,
                                "Close"
                            }
                        }
                        QrScannerView {
                            paused: false,
                            on_code: move |code: String| {
                                state.write().show_qr_scanner = false;
                                submit_pairing_code(state, code);
                            },
                            on_state: move |_| {},
                        }
                    }
                }
                if let Some(err) = global_error {
                    div { class: "error", "{err}" }
                }
                // App-lock cover over the pairing screen too: the lock
                // covers the whole app, like Swift's root-view overlay.
                if app_locked {
                    AppLockOverlay {
                        method_label: app_lock_label.clone(),
                        last_error: app_lock_error.clone(),
                        auth_in_flight: app_lock_busy,
                        on_unlock: move |_| unlock_app(state),
                    }
                }
            }
        };
    }

    // `connected` implies the registry chose an active Host.
    let active_id = active_id.expect("connected implies an active host");
    let (snapshot, host_error, transport) = {
        let s = state.read();
        let h = s.hosts.get(&active_id).expect("active host is connected");
        (
            h.snapshot.clone(),
            h.last_error.clone(),
            h.client.transport_kind(),
        )
    };
    let sessions = snapshot
        .as_ref()
        .map(|b| b.sessions.clone())
        .unwrap_or_default();
    let approvals = snapshot
        .as_ref()
        .map(|b| b.pending_approvals.clone())
        .unwrap_or_default();
    // Preset drawer rows: the snapshot's enabled presets that carry a
    // project id (the Host's create route requires `projectID`), kept in
    // snapshot order like the desktop "+" menu.
    let drawer_presets: Vec<PresetSummary> = snapshot
        .as_ref()
        .map(|b| {
            launchable_presets(&b.presets)
                .into_iter()
                .filter(|p| p.project_id.is_some())
                .map(|p| (*p).clone())
                .collect()
        })
        .unwrap_or_default();
    // The "+" affordance shows only when the Host allows session creation
    // (Swift's `supportsSessionCreation`) and a launchable preset exists.
    let can_create =
        supports_session_creation(snapshot.as_ref().and_then(|b| b.host_protocol.as_ref()))
            && !drawer_presets.is_empty();
    let host_name = snapshot.as_ref().and_then(|b| b.host_name.clone());
    let view = state
        .read()
        .views
        .get(&active_id)
        .cloned()
        .unwrap_or_default();
    let selected = view.selected_session.clone();
    let terminal_snapshot = view.terminal_snapshot.clone();
    let terminal_error = view.terminal_error.clone();

    // Measure one terminal cell in CSS px once per terminal open, so the
    // resize handler below can convert viewport px into grid cells. Runs
    // after render, when `.terminal-view` is mounted. Guarded on
    // `cell_px`: without the guard every state write (each poll chunk
    // publishes a snapshot) would spawn another measurement.
    use_effect(move || {
        let (id, needs_measure) = {
            let s = state.read();
            let id = s.hosts.active_id().map(|id| id.to_string());
            let need = id
                .as_deref()
                .and_then(|id| s.views.get(id))
                .is_some_and(|v| v.selected_session.is_some() && v.cell_px.is_none());
            (id, need)
        };
        let Some(id) = id else { return };
        if !needs_measure {
            return;
        }
        spawn(async move {
            let mut ev = dioxus::document::eval(CELL_PROBE_JS);
            if let Ok((cw, ch)) = ev.recv::<(f64, f64)>().await {
                if cw > 0.0 && ch > 0.0 {
                    // The user may have navigated away while the probe ran;
                    // only keep the measurement for a still-open terminal.
                    let mut s = state.write();
                    if let Some(v) = s.views.get_mut(&id) {
                        if v.selected_session.is_some() && v.cell_px.is_none() {
                            v.cell_px = Some((cw, ch));
                        }
                    }
                }
            }
        });
    });

    let resize_host_id = active_id.clone();
    let back_host_id = active_id.clone();
    let key_host_id = active_id.clone();
    let wheel_host_id = active_id.clone();
    let pd_host_id = active_id.clone();
    let pm_host_id = active_id.clone();
    let pu_host_id = active_id.clone();
    let pc_host_id = active_id.clone();
    let find_btn_host_id = active_id.clone();
    let fq_host_id = active_id.clone();
    let fn_host_id = active_id.clone();
    let fp_host_id = active_id.clone();
    let fc_host_id = active_id.clone();

    // Predictive scroll translation in px (dark behind
    // SCROLL_PREDICTION_DISPLAY_ENABLED to match the Swift client): the
    // engine keeps tracking confidence and path latency while closed, so
    // flipping the switch needs no other change.
    let scroll_translate_px = if SCROLL_PREDICTION_DISPLAY_ENABLED {
        view.scroll.offset_rows() as f64 * view.cell_px.map(|(_, h)| h).unwrap_or(0.0)
    } else {
        0.0
    };

    rsx! {
        div { class: "mobile-app",
            style { "{APP_CSS}" }
            ConnectionBar {
                host_name,
                connected,
                transport,
                on_refresh: move |_| refresh(state),
            }
            if let Some(warning) = state.read().push.state().sidebar_warning() {
                div { class: "push-warning", "{warning}" }
            }
            if let Some(err) = host_error {
                div { class: "error", "{err}" }
            }
            if selected.is_none() {
                button {
                    class: "hosts",
                    onclick: move |_| state.write().show_host_list = true,
                    "‹ Hosts"
                }
                if can_create {
                    button {
                        class: "new-session",
                        "data-testid": "new-session-button",
                        onclick: move |_| open_preset_drawer(state),
                        "＋ New session"
                    }
                }
                for a in approvals {
                    {
                        let id = a.id.clone();
                        rsx! {
                            ApprovalCard {
                                key: "{id}",
                                approval: a,
                                on_answer: move |approved| answer_approval(state, id.clone(), approved),
                            }
                        }
                    }
                }
                SessionList {
                    sessions,
                    selected_id: selected,
                    on_select: move |id: String| open_terminal(state, id),
                    on_archive: move |id: String| organize_session(state, id, true),
                    on_restore: move |id: String| organize_session(state, id, false),
                    on_organize: move |id: String| open_organize_sheet(state, id),
                    // Presence chips are a desktop-local affordance (the
                    // desktop watches the Host's presence files); the phone
                    // never reads its own filesystem for this.
                    viewers: None,
                }
                // Session organize sheet: rename / pin / notify-when-done /
                // restart / resume-agent / stop / archive / remove. Renders
                // over the session list while open.
                if let Some(org_id) = view.organize_session_id.clone() {
                    if let Some(session) =
                        snapshot.as_ref().and_then(|b| b.sessions.iter().find(|s| s.id == org_id).cloned())
                    {
                        {
                            let save_id = org_id.clone();
                            let action_id = org_id.clone();
                            let archive_project = session.project_id.clone();
                            rsx! {
                                SessionOrganizeSheet {
                                    session,
                                    host_protocol: snapshot.as_ref().and_then(|b| b.host_protocol.clone()),
                                    projects: snapshot.as_ref().map(|b| b.projects.clone()).unwrap_or_default(),
                                    on_save: move |patch: SessionOrganizePatch| save_organize_session(state, save_id.clone(), patch),
                                    on_action: move |action: SessionSheetAction| fire_organize_action(state, action_id.clone(), action),
                                    on_open_archive: move |_| open_archive_sheet(state, archive_project.clone()),
                                    on_close: move |_| close_sheets(state),
                                }
                            }
                        }
                    }
                }
                // Archive library for the session's project. The DTO carries
                // project ids, not names, so the sheet labels itself with the
                // id — a deliberate adaptation of Swift's "Archive — <name>".
                if let Some(archive) = view.archive_sheet.clone() {
                    ArchivedSessionsSheet {
                        project_name: archive.project_id.clone(),
                        sessions: archive.sessions,
                        load_error: archive.load_error,
                        on_restore: move |action: ArchiveAction| archive_restore(state, action),
                        on_close: move |_| close_sheets(state),
                    }
                }
                // Preset drawer: "New session" bottom sheet. The flat
                // session list has no project names, so the header shows
                // just "New session" (project_name: None) — each row
                // launches with its preset's own project id.
                if view.preset_drawer_open {
                    PresetDrawer {
                        project_name: None::<String>,
                        presets: drawer_presets.clone(),
                        launching_id: view.launching_preset_id.clone(),
                        on_launch: move |id: String| launch_preset(state, id),
                        on_close: move |_| close_preset_drawer(state),
                    }
                }
            } else if view.gallery.session_id.is_some() {
                {
                    // Gallery screen for the selected session, rendered by the
                    // shared gallery components. Grid thumbnails load lazily
                    // through the Host's max_dim thumbnail transport; detail
                    // always uses the full bytes.
                    // Block-scoped: `let` bindings are not rsx nodes.
                    let g = view.gallery.clone();
                    let gback = active_id.clone();
                    let entries: Vec<GalleryEntry> = g
                        .entries
                        .iter()
                        .map(|meta| GalleryEntry {
                            meta: meta.clone(),
                            preview_url: g
                                .thumbs
                                .get(&gallery_thumb_key(&meta.kind, &meta.name))
                                .cloned(),
                        })
                        .collect();
                    let grid_open = active_id.clone();
                    let grid_del = active_id.clone();
                    let up_host = active_id.clone();
                    let shot_host = active_id.clone();
                    let ref_host = active_id.clone();
                    rsx! {
                    button {
                        class: "back",
                        onclick: move |_| gallery_close(state, gback.clone()),
                        "‹ Terminal"
                    }
                    if let Some(meta) = g.detail_meta.clone() {
                        {
                            let d_host = active_id.clone();
                            let e_host = active_id.clone();
                            let a_host = active_id.clone();
                            let n_host = active_id.clone();
                            let s_host = active_id.clone();
                            let m_host = active_id.clone();
                            let entry = GalleryEntry {
                                meta: meta.clone(),
                                preview_url: g
                                    .thumbs
                                    .get(&gallery_thumb_key(&meta.kind, &meta.name))
                                    .cloned(),
                            };
                            rsx! {
                            GalleryDetailView {
                                entry: entry,
                                image_url: g.detail_data_url.clone(),
                                editor: g.editor,
                                on_close: move |_| {
                                    if let Some(v) = state.write().views.get_mut(&d_host) {
                                        v.gallery.detail_meta = None;
                                        v.gallery.detail_data_url = None;
                                        v.gallery.detail_bytes = None;
                                        v.gallery.detail_mime = None;
                                        v.gallery.editor = None;
                                    }
                                },
                                on_delete: move |entry: GalleryEntry| {
                                    gallery_delete_entry(state, e_host.clone(), entry.meta)
                                },
                                on_editor: move |mode: Option<AnnotationMode>| {
                                    if let Some(v) = state.write().views.get_mut(&a_host) {
                                        v.gallery.editor = mode;
                                    }
                                },
                                on_annotation_done: move |result: AnnotationResult| {
                                    gallery_annotation_done(state, n_host.clone(), result)
                                },
                                on_add_to_message: move |_| {
                                    gallery_add_to_message(state, m_host.clone())
                                },
                                on_share: move |_| gallery_share_entry(state, s_host.clone()),
                            }
                            }
                        }
                    } else {
                        BrowserGalleryPanel {
                            entries: entries,
                            loading: g.loading,
                            on_refresh: move |_| gallery_refresh(state, ref_host.clone()),
                            on_open: move |entry: GalleryEntry| {
                                gallery_open_entry(state, grid_open.clone(), entry.meta)
                            },
                            on_upload: move |_| gallery_upload_pick(state, up_host.clone()),
                            on_screenshot: move |_| gallery_screenshot(state, shot_host.clone()),
                            on_delete: move |entry: GalleryEntry| {
                                gallery_delete_entry(state, grid_del.clone(), entry.meta)
                            },
                        }
                    }
                    if let Some(busy) = g.busy.clone() {
                        div { class: "gallery-status", "{busy}" }
                    }
                    if let Some(err) = g.error.clone() {
                        div { class: "gallery-status error", "{err}" }
                    }
                    }
                }
            } else {
                button {
                    class: "back",
                    onclick: move |_| {
                        if let Some(v) = state.write().views.get_mut(&back_host_id) {
                            v.close_terminal();
                        }
                    },
                    "‹ Sessions"
                }
                button {
                    class: "gallery-open",
                    "data-testid": "gallery-open-button",
                    onclick: move |_| gallery_open(state),
                    "Gallery"
                }
                button {
                    class: "find-open",
                    onclick: move |_| {
                        // Open the terminal find bar (TerminalFindBar.swift
                        // parity); the poll loop keeps matches fresh.
                        if let Some(v) = state.write().views.get_mut(&find_btn_host_id) {
                            v.find = Some(FindState::new());
                        }
                    },
                    "Find"
                }
                {
                    // Push-to-talk dictation: the shared view owns the mic
                    // button, transcript pill, and speech backend; commits
                    // bracketed-paste into the live terminal.
                    let d_host = active_id.clone();
                    rsx! {
                        DictationView {
                            settings: DictationSettings::default(),
                            on_commit: move |text: String| {
                                paste_into_terminal(state, &d_host, &text)
                            },
                        }
                    }
                }
                if let Some(err) = terminal_error {
                    div { class: "terminal-error", "{err}" }
                }
                if let Some(snap) = terminal_snapshot {
                    div {
                        class: "terminal-wrap",
                        // Predictive scroll translation; zero (attribute
                        // omitted) while the display switch is dark.
                        style: if scroll_translate_px != 0.0 {
                            format!("transform:translateY({scroll_translate_px}px)")
                        } else {
                            String::new()
                        },
                        // Wheel gestures scroll the remote TUI (forwarded
                        // as alternate-scroll) and feed the scroll
                        // prediction engine; the page itself must not move.
                        onwheel: move |evt| {
                            evt.prevent_default();
                            record_wheel(state, wheel_host_id.clone(), evt.data().delta());
                        },
                        // Press-and-hold opens word-anchored text
                        // selection. Pointer events unify touch and mouse;
                        // only the primary pointer (or touch/pen) starts
                        // tracking, and travel past the slop cancels.
                        onpointerdown: move |evt| {
                            let data = evt.data();
                            if !data.is_primary() {
                                return;
                            }
                            let primary_button = match data.trigger_button() {
                                None => true, // touch / pen
                                Some(b) => b == MouseButton::Primary,
                            };
                            if !primary_button {
                                return;
                            }
                            let p = data.client_coordinates();
                            press_started(state, pd_host_id.clone(), p.x, p.y);
                        },
                        onpointermove: move |evt| {
                            let p = evt.data().client_coordinates();
                            press_moved(state, &pm_host_id, p.x, p.y);
                        },
                        onpointerup: move |_| press_ended(state, &pu_host_id),
                        onpointercancel: move |_| press_ended(state, &pc_host_id),
                        // Viewport changes land here: rotation, split view,
                        // and the virtual keyboard. `apply_viewport_size`
                        // drops keyboard-driven changes (width unchanged)
                        // so the remote grid never follows the keyboard.
                        onresize: move |evt| {
                            let Some((cell_w, cell_h)) = state.read().views.get(&resize_host_id).and_then(|v| v.cell_px) else {
                                return; // cells not measured yet
                            };
                            let Ok(px) = evt.data().get_content_box_size() else {
                                return;
                            };
                            let (cols, rows) =
                                fit_grid(px.width, px.height, cell_w, cell_h);
                            apply_viewport_size(state, resize_host_id.clone(), cols, rows);
                        },
                        if let Some(find_state) = view.find.clone() {
                            {
                                let fq = fq_host_id.clone();
                                let fnx = fn_host_id.clone();
                                let fpx = fp_host_id.clone();
                                let fcx = fc_host_id.clone();
                                rsx! {
                                    FindBar {
                                        query: find_state.query().to_string(),
                                        counter: find_state.counter_text(),
                                        on_query: move |q: String| {
                                            let mut s = state.write();
                                            if let Some(v) = s.views.get_mut(&fq) {
                                                let snap = v.terminal_snapshot.clone();
                                                if let (Some(f), Some(snap)) =
                                                    (v.find.as_mut(), snap.as_ref())
                                                {
                                                    f.set_query(snap, q);
                                                }
                                            }
                                        },
                                        on_next: move |_| {
                                            if let Some(v) = state.write().views.get_mut(&fnx) {
                                                if let Some(f) = v.find.as_mut() {
                                                    f.next();
                                                }
                                            }
                                        },
                                        on_prev: move |_| {
                                            if let Some(v) = state.write().views.get_mut(&fpx) {
                                                if let Some(f) = v.find.as_mut() {
                                                    f.prev();
                                                }
                                            }
                                        },
                                        on_close: move |_| {
                                            if let Some(v) = state.write().views.get_mut(&fcx) {
                                                v.find = None;
                                            }
                                        },
                                    }
                                }
                            }
                        }
                        TerminalView {
                            snapshot: snap,
                            selection_request: view.selection_request,
                            find: view.find.as_ref().and_then(|f| f.highlight()),
                            prediction: view.prediction.anchor().and_then(|anchor| {
                                view.prediction.displayed_text().map(|chars| PredictionOverlay {
                                    row: anchor.row,
                                    col: anchor.column,
                                    text: chars.into_iter().collect(),
                                })
                            }),
                            on_key: move |seq: String| {
                                let mut s = state.write();
                                if let Some(v) = s.views.get_mut(&key_host_id) {
                                    record_keystroke(v, &seq);
                                    if let Some(tx) = v.terminal_writer.clone() {
                                        let _ = tx.send(seq);
                                    }
                                }
                            },
                            on_copy_all: move |text: String| copy_to_clipboard(&text),
                            on_path_click: move |req: PathClickRequest| {
                                handle_path_click(state, active_id.clone(), req)
                            },
                        }
                    }
                }
            }
            // App-lock cover: opaque, over everything (including the
            // session list), like Swift's root-view overlay. It is also
            // what the app-switcher snapshot captures while armed.
            if app_locked {
                AppLockOverlay {
                    method_label: app_lock_label.clone(),
                    last_error: app_lock_error.clone(),
                    auth_in_flight: app_lock_busy,
                    on_unlock: move |_| unlock_app(state),
                }
            }
        }
    }
}
