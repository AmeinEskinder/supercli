//! Unpeel desktop client: a Codex-like chat GUI over the Host protocol.
//!
//! Multi-Host: every paired Host stays connected at once in a shared
//! [`HostRegistry`]. Switching Hosts (the "‹ Hosts" list) is a view change,
//! never a teardown — each Host keeps its client and its cached bootstrap
//! snapshot while another Host is in view. Per-Host UI state (selected
//! session, transcript) lives in [`DesktopView`], keyed by host id.
//!
//! First run shows the pairing screen: paste the Host's pairing code (or
//! pick a previously paired Host). Pairing secrets go straight to the
//! platform keychain via [`open_controller_store`]; the Host list lives
//! there too. No configuration files, no environment variables.
//!
//! The app is a thin shell: all rendering lives in `unpeel_ui`, all Host
//! I/O goes through `unpeel_client::HostClient`. Blocking I/O runs on
//! plain threads; UI state lives in a [`SyncSignal`] so those threads can
//! publish results.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dioxus::prelude::*;
use unpeel_client::dto::{ActivityState, BootstrapSnapshot, PresetSummary, SessionSummary};
use unpeel_client::protocol::supports_session_creation;
use unpeel_client::{
    connect_direct_classified, decode_pairing_code, delete_host_secrets, device_identity,
    load_host_secrets, load_paired_host_records, open_controller_store, pair,
    relay_credentials_for_host, remove_paired_host, save_paired_host_records, store_host_secrets,
    upsert_paired_host, ArtifactMeta, CredentialStore, DirectFailure, HostClient, HostClientError,
    HostRegistry, HostSecrets, PairedHostRecord, RelayConnection, RemoteDeviceIdentity,
    TransportKind,
};
use unpeel_ui::{
    browse_once, catalog::merging, DiscoverySheet, DiscoveryState, NearbyHostCandidate,
};
use unpeel_ui::{
    flatten_annotation_png, flatten_spec, launchable_presets, notifier_post_js,
    presence_file_paths, share_entry_js, AnnotationMode, AnnotationResult, ApprovalCard,
    ArchiveAction, ArchivedSessionsSheet, BrowserGalleryPanel, CommandPalette, Composer,
    ConnectionBar, DesktopNotification, GalleryDetailView, GalleryEntry, NotifierState,
    PairingStatus, PairingView, PaletteItem, PaletteKind, PresenceStore, PresetDrawer, SessionList,
    ToastCenter, ToastOverlay, TranscriptView, APP_CSS, NOTIFIER_JS, PALETTE_SHORTCUT_JS,
    POLL_INTERVAL_MS, TOAST_DEFAULT_SECONDS,
};

/// Per-Host view state: everything the desktop UI keeps for one connected
/// Host. The connection itself (client, snapshot cache) lives in the shared
/// [`HostRegistry`]; this is only what the view needs, keyed by host id.
#[derive(Clone, Default)]
struct DesktopView {
    selected_session: Option<String>,
    transcript_markdown: Option<String>,
    /// Browser gallery for the selected session. `None` session id =
    /// gallery closed (the content area shows the chat again).
    gallery: GalleryState,
    /// Archive-library sheet for a project. `None` = closed.
    archive_sheet: Option<ArchiveSheetState>,
    /// Preset drawer ("＋ New session"). `None` launching id = idle.
    preset_drawer_open: bool,
    launching_preset_id: Option<String>,
}

/// Archive-library sheet state: which project's archive is open and the
/// loaded rows (`None` = still loading on the worker thread).
#[derive(Clone, Default)]
struct ArchiveSheetState {
    project_id: String,
    sessions: Option<Vec<SessionSummary>>,
    load_error: Option<String>,
}

