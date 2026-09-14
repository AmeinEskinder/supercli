use crate::resume::{
    has_resume_flag, id_in_command, join, quoted, strip_resume_flags, tokenize, unquote,
    with_flag, ResumeAdapter,
};
use std::path::{Component, Path};

const RESUME_FLAGS: &[(&str, bool)] = &[
    ("-c", false),
    ("--continue", false),
    ("-r", true),
    ("--resume", true),
    ("--session", true),
];
const ID_FLAGS: &[&str] = &["--session", "--resume", "-r"];

fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    let has_resume_marker = has_resume_flag(&tokens, RESUME_FLAGS);
    let id = provider_session_id
        .map(str::to_string)
        .filter(|id| !id.is_empty())
        .or_else(|| id_in_command(&tokens, ID_FLAGS));
    let stripped = strip_resume_flags(tokens, RESUME_FLAGS);
    match id {
        Some(id) => join(with_flag(stripped, &["--session", &quoted(&id)])),
        None if has_resume_marker => command.trim().to_string(),
        // An older launch's managed `--session-dir`, when present, stays in
        // `stripped` and makes continue exact by construction. New launches
        // run `pi` as typed, so continue-last is Pi's own most-recent
        // conversation.
        None => join(with_flag(stripped, &["--continue"])),
    }
}

fn fresh(command: &str) -> String {
    join(strip_resume_flags(tokenize(command), RESUME_FLAGS))
}

fn managed_session_dir(command: &str, root: &str) -> Option<String> {
    let tokens = tokenize(command);
    let directory = tokens
        .windows(2)
        .find(|pair| pair[0] == "--session-dir")
        .map(|pair| unquote(&pair[1]))?;
    let relative = Path::new(&directory).strip_prefix(Path::new(root)).ok()?;
    let mut components = relative.components();
    let first = components.next()?;
    if !matches!(first, Component::Normal(_))
        || components.any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(directory)
}

pub(super) const ADAPTER: ResumeAdapter =
    ResumeAdapter::new(resumed, fresh).with_managed_session_dir(managed_session_dir);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_pinned_storage_survives_resume() {
        let pinned = "pi --yolo --session-dir '/root/.unpeel/pi-sessions/s1'";
        assert_eq!(
            managed_session_dir(pinned, "/root/.unpeel"),
            Some("/root/.unpeel/pi-sessions/s1".to_string())
        );
        assert_eq!(
            resumed(pinned, None),
            "pi --yolo --session-dir '/root/.unpeel/pi-sessions/s1' --continue"
        );
        assert_eq!(resumed("pi --yolo", None), "pi --yolo --continue");
    }

    #[test]
    fn managed_storage_rejects_path_traversal() {
        assert_eq!(
            managed_session_dir(
                "pi --session-dir '/root/.unpeel/pi-sessions/../escape'",
                "/root/.unpeel/pi-sessions"
            ),
            None
        );
    }

    #[test]
    fn legacy_resume_markers_are_removed_and_never_re_pinned() {
        assert_eq!(
            resumed("pi -r old -c --yolo", Some("new")),
            "pi --yolo --session 'new'"
        );
        assert_eq!(
            fresh("pi -c --resume=old --session stale --yolo"),
            "pi --yolo"
        );
        assert_eq!(
            resumed("pi --resume latest --yolo", None),
            "pi --resume latest --yolo"
        );
        assert_eq!(
            managed_session_dir("pi --session-dir=/tmp/custom", "/root/.unpeel"),
            None
        );
    }
}
