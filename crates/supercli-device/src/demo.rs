//! Scripted demo backend: three fake devices for Devices-panel / /farm UI
//! development and verification. Always compiled (not `cfg(test)`).
//!
//! This is a REAL [`DeviceBackend`] implementation — it records every call
//! and returns canned-but-well-formed data — but the devices are scripted,
//! not attached hardware. It is only registered when the
//! `SUPERCLI_FARM_DEMO` environment variable is set; the device names carry
//! a "Demo" prefix so a scripted device can never be mistaken for hardware.
//!
//! Anything that needs real hardware (streaming video, real input injection)
//! fails honestly via the canned paths below.

use super::{
    DeviceBackend, DeviceError, DeviceId, DeviceInfo, DeviceState, DeviceStream, Platform,
};
use std::collections::HashMap;
use std::sync::Mutex;

/// One recorded backend call: method name + stringified args.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemoCall {
    pub method: &'static str,
    pub args: Vec<String>,
}

struct DemoDevice {
    info: DeviceInfo,
    /// Screen pixels (width, height).
    pixels: (u32, u32),
    dpi: u32,
    /// JPEG bytes served by `screenshot()` (a scripted home screen).
    screen_jpeg: &'static [u8],
}

struct DemoInner {
    devices: Vec<DemoDevice>,
    calls: Vec<DemoCall>,
}

/// Scripted [`DeviceBackend`] with three demo Android devices.
pub struct DemoBackend {
    inner: Mutex<DemoInner>,
}

impl DemoBackend {
    pub fn new() -> Self {
        DemoBackend {
            inner: Mutex::new(DemoInner {
                devices: vec![
                    DemoDevice {
                        info: DeviceInfo {
                            id: DeviceId::new("demo-pixel7-1"),
                            name: "Demo Pixel 7 #1".to_string(),
                            platform: Platform::Android,
                            state: DeviceState::Running,
                        },
                        pixels: (1080, 2400),
                        dpi: 420,
                        screen_jpeg: include_bytes!("demo_assets/demo_pixel7_1.jpg"),
                    },
                    DemoDevice {
                        info: DeviceInfo {
                            id: DeviceId::new("demo-pixel7-2"),
                            name: "Demo Pixel 7 #2".to_string(),
                            platform: Platform::Android,
                            state: DeviceState::Running,
                        },
                        pixels: (1080, 2400),
                        dpi: 420,
                        screen_jpeg: include_bytes!("demo_assets/demo_pixel7_2.jpg"),
                    },
                    DemoDevice {
                        info: DeviceInfo {
                            id: DeviceId::new("demo-pixel8-pro"),
                            name: "Demo Pixel 8 Pro".to_string(),
                            platform: Platform::Android,
                            state: DeviceState::Running,
                        },
                        pixels: (1008, 2244),
                        dpi: 480,
                        screen_jpeg: include_bytes!("demo_assets/demo_pixel8_pro.jpg"),
                    },
                ],
                calls: Vec::new(),
            }),
        }
    }

    /// All recorded calls, in order.
    pub fn calls(&self) -> Vec<DemoCall> {
        self.inner.lock().unwrap().calls.clone()
    }

    /// Device points for a device: `points = pixels * 160 / dpi`.
    pub fn points(&self, id: &DeviceId) -> Option<(u32, u32)> {
        let inner = self.inner.lock().unwrap();
        inner
            .devices
            .iter()
            .find(|d| d.info.id == *id)
            .map(|d| (d.pixels.0 * 160 / d.dpi, d.pixels.1 * 160 / d.dpi))
    }

    fn record(&self, method: &'static str, args: Vec<String>) {
        self.inner
            .lock()
            .unwrap()
            .calls
            .push(DemoCall { method, args });
    }

    fn find(&self, id: &DeviceId) -> Result<(u32, u32, u32, &'static [u8]), DeviceError> {
        let inner = self.inner.lock().unwrap();
        inner
            .devices
            .iter()
            .find(|d| d.info.id == *id)
            .map(|d| (d.pixels.0, d.pixels.1, d.dpi, d.screen_jpeg))
            .ok_or_else(|| DeviceError::Unsupported(format!("demo: unknown device '{id}'")))
    }
}