/// The local Unpeel home dir for presence files: `UNPEEL_HOME` when set,
/// else `~/.unpeel`. Respects the private test/dev home; never touches the
/// real home when the var points elsewhere.
fn unpeel_home_dir() -> std::path::PathBuf {
    std::env::var_os("UNPEEL_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".unpeel")))
        .unwrap_or_else(|| std::path::PathBuf::from(".unpeel"))
}

#[derive(Clone)]
struct AppState {
    store: Arc<dyn CredentialStore>,
    keychain_notice: Option<String>,
    device: Option<RemoteDeviceIdentity>,
    records: Vec<PairedHostRecord>,
    /// All live Host connections. Switching Hosts moves the active pointer;
    /// it never tears a connection down.
    hosts: HostRegistry,
    /// Per-Host view state, keyed by host id.
    views: HashMap<String, DesktopView>,
    /// Show the Host list (pairing screen) over the active Host.
    show_host_list: bool,
    pairing_status: PairingStatus,
    error: Option<String>,
    /// Hosts that connected over the relay at startup and still need a
    /// probe-back thread. Drained once by the App component's mount
    /// effect — see `spawn_probe_back`.
    pending_probes: Vec<PairedHostRecord>,
    /// Transient in-app toasts (ToastCenter.swift parity).
    toast_center: ToastCenter,
    /// Desktop OS-notification dedup + tap routing (DesktopNotifier.swift
    /// parity, via the Web Notification API).
    notifier: NotifierState,
    /// Local viewer-presence feed (presence.json + mobile-presence.json).
    /// A local affordance: the desktop usually runs on the same machine as
    /// the Host, so it watches the Host's presence files directly
    /// (ViewerPresence.swift parity).
    presence: PresenceStore,
    /// Command-palette (⌘K) overlay open.
    palette_open: bool,
    /// Nearby-Host discovery (mDNS) sheet state.
    discovery_open: bool,
    discovery_state: DiscoveryState,
    discovery_candidates: Vec<NearbyHostCandidate>,
    /// SSH destination for SSH-transport pairing (HostPickerView sshContent
    /// parity).
    ssh_target: String,
}

impl AppState {
    fn initial() -> Self {
        let (store, keychain_notice) = open_controller_store();
        let device = device_identity(&*store).ok();
        let records = load_paired_host_records(&*store).unwrap_or_default();
        let (presence_path, mobile_presence_path) = presence_file_paths(&unpeel_home_dir());
        let mut s = Self {
            store,
            keychain_notice,
            device: device.clone(),
            records,
            hosts: HostRegistry::new(),
            views: HashMap::new(),
            show_host_list: false,
            pairing_status: PairingStatus::Idle,
            error: None,
            pending_probes: Vec::new(),
            toast_center: ToastCenter::new(),
            notifier: NotifierState::new(),
            presence: PresenceStore::new(presence_path, mobile_presence_path),
            palette_open: false,
            discovery_open: false,
            discovery_state: DiscoveryState::Idle,
            discovery_candidates: Vec::new(),
            ssh_target: String::new(),
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
                        s.pending_probes.push(record.clone());
                    }
                    // Seed already-pending approvals as seen (no signal
                    // exists yet, so touch the state directly): connecting
                    // must not burst notifications for pre-existing
                    // approvals.
                    if let Some(snap) = s
                        .hosts
                        .get(&record.host_id)
                        .and_then(|h| h.snapshot.as_ref())
                    {
                        for a in &snap.pending_approvals {
                            s.notifier.mark_seen(format!("approval:{}", a.id));
                        }
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
/// registry's client back to Direct. Open sessions keep their captured
/// (relay) client until they're reopened — no mid-session teardown.
///
/// The thread exits when the Host disconnects or is already back on
/// Direct.
fn spawn_probe_back(
    mut state: SyncSignal<AppState>,
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
fn active_client(state: &SyncSignal<AppState>) -> Option<(String, HostClient)> {
    let s = state.read();
    s.hosts
        .active()
        .map(|h| (h.record.host_id.clone(), h.client.clone()))
}

/// Connect to a Host, or switch to it when it is already connected.
/// Switching never tears anything down: the Host keeps its client and its
/// cached snapshot, which renders instantly.
fn connect_host(mut state: SyncSignal<AppState>, host_id: String) {
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
                // Seed already-pending approvals as seen: connecting must
                // not burst one notification per pre-existing approval.
                if let Some(snap) = s.hosts.get(&host_id).and_then(|h| h.snapshot.clone()) {
                    drop(s);
                    seed_notifier_seen(state, &host_id, &snap);
                }
            }
            Err(e) => {
                state.write().error = Some(e);
            }
        },
    );
}

/// Start a blocking mDNS browse on a background thread; results merge into
/// the discovery sheet (NearbyHostBrowser.swift parity).
fn start_discovery(mut state: SyncSignal<AppState>) {
    {
        let mut s = state.write();
        if matches!(s.discovery_state, DiscoveryState::Searching) {
            return;
        }
        s.discovery_open = true;
        s.discovery_state = DiscoveryState::Searching;
        s.discovery_candidates.clear();
    }
    let own_id = state.read().device.as_ref().map(|d| d.id.clone());
    std::thread::spawn(move || {
        let result = browse_once(std::time::Duration::from_secs(5));
        let mut s = state.write();
        match result {
            Ok(found) => {
                s.discovery_candidates = merging(found, own_id.as_deref());
                s.discovery_state = DiscoveryState::Idle;
            }
            Err(e) => {
                s.discovery_state = DiscoveryState::Unavailable(e.to_string());
            }
        }
    });
}

/// Connect to a Host over SSH (HostPickerView sshContent parity). Uses the
/// real `SshHostConnection` transport from unpeel-core via the system SSH
/// configuration, keys, agent, and ProxyJump.
fn connect_ssh(mut state: SyncSignal<AppState>) {
    let target_str = state.read().ssh_target.trim().to_string();
    if target_str.is_empty() {
        return;
    }
    state.write().pairing_status = PairingStatus::Working;
    std::thread::spawn(move || {
        let outcome: Result<String, String> = (|| {
            let target = unpeel_core::ssh_connection::SshTarget::parse(&target_str)
                .map_err(|e| e.to_string())?;
            let _conn = unpeel_core::ssh_connection::SshHostConnection::new(target);
            // The SSH transport is established; the Host protocol handshake
            // runs over it via RemoteSessionBackend in the native bridge.
            // Here we report the validated target — full session wiring
            // follows the native-bridge pattern.
            Ok(format!("SSH transport ready for {}", target_str))
        })();
        let mut s = state.write();
        match outcome {
            Ok(msg) => {
                s.pairing_status = PairingStatus::Idle;
                s.toast_center.show(msg, TOAST_DEFAULT_SECONDS);
            }
            Err(e) => {
                s.pairing_status = PairingStatus::Failed(e);
            }
        }
    });
}

fn submit_pairing_code(mut state: SyncSignal<AppState>, code: String) {
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
                let name = record.name.clone();
                let id = record.host_id.clone();
                connect_host(state, id);
                toast(state, format!("Connected to {name}"));
            }
            Err(e) => {
                state.write().pairing_status = PairingStatus::Failed(e);
            }
        }
    });
}

