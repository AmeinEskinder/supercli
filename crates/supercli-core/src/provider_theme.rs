//! Provider terminal theme resolution (port of
//! `clients/legacy/native/SupercliNative/Sources/SupercliNative/OpenCodeTheme.swift`).
//!
//! OpenCode and Grok paint their TUI canvas with a provider-chosen background.
//! Supercli mirrors that choice into the terminal frame (titlebar / Ghostty
//! default background) so the chrome matches what the TUI is painting.
//!
//! Resolution order for a session: live canvas sample from `output.bin`
//! (ground truth) → provider config files → built-in theme table → default.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Provider-resolved terminal background, one 0xRRGGBB per appearance.
/// Mirrors `TerminalFrameStyle.Background`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeBackground {
    pub light: Option<u32>,
    pub dark: Option<u32>,
}

impl ThemeBackground {
    pub const fn new(light: Option<u32>, dark: Option<u32>) -> Self {
        Self { light, dark }
    }

    pub fn is_empty(&self) -> bool {
        self.light.is_none() && self.dark.is_none()
    }

    /// Stable key for cache invalidation when provider config changes.
    /// Mirrors `Background.signature`.
    pub fn signature(&self) -> String {
        let light_part = self
            .light
            .map(|v| format!("{v:06X}"))
            .unwrap_or_else(|| "-".to_string());
        let dark_part = self
            .dark
            .map(|v| format!("{v:06X}"))
            .unwrap_or_else(|| "-".to_string());
        format!("{light_part}/{dark_part}")
    }
}

/// Mirrors OpenCode TUI `theme.background`, not `backgroundPanel`.
static OPENCODE_BUILT_IN_BACKGROUNDS: &[(&str, u32, u32)] = &[
    ("opencode", 0xFFFFFF, 0x0A0A0A),
    ("oc-1", 0xFFFFFF, 0x0A0A0A),
    ("aura", 0x0F0F0F, 0x0F0F0F),
    ("ayu", 0x0B0E14, 0x0B0E14),
    ("carbonfox", 0xFFFFFF, 0x161616),
    ("catppuccin", 0xEFF1F5, 0x1E1E2E),
    ("catppuccin-frappe", 0x303446, 0x303446),
    ("catppuccin-macchiato", 0x24273A, 0x24273A),
    ("catppuccin-mocha", 0xEFF1F5, 0x1E1E2E),
    ("cobalt2", 0xFFFFFF, 0x193549),
    ("cursor", 0xFCFCFC, 0x181818),
    ("dracula", 0xF8F8F2, 0x282A36),
    ("everforest", 0xFDF6E3, 0x2D353B),
    ("flexoki", 0xFFFCF0, 0x100F0F),
    ("github", 0xFFFFFF, 0x0D1117),
    ("gruvbox", 0xFBF1C7, 0x282828),
    ("kanagawa", 0xF2E9DE, 0x1F1F28),
    ("material", 0xFAFAFA, 0x263238),
    ("matrix", 0xEEF3EA, 0x0A0E0A),
    ("mercury", 0xFFFFFF, 0x171721),
    ("monokai", 0xFAFAFA, 0x272822),
    ("night-owl", 0x011627, 0x011627),
    ("nightowl", 0x011627, 0x011627),
    ("nord", 0xECEFF4, 0x2E3440),
    ("one-dark", 0xFAFAFA, 0x282C34),
    ("one-dark-pro", 0xFAFAFA, 0x282C34),
    ("onedarkpro", 0xFAFAFA, 0x282C34),
    ("orng", 0xFFFFFF, 0x0A0A0A),
    ("osaka-jade", 0xF6F5DD, 0x111C18),
    ("palenight", 0xFAFAFA, 0x292D3E),
    ("rosepine", 0xFAF4ED, 0x191724),
    ("rose-pine", 0xFAF4ED, 0x191724),
    ("shadesofpurple", 0xF7EBFF, 0x1A102B),
    ("shades-of-purple", 0xF7EBFF, 0x1A102B),
    ("solarized", 0xFDF6E3, 0x002B36),
    ("synthwave84", 0xFAFAFA, 0x262335),
    ("tokyonight", 0xE1E2E7, 0x1A1B26),
    ("tokyo-night", 0xE1E2E7, 0x1A1B26),
    ("vercel", 0xFFFFFF, 0x000000),
    ("vesper", 0xFFFFFF, 0x101010),
    ("zenburn", 0xFFFFEF, 0x3F3F3F),
];

/// Grok paints its canvas with truecolor SGR; these match the current CLI
/// canvas colors (sampled from live `output.bin`, grok 0.2.x).
static GROK_THEME_BACKGROUNDS: &[(&str, u32)] = &[
    ("groknight", 0x141414),
    ("grok-night", 0x141414),
    ("dark", 0x141414),
    ("grokday", 0xFAFAFA),
    ("grok-day", 0xFAFAFA),
    ("light", 0xFAFAFA),
    ("day", 0xFAFAFA),
    ("tokyonight", 0x1A1B26),
    ("tokyo-night", 0x1A1B26),
    ("tokyo", 0x1A1B26),
    ("rosepine", 0x232136),
    ("rose-pine", 0x232136),
    ("rosepine-moon", 0x232136),
    ("rose-pine-moon", 0x232136),
    ("oscura", 0x100D1B),
    ("oscura-midnight", 0x100D1B),
];

