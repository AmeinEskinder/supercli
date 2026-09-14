use super::Integration;

mod resume {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/muse-code/adapter/resume.rs"
    ));
}

pub(crate) mod setup {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../runtimes/muse-code/adapter/setup.rs"
    ));
}

/// Muse loads native plugins only with `MUSE_EXPERIMENTAL_PLUGINS=1` in its
/// environment. Unpeel no longer exports it at launch; the installed
/// integration documents it and the user's shell sets it.
pub(crate) const INTEGRATION: Integration = Integration::new(Some(setup::install_muse_hooks))
    // Muse 1.0.3 interrupts the foreground turn on ESC without emitting Stop.
    // https://dev.meta.ai/docs/muse-code/interactive#steering
    .with_escape_cancellation()
    .with_resume_adapter(resume::ADAPTER);
