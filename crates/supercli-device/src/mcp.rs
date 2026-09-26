//! Device MCP server: JSON-RPC 2.0 over stdio, following the same shape as
//! `supercli-core`'s `browser_mcp` (`initialize` / `tools/list` /
//! `tools/call`). Exposes the safe device surface to agent sessions:
//! tap, swipe, type, describe-ui, screenshot, and one-shot logs.
//!
//! Dangerous operations (install/uninstall/erase) are deliberately NOT
//! exposed here; they live behind [`crate::danger::GuardedBackend`] and its
//! approval gate.
//!
//! Pure std: JSON output is hand-rolled, input reuses the crate's parser.
//! Compiled only with the `device` cargo feature.

use super::json::{parse_json, Json};
use super::logs::{base64_encode, LogFilter, LogLevel, LogStream};
use super::ui::json_escape;
use super::{DeviceBackend, DeviceError, DeviceId};
use std::io::{BufRead, Write};

/// Hidden CLI arg convention, mirroring `__browser_mcp__`.
pub const DEVICE_MCP_ARG: &str = "__device_mcp__";

const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "supercli-device";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One MCP tool definition: name, description, JSON-schema-ish params.
struct ToolDef {
    name: &'static str,
    description: &'static str,
    /// (param name, type, required, description)
    params: &'static [(&'static str, &'static str, bool, &'static str)],
}

const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "device_tap",
        description: "Tap at device-point coordinates on the device screen.",
        params: &[
            ("device_id", "string", true, "adb serial or simctl UDID"),
            ("x", "number", true, "x in device points"),
            ("y", "number", true, "y in device points"),
        ],
    },
    ToolDef {
        name: "device_swipe",
        description: "Swipe from (x1,y1) to (x2,y2) over duration_ms.",
        params: &[
            ("device_id", "string", true, "adb serial or simctl UDID"),
            ("x1", "number", true, "start x in device points"),
            ("y1", "number", true, "start y in device points"),
            ("x2", "number", true, "end x in device points"),
            ("y2", "number", true, "end y in device points"),
            ("duration_ms", "number", false, "swipe duration, default 300"),
        ],
    },
    ToolDef {
        name: "device_type",
        description: "Type text into the currently focused field.",
        params: &[
            ("device_id", "string", true, "adb serial or simctl UDID"),
            ("text", "string", true, "text to type"),
        ],
    },
    ToolDef {
        name: "device_describe_ui",
        description: "Structured accessibility tree: elements with ids, labels, bounds, clickability. Prefer this over screenshots for acting on UI.",
        params: &[("device_id", "string", true, "adb serial or simctl UDID")],
    },
    ToolDef {
        name: "device_screenshot",
        description: "Capture the screen; returns base64 PNG.",
        params: &[("device_id", "string", true, "adb serial or simctl UDID")],
    },
    ToolDef {
        name: "device_logs",
        description: "One-shot device log dump, parsed and filtered.",
        params: &[
            ("device_id", "string", true, "adb serial or simctl UDID"),
            ("tag", "string", false, "exact tag match"),
            ("min_level", "string", false, "V|D|I|W|E|F, default V"),
            ("contains", "string", false, "case-insensitive substring"),
            ("max_lines", "number", false, "cap, default 200, max 2000"),
        ],
    },
];

