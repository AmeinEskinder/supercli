use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/cline/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/cline/adapter/setup.rs"
    ));
}

pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_cline_hooks)).with_resume_adapter(resume::ADAPTER);