impl Default for DemoBackend {
    fn default() -> Self {
        DemoBackend::new()
    }
}

impl DeviceBackend for DemoBackend {
    fn list(&self) -> Result<Vec<DeviceInfo>, DeviceError> {
        self.record("list", vec![]);
        Ok(self
            .inner
            .lock()
            .unwrap()
            .devices
            .iter()
            .map(|d| d.info.clone())
            .collect())
    }

    fn boot(&self, id: &DeviceId) -> Result<(), DeviceError> {
        self.record("boot", vec![id.to_string()]);
        Ok(())
    }

    fn stop(&self, id: &DeviceId) -> Result<(), DeviceError> {
        self.record("stop", vec![id.to_string()]);
        Ok(())
    }

    fn install(&self, id: &DeviceId, path: &std::path::Path) -> Result<(), DeviceError> {
        self.record("install", vec![id.to_string(), path.display().to_string()]);
        Ok(())
    }

    fn launch(&self, id: &DeviceId, app_id: &str) -> Result<(), DeviceError> {
        self.record("launch", vec![id.to_string(), app_id.to_string()]);
        Ok(())
    }

    fn screenshot(&self, id: &DeviceId) -> Result<Vec<u8>, DeviceError> {
        let (_, _, _, jpeg) = self.find(id)?;
        self.record("screenshot", vec![id.to_string()]);
        Ok(jpeg.to_vec())
    }

    fn logs(&self, id: &DeviceId, clear: bool) -> Result<String, DeviceError> {
        self.find(id)?;
        self.record("logs", vec![id.to_string(), clear.to_string()]);
        if clear {
            Ok(String::new())
        } else {
            Ok(format!(
                "09-26 02:30:00.000  1234  5678 I demo: {id} log line one\n\
                 09-26 02:30:01.000  1234  5678 W demo: {id} log line two\n"
            ))
        }
    }

    fn tap(&self, id: &DeviceId, x: u32, y: u32) -> Result<(), DeviceError> {
        self.find(id)?;
        self.record("tap", vec![id.to_string(), x.to_string(), y.to_string()]);
        Ok(())
    }

    fn type_text(&self, id: &DeviceId, text: &str) -> Result<(), DeviceError> {
        self.find(id)?;
        self.record("type_text", vec![id.to_string(), text.to_string()]);
        Ok(())
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
        self.find(id)?;
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
        );
        Ok(())
    }

    fn stream(&self, _id: &DeviceId) -> Result<DeviceStream, DeviceError> {
        // No child process to wrap; the serve layer synthesizes the wire
        // stream from screenshot() frames instead.
        Err(DeviceError::Unsupported(
            "demo: live H.264 streaming is not scripted; the web layer serves JPEG seeds"
                .to_string(),
        ))
    }

    fn describe_ui(&self, id: &DeviceId) -> Result<String, DeviceError> {
        self.find(id)?;
        self.record("describe_ui", vec![id.to_string()]);
        Ok(r#"{"elements":[
          {"id":"btn-settings","label":"Settings","type":"button",
           "bounds":{"x":100,"y":200,"width":200,"height":48},"actions":["tap"]},
          {"id":"demo-banner","label":"Scripted demo device","type":"text",
           "bounds":{"x":16,"y":68,"width":300,"height":24},"actions":[]}
        ]}"#
        .to_string())
    }

    fn density_dpi(&self, id: &DeviceId) -> Result<u32, DeviceError> {
        let (_, _, dpi, _) = self.find(id)?;
        Ok(dpi)
    }

    fn key(&self, id: &DeviceId, keycode: &str) -> Result<(), DeviceError> {
        self.find(id)?;
        match keycode {
            "home" | "back" | "power" | "lock" => {
                self.record("key", vec![id.to_string(), keycode.to_string()]);
                Ok(())
            }
            other => Err(DeviceError::Unsupported(format!(
                "demo: unknown keycode '{other}'"
            ))),
        }
    }
}