/// Run the MCP server: read JSON-RPC lines from `input`, write responses to
/// `output`. Returns when input reaches EOF.
pub fn run_stdio_mcp(
    backend: &dyn DeviceBackend,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<(), String> {
    let mut line = String::new();
    loop {
        line.clear();
        let n = input
            .read_line(&mut line)
            .map_err(|e| format!("MCP stdin: {e}"))?;
        if n == 0 {
            return Ok(()); // EOF
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let msg: Json = match parse_json(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // drop unparsable lines, like browser_mcp
        };
        if let Some(resp) = handle_message(backend, &msg) {
            output
                .write_all(resp.as_bytes())
                .and_then(|_| output.write_all(b"\n"))
                .and_then(|_| output.flush())
                .map_err(|e| format!("MCP stdout: {e}"))?;
        }
    }
}

fn handle_message(backend: &dyn DeviceBackend, msg: &Json) -> Option<String> {
    let method = msg.get("method")?.as_str()?;
    // Notifications (no id) get no response.
    let id = msg.get("id")?;
    if matches!(id, Json::Null) {
        return None;
    }
    let id_json = json_value(id);
    let result = match method {
        "initialize" => Some(format!(
            "{{\"protocolVersion\":\"{PROTOCOL_VERSION}\",\
             \"serverInfo\":{{\"name\":\"{SERVER_NAME}\",\"version\":\"{SERVER_VERSION}\"}},\
             \"capabilities\":{{\"tools\":{{}}}}}}"
        )),
        "tools/list" => Some(tools_list_json()),
        "tools/call" => {
            let params = msg.get("params")?;
            let name = params.get("name")?.as_str()?;
            let args = params.get("arguments").cloned().unwrap_or(Json::Null);
            Some(call_tool(backend, name, &args))
        }
        _ => None,
    };
    match result {
        Some(r) => Some(format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id_json},\"result\":{r}}}"
        )),
        None => Some(format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id_json},\
             \"error\":{{\"code\":-32601,\"message\":\"method not found: {}\"}}}}",
            json_escape(method)
        )),
    }
}

fn tools_list_json() -> String {
    let mut s = String::from("{\"tools\":[");
    for (i, t) in TOOLS.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"name\":\"{}\",\"description\":\"{}\",\"inputSchema\":{}}}",
            t.name,
            json_escape(t.description),
            schema_json(t.params)
        ));
    }
    s.push_str("]}");
    s
}

fn schema_json(params: &[(&str, &str, bool, &str)]) -> String {
    let mut s = String::from("{\"type\":\"object\",\"properties\":{");
    let mut required: Vec<&str> = Vec::new();
    for (i, (name, ty, req, desc)) in params.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "\"{name}\":{{\"type\":\"{ty}\",\"description\":\"{}\"}}",
            json_escape(desc)
        ));
        if *req {
            required.push(name);
        }
    }
    s.push_str("},\"required\":[");
    for (i, r) in required.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(r);
        s.push('"');
    }
    s.push_str("]}");
    s
}

/// Serialize a parsed [`Json`] back to text (for echoing ids).
fn json_value(v: &Json) -> String {
    match v {
        Json::Null => "null".to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Json::Str(s) => format!("\"{}\"", json_escape(s)),
        Json::Arr(a) => {
            let mut s = String::from("[");
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&json_value(x));
            }
            s.push(']');
            s
        }
        Json::Obj(pairs) => {
            let mut s = String::from("{");
            for (i, (k, x)) in pairs.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                s.push_str(&format!("\"{}\":{}", json_escape(k), json_value(x)));
            }
            s.push('}');
            s
        }
    }
}

/// MCP content envelope: `{"content":[{"type":"text","text":"…"}]}`.
fn text_result(text: &str) -> String {
    format!(
        "{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}]}}",
        json_escape(text)
    )
}

fn tool_error(message: &str) -> String {
    format!(
        "{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}],\"isError\":true}}",
        json_escape(&format!("error: {message}"))
    )
}

fn device_err(e: DeviceError) -> String {
    tool_error(&e.to_string())
}

// --- argument helpers -------------------------------------------------------

fn arg_str<'a>(args: &'a Json, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("missing required string argument '{name}'"))
}

fn arg_num(args: &Json, name: &str, default: f64) -> Result<f64, String> {
    match args.get(name) {
        None | Some(Json::Null) => Ok(default),
        Some(Json::Num(n)) => Ok(*n),
        Some(other) => Err(format!(
            "argument '{name}' must be a number, got {}",
            json_value(other)
        )),
    }
}

fn arg_device_id(args: &Json) -> Result<DeviceId, String> {
    Ok(DeviceId::new(arg_str(args, "device_id")?))
}

