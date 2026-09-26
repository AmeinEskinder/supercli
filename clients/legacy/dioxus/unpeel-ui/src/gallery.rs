//! Browser gallery UI: the session artifact panel ported from the Swift
//! `BrowserGalleryPanel` (`clients/ios/SupercliIOS/Sources/SupercliIOS/`).
//!
//! Convention (same as `components.rs`): these are pure renderers over
//! `supercli_client` DTOs. The app shell does Host I/O — listing via
//! [`supercli_client::HostClient::browser_artifacts`], chunk reads via
//! `artifact_bytes`, deletion via `delete_artifact`, uploads via
//! `upload_artifact` — and hands snapshots down; intents travel up through
//! `EventHandler`s.
//!
//! Swift panel coverage: artifact list with thumbnails, detail view,
//! deletion, uploads, crop/arrows annotation entry points, and
//! add-to-message. Annotation flattening (baking markup into real image
//! bytes) happens in the launcher, which owns the canvas export path.

use dioxus::prelude::*;
use supercli_client::ArtifactMeta;

use crate::annotation::{Arrow, ArrowMarkupView, CropRect, CropView, FreehandView, Stroke};
use crate::i18n::t;

/// One gallery row as the app shell hands it to the panel.
#[derive(Clone, PartialEq)]
pub struct GalleryEntry {
    pub meta: ArtifactMeta,
    /// `data:` URL of the image preview, resolved by the shell through
    /// `HostClient::artifact_bytes`. `None` while loading / for
    /// non-image artifacts.
    pub preview_url: Option<String>,
}

impl GalleryEntry {
    pub fn is_image(&self) -> bool {
        self.meta.kind == "image" || self.meta.kind == "screenshot"
    }
}

/// Which annotation editor the detail view has open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationMode {
    Arrows,
    Freehand,
    Crop,
}

/// Result of one annotation session, handed back to the shell for
/// flattening into real image bytes and re-upload.
#[derive(Debug, Clone)]
pub enum AnnotationResult {
    Arrows(Vec<Arrow>),
    Freehand(Vec<Stroke>),
    Crop(CropRect),
}

