//! Voice dictation, ported from the Swift `VoiceDictationController` +
//! `DictationReflection`.
//!
//! The audio/speech backend is platform-specific: iOS uses the Speech
//! framework, the Dioxus mobile launcher drives the Web Speech API through
//! JS eval in its webview and feeds transcripts back here. This module owns
//! the platform-independent half: the recording/refining state machine, the
//! commit decision (short utterances skip the reflection pass), the
//! reflection-output sanitizer (exact port of Swift's
//! `DictationReflector.sanitized`), and the user-facing settings. It also
//! ships [`DictationView`], the renderer-side mic button + transcript pill;
//! the launcher only has to place it and bracketed-paste `on_commit`.

use dioxus::prelude::*;

/// Minimum word count for the reflection pass. Menu answers and
/// confirmations ("yes", "2", "continue") skip it — latency there is pure
/// cost. Mirrors `VoiceDictationController.commit`.
pub const REFLECTION_MIN_WORDS: usize = 5;

/// Dictation lifecycle, mirroring the Swift controller's published state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DictationPhase {
    /// Nothing happening.
    #[default]
    Idle,
    /// Mic open, live transcript accumulating.
    Recording,
    /// Recording stopped; the reflection model is polishing the transcript.
    Refining,
}

impl DictationPhase {
    pub fn is_active(&self) -> bool {
        !matches!(self, DictationPhase::Idle)
    }
}

/// User-facing dictation preferences. The launcher persists
/// `reflection_enabled` (Swift used `UserDefaults`); the default is ON —
/// the pass is conservative and always falls back to the verbatim
/// transcript, so enabling it is never worse than off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DictationSettings {
    pub reflection_enabled: bool,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            reflection_enabled: true,
        }
    }
}

/// Decide whether a finished transcript should go through the reflection
/// pass. Mirrors the Swift commit gate: enabled, long enough, and a
/// reflector available.
pub fn should_refine(text: &str, settings: &DictationSettings, reflector_available: bool) -> bool {
    settings.reflection_enabled
        && reflector_available
        && text.split_whitespace().count() >= REFLECTION_MIN_WORDS
}

/// Port of Swift's `DictationReflector.sanitized`.
///
/// Dictated text often *looks like a question to the model*; these checks
/// catch it answering instead of cleaning. Returns `None` when the pass
/// should be skipped — the caller then commits the verbatim text.
pub fn sanitize_reflection(output: Option<&str>, original: &str) -> Option<String> {
    let mut text = output?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        text = text[1..text.len() - 1].to_string();
    }
    // Cleanup only removes; a much longer result means the model answered.
    if text.len() > original.len() * 2 + 40 {
        return None;
    }
    // Spoken transcripts are a single line; a multi-line result is a
    // wrong-shaped answer (and would submit early via the paste path).
    if text.contains('\n') {
        return None;
    }
    Some(text)
}

/// The recording side of the controller: accumulates the live transcript
/// and resolves the finished text on stop/cancel. The launcher feeds
/// `on_transcript` from its speech backend and calls `stop` / `cancel`
/// from the mic UI. Backend errors and recognizer deaths land in
/// `error` (mirroring Swift's `errorMessage`) without destroying a
/// recoverable transcript.
pub struct DictationSession {
    phase: DictationPhase,
    transcript: String,
    error: Option<String>,
    /// Verbatim text captured at stop time while a native-shell reflection
    /// pass is in flight; the fallback commit when the shell answers empty
    /// or never answers (the JS backstop in [`native_reflect_js`]).
    refine_original: String,
    /// Nonce of the in-flight reflection; stale `dictation:refined:`
    /// answers (a late shell reply, a backstop from an older session) can
    /// never commit over a newer dictation.
    refine_nonce: u64,
    refine_counter: u64,
}

/// The mic button's effect, decided by the pre-toggle phase. The old
/// `Option<String>` return was ambiguous: `None` meant both "started
/// recording" and "stopped an empty recording", so the view sent the
/// backend a second `start` when the user stopped an empty recording.
/// The enum makes every case explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationAction {
    /// Idle → recording: the backend must start listening.
    Started,
    /// Recording → idle with a transcript: the backend must stop, then
    /// the transcript commits.
    Stopped(String),
    /// Recording → idle with nothing captured: the backend must still
    /// stop, but there is nothing to commit.
    StoppedEmpty,
    /// Tap while refining: ignored, the backend keeps its state.
    Ignored,
}

