//! Fake backend for unit tests: in-memory device list, records every call.
//!
//! Compiled only under `cfg(test)`. No platform tools are touched.

use super::{
    DeviceBackend, DeviceError, DeviceId, DeviceInfo, DeviceState, DeviceStream, Platform,
};
use std::sync::Mutex;

/// One recorded backend call: method name + stringified args.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeCall {
    pub method: &'static str,
    pub args: Vec<String>,
}

/// Minimal valid PNG (1×1, RGBA): enough for magic-byte assertions.
pub const FAKE_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, // magic
    0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR len+type
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
    0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, // depth/color/…
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, // IDAT len+type
    0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, // zlib stub
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, // …
    0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, // IEND len+type
    0x42, 0x60, 0x82,
];

struct FakeInner {
    devices: Vec<DeviceInfo>,
    calls: Vec<FakeCall>,
    /// When set, the next call fails with this error instead of succeeding.
    fail_next: Option<DeviceErrorKind>,
}

/// Injectable failure kinds (DeviceError itself is not Clone).
#[derive(Clone, Copy, Debug)]
enum DeviceErrorKind {
    Timeout,
    ToolFailed,
}

impl DeviceErrorKind {
    fn into_error(self, tool: &str) -> DeviceError {
        match self {
            DeviceErrorKind::Timeout => DeviceError::Timeout {
                tool: tool.to_string(),
            },
            DeviceErrorKind::ToolFailed => DeviceError::ToolFailed {
                tool: tool.to_string(),
                code: Some(1),
                stderr: "fake failure".to_string(),
            },
        }
    }
}

/// In-memory [`DeviceBackend`] for unit tests.
pub struct FakeBackend {
    inner: Mutex<FakeInner>,
}

impl FakeBackend {
    pub fn new(devices: Vec<DeviceInfo>) -> Self {
        FakeBackend {
            inner: Mutex::new(FakeInner {
                devices,
                calls: Vec::new(),
                fail_next: None,
            }),
        }
    }

    pub fn android_running() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::new("emulator-5554"),
            name: "Pixel_8_API_34".to_string(),
            platform: Platform::Android,
            state: DeviceState::Running,
        }
    }

    pub fn ios_available() -> DeviceInfo {
        DeviceInfo {
            id: DeviceId::new("A1B2C3D4-E5F6-7890-ABCD-EF1234567890"),
            name: "iPhone 16 Pro".to_string(),
            platform: Platform::IOS,
            state: DeviceState::Available,
        }
    }

    /// Make the next backend call fail with the given kind.
    pub fn fail_next(&self, kind: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.fail_next = Some(match kind {
            "timeout" => DeviceErrorKind::Timeout,
            _ => DeviceErrorKind::ToolFailed,
        });
    }

    /// All recorded calls, in order.
    pub fn calls(&self) -> Vec<FakeCall> {
        self.inner.lock().unwrap().calls.clone()
    }

    fn record(&self, method: &'static str, args: Vec<String>) -> Result<(), DeviceError> {
        let mut inner = self.inner.lock().unwrap();
        inner.calls.push(FakeCall { method, args });
        if let Some(kind) = inner.fail_next.take() {
            return Err(kind.into_error(method));
        }
        Ok(())
    }
}

