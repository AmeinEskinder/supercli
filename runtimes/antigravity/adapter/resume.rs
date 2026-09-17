use crate::resume::{
    has_resume_flag, join, quoted, strip_resume_flags, tokenize, with_flag, ResumeAdapter,
};

// From the `agy` reference (2026-09): `--continue` / `-c` extends the most
// recent conversation, `--conversation <id>` resumes a specific one, and the
// `/resume` slash command opens the picker inside a session.
const RESUME_FLAGS: &[(&str, bool)] = &[
    ("-c", false),
    ("--continue", false),
    ("--conversation", true),
];

// Antigravity has no hooks, so a provider conversation id is normally
// unknown and resume falls back to the documented continue-last. A command
// that already carries a resume marker is exact by construction.
fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    if has_resume_flag(&tokens, RESUME_FLAGS) {
        return command.trim().to_string();
    }
    match provider_session_id.filter(|id| !id.is_empty()) {
        Some(id) => join(with_flag(tokens, &["--conversation", &quoted(id)])),
        None => join(with_flag(tokens, &["--continue"])),
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
    fn plain_launch_resumes_via_documented_continue_last() {
        assert_eq!(resumed("agy", None), "agy --continue");
        assert_eq!(
            resumed("agy --dangerously-skip-permissions", None),
            "agy --dangerously-skip-permissions --continue"
        );
        assert_eq!(resumed("agy", Some("conv-1")), "agy --conversation 'conv-1'");
    }

    #[test]
    fn existing_resume_markers_stay_exact() {
        for command in ["agy -c", "agy --continue", "agy --conversation conv-1"] {
            assert_eq!(resumed(command, None), command, "must not double-resume");
            assert_eq!(resumed(command, Some("other")), command);
        }
    }

    #[test]
    fn fresh_strips_every_resume_form() {
        assert_eq!(fresh("agy -c"), "agy");
        assert_eq!(fresh("agy --continue --model gemini-3-pro"), "agy --model gemini-3-pro");
        assert_eq!(fresh("agy --conversation conv-1"), "agy");
        // A bare `--conversation` must not eat an unrelated following flag.
        assert_eq!(fresh("agy --conversation --model x"), "agy --model x");
    }
}
