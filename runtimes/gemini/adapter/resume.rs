use crate::resume::{
    has_resume_flag, id_in_command, join, quoted, strip_resume_flags, tokenize, with_flag,
    ResumeAdapter,
};

const RESUME_FLAGS: &[(&str, bool)] = &[("-r", true), ("--resume", true), ("--session-id", true)];
const ID_FLAGS: &[&str] = &["--session-id", "--resume", "-r"];

fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    let has_resume_marker = has_resume_flag(&tokens, RESUME_FLAGS);
    let id = provider_session_id
        .map(str::to_string)
        .filter(|id| !id.is_empty())
        .or_else(|| id_in_command(&tokens, ID_FLAGS));
    let stripped = strip_resume_flags(tokens, RESUME_FLAGS);
    match id {
        Some(id) => join(with_flag(stripped, &["--resume", &quoted(&id)])),
        None if has_resume_marker => command.trim().to_string(),
        None => join(with_flag(stripped, &["--resume", "latest"])),
    }
}

fn fresh(command: &str) -> String {
    join(strip_resume_flags(tokenize(command), RESUME_FLAGS))
}

pub(super) const ADAPTER: ResumeAdapter = ResumeAdapter::new(resumed, fresh);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_latest_forms_replace_existing_markers() {
        assert_eq!(
            resumed("gemini --yolo", None),
            "gemini --yolo --resume latest"
        );
        assert_eq!(
            resumed("gemini --yolo --resume old", Some("new")),
            "gemini --yolo --resume 'new'"
        );
        assert_eq!(
            resumed("gemini -r old --yolo", Some("new")),
            "gemini --yolo --resume 'new'"
        );
        assert_eq!(fresh("gemini -r=old --yolo"), "gemini --yolo");
        // Without a hook-captured id an existing marker is preserved (in its
        // normalized spelling); nothing mints a new one.
        assert_eq!(resumed("gemini -r old", None), "gemini --resume 'old'");
        assert_eq!(resumed("gemini --resume=latest", None), "gemini --resume=latest");
    }
}