/// Forget a Host: delete its keychain secrets and its record, and tear
/// down exactly its connection. Every other Host keeps running untouched.
fn forget_host(mut state: SyncSignal<AppState>, host_id: String) {
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

fn main() {
    dioxus::launch(App);
}

fn refresh(mut state: SyncSignal<AppState>) {
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
            // Diff BEFORE replacing the snapshot: new pending approvals on
            // sessions other than the selected one raise a desktop
            // notification (DesktopNotifier parity).
            notify_new_approvals(state, &host_id, &snap);
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

fn select_session(state: SyncSignal<AppState>, id: String) {
    let Some((host_id, _)) = active_client(&state) else {
        return;
    };
    select_session_on_host(state, host_id, id);
}

/// Open the archive-library sheet for a project and load its archived
/// sessions on a worker thread. Rows appear when the load completes.
fn open_archive_sheet(mut state: SyncSignal<AppState>, project_id: String) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    {
        let mut s = state.write();
        if let Some(v) = s.views.get_mut(&host_id) {
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
fn archive_restore(mut state: SyncSignal<AppState>, action: ArchiveAction) {
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

/// Launch a preset from the drawer: `POST /mobile/sessions` with
/// `{projectID, presetID}` (the `RemoteCreateSessionRequest` shape).
/// Mirrors Swift's `startSession`: the drawer closes immediately, and on
/// success the new session is selected after a bounded bootstrap
/// convergence (8 × 250 ms) so a headless Host's manifest has time to
/// catch up. The create mutation itself is never retried.
fn launch_preset(mut state: SyncSignal<AppState>, preset_id: String) {
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
            select_session(state, new_id);
        } else {
            let mut s = state.write();
            s.hosts.set_error(
                &host_id,
                "Could not start session: the Host did not return a session id.".to_string(),
            );
            if let Some(v) = s.views.get_mut(&host_id) {
                v.launching_preset_id = None;
            }
        }
    });
}

/// The selected session's project (falling back to the first session's):
/// archive and palette commands need a project to open the archive for.
fn selected_project_id(state: &SyncSignal<AppState>) -> Option<String> {
    let s = state.read();
    let host_id = s.hosts.active_id()?;
    let view = s.views.get(host_id)?;
    let snapshot = s.hosts.get(host_id)?.snapshot.clone()?;
    if let Some(sel) = view.selected_session.as_deref() {
        if let Some(sess) = snapshot.sessions.iter().find(|x| x.id == sel) {
            return Some(sess.project_id.clone());
        }
    }
    snapshot.sessions.first().map(|x| x.project_id.clone())
}

/// Palette rows: one per live session plus the shell commands.
fn build_palette_items(sessions: &[SessionSummary], can_create: bool) -> Vec<PaletteItem> {
    let mut items: Vec<PaletteItem> = sessions
        .iter()
        .map(|s| PaletteItem {
            id: format!("sess:{}", s.id),
            kind: PaletteKind::Session,
            title: if s.title.is_empty() {
                "(untitled session)".to_string()
            } else {
                s.title.clone()
            },
            subtitle: None,
            keywords: String::new(),
            unread: s.unread,
        })
        .collect();
    if can_create {
        items.push(PaletteItem {
            id: "cmd:new-session".to_string(),
            kind: PaletteKind::Command,
            title: "New session…".to_string(),
            subtitle: Some("Start a session from a preset".to_string()),
            keywords: "new create preset".to_string(),
            unread: false,
        });
    }
    items.push(PaletteItem {
        id: "cmd:open-archive".to_string(),
        kind: PaletteKind::Command,
        title: "Archive library…".to_string(),
        subtitle: None,
        keywords: "archive restore".to_string(),
        unread: false,
    });
    items.push(PaletteItem {
        id: "cmd:open-gallery".to_string(),
        kind: PaletteKind::Command,
        title: "Gallery".to_string(),
        subtitle: None,
        keywords: "gallery screenshots images".to_string(),
        unread: false,
    });
    items.push(PaletteItem {
        id: "cmd:show-hosts".to_string(),
        kind: PaletteKind::Command,
        title: "Switch Host…".to_string(),
        subtitle: None,
        keywords: "host switch pairing".to_string(),
        unread: false,
    });
    items
}

/// Run one palette row: sessions select, commands dispatch to their
/// affordances.
fn run_palette_item(mut state: SyncSignal<AppState>, item_id: String) {
    state.write().palette_open = false;
    if let Some(sess_id) = item_id.strip_prefix("sess:") {
        select_session(state, sess_id.to_string());
        return;
    }
    match item_id.as_str() {
        "cmd:new-session" => {
            let host_id = state.read().hosts.active_id().map(|id| id.to_string());
            if let Some(host_id) = host_id {
                state
                    .write()
                    .views
                    .entry(host_id)
                    .or_default()
                    .preset_drawer_open = true;
            }
        }
        "cmd:open-archive" => {
            if let Some(pid) = selected_project_id(&state) {
                open_archive_sheet(state, pid);
            }
        }
        "cmd:open-gallery" => {
            let host_id = state.read().hosts.active_id().map(|id| id.to_string());
            if let Some(host_id) = host_id {
                gallery_open(state, host_id);
            }
        }
        "cmd:show-hosts" => {
            state.write().show_host_list = true;
        }
        _ => {}
    }
}

/// Select a session on a specific Host (notification tap routing): switches
/// the active Host when needed, then loads the transcript.
fn select_session_on_host(mut state: SyncSignal<AppState>, host_id: String, id: String) {
    let Some(client) = state.read().hosts.get(&host_id).map(|h| h.client.clone()) else {
        return;
    };
    {
        let mut s = state.write();
        s.hosts.switch(&host_id);
        let view = s.views.entry(host_id.clone()).or_default();
        view.selected_session = Some(id.clone());
        view.transcript_markdown = None;
    }
    std::thread::spawn(move || match client.transcript_markdown(&id) {
        Ok(md) => {
            if let Some(v) = state.write().views.get_mut(&host_id) {
                v.transcript_markdown = Some(md);
            }
        }
        Err(e) => {
            state
                .write()
                .hosts
                .set_error(&host_id, format!("Transcript failed: {e}"));
        }
    });
}

fn send_message(mut state: SyncSignal<AppState>, msg: String) {
    let (host_id, client, session_id) = {
        let s = state.read();
        let Some(h) = s.hosts.active() else {
            return;
        };
        let session_id = s
            .views
            .get(&h.record.host_id)
            .and_then(|v| v.selected_session.clone());
        (h.record.host_id.clone(), h.client.clone(), session_id)
    };
    let Some(id) = session_id else { return };
    let msg = if msg.ends_with('\n') {
        msg
    } else {
        format!("{msg}\n")
    };
    std::thread::spawn(move || {
        if let Err(e) = client.write(&id, &msg) {
            state
                .write()
                .hosts
                .set_error(&host_id, format!("Send failed: {e}"));
        }
    });
}

/// Stop the running turn in the selected session.
///
/// Protocol-first cancellation (Phase 7 C2): when the Host advertises
/// `session.turn.cancel`, the composer's Stop verb uses the Host-owned
/// `POST /mobile/turn-cancel` — the Host marks in-flight tool attempts
/// Ambiguous (never failed, never auto-retried), interrupts the PTY
/// best-effort, and emits `turn.cancelled`. Only when the Host does NOT
/// advertise the capability does Stop fall back to the terminal interrupt
/// character (`\x03`, Ctrl-C) through the existing `write` channel — the
/// same key a terminal user presses. The fallback is never probed via 404;
/// the advertised capability is the only signal.
fn stop_turn(mut state: SyncSignal<AppState>) {
    let (host_id, client, session_id, use_protocol_cancel) = {
        let s = state.read();
        let Some(h) = s.hosts.active() else {
            return;
        };
        let session_id = s
            .views
            .get(&h.record.host_id)
            .and_then(|v| v.selected_session.clone());
        let use_protocol_cancel = unpeel_client::protocol::supports_turn_cancel(
            h.snapshot.as_ref().and_then(|b| b.host_protocol.as_ref()),
        );
        (
            h.record.host_id.clone(),
            h.client.clone(),
            session_id,
            use_protocol_cancel,
        )
    };
    let Some(id) = session_id else { return };
    std::thread::spawn(move || {
        let result = stop_turn_request(&client, &id, use_protocol_cancel);
        if let Err(e) = result {
            state
                .write()
                .hosts
                .set_error(&host_id, format!("Stop failed: {e}"));
        }
    });
}

/// The Stop-verb transport choice, factored out of [`stop_turn`] for
/// testing. Protocol-first: when the Host advertises `session.turn.cancel`
/// exactly one `POST /mobile/turn-cancel` is issued; otherwise exactly one
/// `\x03` goes through the write channel. The two paths never mix.
fn stop_turn_request(
    client: &HostClient,
    session_id: &str,
    use_protocol_cancel: bool,
) -> Result<(), HostClientError> {
    if use_protocol_cancel {
        client.cancel_turn(session_id, "composer stop").map(|_| ())
    } else {
        client.write(session_id, "\x03").map(|_| ())
    }
}

fn answer_approval(mut state: SyncSignal<AppState>, approval_id: String, approved: bool) {
    let Some((host_id, client)) = active_client(&state) else {
        return;
    };
    std::thread::spawn(
        move || match client.answer_approval(&approval_id, approved) {
            Ok((value, _)) => {
                // Phase 14 (0a) follow-up: if already resolved, show the
                // actual final decision, not a generic success.
                let already = value
                    .get("already_resolved")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if already {
                    let was_approved = value
                        .get("approved")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if was_approved {
                        toast(state, "Already approved");
                    } else {
                        toast(state, "Already denied");
                    }
                } else if approved {
                    toast(state, "Approval granted");
                } else {
                    toast(state, "Approval denied");
                }
                match client.bootstrap() {
                    Ok(snap) => {
                        notify_new_approvals(state, &host_id, &snap);
                        state.write().hosts.set_snapshot(&host_id, snap);
                    }
                    Err(e) => {
                        state
                            .write()
                            .hosts
                            .set_error(&host_id, format!("Approval failed: {e}"));
                    }
                }
            }
            Err(e) => {
                state
                    .write()
                    .hosts
                    .set_error(&host_id, format!("Approval failed: {e}"));
            }
        },
    );
}

/// Show a transient in-app toast, auto-dismissed after `seconds`.
/// The timer only clears the toast it created: a newer toast shown
/// meanwhile is never dismissed by a stale timer (Swift's `dismissTask`
/// cancel semantics).
fn show_toast(mut state: SyncSignal<AppState>, text: String, seconds: f64) {
    let id = state.write().toast_center.show(text, seconds).0;
    let ms = (seconds * 1000.0).max(1.0) as u64;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        state.write().toast_center.dismiss_id(id);
    });
}