fn call_tool(backend: &dyn DeviceBackend, name: &str, args: &Json) -> String {
    match name {
        "device_tap" => {
            let id = arg_device_id(args);
            let x = arg_num(args, "x", f64::NAN);
            let y = arg_num(args, "y", f64::NAN);
            match (id, x, y) {
                (Ok(id), Ok(x), Ok(y))
                    if x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0 =>
                {
                    match backend.tap(&id, x as u32, y as u32) {
                        Ok(()) => text_result(&format!("tapped ({x}, {y}) on {id}")),
                        Err(e) => device_err(e),
                    }
                }
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => tool_error(&e),
                _ => tool_error("x and y must be finite, non-negative numbers"),
            }
        }
        "device_swipe" => {
            let id = arg_device_id(args);
            let nums: Result<Vec<f64>, String> = ["x1", "y1", "x2", "y2"]
                .iter()
                .map(|n| arg_num(args, n, f64::NAN))
                .collect();
            let dur = arg_num(args, "duration_ms", 300.0);
            match (id, nums, dur) {
                (Ok(id), Ok(v), Ok(d))
                    if v.iter().all(|n| n.is_finite() && *n >= 0.0)
                        && d.is_finite()
                        && d >= 0.0 =>
                {
                    match backend.swipe(
                        &id,
                        v[0] as u32,
                        v[1] as u32,
                        v[2] as u32,
                        v[3] as u32,
                        d as u32,
                    ) {
                        Ok(()) => text_result(&format!("swiped on {id}")),
                        Err(e) => device_err(e),
                    }
                }
                (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => tool_error(&e),
                _ => tool_error("coordinates must be finite, non-negative numbers"),
            }
        }
        "device_type" => match (arg_device_id(args), arg_str(args, "text")) {
            (Ok(id), Ok(text)) => match backend.type_text(&id, text) {
                Ok(()) => text_result(&format!("typed {} chars on {id}", text.len())),
                Err(e) => device_err(e),
            },
            (Err(e), _) | (_, Err(e)) => tool_error(&e),
        },
        "device_describe_ui" => match arg_device_id(args) {
            Ok(id) => match super::ui::describe_ui_structured(backend, &id) {
                Ok(nodes) => {
                    let mut s = String::from("[");
                    for (i, n) in nodes.iter().enumerate() {
                        if i > 0 {
                            s.push(',');
                        }
                        s.push_str(&n.to_json());
                    }
                    s.push(']');
                    text_result(&s)
                }
                Err(e) => device_err(e),
            },
            Err(e) => tool_error(&e),
        },
        "device_screenshot" => match arg_device_id(args) {
            Ok(id) => match backend.screenshot(&id) {
                Ok(png) => text_result(&format!(
                    "screenshot png, {} bytes, base64: {}",
                    png.len(),
                    base64_encode(&png)
                )),
                Err(e) => device_err(e),
            },
            Err(e) => tool_error(&e),
        },
        "device_logs" => {
            let id = match arg_device_id(args) {
                Ok(id) => id,
                Err(e) => return tool_error(&e),
            };
            let tag = args.get("tag").and_then(Json::as_str).map(str::to_string);
            let min_level = match args.get("min_level").and_then(Json::as_str) {
                None | Some("") => LogLevel::Verbose,
                Some(s) => match s
                    .chars()
                    .next()
                    .and_then(|c| LogLevel::from_char(c.to_ascii_uppercase()))
                {
                    Some(l) => l,
                    None => return tool_error("min_level must be one of V|D|I|W|E|F"),
                },
            };
            let contains = args
                .get("contains")
                .and_then(Json::as_str)
                .map(str::to_string);
            let max_lines = arg_num(args, "max_lines", 200.0)
                .unwrap_or(200.0)
                .clamp(1.0, 2000.0) as usize;
            let filter = LogFilter {
                tag,
                min_level,
                contains,
            };
            match backend.logs(&id, false) {
                Ok(raw) => {
                    let entries = LogStream::from_text(&raw, filter);
                    let mut s = String::from("[");
                    for (i, e) in entries.iter().take(max_lines).enumerate() {
                        if i > 0 {
                            s.push(',');
                        }
                        s.push_str(&e.to_json());
                    }
                    s.push(']');
                    text_result(&s)
                }
                Err(e) => device_err(e),
            }
        }
        _ => tool_error(&format!("unknown tool '{name}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBackend;
    use std::io::Cursor;

    fn backend() -> FakeBackend {
        FakeBackend::new(vec![FakeBackend::android_running()])
    }

    fn rpc(backend: &FakeBackend, body: &str) -> Vec<String> {
        let mut input = Cursor::new(body.as_bytes().to_vec());
        let mut output: Vec<u8> = Vec::new();
        run_stdio_mcp(backend, &mut input, &mut output).expect("mcp runs");
        String::from_utf8(output)
            .expect("utf-8")
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn initialize_and_tools_list() {
        let b = backend();
        let lines = rpc(
            &b,
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n\
             {\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\",\"params\":{}}\n",
        );
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"protocolVersion\":\"2025-06-18\""));
        assert!(lines[0].contains("\"name\":\"supercli-device\""));
        for tool in [
            "device_tap",
            "device_swipe",
            "device_type",
            "device_describe_ui",
            "device_screenshot",
            "device_logs",
        ] {
            assert!(lines[1].contains(tool), "tools/list has {tool}");
        }
        // No dangerous tools are exposed.
        assert!(!lines[1].contains("install"));
        assert!(!lines[1].contains("uninstall"));
        assert!(!lines[1].contains("erase"));
    }

    #[test]
    fn unknown_method_is_jsonrpc_error() {
        let b = backend();
        let lines = rpc(
            &b,
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"nope\",\"params\":{}}\n",
        );
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\"code\":-32601"));
        assert!(lines[0].contains("\"id\":7"));
    }

    #[test]
    fn notifications_get_no_response() {
        let b = backend();
        let lines = rpc(&b, "{\"jsonrpc\":\"2.0\",\"method\":\"tools/list\"}\n");
        assert!(lines.is_empty());
    }

    #[test]
    fn tap_roundtrips_through_fake_backend() {
        let b = backend();
        let lines = rpc(
            &b,
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
             \"params\":{\"name\":\"device_tap\",\
             \"arguments\":{\"device_id\":\"emulator-5554\",\"x\":100,\"y\":200}}}\n",
        );
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("tapped"), "got {}", lines[0]);
        assert!(!lines[0].contains("isError"));
        let calls = b.calls();
        assert_eq!(calls[0].method, "tap");
        assert_eq!(calls[0].args, vec!["emulator-5554", "100", "200"]);
    }

    #[test]
    fn tap_rejects_bad_coordinates() {
        let b = backend();
        let lines = rpc(
            &b,
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
             \"params\":{\"name\":\"device_tap\",\
             \"arguments\":{\"device_id\":\"emulator-5554\",\"x\":-5,\"y\":200}}}\n",
        );
        assert!(lines[0].contains("isError"), "got {}", lines[0]);
        assert!(b.calls().is_empty(), "backend must not be touched");
    }

    #[test]
    fn describe_ui_returns_structured_tree() {
        let b = backend();
        let lines = rpc(
            &b,
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
             \"params\":{\"name\":\"device_describe_ui\",\
             \"arguments\":{\"device_id\":\"emulator-5554\"}}}\n",
        );
        assert!(lines[0].contains("btn-login"), "got {}", lines[0]);
        assert!(lines[0].contains("clickable"), "got {}", lines[0]);
    }

    #[test]
    fn unknown_tool_is_error_not_crash() {
        let b = backend();
        let lines = rpc(
            &b,
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
             \"params\":{\"name\":\"device_install\",\"arguments\":{}}}\n",
        );
        assert!(lines[0].contains("isError"), "got {}", lines[0]);
        assert!(lines[0].contains("unknown tool"), "got {}", lines[0]);
    }

    #[test]
    fn malformed_lines_are_dropped() {
        let b = backend();
        let lines = rpc(
            &b,
            "this is not json\n\
             {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n",
        );
        assert_eq!(lines.len(), 1);
    }
}
