use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/amp/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/amp/adapter/setup.rs"
    ));
}

/// Amp loads plugins from a project's `.amp/plugins/` (with `PLUGINS=all` in
/// the environment), so the integration installs the shared reporter
/// globally and the project plugin on request
/// (`unpeel integrations install amp --project DIR`).
pub(crate) const INTEGRATION: Integration =
    Integration::new(Some(setup::install_amp_plugin)).with_resume_adapter(resume::ADAPTER);