/// Grid panel: thumbnails, upload, screenshot request, refresh.
///
/// The shell passes `entries` (with `preview_url`s resolved) and handles
/// `on_open` by showing [`GalleryDetailView`]; `on_upload` by running the
/// launcher file picker + `HostClient::upload_artifact`; `on_screenshot`
/// by calling `HostClient::request_screenshot`.
#[component]
pub fn BrowserGalleryPanel(
    entries: Vec<GalleryEntry>,
    loading: bool,
    on_refresh: EventHandler<()>,
    on_open: EventHandler<GalleryEntry>,
    on_delete: EventHandler<GalleryEntry>,
    on_upload: EventHandler<()>,
    on_screenshot: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "gallery-panel", "data-testid": "gallery-panel",
            div { class: "gallery-toolbar",
                span { class: "gallery-title", "data-testid": "gallery-title", {t("gallery.browser_gallery")} }
                div { class: "annotation-spacer" }
                button {
                    "data-testid": "gallery-screenshot",
                    onclick: move |_| on_screenshot.call(()),
                    title: {t("gallery.ask_the_host_to_capture_a_screenshot_int")},
                    {t("gallery.screenshot")}
                }
                button { "data-testid": "gallery-upload", onclick: move |_| on_upload.call(()), {t("gallery.upload")} }
                button { "data-testid": "gallery-refresh", onclick: move |_| on_refresh.call(()), {t("gallery.refresh")} }
            }
            if loading {
                div { class: "gallery-loading", "Loading…" }
            } else if entries.is_empty() {
                div { class: "gallery-empty",
                    "No artifacts yet. Upload an image or ask the Host for a screenshot."
                }
            } else {
                div { class: "gallery-grid",
                    for (idx, entry) in entries.iter().enumerate() {
                        button {
                            class: "gallery-thumb",
                            "data-testid": format!("gallery-entry-{idx}"),
                            onclick: {
                                let entry = entry.clone();
                                move |_| on_open.call(entry.clone())
                            },
                            if let Some(url) = entry.preview_url.as_ref() {
                                img { src: "{url}", alt: "{entry.meta.name}", loading: "lazy" }
                            } else {
                                div { class: "gallery-thumb-placeholder", "{entry.meta.kind}" }
                            }
                            span { class: "gallery-thumb-name", "{entry.meta.name}" }
                            span {
                                class: "gallery-thumb-delete",
                                role: "button",
                                aria_label: {t("gallery.delete")},
                                onclick: {
                                    let entry = entry.clone();
                                    move |ev| {
                                        ev.stop_propagation();
                                        on_delete.call(entry.clone());
                                    }
                                },
                                "×"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Detail view for one artifact: full image, annotate (arrows / freehand /
/// crop), delete (two-tap confirm), add-to-message.
///
/// The shell controls `editor`: `Some(mode)` shows the matching editor
/// over `image_url`; its result comes back through `on_annotation_done`.
/// `on_add_to_message` is optional: terminal shells paste the artifact
/// reference into the PTY; shells without a terminal (the desktop chat
/// GUI) omit it and no button is rendered. `on_share` opens the platform
/// share sheet.
#[component]
pub fn GalleryDetailView(
    entry: GalleryEntry,
    image_url: Option<String>,
    editor: Option<AnnotationMode>,
    on_close: EventHandler<()>,
    on_delete: EventHandler<GalleryEntry>,
    on_editor: EventHandler<Option<AnnotationMode>>,
    on_annotation_done: EventHandler<AnnotationResult>,
    on_add_to_message: Option<EventHandler<GalleryEntry>>,
    on_share: EventHandler<GalleryEntry>,
) -> Element {
    let mut confirming_delete = use_signal(|| false);

    rsx! {
        div { class: "gallery-detail",
            div { class: "gallery-toolbar",
                button { onclick: move |_| on_close.call(()), {t("gallery.gallery")} }
                span { class: "gallery-title", "{entry.meta.name}" }
                div { class: "annotation-spacer" }
                if entry.is_image() {
                    button {
                        onclick: {
                            let entry = entry.clone();
                            move |_| on_share.call(entry.clone())
                        },
                        {t("gallery.share")}
                    }
                    if let Some(on_add_to_message) = on_add_to_message {
                        button {
                            onclick: {
                                let entry = entry.clone();
                                move |_| on_add_to_message.call(entry.clone())
                            },
                            {t("gallery.add_to_message")}
                        }
                    }
                }
                if *confirming_delete.read() {
                    button {
                        class: "danger",
                        onclick: {
                            let entry = entry.clone();
                            move |_| on_delete.call(entry.clone())
                        },
                        {t("gallery.confirm_delete")}
                    }
                } else {
                    button {
                        onclick: move |_| confirming_delete.set(true),
                        {t("gallery.delete")}
                    }
                }
            }
            if let Some(mode) = editor {
                div { class: "gallery-editor",
                    {
                        let src = image_url.clone().unwrap_or_default();
                        match mode {
                            AnnotationMode::Arrows => rsx! {
                                ArrowMarkupView {
                                    image_src: src,
                                    on_done: move |arrows| {
                                        on_editor.call(None);
                                        on_annotation_done.call(AnnotationResult::Arrows(arrows));
                                    },
                                    on_cancel: move |_| on_editor.call(None),
                                }
                            },
                            AnnotationMode::Freehand => rsx! {
                                FreehandView {
                                    image_src: src,
                                    on_done: move |strokes| {
                                        on_editor.call(None);
                                        on_annotation_done.call(AnnotationResult::Freehand(strokes));
                                    },
                                    on_cancel: move |_| on_editor.call(None),
                                }
                            },
                            AnnotationMode::Crop => rsx! {
                                CropView {
                                    image_src: src,
                                    on_done: move |rect| {
                                        on_editor.call(None);
                                        on_annotation_done.call(AnnotationResult::Crop(rect));
                                    },
                                    on_cancel: move |_| on_editor.call(None),
                                }
                            },
                        }
                    }
                }
            } else {
                div { class: "gallery-image-wrap",
                    if let Some(url) = image_url.as_ref() {
                        img { src: "{url}", alt: "{entry.meta.name}", class: "gallery-image" }
                    } else {
                        div { class: "gallery-loading", "Loading…" }
                    }
                }
                if entry.is_image() {
                    div { class: "gallery-actions",
                        button { "data-testid": "gallery-annotate-arrows", onclick: move |_| on_editor.call(Some(AnnotationMode::Arrows)), {t("gallery.arrows")} }
                        button { "data-testid": "gallery-annotate-draw", onclick: move |_| on_editor.call(Some(AnnotationMode::Freehand)), {t("gallery.draw")} }
                        button { "data-testid": "gallery-annotate-crop", onclick: move |_| on_editor.call(Some(AnnotationMode::Crop)), {t("gallery.crop")} }
                    }
                }
                div { class: "gallery-meta",
                    span { "{entry.meta.kind} · {entry.meta.size} bytes" }
                }
            }
        }
    }
}

/// Serialize the webview share arguments as one JSON object.
///
/// The whole payload (filename, MIME type, base64 bytes) travels as a
/// single JSON literal, so a hostile filename can never corrupt the
/// surrounding script: JSON string escaping keeps it inside its quotes,
/// and there is only one substitution point.
pub fn share_entry_payload(name: &str, mime: &str, bytes: &[u8]) -> String {
    use base64::Engine as _;
    let payload = serde_json::json!({
        "name": name,
        "mime": mime,
        "b64": base64::engine::general_purpose::STANDARD.encode(bytes),
    });
    // Serialization of this fixed shape cannot fail; the fallback keeps a
    // total function so callers never have to handle the impossible.
    serde_json::to_string(&payload)
        .unwrap_or_else(|_| "{\"name\":\"image\",\"mime\":\"image/png\",\"b64\":\"\"}".to_string())
}

/// Build the webview share script for one gallery entry (Swift `ShareLink`
/// parity, as far as a webview build goes: `navigator.share` with a `File`
/// when supported, otherwise a download fallback).
///
/// The payload from [`share_entry_payload`] is substituted exactly once.
/// `str::replace` never rescans its own replacement text, so a filename
/// containing placeholder-looking text (e.g. `{B64}`) is embedded verbatim
/// instead of being clobbered by a later substitution — the flaw the old
/// three-token sequential replacement had.
pub fn share_entry_js(name: &str, mime: &str, bytes: &[u8]) -> String {
    const TEMPLATE: &str = r##"
(async (payload) => {
  const { name, mime, b64 } = payload;
  try {
    const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    const file = new File([bytes], name, { type: mime });
    if (navigator.canShare && navigator.canShare({ files: [file] })) {
      await navigator.share({ files: [file] });
      dioxus.send('share:shared');
    } else {
      const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
      const a = document.createElement('a');
      a.href = url;
      a.download = name;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(url), 10000);
      dioxus.send('share:downloaded');
    }
  } catch (e) {
    dioxus.send('share:error:' + (e && e.message ? e.message : 'share failed'));
  }
})({PAYLOAD})
"##;
    TEMPLATE.replace("{PAYLOAD}", &share_entry_payload(name, mime, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, name: &str) -> GalleryEntry {
        GalleryEntry {
            meta: ArtifactMeta {
                kind: kind.into(),
                name: name.into(),
                size: 42,
                modified_at_unix_ms: 0,
            },
            preview_url: None,
        }
    }

    #[test]
    fn image_kinds() {
        assert!(entry("image", "a.png").is_image());
        assert!(entry("screenshot", "s.png").is_image());
        assert!(!entry("file", "f.txt").is_image());
    }

    #[test]
    fn share_payload_round_trips_hostile_name() {
        // A filename containing placeholder-looking tokens must survive
        // verbatim: the old sequential three-token replacement corrupted
        // exactly this input.
        let hostile = "evil{PAYLOAD}{B64}{NAME}{MIME}\"; alert(1); \"";
        let payload = share_entry_payload(hostile, "image/png", b"bytes");
        let v: serde_json::Value = serde_json::from_str(&payload).expect("payload is valid JSON");
        assert_eq!(v["name"], hostile);
        assert_eq!(v["mime"], "image/png");
        assert_eq!(v["b64"], "Ynl0ZXM=");
    }

    #[test]
    fn share_js_substitutes_payload_exactly_once() {
        let hostile = "x{PAYLOAD}y";
        let payload = share_entry_payload(hostile, "image/png", b"bytes");
        let js = share_entry_js(hostile, "image/png", b"bytes");
        assert!(
            js.contains(&payload),
            "the complete JSON payload is embedded verbatim"
        );
        // Every surviving "{PAYLOAD}" comes from the payload data itself
        // (the hostile filename); the template's substitution site is gone.
        // A leftover template placeholder would add one more occurrence.
        assert_eq!(
            js.matches("{PAYLOAD}").count(),
            payload.matches("{PAYLOAD}").count(),
            "template placeholder fully substituted"
        );
        assert!(
            !js.contains("})({PAYLOAD})"),
            "the template's substitution site must not survive"
        );
    }
}
