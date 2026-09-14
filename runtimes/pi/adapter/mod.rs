use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/pi/adapter/resume.rs"
    ));
}

/// Pi has no lifecycle hooks and no MCP registration, so there is nothing to
/// install: detection supplies identity, and resume falls back to Pi's own
/// continue-last.
pub(crate) const INTEGRATION: Integration =
    Integration::new(None).with_resume_adapter(resume::ADAPTER);
