use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/github-copilot/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/github-copilot/adapter/setup.rs"
    ));
}

/// Copilot only reads hooks from a repository's `.github/hooks/`, so the
/// integration installs the shared reporter globally and the project file
/// on request (`unpeel integrations install github-copilot --project DIR`).
pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_copilot_hook)).with_resume_adapter(resume::ADAPTER);