impl DictationSession {
    pub fn new() -> Self {
        Self {
            phase: DictationPhase::Idle,
            transcript: String::new(),
            error: None,
            refine_original: String::new(),
            refine_nonce: 0,
            refine_counter: 0,
        }
    }

    pub fn phase(&self) -> DictationPhase {
        self.phase
    }

    pub fn transcript(&self) -> &str {
        &self.transcript
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Toggle entry point, mirroring Swift's `toggle(onFinish:)`: idle →
    /// start recording; recording → stop and hand back the transcript for
    /// commit (empty transcript still stops the backend, it just commits
    /// nothing).
    pub fn toggle(&mut self) -> DictationAction {
        match self.phase {
            DictationPhase::Idle => {
                self.phase = DictationPhase::Recording;
                self.transcript.clear();
                self.error = None;
                DictationAction::Started
            }
            DictationPhase::Recording => {
                self.phase = DictationPhase::Idle;
                self.error = None;
                let text = std::mem::take(&mut self.transcript);
                if text.trim().is_empty() {
                    DictationAction::StoppedEmpty
                } else {
                    DictationAction::Stopped(text)
                }
            }
            DictationPhase::Refining => DictationAction::Ignored,
        }
    }

    /// Live transcript update from the speech backend. Latest wins, like the
    /// Swift analyzer's finalized + volatile text.
    pub fn on_transcript(&mut self, text: &str) {
        if self.phase == DictationPhase::Recording {
            self.transcript = text.to_string();
        }
    }

    /// The recognizer died mid-recording (network drop, audio session
    /// stolen). Mirrors Swift: keep whatever transcript was captured, drop
    /// back to idle, and surface an error so the pill can offer Paste.
    pub fn backend_interrupted(&mut self) {
        if self.phase == DictationPhase::Recording {
            self.phase = DictationPhase::Idle;
            self.error = Some("Dictation was interrupted".to_string());
        }
    }

    /// Commit path for a transcript kept across an interruption: idle with
    /// a non-empty transcript. Clears the error on take.
    pub fn take_kept(&mut self) -> Option<String> {
        if self.phase == DictationPhase::Idle && !self.transcript.trim().is_empty() {
            self.error = None;
            Some(std::mem::take(&mut self.transcript))
        } else {
            None
        }
    }

    /// Backend surfaced a problem (mic denied, recognizer unavailable).
    /// Does not touch the transcript.
    pub fn note_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Drop everything without committing — the pill's X. Also abandons an
    /// in-flight reflection; a late `dictation:refined:` answer finds no
    /// matching nonce and is ignored.
    pub fn cancel(&mut self) {
        self.phase = DictationPhase::Idle;
        self.transcript.clear();
        self.error = None;
        self.refine_original.clear();
        self.refine_nonce = 0;
    }

    /// Enter the refining phase for a native-shell reflection pass, keeping
    /// the verbatim text for the fallback commit. Returns the session nonce
    /// the shell's `dictation:refined:` answer must carry, or `None` when
    /// no reflection can start (not idle — the caller then commits
    /// verbatim).
    pub fn begin_refining(&mut self, original: &str) -> Option<u64> {
        if self.phase != DictationPhase::Idle {
            return None;
        }
        self.phase = DictationPhase::Refining;
        self.refine_counter = self.refine_counter.wrapping_add(1);
        self.refine_nonce = self.refine_counter;
        self.refine_original = original.to_string();
        Some(self.refine_nonce)
    }

    /// Complete a native-shell reflection pass. Only the answer carrying the
    /// live nonce, received while still refining, commits: stale or foreign
    /// answers are ignored. The returned text is the commit payload — the
    /// sanitized refinement, or the verbatim original when the shell sent
    /// back empty/failed (Swift's verbatim-fallback rule).
    pub fn complete_refining(&mut self, nonce: u64, refined: &str) -> Option<String> {
        if self.phase != DictationPhase::Refining || nonce != self.refine_nonce {
            return None;
        }
        self.phase = DictationPhase::Idle;
        self.refine_nonce = 0;
        let original = std::mem::take(&mut self.refine_original);
        Some(sanitize_reflection(Some(refined), &original).unwrap_or(original))
    }
}

impl Default for DictationSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Webview speech backend for dictation. One eval installs the recognizer
/// and pumps commands; JS → Rust messages are `dictation:ready`,
/// `dictation:unavailable` (no Web Speech API in this webview),
/// `dictation:text:<final 0|1>:<transcript>`, `dictation:error:<kind>`
/// (`microphone` = mic denied/unavailable, anything else = recognizer),
/// and `dictation:ended` (the recognizer stopped on its own — network
/// drop, audio session stolen). Rust → JS commands are `"start"`,
/// `"stop"`, `"cancel"`. Everything after the second colon of a `text`
/// message is transcript (colons inside it are preserved).
///
/// This is the webview half of dictation parity. The native iOS shell will
/// drive `SFSpeechRecognizer` / `SpeechAnalyzer` through the same
/// `DictationSession` API, and an on-device reflector will feed the
/// refining phase; in this build `reflector_available` is false so commits
/// are always verbatim after the Swift-parity gate.
pub const DICTATION_JS: &str = r##"
(async () => {
  const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SR) { dioxus.send('dictation:unavailable'); return; }
  let rec = null, stream = null, live = false;
  window.__unpeelDictationTeardown = () => {
    live = false;
    if (rec) { try { rec.abort(); } catch (e) {} rec = null; }
    if (stream) { stream.getTracks().forEach(t => t.stop()); stream = null; }
  };
  dioxus.send('dictation:ready');
  for (;;) {
    const cmd = await dioxus.recv();
    if (cmd === 'start') {
      if (live) continue;
      if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
        dioxus.send('dictation:error:microphone');
        continue;
      }
      try {
        stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      } catch (e) {
        dioxus.send('dictation:error:microphone');
        continue;
      }
      rec = new SR();
      rec.continuous = true;
      rec.interimResults = true;
      rec.onresult = (e) => {
        // Swift parity: the transcript is finalized segments plus the
        // current volatile (interim) segment. `e.results` persists across
        // events in a continuous session, so rebuild the whole transcript
        // every time — sending only the changed slice (from resultIndex)
        // would drop earlier finals, and the Rust side keeps latest-wins.
        let finalText = '', interim = '';
        for (let i = 0; i < e.results.length; i++) {
          const t = e.results[i][0].transcript;
          if (e.results[i].isFinal) finalText += t; else interim += t;
        }
        const text = finalText + interim;
        const fin = e.results[e.results.length - 1].isFinal ? '1' : '0';
        dioxus.send('dictation:text:' + fin + ':' + text);
      };
      rec.onerror = (e) => dioxus.send('dictation:error:' + (e.error || 'recognizer'));
      rec.onend = () => {
        if (live) { live = false; dioxus.send('dictation:ended'); }
      };
      try { rec.start(); live = true; }
      catch (e) { dioxus.send('dictation:error:recognizer'); }
    } else if (cmd === 'stop' || cmd === 'cancel') {
      const wasLive = live;
      live = false;
      if (rec) {
        try { if (cmd === 'cancel') { rec.abort(); } else { rec.stop(); } }
        catch (e) {}
        rec = null;
      }
      if (stream) { stream.getTracks().forEach(t => t.stop()); stream = null; }
      if (cmd === 'cancel' && wasLive) { dioxus.send('dictation:ended'); }
    }
  }
})()
"##;