/// Screen geometry for the wire-format 0x01 description.
#[derive(Clone, Copy, Debug)]
pub struct DemoGeometry {
    pub pixels: (u32, u32),
    pub points: (u32, u32),
    pub dpi: u32,
}

impl DemoBackend {
    /// Geometry (pixels, points, dpi) for a demo device id.
    pub fn geometry(&self, id: &DeviceId) -> Option<DemoGeometry> {
        let inner = self.inner.lock().unwrap();
        inner
            .devices
            .iter()
            .find(|d| d.info.id == *id)
            .map(|d| DemoGeometry {
                pixels: d.pixels,
                points: (d.pixels.0 * 160 / d.dpi, d.pixels.1 * 160 / d.dpi),
                dpi: d.dpi,
            })
    }

    /// Is this one of the scripted demo device ids?
    pub fn is_demo_device(&self, id: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.devices.iter().any(|d| d.info.id.as_str() == id)
    }
}

/// Devices the demo backend scripts, for documentation/tests.
pub fn demo_device_ids() -> Vec<HashMap<String, String>> {
    vec![
        [("id", "demo-pixel7-1"), ("name", "Demo Pixel 7 #1")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        [("id", "demo-pixel7-2"), ("name", "Demo Pixel 7 #2")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        [("id", "demo-pixel8-pro"), ("name", "Demo Pixel 8 Pro")]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_lists_three_scripted_devices() {
        let b = DemoBackend::new();
        let devices = b.list().expect("list");
        assert_eq!(devices.len(), 3);
        let ids: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["demo-pixel7-1", "demo-pixel7-2", "demo-pixel8-pro"]
        );
        for d in &devices {
            assert_eq!(d.platform, Platform::Android);
            assert_eq!(d.state, DeviceState::Running);
            assert!(d.name.starts_with("Demo "), "name flags scripted device");
        }
    }

    #[test]
    fn demo_points_derive_from_pixels_and_dpi() {
        let b = DemoBackend::new();
        // 1080px @420dpi -> 411pt; 2400px @420dpi -> 914pt
        assert_eq!(b.points(&DeviceId::new("demo-pixel7-1")), Some((411, 914)));
        // 1008px @480dpi -> 336pt; 2244px @480dpi -> 748pt
        assert_eq!(
            b.points(&DeviceId::new("demo-pixel8-pro")),
            Some((336, 748))
        );
        assert_eq!(b.density_dpi(&DeviceId::new("demo-pixel7-1")).unwrap(), 420);
    }

    #[test]
    fn demo_tap_swipe_key_record_pixel_args() {
        let b = DemoBackend::new();
        let id = DeviceId::new("demo-pixel7-1");
        b.tap(&id, 540, 1200).unwrap();
        b.swipe(&id, 100, 200, 300, 400, 300).unwrap();
        b.key(&id, "home").unwrap();
        b.type_text(&id, "hi").unwrap();
        let calls = b.calls();
        let methods: Vec<&str> = calls.iter().map(|c| c.method).collect();
        assert_eq!(methods, vec!["tap", "swipe", "key", "type_text"]);
        assert_eq!(calls[0].args, vec!["demo-pixel7-1", "540", "1200"]);
        assert!(b.key(&id, "bogus").is_err());
    }

    #[test]
    fn demo_unknown_device_is_honest_error() {
        let b = DemoBackend::new();
        let id = DeviceId::new("nope");
        assert!(b.tap(&id, 1, 1).is_err());
        assert!(b.screenshot(&id).is_err());
        assert!(!b.is_demo_device("nope"));
        assert!(b.is_demo_device("demo-pixel7-2"));
    }

    #[test]
    fn demo_screenshot_returns_jpeg_magic() {
        let b = DemoBackend::new();
        let bytes = b.screenshot(&DeviceId::new("demo-pixel7-1")).unwrap();
        assert!(bytes.len() > 1000);
        assert_eq!(&bytes[0..2], &[0xFF, 0xD8], "JPEG SOI magic");
    }
}
