//! Shared stylesheet for the `unpeel-ui` components.
//!
//! The Dioxus launchers have no CSS pipeline; each launcher's root
//! includes `style { "{APP_CSS}" }` once. Component class names are owned
//! here, next to the components that use them.

/// All component styles, injected once by each launcher root.
pub const APP_CSS: &str = r#"
/* ---------- annotation editors (annotation.rs) ---------- */
.annotation-editor { display: flex; flex-direction: column; height: 100%; background: #111; color: #eee; }
.annotation-toolbar { display: flex; gap: 8px; padding: 8px 12px; align-items: center; background: #1b1b1b; }
.annotation-toolbar button { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 6px 12px; font-size: 14px; }
.annotation-toolbar button:active { background: #3a3a3a; }
.annotation-done { background: #0a6cff !important; border-color: #0a6cff !important; color: #fff !important; }
.annotation-spacer { flex: 1; }
.annotation-canvas { position: relative; flex: 1; overflow: hidden; touch-action: none; user-select: none; -webkit-user-select: none; }
.annotation-image { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; pointer-events: none; }
.annotation-overlay { position: absolute; overflow: hidden; }
.annotation-palette { display: flex; gap: 10px; padding: 10px 12px; background: #1b1b1b; justify-content: center; }
.swatch { width: 32px; height: 32px; border-radius: 50%; border: 2px solid transparent; }
.swatch.selected { border-color: #fff; }
.crop-rect { position: absolute; border: 2px solid #0a6cff; box-sizing: border-box; touch-action: none; }
.crop-mask { position: absolute; background: rgba(0,0,0,0.5); pointer-events: none; }
.crop-handle { position: absolute; width: 28px; height: 28px; margin: -14px 0 0 -14px; border-radius: 50%; background: #0a6cff; border: 2px solid #fff; touch-action: none; }

/* ---------- browser gallery (gallery.rs) ---------- */
.gallery-panel { display: flex; flex-direction: column; height: 100%; background: #111; color: #eee; }
.gallery-toolbar { display: flex; gap: 8px; padding: 8px 12px; align-items: center; background: #1b1b1b; }
.gallery-toolbar button { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 6px 12px; font-size: 14px; }
.gallery-toolbar button.danger { background: #a00; border-color: #a00; color: #fff; }
.gallery-title { font-weight: 600; font-size: 15px; }
.gallery-loading, .gallery-empty { padding: 32px; text-align: center; color: #888; }
.gallery-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: 10px; padding: 12px; overflow-y: auto; }
.gallery-thumb { position: relative; background: #222; border: 1px solid #333; border-radius: 8px; padding: 0; overflow: hidden; aspect-ratio: 1; }
.gallery-thumb img { width: 100%; height: 100%; object-fit: cover; display: block; }
.gallery-thumb-placeholder { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; color: #666; font-size: 12px; }
.gallery-thumb-name { position: absolute; left: 0; right: 0; bottom: 0; background: rgba(0,0,0,0.65); color: #ddd; font-size: 11px; padding: 3px 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: left; }
.gallery-thumb-delete { position: absolute; top: 4px; right: 4px; width: 24px; height: 24px; border-radius: 50%; background: rgba(0,0,0,0.6); color: #fff; font-size: 16px; line-height: 24px; text-align: center; cursor: pointer; }
.gallery-detail { display: flex; flex-direction: column; height: 100%; background: #111; color: #eee; }
.gallery-image-wrap { flex: 1; display: flex; align-items: center; justify-content: center; overflow: hidden; background: #000; }
.gallery-image { max-width: 100%; max-height: 100%; object-fit: contain; }
.gallery-actions { display: flex; gap: 10px; padding: 10px 12px; justify-content: center; background: #1b1b1b; }
.gallery-actions button { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 8px 16px; font-size: 14px; }
.gallery-meta { padding: 6px 12px; color: #888; font-size: 12px; text-align: center; }
.gallery-editor { flex: 1; display: flex; flex-direction: column; min-height: 0; }
.gallery-status { padding: 8px 12px; color: #9cf; font-size: 13px; text-align: center; background: #111; }
.gallery-status.error { color: #f88; }
.gallery-open { margin: 8px 12px 0; background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 8px 16px; font-size: 14px; }
.find-open { margin: 8px 0 0; background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 8px 16px; font-size: 14px; }

/* ---------- QR scanner (qr.rs) ---------- */
.qr-scanner { display: flex; flex-direction: column; align-items: center; background: #000; color: #eee; }
.qr-preview { width: 100%; aspect-ratio: 1; max-height: 60vh; background: #111; overflow: hidden; }
.qr-preview video { width: 100%; height: 100%; object-fit: cover; }
.qr-status { padding: 16px; text-align: center; font-size: 15px; color: #ccc; }
.qr-denied { color: #f66; }
.qr-reason { margin-top: 8px; font-size: 13px; color: #999; }
.qr-hint { margin-top: 8px; font-size: 13px; color: #888; }
.qr-open { margin: 8px 12px; background: #1f6feb; color: #fff; border: none; border-radius: 8px; padding: 10px; font-size: 15px; }
.sheet { position: fixed; inset: 0; background: rgba(0,0,0,0.85); display: flex; flex-direction: column; z-index: 50; }
.sheet-header { display: flex; justify-content: space-between; align-items: center; padding: 12px 16px; color: #eee; font-size: 16px; background: #1b1b1b; }
.sheet-header button { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 6px 12px; font-size: 14px; }

/* ---------- session / project organize + archive sheets (organize.rs) ---------- */
.sheet-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.55); z-index: 60; display: flex; align-items: flex-end; justify-content: center; }
.sheet.organize-sheet, .sheet.archive-sheet { position: relative; inset: auto; z-index: auto; width: 100%; max-width: 560px; max-height: 85vh; background: #1b1b1b; border-radius: 14px 14px 0 0; display: flex; flex-direction: column; }
.sheet-title { color: #eee; font-size: 16px; font-weight: 600; }
.sheet-close { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 6px 12px; font-size: 14px; }
.sheet-body { padding: 12px 16px 20px; overflow-y: auto; color: #ddd; }
.field-label { display: block; font-size: 12px; color: #999; margin: 10px 0 4px; text-transform: uppercase; letter-spacing: 0.04em; }
.text-field { width: 100%; background: #111; color: #eee; border: 1px solid #444; border-radius: 8px; padding: 10px; font-size: 15px; }
.toggle-row { display: flex; align-items: center; justify-content: space-between; padding: 10px 0; border-bottom: 1px solid #2a2a2a; font-size: 15px; color: #ddd; }
.toggle-row input[type="checkbox"] { width: 20px; height: 20px; }
.primary-button { width: 100%; margin-top: 14px; background: #0a6cff; color: #fff; border: none; border-radius: 8px; padding: 12px; font-size: 15px; font-weight: 600; }
.action-list { margin-top: 18px; display: flex; flex-direction: column; gap: 8px; }
.action-button { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 8px; padding: 12px; font-size: 15px; text-align: left; }
.action-button.destructive { color: #ff7a7a; border-color: #5a2a2a; }
.archive-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 10px 0; border-bottom: 1px solid #2a2a2a; }
.archive-title { color: #ddd; font-size: 14px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.archive-row button { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 6px 10px; font-size: 13px; }
.color-swatches { display: flex; gap: 8px; padding: 8px 0; }
.color-swatches button { width: 32px; height: 32px; border-radius: 50%; border: 2px solid transparent; }
.color-swatches button.selected { border-color: #fff; }
.loading-state, .empty-state { color: #888; font-size: 14px; text-align: center; padding: 24px 0; }

/* ---------- Push notifications (push.rs) ---------- */
.push-row { display: flex; align-items: center; justify-content: space-between; padding: 8px 12px; color: #aaa; font-size: 13px; }
.push-retry { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 6px 12px; font-size: 13px; }
.push-warning { padding: 8px 12px; background: #3a2a00; color: #f5c518; font-size: 13px; text-align: center; }

/* ---------- dictation (dictation.rs) ---------- */
.dictation-wrap { display: inline-flex; align-items: center; gap: 8px; position: relative; }
.dictation-mic { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 50%; width: 40px; height: 40px; font-size: 18px; }
.dictation-mic.recording { background: #a00; border-color: #a00; animation: dictation-pulse 1.2s infinite; }
@keyframes dictation-pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.55; } }
.dictation-pill { position: absolute; bottom: 48px; left: 50%; transform: translateX(-50%); display: flex; align-items: center; gap: 8px; background: #1b1b1b; border: 1px solid #444; border-radius: 12px; padding: 8px 12px; max-width: 80vw; z-index: 30; }
.dictation-text { color: #eee; font-size: 14px; max-width: 50vw; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dictation-hint { color: #888; font-size: 13px; font-style: italic; }
.dictation-error { color: #f88; font-size: 13px; }
.dictation-refining { color: #9cf; font-size: 13px; }
.dictation-pill button { background: #2a2a2a; color: #eee; border: 1px solid #444; border-radius: 6px; padding: 6px 10px; font-size: 13px; }
.dictation-pill button.dictation-paste { background: #0a6cff; border-color: #0a6cff; color: #fff; }

/* ---------- preset drawer (presets.rs) ---------- */
.sheet-handle { width: 42px; height: 4px; border-radius: 2px; background: rgba(255,255,255,0.26); margin: 10px auto 12px; }
.preset-drawer { position: relative; inset: auto; z-index: auto; width: 100%; max-width: 420px; max-height: 80vh; background: #1b1c22; border-radius: 28px 28px 0 0; display: flex; flex-direction: column; border: 1px solid rgba(255,255,255,0.08); border-bottom: none; box-shadow: 0 -12px 28px rgba(0,0,0,0.42); margin: 0 9px; touch-action: pan-y; }
.preset-drawer-header { padding: 0 18px 10px; }
.preset-drawer-titles { display: flex; flex-direction: column; gap: 2px; }
.preset-drawer-title { color: #eee; font-size: 17px; font-weight: 600; }
.preset-drawer-project { color: #999; font-size: 12px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.preset-drawer-list { display: flex; flex-direction: column; gap: 7px; padding: 0 12px 16px; overflow-y: auto; }
.preset-row { display: flex; align-items: center; gap: 11px; width: 100%; min-height: 52px; padding: 9px 10px; background: rgba(255,255,255,0.06); border: none; border-radius: 14px; color: #eee; text-align: left; cursor: pointer; }
.preset-row.launching { opacity: 0.72; cursor: default; }
.preset-icon { flex: none; width: 28px; height: 28px; border-radius: 9px; display: flex; align-items: center; justify-content: center; font-size: 13px; font-weight: 700; }
.preset-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.preset-title { font-size: 14px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.preset-command { font-size: 11px; font-family: monospace; color: #999; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.preset-go { flex: none; color: #999; font-size: 12px; font-weight: 600; }
.new-session { background: #0a6cff; color: #fff; border: none; border-radius: 8px; padding: 10px 16px; font-size: 15px; font-weight: 600; margin: 8px 12px 0; width: calc(100% - 24px); }

/* ---------- app lock overlay (app_lock.rs) ---------- */
.app-lock-overlay { position: fixed; inset: 0; z-index: 200; background: #0b0b0e; display: flex; align-items: center; justify-content: center; }
.app-lock-card { display: flex; flex-direction: column; align-items: center; gap: 18px; padding: 32px; text-align: center; }
.app-lock-icon { color: #5eead4; }
.app-lock-title { color: #fff; font-size: 17px; font-weight: 600; }
.app-lock-error { color: rgba(255,255,255,0.55); font-size: 13px; max-width: 280px; }
.app-lock-unlock { background: #0a6cff; color: #fff; border: none; border-radius: 10px; padding: 12px 18px; font-size: 15px; font-weight: 600; margin-top: 6px; }
.app-lock-unlock:disabled { opacity: 0.6; }
.security-row { display: flex; align-items: center; gap: 10px; padding: 10px 12px; color: #ddd; font-size: 14px; }
.security-row .security-text { flex: 1; display: flex; flex-direction: column; gap: 2px; }
.security-row .security-sub { font-size: 12px; color: #999; }
.security-toggle { width: 48px; height: 28px; border-radius: 14px; border: 1px solid #555; background: #2a2a2a; position: relative; flex: none; }
.security-toggle[aria-checked="true"] { background: #0a6cff; border-color: #0a6cff; }
.security-toggle:disabled { opacity: 0.4; }
.security-toggle::after { content: ""; position: absolute; top: 2px; left: 2px; width: 22px; height: 22px; border-radius: 50%; background: #fff; transition: left 0.15s; }
.security-toggle[aria-checked="true"]::after { left: 22px; }

/* ---------- terminal find bar (find.rs) ---------- */
.terminal-wrap { position: relative; }
.find-bar { position: absolute; top: 8px; right: 8px; z-index: 30; display: flex; align-items: center; gap: 6px; padding: 6px 8px 6px 10px; border-radius: 8px; background: rgba(28,28,32,0.94); border: 1px solid rgba(255,255,255,0.12); box-shadow: 0 4px 16px rgba(0,0,0,0.4); }
.find-field { width: 170px; background: transparent; border: none; outline: none; color: #fff; font-size: 13px; }
.find-field::placeholder { color: #888; }
.find-count { min-width: 44px; text-align: right; font-size: 12px; color: #999; font-variant-numeric: tabular-nums; }
.find-btn { background: transparent; border: none; color: #ccc; font-size: 13px; padding: 4px 6px; border-radius: 4px; }
.find-btn:hover { background: rgba(255,255,255,0.1); color: #fff; }
.find-match { background-color: rgba(255,213,79,0.45); border-radius: 2px; }
.find-match-current { background-color: rgba(255,167,38,0.85); border-radius: 2px; }

/* ---------- toasts (toast.rs) ---------- */
.toast-overlay { position: fixed; top: 0; right: 0; z-index: 150; display: flex; justify-content: flex-end; pointer-events: none; }
.toast-capsule { display: flex; align-items: center; gap: 8px; margin: 46px 14px 0 0; padding: 10px 16px; border-radius: 999px; background: rgba(40,40,46,0.92); border: 1px solid rgba(255,255,255,0.12); box-shadow: 0 6px 16px rgba(0,0,0,0.3); pointer-events: auto; cursor: pointer; max-width: 340px; }
.toast-icon { font-size: 13px; }
.toast-text { font-size: 13px; font-weight: 500; color: #fff; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
"#;