/// Fire-and-forget teardown for unmount: releases the mic and recognizer
/// even if the control eval is already gone.
pub const DICTATION_TEARDOWN_JS: &str = r##"
if (typeof window.__unpeelDictationTeardown === 'function') {
  window.__unpeelDictationTeardown();
}
"##;

/// Native iOS shell speech contract.
///
/// The webview cannot drive `SFSpeechRecognizer` / `SpeechAnalyzer` — those
/// live in the native shell. The shell exposes one JS function:
///
/// ```js
/// window.__unpeelNativeSpeech(cmd)  // cmd: "start" | "stop" | "cancel"
/// ```
///
/// and feeds results back through the SAME `dictation:` message protocol the
/// Web Speech backend uses (`ready`, `unavailable`, `ended`, `error:<kind>`),
/// with one addition: live transcripts arrive as
/// `dictation:ntext:{"final":0|1,"text":"..."}` (JSON — the raw `text:`
/// form can't safely embed a transcript the shell didn't escape).
/// `cancel` must also abandon an in-flight reflection pass.
///
/// The shell must install `__unpeelNativeSpeech` before the webview probes
/// for it (a `WKUserScript` at document start is the reliable point).
/// Capability declarations: `NSSpeechRecognitionUsageDescription` and
/// `NSMicrophoneUsageDescription` in Info.plist. Full Mac-side instructions
/// plus drop-in Swift live in `clients/dioxus/native-shell/`.
///
/// Probe: evaluates to `true` when the native speech backend is present.
pub const NATIVE_SPEECH_PROBE_JS: &str = r#"typeof window.__unpeelNativeSpeech === 'function'"#;

