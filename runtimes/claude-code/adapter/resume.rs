use crate::resume::{
    has_resume_flag, id_in_command, join, quoted, strip_resume_flags, tokenize,
    uuid_flag_value, with_flag, ResumeAdapter,
};

const RESUME_FLAGS: &[(&str, bool)] = &[
    ("-r", true),
    ("--resume", true),
    ("-c", false),
    ("--continue", false),
    ("--from-pr", true),
    ("--session-id", true),
];
const ID_FLAGS: &[&str] = &["--session-id", "--resume", "-r"];

fn conversation_exists_on_disk(id: &str) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let projects = home.join(".claude").join("projects");
    let Ok(entries) = std::fs::read_dir(projects) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| entry.path().join(format!("{id}.jsonl")).exists())
}

fn resumed(command: &str, provider_session_id: Option<&str>) -> String {
    let tokens = tokenize(command);
    let has_resume_marker = has_resume_flag(&tokens, RESUME_FLAGS);
    let reserved_id = id_in_command(&tokens, &["--session-id"]);
    let id = provider_session_id
        .map(str::to_string)
        .filter(|id| !id.is_empty())
        .or_else(|| id_in_command(&tokens, ID_FLAGS));
    let stripped = strip_resume_flags(tokens, RESUME_FLAGS);
    match id {
        // A user-reserved `--session-id` whose conversation never hit disk
        // (no prompt sent yet) must relaunch as-is: `--resume` would
        // hard-fail.
        Some(ref id)
            if reserved_id.as_deref() == Some(id.as_str()) && !conversation_exists_on_disk(id) =>
        {
            command.to_string()
        }
        Some(id) => join(with_flag(stripped, &["--resume", &quoted(&id)])),
        None if has_resume_marker => command.trim().to_string(),
        None => join(with_flag(stripped, &["--continue"])),
    }
}

fn fresh(command: &str) -> String {
    join(strip_resume_flags(tokenize(command), RESUME_FLAGS))
}

fn failure_markers(command: &str) -> Option<Vec<String>> {
    let id = uuid_flag_value(command.trim(), &["-r", "--resume"])?;
    Some(vec![
        "No conversation found with session ID".to_string(),
        id,
    ])
}

pub(super) const ADAPTER: ResumeAdapter =
    ResumeAdapter::new(resumed, fresh).with_failure_markers(failure_markers);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unused_reserved_id_does_not_become_a_failing_resume() {
        let command = "claude --dangerously-skip-permissions --session-id 'abc-123'";
        assert_eq!(resumed(command, None), command);
        assert_eq!(fresh(command), "claude --dangerously-skip-permissions");

        // A hook-captured id is not a launch reservation. Even when its
        // transcript has gone missing, preserve the exact resume attempt so
        // the Host-published failure markers can offer Start fresh.
        assert_eq!(
            resumed("claude --model opus", Some("missing-hook-id")),
            "claude --model opus --resume 'missing-hook-id'"
        );
    }

    #[test]
    fn continue_is_the_fallback_when_no_hook_captured_an_id() {
        assert_eq!(
            resumed("claude --dangerously-skip-permissions", None),
            "claude --dangerously-skip-permissions --continue"
        );
        assert_eq!(
            resumed("claude", Some("hook-captured")),
            "claude --resume 'hook-captured'"
        );
    }

    #[test]
    fn legacy_short_and_creation_markers_remain_resume_aware() {
        assert_eq!(
            resumed("claude -r stale -c --model opus", Some("current")),
            "claude --model opus --resume 'current'"
        );
        assert_eq!(
            fresh("claude -r stale -c --from-pr 42 --session-id=reserved --model opus"),
            "claude --model opus"
        );
        assert_eq!(
            resumed("claude --from-pr 42 --model opus", None),
            "claude --from-pr 42 --model opus"
        );
    }
}