impl DeviceBackend for FakeBackend {
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
        self.record("list", vec![])?;
        Ok(self.inner.lock().unwrap().devices.clone())
    }

    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError> {
        self.record("boot", vec![id.to_string()])
    }

    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError> {
        self.record("stop", vec![id.to_string()])
    }

    fn install(&self, id: &DeviceId, path: &std::path::Path) -> Result<(), DeviceError> {
        self.record("install", vec![id.to_string(), path.display().to_string()])
    }

    fn launch(&self, id: &DeviceId, app_id: &str) -> Result<(), DeviceError> {
        self.record("launch", vec![id.to_string(), app_id.to_string()])
    }

    fn screenshot(&self, id: &DeviceId) -> Result<Vec<u8>, DeviceError> {
        self.record("screenshot", vec![id.to_string()])?;
        Ok(FAKE_PNG.to_vec())
    }

    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError> {
        self.record("logs", vec![id.to_string(), clear.to_string()])?;
        if clear {
            Ok(String::new())
        } else {
            Ok("01-01 00:00:01.000  1234  5678 I fake: log line one\n\
                01-01 00:00:02.000  1234  5678 W fake: log line two\n"
                .to_string())
        }
    }

    fn tap(&self, id: &DeviceId, x: u32, y: u32) -> Result<(), DeviceError> {
        self.record("tap", vec![id.to_string(), x.to_string(), y.to_string()])
    }

    fn type_text(&self, id: &DeviceId, text: &str) -> Result<(), DeviceError> {
        self.record("type_text", vec![id.to_string(), text.to_string()])
    }

    fn swipe(
        &self,
        id: &DeviceId,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
        duration_ms: u32,
    ) -> Result<(), DeviceError> {
        self.record(
            "swipe",
            vec![
                id.to_string(),
                x1.to_string(),
                y1.to_string(),
                x2.to_string(),
                y2.to_string(),
                duration_ms.to_string(),
            ],
        )
    }

    fn stream(&self, id: &DeviceId) -> Result<DeviceStream, DeviceError> {
        // The fake has no child process to wrap; report honestly.
        let _ = self.record("stream", vec![id.to_string()]);
        Err(DeviceError::Unsupported(
            "FakeBackend does not stream; use a real backend".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn backend() -> FakeBackend {
        FakeBackend::new(vec![
            FakeBackend::android_running(),
            FakeBackend::ios_available(),
        ])
    }

    #[test]
    fn list_returns_scripted_devices() {
        let b = backend();
        let devices = b.list().expect("list");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].id.as_str(), "emulator-5554");
        assert_eq!(devices[0].platform, Platform::Android);
        assert_eq!(devices[0].state, DeviceState::Running);
        assert_eq!(devices[1].platform, Platform::IOS);
        assert_eq!(devices[1].state, DeviceState::Available);
        assert_eq!(
            b.calls(),
            vec![FakeCall {
                method: "list",
                args: vec![]
            }]
        );
    }

    #[test]
    fn boot_stop_record_device_id() {
        let b = backend();
        let id = DeviceId::new("Pixel_8_API_34");
        b.boot(&id).unwrap();
        b.stop(&id).unwrap();
        assert_eq!(
            b.calls(),
            vec![
                FakeCall {
                    method: "boot",
                    args: vec!["Pixel_8_API_34".to_string()]
                },
                FakeCall {
                    method: "stop",
                    args: vec!["Pixel_8_API_34".to_string()]
                },
            ]
        );
    }

    #[test]
    fn install_launch_record_args() {
        let b = backend();
        let id = DeviceId::new("emulator-5554");
        b.install(&id, Path::new("/tmp/app.apk")).unwrap();
        b.launch(&id, "com.example.app").unwrap();
        let calls = b.calls();
        assert_eq!(calls[0].method, "install");
        assert_eq!(calls[0].args, vec!["emulator-5554", "/tmp/app.apk"]);
        assert_eq!(calls[1].method, "launch");
        assert_eq!(calls[1].args, vec!["emulator-5554", "com.example.app"]);
    }

    #[test]
    fn screenshot_returns_png_magic() {
        let b = backend();
        let bytes = b.screenshot(&DeviceId::new("emulator-5554")).unwrap();
        assert!(bytes.len() >= 8);
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
    }

    #[test]
    fn logs_returns_canned_text_and_clear_is_empty() {
        let b = backend();
        let id = DeviceId::new("emulator-5554");
        let logs = b.logs(&id, false).unwrap();
        assert!(logs.contains("log line one"));
        assert!(logs.contains("log line two"));
        assert_eq!(b.logs(&id, true).unwrap(), "");
        let calls = b.calls();
        assert_eq!(calls[1].args, vec!["emulator-5554", "true"]);
    }

    #[test]
    fn tap_type_swipe_pass_coordinates_through() {
        let b = backend();
        let id = DeviceId::new("emulator-5554");
        b.tap(&id, 100, 200).unwrap();
        b.type_text(&id, "hello world").unwrap();
        b.swipe(&id, 10, 20, 30, 40, 500).unwrap();
        let calls = b.calls();
        assert_eq!(calls[0].args, vec!["emulator-5554", "100", "200"]);
        assert_eq!(calls[1].args, vec!["emulator-5554", "hello world"]);
        assert_eq!(
            calls[2].args,
            vec!["emulator-5554", "10", "20", "30", "40", "500"]
        );
    }

    #[test]
    fn stream_is_honestly_unsupported_on_fake() {
        let b = backend();
        let err = b.stream(&DeviceId::new("emulator-5554")).unwrap_err();
        assert!(matches!(err, DeviceError::Unsupported(_)));
        // The attempt is still recorded.
        assert_eq!(b.calls()[0].method, "stream");
    }

    #[test]
    fn injected_timeout_error_propagates() {
        let b = backend();
        b.fail_next("timeout");
        let err = b.boot(&DeviceId::new("Pixel_8_API_34")).unwrap_err();
        match err {
            DeviceError::Timeout { tool } => assert_eq!(tool, "boot"),
            other => panic!("expected Timeout, got {other:?}"),
        }
        // Failure is one-shot: the next call succeeds.
        b.boot(&DeviceId::new("Pixel_8_API_34")).unwrap();
    }

    #[test]
    fn injected_tool_failed_error_propagates() {
        let b = backend();
        b.fail_next("failed");
        let err = b
            .install(&DeviceId::new("emulator-5554"), Path::new("x.apk"))
            .unwrap_err();
        match err {
            DeviceError::ToolFailed { tool, code, .. } => {
                assert_eq!(tool, "install");
                assert_eq!(code, Some(1));
            }
            other => panic!("expected ToolFailed, got {other:?}"),
        }
    }

    #[test]
    fn calls_are_recorded_in_order() {
        let b = backend();
        let id = DeviceId::new("emulator-5554");
        b.list().unwrap();
        b.tap(&id, 1, 2).unwrap();
        b.logs(&id, false).unwrap();
        let methods: Vec<&str> = b.calls().iter().map(|c| c.method).collect();
        assert_eq!(methods, vec!["list", "tap", "logs"]);
    }

    #[test]
    fn error_kinds_are_not_confused() {
        // Timeout vs ToolFailed vs Unsupported vs NotMacOSHost are distinct.
        let timeout = DeviceError::Timeout {
            tool: "t".to_string(),
        };
        let failed = DeviceError::ToolFailed {
            tool: "t".to_string(),
            code: Some(1),
            stderr: String::new(),
        };
        let unsupported = DeviceError::Unsupported("x".to_string());
        assert_ne!(
            std::mem::discriminant(&timeout),
            std::mem::discriminant(&failed)
        );
        assert_ne!(
            std::mem::discriminant(&timeout),
            std::mem::discriminant(&unsupported)
        );
        assert_ne!(
            std::mem::discriminant(&timeout),
            std::mem::discriminant(&DeviceError::NotMacOSHost)
        );
        assert_ne!(
            std::mem::discriminant(&failed),
            std::mem::discriminant(&DeviceError::ToolMissing("t".to_string()))
        );
    }
}