/// Probe for the native reflection backend (FoundationModels). Independent
/// from the speech probe: a shell may bridge speech without reflection.
pub const NATIVE_REFLECT_PROBE_JS: &str = r#"typeof window.__unpeelNativeReflect === 'function'"#;

/// Pump script for native mode. The script itself does nothing — it only
/// needs to complete, because (like the app-lock visibility bridge) a
/// finished eval's `recv()` still receives the shell's `dioxus.send`
/// broadcasts for the handle's lifetime.
pub const NATIVE_SPEECH_PUMP_JS: &str = r#"(function(){ return 0; })()"#;

/// Build the one-shot eval source driving the native shell backend.
/// Returns `None` for anything outside the `start`/`stop`/`cancel` command
/// set — command strings are never interpolated unchecked.
pub fn native_speech_cmd_js(cmd: &str) -> Option<String> {
    match cmd {
        "start" | "stop" | "cancel" => Some(format!("window.__unpeelNativeSpeech(\"{cmd}\");")),
        _ => None,
    }
}

/// A native live-transcript message: `{"final":0|1,"text":"..."}`.
#[derive(serde::Deserialize)]
struct NativeTextMessage {
    #[serde(rename = "final")]
    is_final: u8,
    text: String,
}

/// Parse the JSON body of a `dictation:ntext:` message. Returns
/// `(is_final, text)`, or `None` for malformed input (the pump ignores it).
pub fn parse_native_text(json: &str) -> Option<(bool, String)> {
    let msg: NativeTextMessage = serde_json::from_str(json).ok()?;
    if msg.is_final > 1 {
        return None;
    }
    Some((msg.is_final == 1, msg.text))
}

/// Parse the body of a `dictation:refined:` message —
/// `{nonce}:{json-string}` — into `(nonce, refined_text)`.
pub fn parse_native_refined(body: &str) -> Option<(u64, String)> {
    let (nonce, json) = body.split_once(':')?;
    let nonce: u64 = nonce.parse().ok()?;
    let text: String = serde_json::from_str(json).ok()?;
    Some((nonce, text))
}

/// Launcher-side backstop for a native reflection pass: the Swift contract
/// caps the model at 4s, so 6s covers the JS→native→JS round trip with
/// margin. Fires the verbatim fallback when the shell never answers.
pub const REFINE_TIMEOUT_MS: u64 = 6000;

/// Build the one-shot eval source that starts a native reflection pass.
///
/// `text` is JSON-encoded (a JSON string literal is valid JS, so hostile
/// transcripts can't break out). The shell contract:
/// - `window.__unpeelNativeReflect(nonce, text)` runs the FoundationModels
///   cleanup (4s cap, Swift `sanitized` rules) and answers exactly once
///   with `dioxus.send('dictation:refined:' + nonce + ':' +
///   JSON.stringify(refined))` — `JSON.stringify("")` on any failure or
///   timeout, which the launcher commits as verbatim.
/// - The `setTimeout` backstop below covers a shell that never answers at
///   all. Double answers are harmless: the nonce + phase check in
///   [`DictationSession::complete_refining`] commits exactly once and
///   ignores the stale one.
pub fn native_reflect_js(nonce: u64, text: &str) -> String {
    let arg = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"(function() {{
  var nonce = {nonce};
  function answer(t) {{
    try {{ dioxus.send('dictation:refined:' + nonce + ':' + JSON.stringify(t)); }} catch (e) {{}}
  }}
  try {{ window.__unpeelNativeReflect(nonce, {arg}); }}
  catch (e) {{ answer(""); }}
  setTimeout(function() {{ answer(""); }}, {REFINE_TIMEOUT_MS});
}})()"#,
        nonce = nonce,
        arg = arg,
        REFINE_TIMEOUT_MS = REFINE_TIMEOUT_MS,
    )
}