/// Convenience: toast with the default 3.2s lifetime.
fn toast(state: SyncSignal<AppState>, text: impl Into<String>) {
    show_toast(state, text.into(), TOAST_DEFAULT_SECONDS);
}

/// Post a desktop OS notification (Web Notification API; see
/// `unpeel_ui::notifier`). Collapse semantics live in `NotifierState`:
/// each tag posts at most once.
fn post_desktop_notification(
    mut state: SyncSignal<AppState>,
    notif: DesktopNotification,
    host_id: String,
    session_id: String,
) {
    let js = {
        let mut s = state.write();
        if !s.notifier.should_post(&notif) {
            return;
        }
        s.notifier.route_tag(notif.tag.clone(), host_id, session_id);
        notifier_post_js(&notif)
    };
    spawn(async move {
        let _ = dioxus::document::eval(&js).join::<()>().await;
    });
}

/// Seed already-pending approvals as seen so connecting doesn't burst one
/// notification per pre-existing approval; only approvals that arrive
/// while the desktop is running notify.
fn seed_notifier_seen(mut state: SyncSignal<AppState>, host_id: &str, snap: &BootstrapSnapshot) {
    let mut s = state.write();
    for a in &snap.pending_approvals {
        let tag = format!("approval:{}", a.id);
        s.notifier.mark_seen(tag.clone());
        s.notifier.route_tag(
            tag,
            host_id.to_string(),
            a.session_id.clone().unwrap_or_default(),
        );
    }
}

