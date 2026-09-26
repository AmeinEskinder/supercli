//! Client-renderable icon types, ported from SupercliShared's
//! `ChromeIcons.swift` and `ToolIcons.swift`.
//!
//! Two families:
//! - [`ChromeIcon`]: the 8 fixed chrome glyphs (sidebar, folders, bell...).
//! - [`ToolIcon`]: resolved per-runtime/per-app icons. Provider artwork comes
//!   from `runtimes/<slug>/assets/icon.svg`; this module owns only the generic
//!   agent/editor/app/terminal fallbacks so adding a runtime never requires a
//!   new enum case here.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::runtime_catalog::{RuntimeCatalog, RuntimeDescriptor, RuntimeKind};

/// Fixed chrome glyphs for the client shell.
///
/// Mirrors `SupercliChromeIcon` in `ChromeIcons.swift`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChromeIcon {
    FolderClosed,
    FolderOpen,
    Branch,
    Pin,
    Plus,
    SidebarToggle,
    Bell,
    Gallery,
}

impl ChromeIcon {
    /// All variants, in declaration order.
    pub const ALL: [ChromeIcon; 8] = [
        ChromeIcon::FolderClosed,
        ChromeIcon::FolderOpen,
        ChromeIcon::Branch,
        ChromeIcon::Pin,
        ChromeIcon::Plus,
        ChromeIcon::SidebarToggle,
        ChromeIcon::Bell,
        ChromeIcon::Gallery,
    ];