/// Renderer-side dictation UI: the mic button plus the bottom transcript
/// pill. Owns the [`DictationSession`]; the launcher places this next to
/// the terminal input and bracketed-pastes `on_commit` into the session.
/// Shell-owned Host I/O stays in the launcher — this component only touches
/// the webview speech backend and the session state machine.
#[component]
pub fn DictationView(settings: DictationSettings, on_commit: EventHandler<String>) -> Element {
    let mut session = use_signal(DictationSession::new);
    let ctl = use_signal(|| None::<dioxus::document::Eval>);
    // Native iOS shell presence, probed once at mount. When the shell
    // drives speech, the Web Speech backend stays uninstalled (it would
    // double-drive the mic) and commands route to `__unpeelNativeSpeech`.
    let is_native = use_signal(|| false);
    let has_reflect = use_signal(|| false);

    // Mount: probe for the native shell, install the backend, and pump
    // JS → Rust messages. In native mode the pump runs on a no-op eval —
    // a finished eval's recv() still gets the shell's dioxus.send
    // broadcasts (same as the app-lock visibility bridge).
    use_effect(move || {
        let mut ctl = ctl;
        let mut session = session;
        let mut is_native = is_native;
        let mut has_reflect = has_reflect;
        let on_commit = on_commit;
        spawn(async move {
            if ctl.read().is_some() {
                return;
            }
            let native = dioxus::document::eval(NATIVE_SPEECH_PROBE_JS)
                .join::<bool>()
                .await
                .unwrap_or(false);
            is_native.set(native);
            let pump_js = if native {
                has_reflect.set(
                    dioxus::document::eval(NATIVE_REFLECT_PROBE_JS)
                        .join::<bool>()
                        .await
                        .unwrap_or(false),
                );
                NATIVE_SPEECH_PUMP_JS
            } else {
                DICTATION_JS
            };
            let ev = dioxus::document::eval(pump_js);
            ctl.set(Some(ev));
            let mut ev = ev;
            while let Ok(msg) = ev.recv::<String>().await {
                if let Some(rest) = msg.strip_prefix("dictation:") {
                    if let Some(rest) = rest.strip_prefix("text:") {
                        // "text:<final>:<transcript>"; colons in the
                        // transcript are preserved.
                        let (fin, text) = rest.split_once(':').unwrap_or(("0", rest));
                        let _ = fin;
                        session.write().on_transcript(text);
                    } else if let Some(json) = rest.strip_prefix("ntext:") {
                        // Native shell live transcript (JSON — the raw
                        // `text:` form can't safely embed shell text).
                        if let Some((fin, text)) = parse_native_text(json) {
                            let _ = fin;
                            session.write().on_transcript(&text);
                        }
                    } else if let Some(body) = rest.strip_prefix("refined:") {
                        // `refined:{nonce}:{json}` — the shell's reflection
                        // answer, its failure/empty signal, or the JS
                        // backstop. Stale nonces are ignored; the commit
                        // fires exactly once via the phase check.
                        if let Some((nonce, text)) = parse_native_refined(body) {
                            if let Some(final_text) =
                                session.write().complete_refining(nonce, &text)
                            {
                                on_commit.call(final_text);
                            }
                        }
                    } else {
                        match rest {
                            "ready" => session.write().clear_error(),
                            "unavailable" => session
                                .write()
                                .note_error("Speech recognition isn't available here"),
                            "ended" => session.write().backend_interrupted(),
                            r if r.starts_with("error:") => {
                                let kind = r.strip_prefix("error:").unwrap_or("recognizer");
                                let message = if kind == "microphone" {
                                    "Microphone unavailable — check permission"
                                } else {
                                    "Speech recognizer failed"
                                };
                                let mut s = session.write();
                                // If we were recording, the recognizer's
                                // death is an interruption, not a discard:
                                // keep the transcript committable.
                                if s.phase() == DictationPhase::Recording {
                                    s.backend_interrupted();
                                    s.note_error(message);
                                } else {
                                    s.note_error(message);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        });
    });

    // Unmount: release the mic even if the control eval is gone. Native
    // mode routes through the shell's cancel (which also abandons an
    // in-flight reflection pass).
    let is_native_drop = is_native;
    use_drop(move || {
        if *is_native_drop.read() {
            if let Some(js) = native_speech_cmd_js("cancel") {
                let _ = dioxus::document::eval(&js);
            }
        } else {
            let _ = dioxus::document::eval(DICTATION_TEARDOWN_JS);
        }
    });

    let is_native_cmd = is_native;
    let send_cmd = move |cmd: &'static str| {
        if *is_native_cmd.read() {
            if let Some(js) = native_speech_cmd_js(cmd) {
                let _ = dioxus::document::eval(&js);
            }
        } else if let Some(ev) = ctl.read().as_ref() {
            let _ = ev.send(cmd);
        }
    };

    let phase = session.read().phase();
    let transcript = session.read().transcript().to_string();
    let error = session.read().error().map(|s| s.to_string());

    let is_native_mic = is_native;
    let has_reflect_mic = has_reflect;
    let on_mic = move |_: MouseEvent| {
        // The action is decided by the pre-toggle phase, so stopping an
        // empty recording still tells the backend to stop (never a second
        // `start`), and taps while refining are ignored outright. The
        // toggle result is owned, so the write guard is released before
        // the arms run.
        let action = session.write().toggle();
        match action {
            DictationAction::Started => send_cmd("start"),
            DictationAction::Stopped(text) => {
                send_cmd("stop");
                // Native shell reflector: the same Swift gate (enabled,
                // long enough, reflector present). The commit lands
                // asynchronously via `dictation:refined:` in the pump;
                // the webview build has no reflector, so it commits
                // verbatim here.
                let want_reflect = *is_native_mic.read()
                    && *has_reflect_mic.read()
                    && should_refine(&text, &settings, true);
                if want_reflect {
                    if let Some(nonce) = session.write().begin_refining(&text) {
                        let _ = dioxus::document::eval(&native_reflect_js(nonce, &text));
                    } else {
                        on_commit.call(text);
                    }
                } else {
                    on_commit.call(text);
                }
            }
            DictationAction::StoppedEmpty => send_cmd("stop"),
            DictationAction::Ignored => {}
        }
    };

    let on_cancel = move |_: MouseEvent| {
        send_cmd("cancel");
        session.write().cancel();
    };

    let on_paste_kept = move |_: MouseEvent| {
        if let Some(text) = session.write().take_kept() {
            on_commit.call(text);
        }
    };

    let show_pill = phase.is_active() || error.is_some() || !session.read().transcript().is_empty();

    rsx! {
        div { class: "dictation-wrap", "data-testid": "dictation-wrap",
            button {
                class: if phase == DictationPhase::Recording { "dictation-mic recording" } else { "dictation-mic" },
                "data-testid": "dictation-toggle",
                onclick: on_mic,
                title: if phase == DictationPhase::Recording { "Stop dictation" } else { "Dictate" },
                "🎤"
            }
            if show_pill {
                div { class: "dictation-pill", "data-testid": "dictation-status",
                    if phase == DictationPhase::Refining {
                        span { class: "dictation-refining", "Refining…" }
                    } else if !transcript.is_empty() {
                        span { class: "dictation-text", "{transcript}" }
                    } else if phase == DictationPhase::Recording {
                        span { class: "dictation-hint", "Listening…" }
                    }
                    if let Some(message) = error.as_deref() {
                        span { class: "dictation-error", "{message}" }
                    }
                    if phase == DictationPhase::Recording {
                        button { class: "dictation-stop", onclick: on_mic, "Stop" }
                    } else if !session.read().transcript().is_empty() && error.is_some() {
                        button { class: "dictation-paste", onclick: on_paste_kept, "Paste" }
                    }
                    button { class: "dictation-cancel", onclick: on_cancel, "✕" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_accepts_clean_cleanup() {
        let out = sanitize_reflection(Some("  Hello, world.  "), "hello world");
        assert_eq!(out.as_deref(), Some("Hello, world."));
    }

    #[test]
    fn sanitizer_strips_wrapping_quotes() {
        let out = sanitize_reflection(Some("\"Deploy on Friday\""), "deploy on friday");
        assert_eq!(out.as_deref(), Some("Deploy on Friday"));
    }

    #[test]
    fn sanitizer_rejects_empty_and_none() {
        assert_eq!(sanitize_reflection(None, "hi"), None);
        assert_eq!(sanitize_reflection(Some("   "), "hi"), None);
    }

    #[test]
    fn sanitizer_rejects_model_answering() {
        let original = "what time is it";
        let answered = "The current time is 3:04 PM in your timezone. ".repeat(10);
        assert_eq!(sanitize_reflection(Some(&answered), original), None);
    }

    #[test]
    fn sanitizer_rejects_multiline() {
        let out = sanitize_reflection(Some("line one\nline two"), "line one line two");
        assert_eq!(out, None);
    }

    #[test]
    fn sanitizer_allows_boundary_length() {
        let original = "ab";
        let ok_text = "x".repeat(original.len() * 2 + 40);
        assert!(sanitize_reflection(Some(&ok_text), original).is_some());
        let too_long = "x".repeat(original.len() * 2 + 41);
        assert!(sanitize_reflection(Some(&too_long), original).is_none());
    }

    #[test]
    fn refine_gate_skips_short_utterances() {
        let settings = DictationSettings::default();
        assert!(!should_refine("yes", &settings, true));
        assert!(!should_refine("continue please", &settings, true));
        assert!(should_refine(
            "please restart the staging server and tail the logs",
            &settings,
            true
        ));
    }

    #[test]
    fn refine_gate_respects_settings_and_availability() {
        let off = DictationSettings {
            reflection_enabled: false,
        };
        assert!(!should_refine(
            "please restart the staging server and tail the logs",
            &off,
            true
        ));
        let on = DictationSettings::default();
        assert!(!should_refine(
            "please restart the staging server and tail the logs",
            &on,
            false
        ));
    }

    #[test]
    fn toggle_lifecycle() {
        let mut session = DictationSession::new();
        assert_eq!(session.phase(), DictationPhase::Idle);

        // Start recording.
        assert_eq!(session.toggle(), DictationAction::Started);
        assert_eq!(session.phase(), DictationPhase::Recording);

        session.on_transcript("partial…");
        session.on_transcript("restart the staging server");
        assert_eq!(session.transcript(), "restart the staging server");

        // Stop: transcript comes back for commit.
        assert_eq!(
            session.toggle(),
            DictationAction::Stopped("restart the staging server".to_string())
        );
        assert_eq!(session.phase(), DictationPhase::Idle);
    }

    #[test]
    fn empty_recording_commits_nothing() {
        let mut session = DictationSession::new();
        assert_eq!(session.toggle(), DictationAction::Started);
        session.on_transcript("   ");
        // Empty transcript: the backend still stops, but nothing commits.
        assert_eq!(session.toggle(), DictationAction::StoppedEmpty);
        assert_eq!(session.phase(), DictationPhase::Idle);
    }

    #[test]
    fn cancel_drops_transcript() {
        let mut session = DictationSession::new();
        session.toggle();
        session.on_transcript("don't commit this");
        session.cancel();
        assert_eq!(session.phase(), DictationPhase::Idle);
        assert!(session.transcript().is_empty());
    }

    #[test]
    fn transcript_ignored_when_idle() {
        let mut session = DictationSession::new();
        session.on_transcript("stray");
        assert!(session.transcript().is_empty());
    }

    #[test]
    fn backend_interruption_keeps_transcript_committable() {
        let mut session = DictationSession::new();
        session.toggle();
        session.on_transcript("deploy to staging");
        session.backend_interrupted();
        assert_eq!(session.phase(), DictationPhase::Idle);
        assert_eq!(session.transcript(), "deploy to staging");
        assert!(session.error().is_some());
        // The kept transcript can still be committed, then it's gone.
        assert_eq!(session.take_kept().as_deref(), Some("deploy to staging"));
        assert!(session.error().is_none());
        assert_eq!(session.take_kept(), None);
    }

    #[test]
    fn backend_interruption_is_noop_when_idle() {
        let mut session = DictationSession::new();
        session.backend_interrupted();
        assert_eq!(session.phase(), DictationPhase::Idle);
        assert!(session.error().is_none());
    }

    #[test]
    fn toggle_clears_error_and_transcript() {
        let mut session = DictationSession::new();
        session.note_error("boom");
        session.toggle();
        assert!(session.error().is_none());
        assert_eq!(session.phase(), DictationPhase::Recording);
    }

    #[test]
    fn toggle_while_refining_is_ignored() {
        let mut session = DictationSession::new();
        session.toggle(); // start
        session.on_transcript("deploy to staging");
        // Stop with a transcript, then begin the native refining pass and
        // tap mid-refine.
        assert!(matches!(session.toggle(), DictationAction::Stopped(_)));
        let nonce = session.begin_refining("deploy to staging");
        assert!(nonce.is_some());
        assert_eq!(session.toggle(), DictationAction::Ignored);
        assert_eq!(session.phase(), DictationPhase::Refining);
    }

    #[test]
    fn complete_refining_commits_sanitized_refinement() {
        let mut session = DictationSession::new();
        session.toggle();
        session.on_transcript("deploy to staging please");
        let DictationAction::Stopped(text) = session.toggle() else {
            panic!("expected Stopped");
        };
        let nonce = session.begin_refining(&text).expect("refine starts");
        let committed = session
            .complete_refining(nonce, "Deploy to staging, please.")
            .expect("live nonce commits");
        assert_eq!(committed, "Deploy to staging, please.");
        assert_eq!(session.phase(), DictationPhase::Idle);
    }

    #[test]
    fn complete_refining_empty_answer_falls_back_to_verbatim() {
        let mut session = DictationSession::new();
        session.toggle();
        session.on_transcript("ship it now folks");
        let DictationAction::Stopped(text) = session.toggle() else {
            panic!("expected Stopped");
        };
        let nonce = session.begin_refining(&text).expect("refine starts");
        // Empty = shell failure/timeout/backstop: verbatim, never dropped.
        let committed = session.complete_refining(nonce, "").expect("commits");
        assert_eq!(committed, "ship it now folks");
    }

    #[test]
    fn complete_refining_rejects_stale_nonce() {
        let mut session = DictationSession::new();
        session.toggle();
        session.on_transcript("restart the agent now");
        let DictationAction::Stopped(text) = session.toggle() else {
            panic!("expected Stopped");
        };
        let nonce = session.begin_refining(&text).expect("refine starts");
        // A backstop/answer from an older session (nonce - 1) is ignored.
        assert_eq!(
            session.complete_refining(nonce.wrapping_sub(1), "Hi."),
            None
        );
        assert_eq!(session.phase(), DictationPhase::Refining);
        // The live nonce still commits exactly once.
        assert!(session.complete_refining(nonce, "Hi.").is_some());
        assert_eq!(session.complete_refining(nonce, "Hi."), None);
    }

    #[test]
    fn complete_refining_rejects_model_answering() {
        let mut session = DictationSession::new();
        session.toggle();
        session.on_transcript("what time is it right now");
        let DictationAction::Stopped(_) = session.toggle() else {
            panic!("expected Stopped");
        };
        let original = "what time is it right now";
        let nonce = session.begin_refining(original).expect("refine starts");
        let answered = "The current time is 3:04 PM. ".repeat(10);
        let committed = session
            .complete_refining(nonce, &answered)
            .expect("commits");
        assert_eq!(committed, original);
    }

    #[test]
    fn cancel_abandons_refining_and_late_answers_are_ignored() {
        let mut session = DictationSession::new();
        session.toggle();
        session.on_transcript("check the logs please");
        let DictationAction::Stopped(text) = session.toggle() else {
            panic!("expected Stopped");
        };
        let nonce = session.begin_refining(&text).expect("refine starts");
        session.cancel();
        assert_eq!(session.complete_refining(nonce, "Late refinement."), None);
        assert_eq!(session.phase(), DictationPhase::Idle);
    }

    #[test]
    fn parse_native_text_accepts_final_and_partial() {
        let (fin, text) = parse_native_text(r#"{"final":1,"text":"hello world"}"#).expect("parses");
        assert!(fin);
        assert_eq!(text, "hello world");
        let (fin, text) =
            parse_native_text(r#"{"final":0,"text":"he said \"hi\""}"#).expect("parses");
        assert!(!fin);
        assert_eq!(text, "he said \"hi\"");
        assert_eq!(parse_native_text(r#"{"final":2,"text":"x"}"#), None);
        assert_eq!(parse_native_text("not json"), None);
    }

    #[test]
    fn parse_native_refined_splits_nonce_and_json() {
        let (nonce, text) = parse_native_refined(r#"7:"Deploy it.""#).expect("parses");
        assert_eq!(nonce, 7);
        assert_eq!(text, "Deploy it.");
        // Colons inside the JSON string survive the split.
        let (nonce, text) = parse_native_refined(r#"3:"a: b""#).expect("parses");
        assert_eq!(nonce, 3);
        assert_eq!(text, "a: b");
        assert_eq!(parse_native_refined("no-nonce"), None);
        assert_eq!(parse_native_refined("4:not-json"), None);
    }

    #[test]
    fn native_command_builder_allows_only_known_commands() {
        assert_eq!(
            native_speech_cmd_js("start").as_deref(),
            Some("window.__unpeelNativeSpeech(\"start\");")
        );
        assert_eq!(
            native_speech_cmd_js("cancel").as_deref(),
            Some("window.__unpeelNativeSpeech(\"cancel\");")
        );
        assert_eq!(native_speech_cmd_js("start\");evil();//"), None);
        assert_eq!(native_speech_cmd_js(""), None);
    }

    #[test]
    fn native_reflect_js_embeds_nonce_and_escapes_hostile_text() {
        let js = native_reflect_js(9, "say \"hi\" </script>");
        assert!(js.contains("var nonce = 9;"), "nonce embedded:\n{js}");
        // The transcript is one JSON string literal — quotes and backslashes
        // can't break out of it. (No </ escaping needed: this goes to
        // evaluateJavaScript, not into an HTML <script> block.)
        assert!(
            js.contains(r#"window.__unpeelNativeReflect(nonce, "say \"hi\" </script>")"#),
            "escaped:\n{js}"
        );
        assert!(
            js.contains("setTimeout(function() { answer(\"\"); }, 6000);"),
            "backstop:\n{js}"
        );
    }
}
