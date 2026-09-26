//! Structured UI trees for agent perception.
//!
//! [`DeviceBackend::describe_ui`] returns a raw string: `uiautomator dump`
//! XML on Android, baguette a11y JSON on iOS. Agents act on elements, not
//! pixels, so this module parses either format into a [`UiNode`] tree with
//! device-point bounds, labels, and clickability. Pure std: the XML side is
//! a small hand-rolled attribute scanner (no XML dependency), the JSON side
//! reuses the crate's minimal parser.
//!
//! Compiled only with the `device` cargo feature.

use super::json::{parse_json, Json};
use super::{DeviceBackend, DeviceError, DeviceId};

/// Rectangle in device points (baguette's convention; uiautomator pixels
/// are reported as-is — callers scale by the device density if needed).
#[derive(Clone, Debug, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    /// Center point, for tap targeting.
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }
}

/// One accessibility element.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNode {
    /// Resource id (Android) or element id (iOS); may be absent.
    pub id: Option<String>,
    /// Class/type, e.g. `android.widget.Button` or `button`.
    pub class: Option<String>,
    /// Visible text of the element.
    pub text: String,
    /// Accessibility label / content-desc.
    pub label: String,
    /// Bounds in device points; None when the platform omits them.
    pub bounds: Option<UiRect>,
    /// Whether the element accepts tap.
    pub clickable: bool,
    /// Whether the element is currently enabled.
    pub enabled: bool,
    pub children: Vec<UiNode>,
}

impl UiNode {
    /// Depth-first search for the first node whose text or label contains
    /// `needle` (case-insensitive).
    pub fn find_by_text(&self, needle: &str) -> Option<&UiNode> {
        let n = needle.to_lowercase();
        if self.text.to_lowercase().contains(&n) || self.label.to_lowercase().contains(&n) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find_by_text(needle))
    }

    /// All clickable leaves, depth-first. What an agent usually wants.
    pub fn clickable_leaves(&self) -> Vec<&UiNode> {
        let mut out = Vec::new();
        self.collect_clickable(&mut out);
        out
    }

    fn collect_clickable<'a>(&'a self, out: &mut Vec<&'a UiNode>) {
        if self.clickable {
            out.push(self);
        }
        for c in &self.children {
            c.collect_clickable(out);
        }
    }

    /// Serialize one node (and children) as compact JSON for MCP/agent
    /// consumption. Hand-rolled: this crate has no serde dependency.
    pub fn to_json(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        parts.push(match &self.id {
            Some(v) => format!("\"id\":\"{}\"", json_escape(v)),
            None => "\"id\":null".to_string(),
        });
        parts.push(match &self.class {
            Some(v) => format!("\"class\":\"{}\"", json_escape(v)),
            None => "\"class\":null".to_string(),
        });
        parts.push(format!("\"text\":\"{}\"", json_escape(&self.text)));
        parts.push(format!("\"label\":\"{}\"", json_escape(&self.label)));
        parts.push(match &self.bounds {
            Some(b) => format!(
                "\"bounds\":{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
                trim_f(b.x),
                trim_f(b.y),
                trim_f(b.width),
                trim_f(b.height)
            ),
            None => "\"bounds\":null".to_string(),
        });
        parts.push(format!("\"clickable\":{}", self.clickable));
        parts.push(format!("\"enabled\":{}", self.enabled));
        let children: Vec<String> = self.children.iter().map(|c| c.to_json()).collect();
        parts.push(format!("\"children\":[{}]", children.join(",")));
        format!("{{{}}}", parts.join(","))
    }
}

fn trim_f(v: f32) -> String {
    // Avoid `100.0` noise; keep full precision otherwise.
    if v == v.trunc() && v.abs() < 1e9 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// JSON string escaping (subset sufficient for UI text).
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Why a describe-ui payload could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiParseError {
    Xml(String),
    Json(String),
}

impl std::fmt::Display for UiParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiParseError::Xml(m) => write!(f, "uiautomator XML parse error: {m}"),
            UiParseError::Json(m) => write!(f, "baguette JSON parse error: {m}"),
        }
    }
}

impl std::error::Error for UiParseError {}

