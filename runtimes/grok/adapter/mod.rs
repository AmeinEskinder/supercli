use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/grok/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/grok/adapter/setup.rs"
    ));
}

pub(crate) const INTEGRATION: Integration = Integration::new(Some(setup::install_grok_hooks))
    .with_resume_adapter(resume::ADAPTER)
    // ESC before the first response rewinds the prompt; Grok intentionally
    // omits StopCancelled for that path. The idle ping can arrive much later.
    .with_escape_cancellation();