    /// Asset name (the Swift raw value).
    pub fn asset_name(&self) -> &'static str {
        match self {
            ChromeIcon::FolderClosed => "folderClosed",
            ChromeIcon::FolderOpen => "folderOpen",
            ChromeIcon::Branch => "branch",
            ChromeIcon::Pin => "pin",
            ChromeIcon::Plus => "plus",
            ChromeIcon::SidebarToggle => "sidebarToggle",
            ChromeIcon::Bell => "bell",
            ChromeIcon::Gallery => "gallery",
        }
    }

    /// Rotation applied when rendering (the branch glyph is drawn rotated).
    pub fn rotation_degrees(&self) -> f64 {
        match self {
            ChromeIcon::Branch => 90.0,
            _ => 0.0,
        }
    }

    /// Inline SVG source for the glyph.
    pub fn svg_source(&self) -> &'static str {
        match self {
            ChromeIcon::FolderClosed => r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="folderClosedGlass" x1="36" y1="38" x2="222" y2="218" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#FFFFFF" stop-opacity="0.98"/><stop offset="0.45" stop-color="#FFFFFF" stop-opacity="0.80"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0.52"/></linearGradient></defs><path d="M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z" fill="url(#folderClosedGlass)"></path><path d="M216,72H131.31L104,44.69A15.88,15.88,0,0,0,92.69,40H40A16,16,0,0,0,24,56V200.62A15.41,15.41,0,0,0,39.39,216h177.5A15.13,15.13,0,0,0,232,200.89V88A16,16,0,0,0,216,72ZM40,56H92.69l16,16H40Z" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>"##,
            ChromeIcon::FolderOpen => r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="folderOpenGlass" x1="34" y1="46" x2="224" y2="216" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#FFFFFF" stop-opacity="0.98"/><stop offset="0.45" stop-color="#FFFFFF" stop-opacity="0.80"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0.52"/></linearGradient></defs><path d="M245,110.64A16,16,0,0,0,232,104H216V88a16,16,0,0,0-16-16H130.67L102.94,51.2a16.14,16.14,0,0,0-9.6-3.2H40A16,16,0,0,0,24,64V208h0a8,8,0,0,0,8,8H211.1a8,8,0,0,0,7.59-5.47l28.49-85.47A16.05,16.05,0,0,0,245,110.64ZM93.34,64,123.2,86.4A8,8,0,0,0,128,88h72v16H69.77a16,16,0,0,0-15.18,10.94L40,158.7V64Z" fill="url(#folderOpenGlass)"></path><path d="M245,110.64A16,16,0,0,0,232,104H216V88a16,16,0,0,0-16-16H130.67L102.94,51.2a16.14,16.14,0,0,0-9.6-3.2H40A16,16,0,0,0,24,64V208h0a8,8,0,0,0,8,8H211.1a8,8,0,0,0,7.59-5.47l28.49-85.47A16.05,16.05,0,0,0,245,110.64ZM93.34,64,123.2,86.4A8,8,0,0,0,128,88h72v16H69.77a16,16,0,0,0-15.18,10.94L40,158.7V64Z" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>"##,
            ChromeIcon::Branch => r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 3h5v5"/><path d="M8 3H3v5"/><path d="M12 22v-8.3a4 4 0 0 0-1.172-2.872L3 3"/><path d="m15 9 6-6"/></svg>"##,
            ChromeIcon::Pin => r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M233.91,82.79,173.22,22.1a14,14,0,0,0-19.81,0L98.93,76.77c-9.52-3.25-34-8.34-59.71,12.41A14,14,0,0,0,38.1,110l49.71,49.71-44.05,44a6,6,0,1,0,8.48,8.48l44.05-44.05L146,217.89a14,14,0,0,0,9.9,4.11q.49,0,1,0a14,14,0,0,0,10.19-5.54c19.72-26.21,17.15-47.23,12.46-59.3l54.37-54.55A14,14,0,0,0,233.91,82.79ZM225.42,94.1h0l-57.27,57.46a6,6,0,0,0-1.11,6.92c9.94,19.88-1.71,40.32-9.54,50.72a2,2,0,0,1-3,.2L46.58,101.51a2,2,0,0,1,.18-3c12.5-10.09,24.5-12.76,33.7-12.76a42.13,42.13,0,0,1,17.25,3.41A6,6,0,0,0,104.64,88L161.9,30.59a2,2,0,0,1,2.83,0l60.69,60.68A2,2,0,0,1,225.42,94.1Z"></path></svg>"##,
            ChromeIcon::Plus => r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M222,128a6,6,0,0,1-6,6H134v82a6,6,0,0,1-12,0V134H40a6,6,0,0,1,0-12h82V40a6,6,0,0,1,12,0v82h82A6,6,0,0,1,222,128Z"></path></svg>"##,
            ChromeIcon::SidebarToggle => glass_svg(
                "M216,40H40A16,16,0,0,0,24,56V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A16,16,0,0,0,216,40Zm0,160H88V56H216V200Z",
                "sidebarToggleGlass",
            ),
            ChromeIcon::Bell => glass_svg(
                "M221.8,175.94C216.25,166.38,208,139.33,208,104a80,80,0,1,0-160,0c0,35.34-8.26,62.38-13.81,71.94A16,16,0,0,0,48,200H88.81a40,40,0,0,0,78.38,0H208a16,16,0,0,0,13.8-24.06ZM128,216a24,24,0,0,1-22.62-16h45.24A24,24,0,0,1,128,216Z",
                "bellGlass",
            ),
            ChromeIcon::Gallery => glass_svg(
                "M208,32H80A16,16,0,0,0,64,48V64H48A16,16,0,0,0,32,80V208a16,16,0,0,0,16,16H176a16,16,0,0,0,16-16V192h16a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32ZM80,48H208v69.38l-16.7-16.7a16,16,0,0,0-22.62,0L93.37,176H80Zm96,160H48V80H64v96a16,16,0,0,0,16,16h96ZM104,88a16,16,0,1,1,16,16A16,16,0,0,1,104,88Z",
                "galleryGlass",
            ),
        }
    }
}