// ---------------------------------------------------------------------------
// uiautomator XML
// ---------------------------------------------------------------------------

/// Unescape the five predefined XML entities.
fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Scan `name="value"` pairs inside a tag. Values are XML-unescaped.
/// Malformed pairs are skipped rather than fatal: uiautomator output is
/// machine-generated and we prefer a partial tree over no tree.
fn scan_attrs(tag: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let b = tag.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // Skip to the next attribute name start.
        while i < b.len() && !(b[i].is_ascii_alphabetic() || b[i] == b'_' || b[i] == b'-') {
            i += 1;
        }
        let ns = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') {
            i += 1;
        }
        if ns == i {
            break;
        }
        let name = &tag[ns..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || b[i] != b'=' {
            continue;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() || (b[i] != b'"' && b[i] != b'\'') {
            continue;
        }
        let q = b[i];
        i += 1;
        let vs = i;
        while i < b.len() && b[i] != q {
            i += 1;
        }
        let value = tag.get(vs..i).unwrap_or("");
        if i < b.len() {
            i += 1; // closing quote
        }
        attrs.push((name.to_string(), xml_unescape(value)));
    }
    attrs
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> &'a str {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

/// `[100,200][300,250]` → rect. uiautomator reports pixels; kept as f32 so
/// callers can scale by density into device points.
fn parse_bounds(s: &str) -> Option<UiRect> {
    let nums: Vec<f32> = s
        .split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    if nums.len() == 4 {
        let (x1, y1, x2, y2) = (nums[0], nums[1], nums[2], nums[3]);
        Some(UiRect {
            x: x1,
            y: y1,
            width: (x2 - x1).max(0.0),
            height: (y2 - y1).max(0.0),
        })
    } else {
        None
    }
}

fn node_from_attrs(attrs: &[(String, String)]) -> UiNode {
    let rid = attr(attrs, "resource-id");
    let cls = attr(attrs, "class");
    UiNode {
        id: if rid.is_empty() {
            None
        } else {
            Some(rid.to_string())
        },
        class: if cls.is_empty() {
            None
        } else {
            Some(cls.to_string())
        },
        text: attr(attrs, "text").to_string(),
        label: attr(attrs, "content-desc").to_string(),
        bounds: {
            let b = attr(attrs, "bounds");
            if b.is_empty() {
                None
            } else {
                parse_bounds(b)
            }
        },
        clickable: attr(attrs, "clickable") == "true",
        enabled: attr(attrs, "enabled") != "false",
        children: Vec::new(),
    }
}

/// Parse `uiautomator dump` XML into top-level nodes. Only `<node>` elements
/// become [`UiNode`]s; `<hierarchy>` and the XML prolog are structural.
pub fn parse_uiautomator_xml(xml: &str) -> Result<Vec<UiNode>, UiParseError> {
    let mut roots: Vec<UiNode> = Vec::new();
    // Stack of open nodes; each entry is the node being built.
    let mut stack: Vec<UiNode> = Vec::new();
    let mut pos = 0;
    let b = xml.as_bytes();
    let mut saw_node = false;

    while pos < b.len() {
        let lt = match xml[pos..].find('<') {
            Some(i) => pos + i,
            None => break,
        };
        let gt = match xml[lt..].find('>') {
            Some(i) => lt + i,
            None => return Err(UiParseError::Xml(format!("unterminated tag at byte {lt}"))),
        };
        let tag = &xml[lt + 1..gt]; // inside <...>
        pos = gt + 1;

        if tag.starts_with('?') || tag.starts_with('!') {
            continue; // prolog / comments / doctype
        }
        if let Some(close) = tag.strip_prefix('/') {
            let name = close.trim();
            if name == "node" {
                if let Some(node) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => roots.push(node),
                    }
                }
            }
            continue;
        }
        let self_close = tag.ends_with('/');
        let body = if self_close {
            tag[..tag.len() - 1].trim_end()
        } else {
            tag
        };
        let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
        let name = &body[..name_end];
        if name != "node" {
            continue; // <hierarchy> etc.
        }
        saw_node = true;
        let node = node_from_attrs(&scan_attrs(&body[name_end..]));
        if self_close {
            match stack.last_mut() {
                Some(parent) => parent.children.push(node),
                None => roots.push(node),
            }
        } else {
            stack.push(node);
        }
    }

    // Tolerate a truncated tail: flush whatever is open.
    while let Some(node) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => roots.push(node),
        }
    }

    if !saw_node {
        return Err(UiParseError::Xml("no <node> elements found".to_string()));
    }
    Ok(roots)
}