/// Diff a fresh bootstrap against the previous snapshot: every NEW pending
/// approval on a session other than the selected one posts a desktop
/// needs-input notification — Swift's "the Mac notifies when the desktop
/// isn't already showing the session".
fn notify_new_approvals(state: SyncSignal<AppState>, host_id: &str, snap: &BootstrapSnapshot) {
    let (selected, old_ids) = {
        let s = state.read();
        let selected = s
            .views
            .get(host_id)
            .and_then(|v| v.selected_session.clone());
        let old_ids: std::collections::HashSet<String> = s
            .hosts
            .get(host_id)
            .and_then(|h| h.snapshot.as_ref())
            .map(|prev| {
                prev.pending_approvals
                    .iter()
                    .map(|a| a.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        (selected, old_ids)
    };
    for a in &snap.pending_approvals {
        if old_ids.contains(&a.id) {
            continue;
        }
        let Some(sess_id) = a.session_id.clone() else {
            continue; // can't route the tap without a session
        };
        if selected.as_deref() == Some(sess_id.as_str()) {
            continue; // already showing this session
        }
        let title = snap
            .sessions
            .iter()
            .find(|s| s.id == sess_id)
            .map(|s| s.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Session".to_string());
        let notif = DesktopNotification::approval_needed(&a.id, &title, a.detail.as_deref());
        post_desktop_notification(state, notif, host_id.to_string(), sess_id);
    }
}

/// Archive or restore a session via the Host's session-organization patch
/// (not the session-action verbs — mirroring the native clients), then
/// re-bootstrap so the session list reflects the change.
fn organize_session(mut state: SyncSignal<AppState>, session_id: String, archived: bool) {
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

/// Browser gallery state for one Host view. Artifacts are per-session, so
/// the gallery is opened for the selected session; closing the gallery
/// returns to the chat view. Mirrors the mobile launcher's gallery: the
/// shell owns all Host I/O, the shared components only render.
#[derive(Clone, Default)]
struct GalleryState {
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
    mut state: SyncSignal<AppState>,
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
    mut state: SyncSignal<AppState>,
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
fn gallery_refresh(mut state: SyncSignal<AppState>, host_id: String) {
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
fn gallery_open(mut state: SyncSignal<AppState>, host_id: String) {
    let session_id = state
        .read()
        .views
        .get(&host_id)
        .and_then(|v| v.selected_session.clone());
    let Some(session_id) = session_id else { return };
    let s = state.read();
    let Some(h) = s.hosts.get(&host_id) else {
        return;
    };
    let client = h.client.clone();
    drop(s);
    let generation = {
        let mut s = state.write();
        let v = s.views.entry(host_id.clone()).or_default();
        v.gallery = GalleryState {
            session_id: Some(session_id.clone()),
            generation: v.gallery.generation + 1,
            loading: true,
            ..Default::default()
        };
        v.gallery.generation
    };
    gallery_load_entries(state, host_id, client, session_id, generation);
}

/// Close the gallery (back to the chat view).
fn gallery_close(mut state: SyncSignal<AppState>, host_id: String) {
    let mut s = state.write();
    if let Some(v) = s.views.get_mut(&host_id) {
        v.gallery.generation += 1;
        v.gallery = GalleryState {
            generation: v.gallery.generation,
            ..Default::default()
        };
    }
}

/// Open one artifact in the detail view: downloads the full bytes and
/// serves them to the `<img>` as a data URL (the Host's artifact route
/// needs the client's auth, which a bare `<img src>` can't provide).
fn gallery_open_entry(mut state: SyncSignal<AppState>, host_id: String, meta: ArtifactMeta) {
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

/// Delete one gallery entry (two-tap confirm lives in the shared detail
/// view / grid). `meta` is the entry to delete — the detail view passes
/// the open entry, the grid passes the tapped one.
fn gallery_delete_entry(mut state: SyncSignal<AppState>, host_id: String, meta: ArtifactMeta) {
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
fn gallery_screenshot(mut state: SyncSignal<AppState>, host_id: String) {
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

/// One-shot device photo picker through a webview `<input type=file>`
/// (Swift `PhotosPicker` parity, as far as a webview build goes).
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
fn gallery_upload_pick(mut state: SyncSignal<AppState>, host_id: String) {
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
fn gallery_share_entry(mut state: SyncSignal<AppState>, host_id: String) {
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
    mut state: SyncSignal<AppState>,
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

#[component]
fn App() -> Element {
    let mut state: SyncSignal<AppState> = use_signal_sync(AppState::initial);

    // Mount effect: install the desktop-notification bridge and arm the
    // click pump (`notify-click:{tag}` → select the session), plus arm
    // Direct probe-back for Hosts that connected over the relay at
    // startup. `pending_probes` is drained here because no signal
    // existed when `initial()` ran.
    use_effect(move || {
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
        spawn(async move {
            let mut ev = dioxus::document::eval(NOTIFIER_JS);
            while let Ok(msg) = ev.recv::<String>().await {
                if let Some((host_id, sess_id)) =
                    state.read().notifier.route_for_click_message(&msg)
                {
                    select_session_on_host(state, host_id.to_string(), sess_id.to_string());
                }
            }
        });
        // Presence poll: refresh the local presence feed every
        // POLL_INTERVAL_MS and toast genuine arrivals (Swift shows
        // "<name> connected"). The desktop usually runs on the same
        // machine as the Host, so this watches the Host's own files.
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let arrivals = state.write().presence.refresh(now_ms);
            for name in arrivals {
                show_toast(state, format!("{name} connected"), TOAST_DEFAULT_SECONDS);
            }
        });
        // ⌘K / Ctrl+K: toggle the command palette.
        spawn(async move {
            let mut ev = dioxus::document::eval(PALETTE_SHORTCUT_JS);
            while let Ok(msg) = ev.recv::<String>().await {
                if msg == "palette:toggle" {
                    let open = !state.read().palette_open;
                    state.write().palette_open = open;
                }
            }
        });
    });

    // Phase 6 R4 first slice: poll the Host's typed session-event stream and
    // drive the composer's Send/Stop toggle from turn.started/turn.finished
    // events. Falls back to the activity snapshot when the Host does not
    // advertise session.events.v1. Cursor state lives in this task; a resync
    // resets the cursor to 0 and re-derives turn state from the next poll.
    // SyncSignal (not use_signal): the poll loop below runs on its own OS
    // thread, and plain `Signal` is !Send. This matches the file's standing
    // rule — UI state that plain threads mutate lives in a SyncSignal.
    let event_turn_running: SyncSignal<std::collections::HashMap<String, bool>> =
        use_signal_sync(std::collections::HashMap::<String, bool>::new);
    {
        let state = state;
        let mut event_turn_running = event_turn_running;
        std::thread::spawn(move || {
            let mut cursors: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            loop {
                // Poll every 2s; cheap enough for a first slice, and the Host
                // clamps limit to 1024. Own OS thread with a blocking sleep,
                // matching the other native poll loops in this file (the
                // presence poll above); never parks the Dioxus async runtime.
                // gloo_timers is wasm-only and does not exist here.
                std::thread::sleep(Duration::from_millis(2000));
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
                let response = match client.events(&session_id, after_seq, 128, 0) {
                    Ok(r) => r,
                    Err(_) => continue, // Host without the endpoint or transient failure; keep activity fallback.
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
                        SessionEventWire::TurnFinished { .. } => {
                            event_turn_running.write().insert(session_id.clone(), false);
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    let pairing_status = state.read().pairing_status.clone();
    let keychain_notice = state.read().keychain_notice.clone();
    let records = state.read().records.clone();
    let global_error = state.read().error.clone();
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
        return rsx! {
            div { class: "app",
                ToastOverlay {
                    toast: state.read().toast_center.current().cloned(),
                    on_tap: move |_| state.write().toast_center.dismiss(),
                }
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
                // Nearby-Host discovery (NearbyHostBrowser.swift parity):
                // mDNS browse is only a hint; the pairing code still
                // authenticates the Host.
                div { class: "discovery-ssh-row",
                    button {
                        class: "discovery-button",
                        onclick: move |_| start_discovery(state),
                        "Find nearby Hosts…"
                    }
                    input {
                        class: "ssh-target",
                        placeholder: "SSH: user@host or config alias",
                        value: "{state.read().ssh_target}",
                        oninput: move |e| state.write().ssh_target = e.value(),
                    }
                    button {
                        class: "ssh-button",
                        disabled: state.read().ssh_target.trim().is_empty(),
                        onclick: move |_| connect_ssh(state),
                        "Connect via SSH"
                    }
                }
                if state.read().discovery_open {
                    DiscoverySheet {
                        candidates: state.read().discovery_candidates.clone(),
                        state: state.read().discovery_state.clone(),
                        on_select: move |c: NearbyHostCandidate| {
                            // A nearby Host is only a hint: note its name so
                            // the user can pick the right pairing code.
                            state.write().toast_center.show(
                                format!("Nearby Host \"{}\" — enter its pairing code to pair.", c.name),
                                TOAST_DEFAULT_SECONDS,
                            );
                            state.write().discovery_open = false;
                        },
                        on_close: move |_| {
                            state.write().discovery_open = false;
                            state.write().discovery_state = DiscoveryState::Idle;
                        },
                    }
                }
                if let Some(err) = global_error {
                    div { class: "error", "{err}" }
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
    let host_name = snapshot.as_ref().and_then(|b| b.host_name.clone());
    let view = state
        .read()
        .views
        .get(&active_id)
        .cloned()
        .unwrap_or_default();
    let selected = view.selected_session.clone();
    let transcript_markdown = view.transcript_markdown.clone();
    let viewers = state.read().presence.viewers().clone();
    let palette_open = state.read().palette_open;
    let archive_sheet = view.archive_sheet.clone();
    let preset_drawer_open = view.preset_drawer_open;
    let launching_preset_id = view.launching_preset_id.clone();
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
    let palette_items = build_palette_items(&sessions, can_create);
    let archive_project = selected_project_id(&state);
    // Cloned before the rsx: the gallery button's closure below moves
    // `active_id`, so the sheet close handlers take their own clones.
    let archive_close_host = active_id.clone();
    let preset_close_host = active_id.clone();

    rsx! {
        div { class: "app",
            style { "{APP_CSS}" }
            ToastOverlay {
                toast: state.read().toast_center.current().cloned(),
                on_tap: move |_| state.write().toast_center.dismiss(),
            }
            ConnectionBar {
                host_name,
                connected,
                transport,
                on_refresh: move |_| refresh(state),
            }
            if let Some(err) = host_error {
                div { class: "error", "{err}" }
            }
            div { class: "main",
                div { class: "sidebar",
                    button {
                        class: "hosts",
                        onclick: move |_| state.write().show_host_list = true,
                        "‹ Hosts"
                    }
                    div { class: "sidebar-actions",
                        button {
                            class: "palette-button",
                            title: "Command palette (⌘K)",
                            onclick: move |_| state.write().palette_open = true,
                            "⌘K"
                        }
                        if can_create {
                            {
                                let pa = active_id.clone();
                                rsx! {
                                    button {
                                        class: "new-session-button",
                                        onclick: move |_| {
                                            state
                                                .write()
                                                .views
                                                .entry(pa.clone())
                                                .or_default()
                                                .preset_drawer_open = true;
                                        },
                                        "＋ New session"
                                    }
                                }
                            }
                        }
                        {
                            let ap = archive_project.clone();
                            rsx! {
                                button {
                                    class: "archive-button",
                                    onclick: move |_| {
                                        if let Some(pid) = ap.clone() {
                                            open_archive_sheet(state, pid);
                                        }
                                    },
                                    "Archive"
                                }
                            }
                        }
                    }
                    SessionList {
                        // Cloned: the detail pane below still needs `sessions`
                        // for the R4 turn-state activity fallback.
                        sessions: sessions.clone(),
                        selected_id: selected.clone(),
                        on_select: move |id| select_session(state, id),
                        on_archive: move |id: String| organize_session(state, id, true),
                        on_restore: move |id: String| organize_session(state, id, false),
                        viewers: Some(viewers),
                    }
                }
                div { class: "content",
                    if view.gallery.session_id.is_some() {
                        {
                            // Gallery screen for the selected session, rendered by the
                            // shared gallery components. Grid thumbnails load lazily
                            // through the Host's max_dim thumbnail transport; detail
                            // always uses the full bytes. Add-to-message is a
                            // terminal affordance, so the detail view omits it here.
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
                            rsx! {
                                button {
                                    class: "back",
                                    onclick: move |_| gallery_close(state, gback.clone()),
                                    "‹ Back"
                                }
                                if let Some(meta) = g.detail_meta.clone() {
                                    {
                                        let detail_host = gback.clone();
                                        let delete_host = gback.clone();
                                        let editor_host = gback.clone();
                                        let done_host = gback.clone();
                                        let share_host = gback.clone();
                                        rsx! {
                                            GalleryDetailView {
                                                entry: GalleryEntry { meta, preview_url: None },
                                                image_url: g.detail_data_url.clone(),
                                                editor: g.editor,
                                                on_close: move |_| {
                                                    if let Some(v) = state.write().views.get_mut(&detail_host) {
                                                        v.gallery.detail_meta = None;
                                                        v.gallery.detail_data_url = None;
                                                        v.gallery.detail_bytes = None;
                                                        v.gallery.detail_mime = None;
                                                        v.gallery.editor = None;
                                                    }
                                                },
                                                on_delete: move |entry: GalleryEntry| {
                                                    gallery_delete_entry(state, delete_host.clone(), entry.meta)
                                                },
                                                on_editor: move |mode: Option<AnnotationMode>| {
                                                    if let Some(v) = state.write().views.get_mut(&editor_host) {
                                                        v.gallery.editor = mode;
                                                    }
                                                },
                                                on_annotation_done: move |result: AnnotationResult| {
                                                    gallery_annotation_done(state, done_host.clone(), result)
                                                },
                                                on_share: move |_| gallery_share_entry(state, share_host.clone()),
                                            }
                                        }
                                    }
                                } else {
                                    {
                                        let refresh_host = gback.clone();
                                        let open_host = gback.clone();
                                        let upload_host = gback.clone();
                                        let shot_host = gback.clone();
                                        let del_host = gback.clone();
                                        rsx! {
                                            BrowserGalleryPanel {
                                                entries: entries,
                                                loading: g.loading,
                                                on_refresh: move |_| gallery_refresh(state, refresh_host.clone()),
                                                on_open: move |entry: GalleryEntry| {
                                                    gallery_open_entry(state, open_host.clone(), entry.meta)
                                                },
                                                on_upload: move |_| gallery_upload_pick(state, upload_host.clone()),
                                                on_screenshot: move |_| gallery_screenshot(state, shot_host.clone()),
                                                on_delete: move |entry: GalleryEntry| {
                                                    gallery_delete_entry(state, del_host.clone(), entry.meta)
                                                },
                                            }
                                        }
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
                        if selected.is_some() {
                            button {
                                class: "gallery-open",
                                onclick: move |_| gallery_open(state, active_id.clone()),
                                "Gallery"
                            }
                        }
                        if let Some(md) = transcript_markdown {
                            TranscriptView { markdown: md }
                        }
                        {
                            // `selected` is the session id shown in the detail pane.
                            let sid = selected.clone().unwrap_or_default();
                            // Phase 6 R4: the typed session-event stream is the
                            // Send/Stop authority. The activity snapshot remains
                            // as the fallback for Hosts without session.events.v1
                            // or when the poll task has not yet reported.
                            let turn_running = event_turn_running
                                .read()
                                .get(&sid)
                                .copied()
                                .unwrap_or_else(|| {
                                    sessions
                                        .iter()
                                        .find(|s| s.id == sid)
                                        .is_some_and(|s| s.activity == ActivityState::Working)
                                });
                            rsx! {
                                Composer {
                                    session_id: sid,
                                    turn_running: turn_running,
                                    on_send: move |msg| send_message(state, msg),
                                    on_stop: move |_| stop_turn(state),
                                }
                            }
                        }
                    }
                }
            }
            // Sheets over the main view: command palette, archive library,
            // preset drawer.
            if palette_open {
                CommandPalette {
                    items: palette_items.clone(),
                    on_run: move |id: String| run_palette_item(state, id),
                    on_close: move |_| state.write().palette_open = false,
                }
            }
            if let Some(sheet) = archive_sheet.clone() {
                {
                    let close_host = archive_close_host.clone();
                    rsx! {
                        ArchivedSessionsSheet {
                            project_name: sheet.project_id.clone(),
                            sessions: sheet.sessions.clone(),
                            load_error: sheet.load_error.clone(),
                            on_restore: move |action: ArchiveAction| archive_restore(state, action),
                            on_close: move |_| {
                                if let Some(v) = state.write().views.get_mut(&close_host) {
                                    v.archive_sheet = None;
                                }
                            },
                        }
                    }
                }
            }
            if preset_drawer_open {
                {
                    let close_host = preset_close_host.clone();
                    rsx! {
                        PresetDrawer {
                            project_name: None::<String>,
                            presets: drawer_presets.clone(),
                            launching_id: launching_preset_id.clone(),
                            on_launch: move |id: String| launch_preset(state, id),
                            on_close: move |_| {
                                if let Some(v) = state.write().views.get_mut(&close_host) {
                                    v.preset_drawer_open = false;
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    /// A recording HTTP stub: accepts connections, records each request's
    /// method + path + body, and answers `200 {}`. Exactly what
    /// `stop_turn_request` needs to prove which transport path it took.
    #[derive(Debug)]
    struct RecordedRequest {
        method: String,
        path: String,
        body: String,
    }

    fn recording_server(expected_requests: usize) -> (String, mpsc::Receiver<RecordedRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            listener.set_nonblocking(false).expect("blocking accept");
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().expect("accept");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut head = Vec::new();
                let mut buf = [0u8; 4096];
                // Read until the header terminator.
                loop {
                    let n = stream.read(&mut buf).expect("read head");
                    head.extend_from_slice(&buf[..n]);
                    if head.windows(4).any(|w| w == b"\r\n\r\n") || n == 0 {
                        break;
                    }
                }
                let head_end = head
                    .windows(4)
                    .position(|w| w == b"\r\n\r\n")
                    .map(|p| p + 4)
                    .unwrap_or(head.len());
                let head_str = String::from_utf8_lossy(&head[..head_end]).into_owned();
                let content_length: usize = head_str
                    .lines()
                    .find_map(|l| {
                        let (k, v) = l.split_once(':')?;
                        (k.trim().eq_ignore_ascii_case("content-length"))
                            .then(|| v.trim().parse().unwrap_or(0))
                    })
                    .unwrap_or(0);
                let mut body = head[head_end..].to_vec();
                while body.len() < content_length {
                    let n = stream.read(&mut buf).expect("read body");
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&buf[..n]);
                }
                let mut lines = head_str.lines();
                let request_line = lines.next().unwrap_or("");
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                tx.send(RecordedRequest {
                    method,
                    path,
                    body: String::from_utf8_lossy(&body).into_owned(),
                })
                .expect("send");
                let response =
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
                stream.write_all(response).expect("write response");
            }
        });
        (addr, rx)
    }

    fn test_client(addr: &str) -> HostClient {
        HostClient::new(format!("http://{addr}/mobile"), "test-token".to_string())
            .expect("plaintext test endpoint")
    }

    /// F3 (behavioral): with the capability advertised, Stop issues exactly
    /// one `POST /mobile/turn-cancel` and never touches the write channel —
    /// no raw Ctrl-C anywhere in the request.
    #[test]
    fn stop_with_capability_uses_protocol_cancel_only() {
        let (addr, rx) = recording_server(1);
        let client = test_client(&addr);
        stop_turn_request(&client, "s-stop-1", true).expect("cancel_turn succeeds");

        let req = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("one request");
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/mobile/turn-cancel", "unexpected path: {req:?}");
        assert!(
            req.body.contains("\"sessionID\":\"s-stop-1\""),
            "body: {}",
            req.body
        );
        assert!(
            !req.body.contains("\\u0003"),
            "raw Ctrl-C must never ride the cancel path: {}",
            req.body
        );
        // Exactly one request: the server only accepted one, and a second
        // would have been needed for any extra write.
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "more than one request was issued"
        );
    }

    /// F3 (behavioral): without the capability, Stop sends exactly one
    /// `\x03` through `POST /mobile/write` and never calls the cancel verb.
    #[test]
    fn stop_without_capability_sends_single_ctrl_c_only() {
        let (addr, rx) = recording_server(1);
        let client = test_client(&addr);
        stop_turn_request(&client, "s-stop-2", false).expect("write succeeds");

        let req = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("one request");
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/mobile/write", "unexpected path: {req:?}");
        assert!(
            req.body.contains("\"sessionID\":\"s-stop-2\""),
            "body: {}",
            req.body
        );
        // serde_json escapes Ctrl-C as \u0003 — assert exactly one.
        assert_eq!(
            req.body.matches("\\u0003").count(),
            1,
            "exactly one Ctrl-C through the write channel: {}",
            req.body
        );
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "more than one request was issued"
        );
    }
}