fn glass_svg(_path: &str, gradient_id: &str) -> &'static str {
    // The three glass icons share one template; return the pre-rendered
    // variant matching the caller's path. This keeps the SVG a 'static str
    // without runtime formatting.
    match gradient_id {
        "sidebarToggleGlass" => r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="sidebarToggleGlass" x1="34" y1="40" x2="224" y2="218" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#FFFFFF" stop-opacity="0.98"/><stop offset="0.45" stop-color="#FFFFFF" stop-opacity="0.80"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0.52"/></linearGradient></defs><path d="M216,40H40A16,16,0,0,0,24,56V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A16,16,0,0,0,216,40Zm0,160H88V56H216V200Z" fill="url(#sidebarToggleGlass)"></path><path d="M216,40H40A16,16,0,0,0,24,56V200a16,16,0,0,0,16,16H216a16,16,0,0,0,16-16V56A16,16,0,0,0,216,40Zm0,160H88V56H216V200Z" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>"##,
        "bellGlass" => r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="bellGlass" x1="34" y1="40" x2="224" y2="218" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#FFFFFF" stop-opacity="0.98"/><stop offset="0.45" stop-color="#FFFFFF" stop-opacity="0.80"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0.52"/></linearGradient></defs><path d="M221.8,175.94C216.25,166.38,208,139.33,208,104a80,80,0,1,0-160,0c0,35.34-8.26,62.38-13.81,71.94A16,16,0,0,0,48,200H88.81a40,40,0,0,0,78.38,0H208a16,16,0,0,0,13.8-24.06ZM128,216a24,24,0,0,1-22.62-16h45.24A24,24,0,0,1,128,216Z" fill="url(#bellGlass)"></path><path d="M221.8,175.94C216.25,166.38,208,139.33,208,104a80,80,0,1,0-160,0c0,35.34-8.26,62.38-13.81,71.94A16,16,0,0,0,48,200H88.81a40,40,0,0,0,78.38,0H208a16,16,0,0,0,13.8-24.06ZM128,216a24,24,0,0,1-22.62-16h45.24A24,24,0,0,1,128,216Z" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>"##,
        _ => r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" fill="none" viewBox="0 0 256 256"><defs><linearGradient id="galleryGlass" x1="34" y1="40" x2="224" y2="218" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#FFFFFF" stop-opacity="0.98"/><stop offset="0.45" stop-color="#FFFFFF" stop-opacity="0.80"/><stop offset="1" stop-color="#FFFFFF" stop-opacity="0.52"/></linearGradient></defs><path d="M208,32H80A16,16,0,0,0,64,48V64H48A16,16,0,0,0,32,80V208a16,16,0,0,0,16,16H176a16,16,0,0,0,16-16V192h16a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32ZM80,48H208v69.38l-16.7-16.7a16,16,0,0,0-22.62,0L93.37,176H80Zm96,160H48V80H64v96a16,16,0,0,0,16,16h96ZM104,88a16,16,0,1,1,16,16A16,16,0,0,1,104,88Z" fill="url(#galleryGlass)"></path><path d="M208,32H80A16,16,0,0,0,64,48V64H48A16,16,0,0,0,32,80V208a16,16,0,0,0,16,16H176a16,16,0,0,0,16-16V192h16a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32ZM80,48H208v69.38l-16.7-16.7a16,16,0,0,0-22.62,0L93.37,176H80Zm96,160H48V80H64v96a16,16,0,0,0,16,16h96ZM104,88a16,16,0,1,1,16,16A16,16,0,0,1,104,88Z" fill="none" stroke="#FFFFFF" stroke-opacity="0.30" stroke-width="8" stroke-linejoin="round"></path></svg>"##,
    }
}

/// A resolved, client-renderable runtime icon.
///
/// Mirrors `SupercliToolIcon` in `ToolIcons.swift`. Provider artwork comes
/// from the runtime's `assets/icon.svg`; this type owns only the generic
/// agent/editor/app/terminal fallbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolIcon {
    pub id: String,
    pub key: String,
    pub label: String,
    pub kind: RuntimeKind,
    pub svg_source: String,
    pub is_template: bool,
    pub fallback_system_name: String,
    pub uses_runtime_asset: bool,
}

impl ToolIcon {
    /// All icons: one per runtime descriptor plus the terminal fallback.
    pub fn all_cases(catalog: &RuntimeCatalog, icon_svg: impl Fn(&RuntimeDescriptor) -> Option<String>) -> Vec<ToolIcon> {
        let mut icons: Vec<ToolIcon> = catalog
            .descriptors()
            .iter()
            .map(|runtime| Self::for_runtime(runtime, icon_svg(runtime)))
            .collect();
        icons.push(ToolIcon::terminal());
        icons
    }

    /// Runtime art first, then an installed Supercli App (by Host-stamped App
    /// id, else by the command's leading binary), else the terminal mark.
    pub fn resolving(
        catalog: &RuntimeCatalog,
        icon_svg: impl Fn(&RuntimeDescriptor) -> Option<String>,
        app_id: Option<&str>,
        provider_id: Option<&str>,
        command: &str,
    ) -> ToolIcon {
        let runtime = provider_id
            .and_then(|id| catalog.by_id(id))
            .or_else(|| catalog.by_command_alias(command));
        if let Some(runtime) = runtime {
            return Self::for_runtime(runtime, icon_svg(runtime));
        }
        AppIconCatalog::global()
            .icon_for_app_or_command(app_id, command)
            .unwrap_or_else(ToolIcon::terminal)
    }