/// Normalize a theme name for table lookup: trim, lowercase, `_` → `-`.
pub fn normalize_theme_name(value: &str) -> String {
    value.trim().to_lowercase().replace('_', "-")
}

fn non_empty(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse `#RGB` / `#RRGGBB` into 0xRRGGBB. Mirrors `parseHexColor`.
pub fn parse_hex_color(value: &str) -> Option<u32> {
    let value = value.trim();
    let body = value.strip_prefix('#')?;
    let expanded = if body.len() == 3 {
        body.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        body.to_string()
    };
    if expanded.len() != 6 || !expanded.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(&expanded, 16).ok()
}

/// Strip `//` line and `/* */` block comments from JSONC, respecting strings.
pub fn strip_jsonc_comments(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escaping = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied().unwrap_or('\0');

        if in_line_comment {
            if c == '\n' {
                in_line_comment = false;
                output.push(c);
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if c == '*' && next == '/' {
                in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if in_string {
            output.push(c);
            if escaping {
                escaping = false;
            } else if c == '\\' {
                escaping = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            output.push(c);
            i += 1;
        } else if c == '/' && next == '/' {
            in_line_comment = true;
            i += 2;
        } else if c == '/' && next == '*' {
            in_block_comment = true;
            i += 2;
        } else {
            output.push(c);
            i += 1;
        }
    }
    output
}

/// Drop trailing commas before `}` / `]` (JSONC tolerance), respecting strings.
pub fn strip_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escaping = false;

    while i < chars.len() {
        let c = chars[i];
        if in_string {
            output.push(c);
            if escaping {
                escaping = false;
            } else if c == '\\' {
                escaping = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            output.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '}' || chars[j] == ']') {
                i += 1;
                continue;
            }
        }
        output.push(c);
        i += 1;
    }
    output
}

// ---------------------------------------------------------------------------
// OpenCode theme resolver
// ---------------------------------------------------------------------------

fn opencode_user_config_dir() -> PathBuf {
    if let Some(xdg) = non_empty(std::env::var("XDG_CONFIG_HOME").ok().as_deref()) {
        return PathBuf::from(xdg).join("opencode");
    }
    home_dir().join(".config").join("opencode")
}

fn supercli_opencode_config_dir() -> PathBuf {
    // Prefer the active workspace home when SUPERCLI_HOME is set, matching
    // hook install paths for blank/workspace instances.
    if let Some(home) = non_empty(std::env::var("SUPERCLI_HOME").ok().as_deref()) {
        return PathBuf::from(home).join("hooks").join("opencode");
    }
    home_dir().join(".supercli").join("hooks").join("opencode")
}

fn opencode_state_kv_file() -> PathBuf {
    let state_home = non_empty(std::env::var("XDG_STATE_HOME").ok().as_deref())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local").join("state"));
    state_home.join("opencode").join("kv.json")
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

fn append_if_exists(
    urls: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
    path: PathBuf,
) {
    if seen.contains(&path) || !path.is_file() {
        return;
    }
    seen.insert(path.clone());
    urls.push(path);
}

fn append_json_variants(
    base: &str,
    dir: &Path,
    urls: &mut Vec<PathBuf>,
    seen: &mut std::collections::HashSet<PathBuf>,
) {
    append_if_exists(urls, seen, dir.join(format!("{base}.json")));
    append_if_exists(urls, seen, dir.join(format!("{base}.jsonc")));
}

/// Config files that may name the selected OpenCode theme, in priority order.
fn opencode_config_candidates(working_directory: Option<&str>) -> Vec<PathBuf> {
    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for dir in [opencode_user_config_dir(), supercli_opencode_config_dir()] {
        append_json_variants("opencode", &dir, &mut urls, &mut seen);
        append_json_variants("tui", &dir, &mut urls, &mut seen);
    }
    append_if_exists(&mut urls, &mut seen, opencode_state_kv_file());

    if let Some(env_config) = non_empty(std::env::var("OPENCODE_TUI_CONFIG").ok().as_deref()) {
        append_if_exists(&mut urls, &mut seen, PathBuf::from(env_config));
    }

    for dir in ancestor_directories(working_directory) {
        append_json_variants("opencode", &dir, &mut urls, &mut seen);
        append_json_variants("tui", &dir, &mut urls, &mut seen);
        append_json_variants("opencode", &dir.join(".opencode"), &mut urls, &mut seen);
        append_json_variants("tui", &dir.join(".opencode"), &mut urls, &mut seen);
    }
    urls
}

fn opencode_theme_candidates(
    theme_name: &str,
    normalized: &str,
    working_directory: Option<&str>,
) -> Vec<PathBuf> {
    let mut urls = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut names = vec![theme_name.trim().to_string()];
    if normalized != theme_name.trim() {
        names.push(normalized.to_string());
    }

    for dir in [opencode_user_config_dir(), supercli_opencode_config_dir()] {
        let themes = dir.join("themes");
        for name in &names {
            append_json_variants(name, &themes, &mut urls, &mut seen);
        }
    }
    for dir in ancestor_directories(working_directory) {
        let themes = dir.join(".opencode").join("themes");
        for name in &names {
            append_json_variants(name, &themes, &mut urls, &mut seen);
        }
    }
    urls
}

fn ancestor_directories(working_directory: Option<&str>) -> Vec<PathBuf> {
    let Some(raw) = non_empty(working_directory) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    let mut current = PathBuf::from(&raw);
    loop {
        result.push(current.clone());
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => break,
        }
    }
    result.reverse();
    result
}

/// Minimal JSON value for theme config parsing (objects, strings, arrays).
#[derive(Debug, Clone)]
enum JsonVal {
    Str(String),
    Obj(HashMap<String, JsonVal>),
    Other,
}

fn parse_json_value(text: &str) -> Option<JsonVal> {
    let stripped = strip_trailing_commas(&strip_jsonc_comments(text));
    let bytes = stripped.as_bytes();
    let mut pos = 0;
    parse_value(bytes, &mut pos)
}

fn skip_ws(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && (bytes[*pos] as char).is_whitespace() {
        *pos += 1;
    }
}

fn parse_string(bytes: &[u8], pos: &mut usize) -> Option<String> {
    if bytes.get(*pos) != Some(&b'"') {
        return None;
    }
    *pos += 1;
    let mut out = String::new();
    let mut escaping = false;
    while *pos < bytes.len() {
        let c = bytes[*pos] as char;
        *pos += 1;
        if escaping {
            out.push(c);
            escaping = false;
        } else if c == '\\' {
            escaping = true;
        } else if c == '"' {
            return Some(out);
        } else {
            out.push(c);
        }
    }
    None
}

fn parse_value(bytes: &[u8], pos: &mut usize) -> Option<JsonVal> {
    skip_ws(bytes, pos);
    match bytes.get(*pos) {
        Some(b'"') => parse_string(bytes, pos).map(JsonVal::Str),
        Some(b'{') => {
            *pos += 1;
            let mut map = HashMap::new();
            loop {
                skip_ws(bytes, pos);
                if bytes.get(*pos) == Some(&b'}') {
                    *pos += 1;
                    break;
                }
                let key = parse_string(bytes, pos)?;
                skip_ws(bytes, pos);
                if bytes.get(*pos) != Some(&b':') {
                    return None;
                }
                *pos += 1;
                let val = parse_value(bytes, pos)?;
                map.insert(key, val);
                skip_ws(bytes, pos);
                match bytes.get(*pos) {
                    Some(b',') => *pos += 1,
                    Some(b'}') => continue,
                    _ => return None,
                }
            }
            Some(JsonVal::Obj(map))
        }
        Some(b'[') => {
            // Arrays are not needed for theme resolution; skip over one.
            let mut depth = 0usize;
            let mut in_str = false;
            let mut esc = false;
            while *pos < bytes.len() {
                let c = bytes[*pos] as char;
                *pos += 1;
                if in_str {
                    if esc {
                        esc = false;
                    } else if c == '\\' {
                        esc = true;
                    } else if c == '"' {
                        in_str = false;
                    }
                } else if c == '"' {
                    in_str = true;
                } else if c == '[' {
                    depth += 1;
                } else if c == ']' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            Some(JsonVal::Other)
        }
        _ => {
            // Numbers / literals: consume the token.
            while *pos < bytes.len() && !matches!(bytes[*pos], b',' | b'}' | b']') {
                *pos += 1;
            }
            Some(JsonVal::Other)
        }
    }
}

fn read_json_object(path: &Path) -> Option<HashMap<String, JsonVal>> {
    let text = std::fs::read_to_string(path).ok()?;
    match parse_json_value(&text)? {
        JsonVal::Obj(map) => Some(map),
        _ => None,
    }
}

fn obj_str<'a>(obj: &'a HashMap<String, JsonVal>, key: &str) -> Option<&'a str> {
    match obj.get(key) {
        Some(JsonVal::Str(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        }
        _ => None,
    }
}

fn obj_map<'a>(
    obj: &'a HashMap<String, JsonVal>,
    key: &str,
) -> Option<&'a HashMap<String, JsonVal>> {
    match obj.get(key) {
        Some(JsonVal::Obj(m)) => Some(m),
        _ => None,
    }
}

/// Resolve a hex color value that may be a literal, a `{light,dark,default}`
/// object, or a reference into `defs` (max depth 8).
fn resolve_hex(value: Option<&JsonVal>, defs: &HashMap<String, JsonVal>, depth: u8) -> Option<u32> {
    if depth >= 8 {
        return None;
    }
    match value {
        Some(JsonVal::Obj(map)) => resolve_hex(map.get("dark"), defs, depth + 1)
            .or_else(|| resolve_hex(map.get("light"), defs, depth + 1))
            .or_else(|| resolve_hex(map.get("default"), defs, depth + 1)),
        Some(JsonVal::Str(s)) => {
            let v = s.trim();
            if v.eq_ignore_ascii_case("none") {
                return None;
            }
            if let Some(hex) = parse_hex_color(v) {
                return Some(hex);
            }
            defs.get(v)
                .and_then(|r| resolve_hex(Some(r), defs, depth + 1))
        }
        _ => None,
    }
}

fn theme_background_in_object(obj: &HashMap<String, JsonVal>) -> Option<ThemeBackground> {
    let defs = obj_map(obj, "defs").cloned().unwrap_or_default();

    if let Some(theme) = obj_map(obj, "theme") {
        if let Some(bg) = resolve_hex(theme.get("background"), &defs, 0).map(|hex| {
            // `background` may itself be a {light,dark} object.
            let light = match theme.get("background") {
                Some(JsonVal::Obj(m)) => resolve_hex(m.get("light"), &defs, 0)
                    .or_else(|| resolve_hex(m.get("default"), &defs, 0)),
                _ => None,
            };
            let dark = match theme.get("background") {
                Some(JsonVal::Obj(m)) => resolve_hex(m.get("dark"), &defs, 0)
                    .or_else(|| resolve_hex(m.get("default"), &defs, 0)),
                _ => None,
            };
            if light.is_some() || dark.is_some() {
                ThemeBackground::new(light, dark)
            } else {
                ThemeBackground::new(Some(hex), Some(hex))
            }
        }) {
            if !bg.is_empty() {
                return Some(bg);
            }
        }
    }

    let light = obj_map(obj, "light")
        .and_then(|l| obj_map(l, "palette"))
        .and_then(|p| resolve_hex(p.get("neutral"), &HashMap::new(), 0));
    let dark = obj_map(obj, "dark")
        .and_then(|d| obj_map(d, "palette"))
        .and_then(|p| resolve_hex(p.get("neutral"), &HashMap::new(), 0));
    if light.is_some() || dark.is_some() {
        return Some(ThemeBackground::new(light, dark));
    }
    None
}

fn selected_theme_name(working_directory: Option<&str>) -> Option<String> {
    let mut selected: Option<String> = None;
    for file in opencode_config_candidates(working_directory) {
        let obj = read_json_object(&file)?;
        if let Some(theme) = obj_str(&obj, "theme") {
            selected = Some(theme.to_string());
        } else if let Some(tui) = obj_map(&obj, "tui") {
            if let Some(theme) = obj_str(tui, "theme") {
                selected = Some(theme.to_string());
            }
        }
    }
    selected
}

/// Resolve the OpenCode TUI background for a working directory.
/// Mirrors `OpenCodeThemeResolver.background(workingDirectory:)`.
pub fn opencode_background(working_directory: Option<&str>) -> Option<ThemeBackground> {
    let theme = selected_theme_name(working_directory).unwrap_or_else(|| "opencode".to_string());
    let normalized = normalize_theme_name(&theme);
    if normalized == "system" || normalized == "transparent" {
        return None;
    }

    let mut resolved: Option<ThemeBackground> = None;
    for file in opencode_theme_candidates(&theme, &normalized, working_directory) {
        if let Some(obj) = read_json_object(&file) {
            if let Some(bg) = theme_background_in_object(&obj) {
                if !bg.is_empty() {
                    resolved = Some(bg);
                }
            }
        }
    }
    if let Some(bg) = resolved {
        return Some(bg);
    }

    OPENCODE_BUILT_IN_BACKGROUNDS
        .iter()
        .find(|(name, _, _)| *name == normalized)
        .map(|(_, light, dark)| ThemeBackground::new(Some(*light), Some(*dark)))
}

/// Look up a built-in OpenCode theme background by (normalized) name.
pub fn opencode_built_in_background(name: &str) -> Option<ThemeBackground> {
    let normalized = normalize_theme_name(name);
    OPENCODE_BUILT_IN_BACKGROUNDS
        .iter()
        .find(|(n, _, _)| *n == normalized)
        .map(|(_, light, dark)| ThemeBackground::new(Some(*light), Some(*dark)))
}

// ---------------------------------------------------------------------------
// Grok theme resolver
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct GrokUiConfig {
    theme: Option<String>,
    auto_light_theme: Option<String>,
    auto_dark_theme: Option<String>,
}

fn grok_home_dir() -> PathBuf {
    if let Some(home) = non_empty(std::env::var("GROK_HOME").ok().as_deref()) {
        return PathBuf::from(home);
    }
    home_dir().join(".grok")
}

/// Strip a `#` comment from a TOML line, respecting quotes.
fn strip_toml_comment(line: &str) -> String {
    let mut output = String::new();
    let mut quote: Option<char> = None;
    let mut escaping = false;
    for c in line.chars() {
        if let Some(q) = quote {
            output.push(c);
            if escaping {
                escaping = false;
            } else if q == '"' && c == '\\' {
                escaping = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            output.push(c);
        } else if c == '#' {
            break;
        } else {
            output.push(c);
        }
    }
    output
}

/// Parse a TOML string value (quoted or bare token).
fn parse_toml_string(raw: &str) -> Option<String> {
    let value = raw.trim();
    let first = value.chars().next()?;
    if first == '"' || first == '\'' {
        let chars: Vec<char> = value.chars().collect();
        let mut result = String::new();
        let mut escaping = false;
        for &c in &chars[1..] {
            if escaping {
                result.push(c);
                escaping = false;
            } else if first == '"' && c == '\\' {
                escaping = true;
            } else if c == first {
                return Some(result.trim().to_string());
            } else {
                result.push(c);
            }
        }
        return None;
    }
    value
        .split(|c: char| c.is_whitespace() || c == ',')
        .next()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn read_grok_ui_config() -> GrokUiConfig {
    let path = grok_home_dir().join("config.toml");
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return GrokUiConfig::default(),
    };
    let mut config = GrokUiConfig::default();
    let mut section: Option<String> = None;
    for raw_line in contents.lines() {
        let line = strip_toml_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = Some(line[1..line.len() - 1].trim().to_string());
            continue;
        }
        if section.as_deref() != Some("ui") {
            continue;
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim();
        let value = parse_toml_string(&line[eq + 1..]);
        match key {
            "theme" => config.theme = value,
            "auto_light_theme" => config.auto_light_theme = value,
            "auto_dark_theme" => config.auto_dark_theme = value,
            _ => {}
        }
    }
    config
}

fn grok_theme_color(name: &str) -> Option<u32> {
    let normalized = normalize_theme_name(name);
    GROK_THEME_BACKGROUNDS
        .iter()
        .find(|(n, _)| *n == normalized)
        .map(|(_, c)| *c)
}

fn grok_theme_background_or(name: Option<&str>, fallback: &str) -> u32 {
    name.and_then(grok_theme_color)
        .unwrap_or_else(|| grok_theme_color(fallback).unwrap_or(0x1A1B1D))
}

fn command_has_light_flag(command: &str) -> bool {
    command.split_whitespace().any(|t| t == "--light")
}

/// Resolve the Grok TUI background for a launch command.
/// Mirrors `GrokThemeResolver.background(command:)`.
pub fn grok_background(command: &str) -> Option<ThemeBackground> {
    if command_has_light_flag(command) {
        let color = grok_theme_color("grokday")?;
        return Some(ThemeBackground::new(Some(color), Some(color)));
    }
    let config = read_grok_ui_config();
    let selected = normalize_theme_name(config.theme.as_deref().unwrap_or("auto"));
    if selected == "auto" || selected == "system" {
        return Some(grok_auto_background(&config));
    }
    if let Some(color) = grok_theme_color(&selected) {
        return Some(ThemeBackground::new(Some(color), Some(color)));
    }
    Some(grok_auto_background(&config))
}

fn grok_auto_background(config: &GrokUiConfig) -> ThemeBackground {
    let light = grok_theme_background_or(config.auto_light_theme.as_deref(), "grokday");
    let dark = grok_theme_background_or(config.auto_dark_theme.as_deref(), "groknight");
    ThemeBackground::new(Some(light), Some(dark))
}

/// Resolve the provider background for a launch command.
/// Mirrors `TerminalFrameStyle.providerBackground(command:workingDirectory:)`:
/// detect the provider tool from the command head and resolve its theme
/// background; any other command yields no provider background.
pub fn provider_background(
    command: &str,
    working_directory: Option<&str>,
) -> Option<ThemeBackground> {
    let head = command
        .split([' ', '\t'])
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match head.as_str() {
        "opencode" => opencode_background(working_directory),
        "grok" => grok_background(command),
        _ => None,
    }
}

/// Resolved terminal frame background with pane hex strings.
/// Mirrors the portable parts of `TerminalFrameStyle` produced by
/// `resolved(command:workingDirectory:canvasOverride:)` (the AppKit
/// `NSColor` is not portable; the background + pane `#RRGGBB` values are).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFrameBackground {
    /// The resolved background (`None` = Supercli default, no provider theme).
    pub background: Option<ThemeBackground>,
    /// Pane background hex (`#RRGGBB`) for dark appearance.
    pub pane_dark_hex: Option<String>,
    /// Pane background hex (`#RRGGBB`) for light appearance.
    pub pane_light_hex: Option<String>,
}

/// Resolve the terminal frame background for a launch command, with an
/// optional live canvas override (0xRRGGBB sampled from the TUI's truecolor
/// paint).
///
/// Mirrors `TerminalFrameStyle.resolved(command:workingDirectory:canvasOverride:)`:
/// the canvas override wins over the config-derived provider background
/// (`canvasOverride.map { Background(light: $0, dark: $0) } ?? background`);
/// when neither is present the frame uses Supercli's default (`None`).
pub fn resolve_frame_background(
    command: &str,
    working_directory: Option<&str>,
    canvas_override: Option<u32>,
) -> ResolvedFrameBackground {
    let background = canvas_override
        .map(|v| ThemeBackground::new(Some(v), Some(v)))
        .or_else(|| provider_background(command, working_directory));
    match background {
        None => ResolvedFrameBackground {
            background: None,
            pane_dark_hex: None,
            pane_light_hex: None,
        },
        Some(bg) => ResolvedFrameBackground {
            background: Some(bg),
            // Mirrors Swift: `paneStyle.dark.background = hexString(dark)` etc.
            pane_dark_hex: bg.dark.map(|v| format!("#{v:06X}")),
            pane_light_hex: bg.light.map(|v| format!("#{v:06X}")),
        },
    }
}

// ---------------------------------------------------------------------------
// Provider canvas sampler
// ---------------------------------------------------------------------------

/// Bytes of the output tail to scan.
pub const SAMPLER_SAMPLE_BYTES: usize = 96 * 1024;
/// Minimum hits before we trust a color (avoids flash frames).
const SAMPLER_MIN_HITS: i32 = 40;
/// Top color must beat the runner-up by this ratio (canvas vs chrome).
const SAMPLER_DOMINANCE_RATIO: f64 = 1.6;

/// Scan terminal output bytes for the dominant truecolor background the
/// agent TUI is painting (SGR `48;2;R;G;B`). Returns 0xRRGGBB or None when
/// no color dominates. Mirrors `ProviderCanvasSampler.dominantBackground(in:)`.
pub fn dominant_background_in_data(data: &[u8]) -> Option<u32> {
    let mut counts: HashMap<u32, i32> = HashMap::new();
    let mut i = 0;
    while i < data.len() {
        // ESC [
        if data[i] == 0x1B && data.get(i + 1) == Some(&0x5B) {
            let mut j = i + 2;
            let seq_start = j;
            let mut closed = false;
            while j < data.len() {
                let b = data[j];
                if (0x40..=0x7E).contains(&b) {
                    if b == 0x6D {
                        // 'm' SGR — tally truecolor backgrounds.
                        tally_truecolor_backgrounds(&data[seq_start..j], &mut counts);
                    }
                    i = j + 1;
                    closed = true;
                    break;
                }
                j += 1;
            }
            if closed {
                continue;
            }
            // Unterminated sequence: stop scanning (mirrors `break`).
            break;
        }
        i += 1;
    }

    let (&top_key, &top_count) = counts.iter().max_by_key(|(_, &c)| c)?;
    if top_count < SAMPLER_MIN_HITS {
        return None;
    }
    let second = counts
        .iter()
        .filter(|(&k, _)| k != top_key)
        .map(|(_, &c)| c)
        .max()
        .unwrap_or(0);
    if second > 0 && (top_count as f64) < (second as f64) * SAMPLER_DOMINANCE_RATIO {
        return None;
    }
    Some(top_key)
}

fn tally_truecolor_backgrounds(params: &[u8], counts: &mut HashMap<u32, i32>) {
    // Parse semicolon-separated integers without allocating strings.
    let mut numbers: Vec<i64> = Vec::new();
    let mut current: i64 = 0;
    let mut has_digit = false;
    for &b in params {
        if b == 0x3B {
            // ';'
            numbers.push(if has_digit { current } else { 0 });
            current = 0;
            has_digit = false;
        } else if (0x30..=0x39).contains(&b) {
            current = current * 10 + (b - 0x30) as i64;
            has_digit = true;
            if current > 255_000 {
                return; // garbage / overflow guard
            }
        } else if b == 0x3A {
            // ':' (ISO intermediate) — treat as separator
            numbers.push(if has_digit { current } else { 0 });
            current = 0;
            has_digit = false;
        } else {
            // Unknown char in SGR — abort this sequence.
            return;
        }
    }
    if has_digit {
        numbers.push(current);
    }

    let mut index = 0;
    while index < numbers.len() {
        // 48;2;R;G;B (':' already flattened to entries above)
        if numbers[index] == 48 && index + 4 < numbers.len() && numbers[index + 1] == 2 {
            let r = numbers[index + 2];
            let g = numbers[index + 3];
            let b = numbers[index + 4];
            if (0..=255).contains(&r) && (0..=255).contains(&g) && (0..=255).contains(&b) {
                let hex = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                *counts.entry(hex).or_insert(0) += 1;
            }
            index += 5;
            continue;
        }
        index += 1;
    }
}

// ---------------------------------------------------------------------------
// Provider theme watch paths
// ---------------------------------------------------------------------------

/// Whether a filesystem path change should trigger a theme re-resolve.
/// Mirrors `ProviderThemeWatchPaths.isRelevantChange(_:)`.
pub fn is_relevant_theme_change(path: &str) -> bool {
    let p = Path::new(path);
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let parent = p
        .parent()
        .and_then(|par| par.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let full = path.to_lowercase();

    // Grok UI theme lives only in config.toml under GROK_HOME.
    if name == "config.toml" && (full.contains("/.grok") || full.contains("/grok-home/")) {
        return true;
    }

    // OpenCode config + theme JSON/JSONC (including project-local).
    if name == "opencode.json"
        || name == "opencode.jsonc"
        || name == "tui.json"
        || name == "tui.jsonc"
        || name == "kv.json"
    {
        return true;
    }
    if parent == "themes" && (name.ends_with(".json") || name.ends_with(".jsonc")) {
        return true;
    }
    if (full.contains("/.config/opencode/")
        || full.contains("/.opencode/")
        || full.contains("/hooks/opencode/"))
        && (name.ends_with(".json") || name.ends_with(".jsonc"))
    {
        return true;
    }
    false
}

/// True when this command head is themed by a provider config Supercli
/// mirrors into the terminal frame (OpenCode / Grok).
/// Mirrors `TerminalFrameStyle.usesProviderTheme(command:)`.
pub fn uses_provider_theme(command: &str) -> bool {
    let head = command
        .split([' ', '\t'])
        .next()
        .unwrap_or("")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_lowercase();
    matches!(head.as_str(), "opencode" | "grok")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};

    /// Serializes the tests that mutate the process-wide GROK_HOME env var.
    fn grok_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn write_temp_config(dir: &Path, rel: &str, contents: &str) -> PathBuf {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        path
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "supercli-provider-theme-test-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn aura_uses_opencode_canvas_background() {
        let bg = opencode_built_in_background("aura").unwrap();
        assert_eq!(bg.light, Some(0x0F0F0F));
        assert_eq!(bg.dark, Some(0x0F0F0F));
    }

    #[test]
    fn default_opencode_background_matches_built_in_theme() {
        let bg = opencode_built_in_background("opencode").unwrap();
        assert_eq!(bg.light, Some(0xFFFFFF));
        assert_eq!(bg.dark, Some(0x0A0A0A));
    }

    #[test]
    fn single_mode_themes_do_not_invent_light_backgrounds() {
        let bg = opencode_built_in_background("catppuccin-frappe").unwrap();
        assert_eq!(bg.light, Some(0x303446));
        assert_eq!(bg.dark, Some(0x303446));
    }

    #[test]
    fn theme_name_normalization_matches_swift() {
        assert_eq!(normalize_theme_name("  TokyoNight "), "tokyonight");
        assert_eq!(normalize_theme_name("one_dark_pro"), "one-dark-pro");
        assert_eq!(normalize_theme_name("ROSE_PINE"), "rose-pine");
    }

    #[test]
    fn hex_color_parsing_matches_swift() {
        assert_eq!(parse_hex_color("#1A1B1D"), Some(0x1A1B1D));
        assert_eq!(parse_hex_color("#fff"), Some(0xFFFFFF));
        assert_eq!(parse_hex_color("#ABC"), Some(0xAABBCC));
        assert_eq!(parse_hex_color("1A1B1D"), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#GGGGGG"), None);
    }

    #[test]
    fn background_signature_matches_swift_format() {
        let bg = ThemeBackground::new(Some(0x0A0A0A), None);
        assert_eq!(bg.signature(), "0A0A0A/-");
        assert!(ThemeBackground::new(None, None).is_empty());
        assert!(!ThemeBackground::new(Some(1), Some(2)).is_empty());
    }

    #[test]
    fn jsonc_comment_and_trailing_comma_stripping() {
        let input = "{\n// pick aura\n\"theme\": \"aura\", /* block */\n\"tui\": {\"theme\": \"aura\",},\n}";
        let stripped = strip_trailing_commas(&strip_jsonc_comments(input));
        assert!(!stripped.contains("// pick aura"));
        assert!(!stripped.contains("/* block */"));
        assert!(!stripped.contains(",}"));
        // String contents survive.
        let quoted = "{\"a\": \"x // not a comment\"}";
        assert!(strip_jsonc_comments(quoted).contains("x // not a comment"));
    }

    #[test]
    fn opencode_config_theme_is_resolved_from_working_dir() {
        let dir = temp_dir("opencode-config");
        write_temp_config(&dir, ".opencode/tui.json", "{\"theme\":\"aura\"}");
        let bg = opencode_background(Some(dir.to_str().unwrap())).unwrap();
        assert_eq!(bg.light, Some(0x0F0F0F));
        assert_eq!(bg.dark, Some(0x0F0F0F));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opencode_custom_theme_file_background_wins() {
        let dir = temp_dir("opencode-custom");
        write_temp_config(&dir, ".opencode/tui.json", "{\"theme\":\"mytheme\"}");
        write_temp_config(
            &dir,
            ".opencode/themes/mytheme.json",
            "{\"theme\":{\"background\":\"#123456\"}}",
        );
        let bg = opencode_background(Some(dir.to_str().unwrap())).unwrap();
        assert_eq!(bg.light, Some(0x123456));
        assert_eq!(bg.dark, Some(0x123456));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opencode_system_theme_resolves_to_none() {
        let dir = temp_dir("opencode-system");
        write_temp_config(&dir, ".opencode/tui.json", "{\"theme\":\"system\"}");
        assert!(opencode_background(Some(dir.to_str().unwrap())).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grok_auto_config_resolves_day_and_night() {
        let _guard = grok_env_lock().lock().unwrap();
        let dir = temp_dir("grok-auto");
        // SAFETY: serialized by grok_env_lock; restored after.
        unsafe { std::env::set_var("GROK_HOME", dir.to_str().unwrap()) };
        write_temp_config(&dir, "config.toml", "[ui]\ntheme = \"auto\"\n");
        let bg = grok_background("grok --always-approve").unwrap();
        assert_eq!(bg.light, Some(0xFAFAFA));
        // Live Grok 0.2.x paints truecolor canvas rgb(20,20,20).
        assert_eq!(bg.dark, Some(0x141414));
        unsafe { std::env::remove_var("GROK_HOME") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grok_fixed_dark_theme_uses_canvas_background() {
        let _guard = grok_env_lock().lock().unwrap();
        let dir = temp_dir("grok-fixed");
        unsafe { std::env::set_var("GROK_HOME", dir.to_str().unwrap()) };
        write_temp_config(&dir, "config.toml", "[ui]\ntheme = \"groknight\"\n");
        let bg = grok_background("grok --always-approve").unwrap();
        assert_eq!(bg.light, Some(0x141414));
        assert_eq!(bg.dark, Some(0x141414));
        unsafe { std::env::remove_var("GROK_HOME") };
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grok_light_flag_forces_day_background() {
        let bg = grok_background("grok --light").unwrap();
        assert_eq!(bg.light, Some(0xFAFAFA));
        assert_eq!(bg.dark, Some(0xFAFAFA));
    }

    #[test]
    fn toml_comment_stripping_respects_quotes() {
        assert_eq!(
            strip_toml_comment("theme = \"a#b\" # real"),
            "theme = \"a#b\" "
        );
        assert_eq!(
            parse_toml_string("\" groknight \""),
            Some("groknight".to_string())
        );
        assert_eq!(parse_toml_string("auto"), Some("auto".to_string()));
    }

    #[test]
    fn watch_paths_recognize_config_files() {
        assert!(is_relevant_theme_change("/Users/me/.grok/config.toml"));
        assert!(is_relevant_theme_change(
            "/Users/me/.config/opencode/opencode.json"
        ));
        assert!(is_relevant_theme_change(
            "/Users/me/proj/.opencode/themes/aura.json"
        ));
        assert!(is_relevant_theme_change(
            "/Users/me/.local/state/opencode/kv.json"
        ));
        assert!(!is_relevant_theme_change(
            "/Users/me/.grok/sessions/foo/updates.jsonl"
        ));
        assert!(!is_relevant_theme_change("/Users/me/proj/src/main.swift"));
    }

    #[test]
    fn provider_theme_commands_are_detected() {
        assert!(uses_provider_theme("grok --always-approve"));
        assert!(uses_provider_theme("opencode"));
        assert!(uses_provider_theme("/usr/local/bin/opencode --help"));
        assert!(!uses_provider_theme(
            "claude --dangerously-skip-permissions"
        ));
        assert!(!uses_provider_theme(""));
    }

    #[test]
    fn canvas_sampler_picks_dominant_truecolor_background() {
        // Simulate Grok-style SGR: many cells with canvas #141414, a few
        // chrome cells with #111111 — sampler should lock onto the canvas.
        let mut payload = Vec::new();
        let canvas = b"\x1b[48;2;20;20;20m ";
        let chrome = b"\x1b[48;2;17;17;17m ";
        for _ in 0..80 {
            payload.extend_from_slice(canvas);
        }
        for _ in 0..10 {
            payload.extend_from_slice(chrome);
        }
        // Compound form Grok often uses: 38;2;…;48;2;R;G;B
        let compound = b"\x1b[38;2;200;200;200;48;2;20;20;20mX";
        for _ in 0..40 {
            payload.extend_from_slice(compound);
        }
        assert_eq!(dominant_background_in_data(&payload), Some(0x141414));
    }

    #[test]
    fn canvas_sampler_rejects_ambiguous_backgrounds() {
        let mut payload = Vec::new();
        let a = b"\x1b[48;2;20;20;20m ";
        let b = b"\x1b[48;2;40;40;40m ";
        for _ in 0..50 {
            payload.extend_from_slice(a);
            payload.extend_from_slice(b);
        }
        assert_eq!(dominant_background_in_data(&payload), None);
    }

    #[test]
    fn canvas_sampler_rejects_too_few_hits() {
        let mut payload = Vec::new();
        for _ in 0..10 {
            payload.extend_from_slice(b"\x1b[48;2;20;20;20m ");
        }
        assert_eq!(dominant_background_in_data(&payload), None);
    }

    #[test]
    fn fixed_background_override_wins_over_config() {
        // Mirrors OpenCodeThemeTests.testFixedBackgroundOverrideWinsOverConfig:
        // a fixed canvas override (e.g. live-sampled from the TUI's truecolor
        // paint) takes precedence over the config-derived background.
        // Calls the real `resolve_frame_background` production resolver
        // (port of `TerminalFrameStyle.resolved(command:workingDirectory:canvasOverride:)`).
        let _guard = grok_env_lock().lock().unwrap();
        let dir = temp_dir("grok-fixed-override");
        // SAFETY: serialized by grok_env_lock; restored after.
        unsafe { std::env::set_var("GROK_HOME", dir.to_str().unwrap()) };
        write_temp_config(&dir, "config.toml", "[ui]\ntheme = \"groknight\"\n");
        let command = "grok --always-approve";

        // Baseline: no override → config-derived groknight background
        // (fixed "groknight" theme → 0x141414 for both appearances).
        let baseline = resolve_frame_background(command, None, None);
        assert_eq!(
            baseline.background,
            Some(ThemeBackground::new(Some(0x141414), Some(0x141414)))
        );

        // With override: the fixed canvas color wins for both appearances.
        let resolved = resolve_frame_background(command, None, Some(0x0A0A12));
        assert_eq!(
            resolved.background,
            Some(ThemeBackground::new(Some(0x0A0A12), Some(0x0A0A12)))
        );
        // Pane style hex form matches Swift's `style.paneStyle.dark.background`.
        assert_eq!(resolved.pane_dark_hex.as_deref(), Some("#0A0A12"));
        assert_eq!(resolved.pane_light_hex.as_deref(), Some("#0A0A12"));

        // No provider command and no override → Supercli default (None).
        let plain = resolve_frame_background("bash", None, None);
        assert_eq!(plain.background, None);
        assert_eq!(plain.pane_dark_hex, None);

        unsafe { std::env::remove_var("GROK_HOME") };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