// ---------------------------------------------------------------------------
// baguette a11y JSON
// ---------------------------------------------------------------------------

fn json_str(v: &Json, key: &str) -> String {
    v.get(key).and_then(Json::as_str).unwrap_or("").to_string()
}

fn node_from_baguette(v: &Json) -> UiNode {
    let bounds = v.get("bounds").map(|b| {
        let x = b.get("x").and_then(num).unwrap_or(0.0);
        let y = b.get("y").and_then(num).unwrap_or(0.0);
        let w = b.get("width").and_then(num).unwrap_or(0.0);
        let h = b.get("height").and_then(num).unwrap_or(0.0);
        UiRect {
            x,
            y,
            width: w.max(0.0),
            height: h.max(0.0),
        }
    });
    // baguette marks taptargets via "actions": ["tap", ...].
    let clickable = v
        .get("actions")
        .and_then(Json::as_arr)
        .map(|a| a.iter().any(|x| x.as_str() == Some("tap")))
        .unwrap_or(false);
    let children = v
        .get("children")
        .and_then(Json::as_arr)
        .map(|a| a.iter().map(node_from_baguette).collect())
        .unwrap_or_default();
    let id = v.get("id").and_then(Json::as_str).map(str::to_string);
    let class = v.get("type").and_then(Json::as_str).map(str::to_string);
    UiNode {
        id,
        class,
        text: json_str(v, "text"),
        label: json_str(v, "label"),
        bounds,
        clickable,
        enabled: v
            .get("enabled")
            .and_then(|j| match j {
                Json::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(true),
        children,
    }
}

fn num(v: &Json) -> Option<f32> {
    match v {
        Json::Num(n) => Some(*n as f32),
        _ => None,
    }
}

/// Parse baguette `describe-ui --json` output. Accepts either
/// `{"elements": [...]}` or a bare `[...]` array.
pub fn parse_baguette_a11y(raw: &str) -> Result<Vec<UiNode>, UiParseError> {
    let v = parse_json(raw).map_err(UiParseError::Json)?;
    let arr = v
        .get("elements")
        .and_then(Json::as_arr)
        .or_else(|| v.as_arr())
        .ok_or_else(|| UiParseError::Json("expected {\"elements\": [...]} or [...]".to_string()))?;
    Ok(arr.iter().map(node_from_baguette).collect())
}

/// Parse either format: leading `<` (after whitespace) → uiautomator XML,
/// otherwise baguette JSON.
pub fn parse_describe_ui(raw: &str) -> Result<Vec<UiNode>, UiParseError> {
    if raw.trim_start().starts_with('<') {
        parse_uiautomator_xml(raw)
    } else {
        parse_baguette_a11y(raw)
    }
}

/// Fetch [`DeviceBackend::describe_ui`] and parse it into a structured tree.
pub fn describe_ui_structured(
    backend: &dyn DeviceBackend,
    id: &DeviceId,
) -> Result<Vec<UiNode>, DeviceError> {
    let raw = backend.describe_ui(id)?;
    parse_describe_ui(&raw).map_err(|e| DeviceError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBackend;
    use crate::Platform;

    const UIAUTOMATOR_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="com.example" content-desc="" checkable="false" checked="false" clickable="false" enabled="true" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[0,0][1080,2400]">
    <node index="1" text="Log in" resource-id="com.example:id/btn-login" class="android.widget.Button" package="com.example" content-desc="Log in button" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[100,200][300,248]" />
    <node index="2" text="" resource-id="com.example:id/input-email" class="android.widget.EditText" package="com.example" content-desc="" checkable="false" checked="false" clickable="true" enabled="true" focusable="true" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[100,300][980,348]" />
    <node index="3" text="AT&amp;T &lt;roaming&gt;" resource-id="" class="android.widget.TextView" package="com.example" content-desc="" checkable="false" checked="false" clickable="false" enabled="false" focusable="false" focused="false" scrollable="false" long-clickable="false" password="false" selected="false" bounds="[0,0][0,0]" />
  </node>
</hierarchy>"#;

    #[test]
    fn uiautomator_xml_parses_hierarchy() {
        let roots = parse_uiautomator_xml(UIAUTOMATOR_FIXTURE).expect("parses");
        assert_eq!(roots.len(), 1);
        let root = &roots[0];
        assert_eq!(root.class.as_deref(), Some("android.widget.FrameLayout"));
        assert_eq!(root.children.len(), 3);

        let btn = &root.children[0];
        assert_eq!(btn.id.as_deref(), Some("com.example:id/btn-login"));
        assert_eq!(btn.text, "Log in");
        assert_eq!(btn.label, "Log in button");
        assert!(btn.clickable);
        assert!(btn.enabled);
        let b = btn.bounds.as_ref().expect("bounds");
        assert_eq!((b.x, b.y, b.width, b.height), (100.0, 200.0, 200.0, 48.0));
        assert_eq!(b.center(), (200.0, 224.0));

        // XML entities are unescaped.
        let tv = &root.children[2];
        assert_eq!(tv.text, "AT&T <roaming>");
        assert!(!tv.clickable);
        assert!(!tv.enabled);
    }

    #[test]
    fn uiautomator_xml_rejects_empty() {
        assert!(parse_uiautomator_xml("<hierarchy></hierarchy>").is_err());
        assert!(parse_uiautomator_xml("not xml at all").is_err());
    }

    #[test]
    fn baguette_json_parses_elements() {
        let raw = crate::fake::FAKE_A11Y_JSON;
        let nodes = parse_baguette_a11y(raw).expect("parses");
        assert_eq!(nodes.len(), 2);
        let btn = &nodes[0];
        assert_eq!(btn.id.as_deref(), Some("btn-login"));
        assert_eq!(btn.label, "Log in");
        assert_eq!(btn.class.as_deref(), Some("button"));
        assert!(btn.clickable);
        let b = btn.bounds.as_ref().expect("bounds");
        assert_eq!((b.x, b.y, b.width, b.height), (100.0, 200.0, 200.0, 48.0));
        assert!(b.contains(150.0, 210.0));
        assert!(!b.contains(10.0, 10.0));
    }

    #[test]
    fn parse_describe_ui_autodetects_format() {
        assert!(parse_describe_ui(UIAUTOMATOR_FIXTURE).is_ok());
        assert!(parse_describe_ui(crate::fake::FAKE_A11Y_JSON).is_ok());
    }

    #[test]
    fn find_by_text_and_clickable_leaves() {
        let nodes = parse_baguette_a11y(crate::fake::FAKE_A11Y_JSON).expect("parses");
        let root = UiNode {
            id: None,
            class: None,
            text: String::new(),
            label: String::new(),
            bounds: None,
            clickable: false,
            enabled: true,
            children: nodes,
        };
        let found = root.find_by_text("log in").expect("found");
        assert_eq!(found.id.as_deref(), Some("btn-login"));
        assert_eq!(root.clickable_leaves().len(), 2);
    }

    #[test]
    fn to_json_roundtrip_shape() {
        let nodes = parse_baguette_a11y(crate::fake::FAKE_A11Y_JSON).expect("parses");
        let j = nodes[0].to_json();
        assert!(j.contains("\"id\":\"btn-login\""));
        assert!(j.contains("\"clickable\":true"));
        assert!(j.contains("\"x\":100"));
        // It is valid JSON per the crate parser.
        assert!(parse_json(&j).is_ok());
    }

    #[test]
    fn structured_via_fake_backend() {
        let b = FakeBackend::new(vec![FakeBackend::android_running()]);
        let nodes =
            describe_ui_structured(&b, &DeviceId::new("emulator-5554")).expect("structured");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id.as_deref(), Some("btn-login"));
        let _ = Platform::Android; // keep import honest if unused elsewhere
    }
}