    /// An installed Supercli App's mark from the Host's catalog.
    pub fn for_app(id: &str, name: &str, icon_svg: Option<&str>) -> ToolIcon {
        let authored = icon_svg.map(str::trim).filter(|s| !s.is_empty());
        ToolIcon {
            id: format!("app:{id}"),
            key: id.to_string(),
            label: name.to_string(),
            kind: RuntimeKind::App,
            svg_source: authored
                .map(str::to_string)
                .unwrap_or_else(|| generic_svg(RuntimeKind::App)),
            is_template: true,
            fallback_system_name: fallback_system_name(RuntimeKind::App).to_string(),
            uses_runtime_asset: authored.is_some(),
        }
    }

    pub fn for_runtime(descriptor: &RuntimeDescriptor, icon_svg: Option<String>) -> ToolIcon {
        let authored = icon_svg
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let has_authored = authored.is_some();
        ToolIcon {
            id: descriptor.id.clone(),
            key: descriptor.display.icon.clone(),
            label: descriptor.label.clone(),
            kind: descriptor.display.kind,
            svg_source: authored
                .map(str::to_string)
                .unwrap_or_else(|| generic_svg(descriptor.display.kind)),
            // The generic fallback is always monochrome regardless of a
            // malformed descriptor's rendering hint.
            is_template: has_authored.then_some(descriptor.display.icon_template).unwrap_or(true),
            fallback_system_name: fallback_system_name(descriptor.display.kind).to_string(),
            uses_runtime_asset: has_authored,
        }
    }

    pub fn terminal() -> ToolIcon {
        ToolIcon {
            id: "terminal".to_string(),
            key: "terminal".to_string(),
            label: "Terminal".to_string(),
            kind: RuntimeKind::Terminal,
            svg_source: generic_svg(RuntimeKind::Terminal),
            is_template: true,
            fallback_system_name: fallback_system_name(RuntimeKind::Terminal).to_string(),
            uses_runtime_asset: false,
        }
    }
}

/// Kind-owned generic marks, so a future markdown-editor or Supercli App
/// CLI does not inherit the agent sparkle when it ships without art.
fn generic_svg(kind: RuntimeKind) -> String {
    match kind {
        RuntimeKind::Agent => r##"<svg width="16" height="16" viewBox="0 0 24 24" fill="#FFFFFF" xmlns="http://www.w3.org/2000/svg"><path d="M12 2l1.64 5.36L19 9l-5.36 1.64L12 16l-1.64-5.36L5 9l5.36-1.64L12 2Z"/><path d="M19 15l.82 2.18L22 18l-2.18.82L19 21l-.82-2.18L16 18l2.18-.82L19 15Z"/></svg>"##.to_string(),
        RuntimeKind::App => r##"<svg width="16" height="16" viewBox="0 0 256 256" fill="#FFFFFF" xmlns="http://www.w3.org/2000/svg"><path d="M208,40H48A16,16,0,0,0,32,56V200a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V56A16,16,0,0,0,208,40Zm0,16V88H48V56ZM48,200V104H208v96Z"/></svg>"##.to_string(),
        RuntimeKind::Editor => r##"<svg width="16" height="16" viewBox="0 0 256 256" fill="#FFFFFF" xmlns="http://www.w3.org/2000/svg"><path d="M208,24H72A16,16,0,0,0,56,40V216a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V40A16,16,0,0,0,208,24Zm0,192H72V40H208ZM96,80h80a8,8,0,0,1,0,16H96a8,8,0,0,1,0-16Zm0,40h80a8,8,0,0,1,0,16H96a8,8,0,0,1,0-16Zm0,40h48a8,8,0,0,1,0,16H96a8,8,0,0,1,0-16Z"/></svg>"##.to_string(),
        RuntimeKind::Terminal => r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="#FFFFFF" viewBox="0 0 256 256"><path d="M116,132.48l-72,64a6,6,0,0,1-8-9L103,128,36,68.49a6,6,0,0,1,8-9l72,64a6,6,0,0,1,0,9ZM216,186H120a6,6,0,0,0,0,12h96a6,6,0,0,0,0-12Z"></path></svg>"##.to_string(),
    }
}

