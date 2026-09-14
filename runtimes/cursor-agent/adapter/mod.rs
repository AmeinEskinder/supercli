use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/cursor-agent/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/cursor-agent/adapter/setup.rs"
    ));
}

pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_cursor_hooks)).with_resume_adapter(resume::ADAPTER);
