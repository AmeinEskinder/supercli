//! Preset drawer: the {t("presets.new_session")} bottom sheet.
//!
//! Port of Swift's `PresetDrawerOverlay` / `PresetDrawerRow`
//! (`SupercliIOSRootView.swift`). The Host sends presets in the same order the
//! desktop "+" menu shows them; the drawer keeps that order and filters to
//! enabled presets only. Launching sends `{projectID, presetID}` — the
//! `RemoteCreateSessionRequest` shape — and the launching row shows a
//! spinner while disabled, exactly like Swift.

use crate::i18n::t;
use dioxus::prelude::*;
use supercli_client::dto::PresetSummary;

/// Row title: the CLI type as the title (so the command isn't repeated on
/// both lines). A custom preset with its own label keeps that label
/// instead. Mirrors Swift's `PresetDrawerRow.titleText`.
pub fn preset_display_title(preset: &PresetSummary) -> String {
    let label = preset.label.trim().to_string();
    if !label.is_empty() && label != preset.command {
        return label;
    }
    preset.cli_id.clone().unwrap_or(label)
}

/// Presets the drawer may offer: enabled only, snapshot order preserved.
/// Swift's `supportsIOSSessionAPI` is unconditionally true (the Host
/// already filters), so `enabled` is the only gate.
pub fn launchable_presets(presets: &[PresetSummary]) -> Vec<&PresetSummary> {
    presets.iter().filter(|p| p.enabled).collect()
}

/// Initials for the preset's icon tile: the CLI id's first letter, or a
/// generic mark when the Host didn't name a CLI.
pub fn preset_icon_initial(preset: &PresetSummary) -> String {
    preset
        .cli_id
        .as_deref()
        .and_then(|id| id.chars().next())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| ">_".to_string())
}

/// {t("presets.new_session")} bottom sheet. `presets` must already be filtered to the
/// launchable set for the drawer's project; `project_name` is `None` when
/// the launcher has no project context (the flat session list), in which
/// case the header shows just {t("presets.new_session")}.
#[component]
pub fn PresetDrawer(
    project_name: Option<String>,
    presets: Vec<PresetSummary>,
    launching_id: Option<String>,
    on_launch: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    // Swipe-down-to-dismiss on the header, mirroring Swift's dismissDrag:
    // track the touch in screen space (unaffected by the sheet's own
    // offset) and dismiss past 90 px, else snap back.
    let mut drag_y = use_signal(|| 0.0f64);
    let mut touch_start = use_signal(|| None::<f64>);

    let on_touch_start = move |e: TouchEvent| {
        if let Some(touch) = e.touches().first() {
            let pt = touch.page_coordinates();
            touch_start.set(Some(pt.y));
        }
    };
    let on_touch_move = move |e: TouchEvent| {
        if let (Some(start), Some(touch)) = (*touch_start.read(), e.touches().first()) {
            let dy = touch.page_coordinates().y - start;
            drag_y.set(dy.max(0.0));
        }
    };
    let on_touch_end = move |_| {
        let dy = *drag_y.read();
        touch_start.set(None);
        if dy > 90.0 {
            on_close.call(());
        } else {
            drag_y.set(0.0);
        }
    };

    let sheet_style = format!("transform: translateY({}px);", *drag_y.read() as i64);

    rsx! {
        div { class: "sheet-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "preset-drawer",
                style: "{sheet_style}",
                onclick: move |e| e.stop_propagation(),
                div {
                    class: "preset-drawer-header",
                    ontouchstart: on_touch_start,
                    ontouchmove: on_touch_move,
                    ontouchend: on_touch_end,
                    div { class: "sheet-handle" }
                    div { class: "preset-drawer-titles",
                        div { class: "preset-drawer-title", {t("presets.new_session")} }
                        if let Some(name) = project_name {
                            div { class: "preset-drawer-project", "{name}" }
                        }
                    }
                    button {
                        class: "sheet-close",
                        aria_label: {t("presets.close_presets")},
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
                div { class: "preset-drawer-list",
                    for preset in presets {
                        {
                            let id = preset.id.clone();
                            let launching = launching_id.as_deref() == Some(id.as_str());
                            let title = preset_display_title(&preset);
                            let initial = preset_icon_initial(&preset);
                            let command = preset.command.clone();
                            let tint = preset
                                .tint_color_hex
                                .map(|h| format!("#{:06x}", h & 0xFF_FFFF))
                                .unwrap_or_else(|| "#8e8e93".to_string());
                            let launch_id = id.clone();
                            rsx! {
                                button {
                                    class: if launching { "preset-row launching" } else { "preset-row" },
                                    disabled: launching,
                                    onclick: move |_| on_launch.call(launch_id.clone()),
                                    span {
                                        class: "preset-icon",
                                        style: "background-color: {tint}22; color: {tint};",
                                        if launching { "…" } else { "{initial}" }
                                    }
                                    span { class: "preset-text",
                                        span { class: "preset-title", "{title}" }
                                        span { class: "preset-command", "{command}" }
                                    }
                                    span { class: "preset-go", "↗" }
                                }
                            }
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

    fn preset(id: &str, label: &str, command: &str) -> PresetSummary {
        PresetSummary {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            plugin_id: None,
            project_id: None,
            cli_id: None,
            enabled: true,
            quick_launch: false,
            is_default: false,
            tint_color_hex: None,
        }
    }

    #[test]
    fn display_title_prefers_custom_label() {
        let p = preset("1", "My agent", "my-agent --x");
        assert_eq!(preset_display_title(&p), "My agent");
    }

    #[test]
    fn display_title_falls_back_to_cli_id_when_label_is_command() {
        let mut p = preset("1", "claude", "claude");
        p.cli_id = Some("claude-code".to_string());
        assert_eq!(preset_display_title(&p), "claude-code");
    }

    #[test]
    fn display_title_empty_label_uses_cli_id() {
        let mut p = preset("1", "", "codex");
        p.cli_id = Some("codex-cli".to_string());
        assert_eq!(preset_display_title(&p), "codex-cli");
    }

    #[test]
    fn display_title_no_cli_id_keeps_label() {
        let p = preset("1", "", "some-cmd");
        assert_eq!(preset_display_title(&p), "");
    }

    #[test]
    fn launchable_filters_disabled_keeps_order() {
        let a = preset("a", "A", "a");
        let mut b = preset("b", "B", "b");
        let c = preset("c", "C", "c");
        b.enabled = false;
        let all = [a, b, c];
        let out: Vec<&str> = launchable_presets(&all)
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(out, vec!["a", "c"]);
    }

    #[test]
    fn icon_initial_uppercases_cli_id() {
        let mut p = preset("1", "x", "x");
        p.cli_id = Some("claude-code".to_string());
        assert_eq!(preset_icon_initial(&p), "C");
    }

    #[test]
    fn icon_initial_fallback_without_cli_id() {
        let p = preset("1", "x", "x");
        assert_eq!(preset_icon_initial(&p), ">_");
    }
}