fn fallback_system_name(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Agent => "sparkles",
        RuntimeKind::App => "square.stack",
        RuntimeKind::Editor => "doc.plaintext",
        RuntimeKind::Terminal => "terminal",
    }
}

/// The Host's App catalog as icons, fed from every bootstrap snapshot's
/// `availableApps`.
///
/// Mirrors `SupercliAppIconCatalog` in `ToolIcons.swift`: lock-protected
/// rather than actor-bound because icon resolvers read from any thread.
pub struct AppIconCatalog {
    by_app_id: Mutex<HashMap<String, ToolIcon>>,
    by_binary: Mutex<HashMap<String, ToolIcon>>,
}

impl AppIconCatalog {
    fn global() -> &'static AppIconCatalog {
        static GLOBAL: std::sync::OnceLock<AppIconCatalog> = std::sync::OnceLock::new();
        GLOBAL.get_or_init(|| AppIconCatalog {
            by_app_id: Mutex::new(HashMap::new()),
            by_binary: Mutex::new(HashMap::new()),
        })
    }

    /// Replace the catalog contents from a bootstrap snapshot.
    pub fn update(apps: &[AppIconSource]) {
        let mut next_by_id = HashMap::new();
        let mut next_by_binary = HashMap::new();
        for app in apps {
            let icon = ToolIcon::for_app(&app.id, &app.name, app.icon_svg.as_deref());
            next_by_id.insert(app.id.to_lowercase(), icon.clone());
            // Binary key uses the command's leading word (like the lookup),
            // so a Host launch by absolute path and the bare binary both match.
            if let Some(binary) = leading_binary(&app.command) {
                next_by_binary.insert(binary, icon);
            }
        }
        let global = Self::global();
        *global.by_app_id.lock().expect("app icon catalog lock") = next_by_id;
        *global.by_binary.lock().expect("app icon catalog lock") = next_by_binary;
    }

    pub fn icon_for_app_id(app_id: &str) -> Option<ToolIcon> {
        if app_id.is_empty() {
            return None;
        }
        Self::global()
            .by_app_id
            .lock()
            .expect("app icon catalog lock")
            .get(&app_id.to_lowercase())
            .cloned()
    }

    /// Whether the command's leading word is an installed plugin (Supercli
    /// App) on the current Host.
    pub fn is_plugin_command(command: &str) -> bool {
        Self::icon_for_command(command).is_some()
    }

    /// Match the command's leading word by binary name, so both the bare
    /// launch-list command and a Host launch by absolute path resolve.
    pub fn icon_for_command(command: &str) -> Option<ToolIcon> {
        let binary = leading_binary(command)?;
        Self::global()
            .by_binary
            .lock()
            .expect("app icon catalog lock")
            .get(&binary)
            .cloned()
    }

    fn icon_for_app_or_command(&self, app_id: Option<&str>, command: &str) -> Option<ToolIcon> {
        if let Some(id) = app_id.filter(|id| !id.is_empty()) {
            if let Some(icon) = self
                .by_app_id
                .lock()
                .expect("app icon catalog lock")
                .get(&id.to_lowercase())
                .cloned()
            {
                return Some(icon);
            }
        }
        Self::icon_for_command(command)
    }

    #[cfg(test)]
    fn clear_for_tests() {
        let global = Self::global();
        global.by_app_id.lock().expect("lock").clear();
        global.by_binary.lock().expect("lock").clear();
    }
}

/// Extract the lowercase binary name from a command's leading word,
/// stripping quotes and directory prefixes.
fn leading_binary(command: &str) -> Option<String> {
    let token = command.split_whitespace().next()?;
    let unquoted = token.trim_matches(|c| c == '\'' || c == '"');
    let binary = unquoted.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase();
    if binary.is_empty() {
        None
    } else {
        Some(binary)
    }
}

