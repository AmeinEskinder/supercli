use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/opencode/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/opencode/adapter/setup.rs"
    ));
}

pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_opencode_plugin)).with_resume_adapter(resume::ADAPTER);