/// One installed Supercli App as seen by the icon catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppIconSource {
    pub id: String,
    pub name: String,
    pub command: String,
    pub icon_svg: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_catalog::builtin_runtime_catalog;

    #[test]
    fn chrome_icons_cover_all_variants_with_svg() {
        assert_eq!(ChromeIcon::ALL.len(), 8);
        for icon in ChromeIcon::ALL {
            assert!(!icon.asset_name().is_empty());
            assert!(icon.svg_source().contains("<svg"));
        }
        assert_eq!(ChromeIcon::Branch.rotation_degrees(), 90.0);
        assert_eq!(ChromeIcon::Bell.rotation_degrees(), 0.0);
        assert_eq!(ChromeIcon::FolderClosed.asset_name(), "folderClosed");
    }

    #[test]
    fn shared_icon_resolution_prefers_host_provider_identity() {
        let catalog = builtin_runtime_catalog();
        let icon = ToolIcon::resolving(catalog, |_| None, None, Some("com.anthropic.claude-code"), "codex");
        assert_eq!(icon.id, "com.anthropic.claude-code");
        assert_eq!(icon.key, "claude");
        // No authored SVG supplied -> generic fallback, not a runtime asset.
        assert!(!icon.uses_runtime_asset);

        let unknown = ToolIcon::resolving(catalog, |_| None, None, None, "unknown-agent-xyz");
        assert_eq!(unknown, ToolIcon::terminal());
    }

    #[test]
    fn generic_fallback_follows_runtime_kind_not_agent_sparkle() {
        let catalog = builtin_runtime_catalog();
        let editor_descriptor = catalog
            .descriptors()
            .iter()
            .find(|d| d.display.kind == RuntimeKind::Editor);
        // If no editor runtime ships, synthesize the kind check from the
        // generic SVG table directly.
        let editor_svg = generic_svg(RuntimeKind::Editor);
        let agent_svg = generic_svg(RuntimeKind::Agent);
        assert_ne!(editor_svg, agent_svg);
        assert_eq!(fallback_system_name(RuntimeKind::Editor), "doc.plaintext");
        if let Some(descriptor) = editor_descriptor {
            let icon = ToolIcon::for_runtime(descriptor, None);
            assert_eq!(icon.kind, RuntimeKind::Editor);
            assert!(!icon.uses_runtime_asset);
            assert!(icon.is_template);
        }
    }

    #[test]
    fn every_runtime_descriptor_resolves_an_icon() {
        let catalog = builtin_runtime_catalog();
        for descriptor in catalog.descriptors() {
            let icon = ToolIcon::for_runtime(descriptor, None);
            assert_eq!(icon.id, descriptor.id, "{}", descriptor.slug);
            assert_eq!(icon.key, descriptor.display.icon, "{}", descriptor.slug);
            assert!(!icon.svg_source.is_empty(), "{}", descriptor.slug);
            // Without authored art every icon is the generic template fallback.
            assert!(!icon.uses_runtime_asset, "{}", descriptor.slug);
            assert!(icon.is_template, "{}", descriptor.slug);
        }
    }

    #[test]
    fn app_icon_catalog_resolves_by_id_and_binary() {
        AppIconCatalog::clear_for_tests();
        AppIconCatalog::update(&[AppIconSource {
            id: "my-app".to_string(),
            name: "My App".to_string(),
            command: "/usr/local/bin/myapp --serve".to_string(),
            icon_svg: Some("<svg>custom</svg>".to_string()),
        }]);

        let by_id = AppIconCatalog::icon_for_app_id("MY-APP").expect("by id");
        assert_eq!(by_id.id, "app:my-app");
        assert!(by_id.uses_runtime_asset);

        let by_command = AppIconCatalog::icon_for_command("myapp --flag").expect("by binary");
        assert_eq!(by_command.id, "app:my-app");
        assert!(AppIconCatalog::is_plugin_command("/usr/local/bin/myapp"));
        assert!(!AppIconCatalog::is_plugin_command("ls -la"));

        AppIconCatalog::clear_for_tests();
    }

    #[test]
    fn resolving_prefers_app_catalog_over_terminal() {
        AppIconCatalog::clear_for_tests();
        AppIconCatalog::update(&[AppIconSource {
            id: "notes".to_string(),
            name: "Notes".to_string(),
            command: "notes".to_string(),
            icon_svg: None,
        }]);
        let catalog = builtin_runtime_catalog();
        let icon = ToolIcon::resolving(catalog, |_| None, Some("notes"), None, "notes");
        assert_eq!(icon.id, "app:notes");
        AppIconCatalog::clear_for_tests();
    }
}
