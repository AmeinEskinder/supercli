//! Q6 — Static test: fail on hardcoded user-facing strings.
//!
//! Scans the `unpeel-ui` source for string literals that look like
//! user-facing text in RSX. Any literal that is not routed through the
//! i18n catalog (`t("...")`) must be on the `ALLOWLIST` below — a
//! hardcoded string added without an allowlist entry fails the test.
//!
//! The allowlist is the migration backlog: strings that predate the i18n
//! scaffolding. New UI text must use `t()` from day one.

use std::collections::HashSet;
use std::path::PathBuf;

/// String literals that predate the i18n scaffolding. Each entry is
/// `filename:line:literal`. Do not add new entries for new UI text —
/// route it through `t()` instead.
const ALLOWLIST: &[&str] = &[
    // Migration backlog: hardcoded strings that predate the i18n
    // scaffolding (Q6). New UI text must use t() — do not add here.
"activity.rs:26:\"Using a web browser\"",
    "activity.rs:133:\"Needs approval\"",
    "annotation.rs:326:\"Cancel\"",
    "annotation.rs:330:\"Undo\"",
    "annotation.rs:334:\"Clear\"",
    "annotation.rs:340:\"Done\"",
    "annotation.rs:402:\"swatch selected\"",
    "annotation.rs:404:\"Color\"",
    "annotation.rs:446:\"Cancel\"",
    "annotation.rs:450:\"Undo\"",
    "annotation.rs:454:\"Clear\"",
    "annotation.rs:460:\"Done\"",
    "annotation.rs:527:\"swatch selected\"",
    "annotation.rs:529:\"Color\"",
    "annotation.rs:580:\"Cancel\"",
    "annotation.rs:584:\"Reset\"",
    "annotation.rs:590:\"Done\"",
    "app_lock.rs:121:\"Biometric unlock is not available in this build\"",
    "app_lock.rs:122:\"Authentication was cancelled\"",
    "app_lock.rs:123:\"Authentication was interrupted\"",
    "app_lock.rs:133:\"Face ID\"",
    "app_lock.rs:134:\"Touch ID\"",
    "app_lock.rs:135:\"Optic ID\"",
    "app_lock.rs:138:\"Passcode\"",
    "app_lock.rs:140:\"Face ID\"",
    "app_lock.rs:237:\"Authentication failed\"",
    "app_lock.rs:357:\"Unlock Unpeel\"",
    "app_lock.rs:415:\"M8 10V7a4 4 0 0 1 8 0v3\"",
    "app_lock.rs:418:\"Unpeel is locked\"",
    "banner.rs:6:\"Remote content\"",
    "banner.rs:38:\"Remote content\"",
    "clickable_path.rs:275:\"HOME\"",
    "command_palette.rs:4:\"jump to anything\"",
    "command_palette.rs:67:\"Session\"",
    "command_palette.rs:68:\"Project\"",
    "command_palette.rs:69:\"Launch\"",
    "command_palette.rs:70:\"Command\"",
    "command_palette.rs:165:\"Type a command or search sessions…\"",
    "components.rs:34:\"dot ok\"",
    "components.rs:34:\"dot bad\"",
    "components.rs:36:\"No host\"",
    "components.rs:44:\"Refresh\"",
    "components.rs:135:\"Restore\"",
    "components.rs:145:\"Archive\"",
    "components.rs:171:\"Hide archived\"",
    "components.rs:196:\"bubble mine\"",
    "components.rs:196:\"bubble theirs\"",
    "components.rs:231:\"Approval requested\"",
    "composer.rs:97:\"Send\"",
    "composer.rs:97:\"Stop\"",
    "composer.rs:166:\"Stop\"",
    "composer.rs:166:\"Send\"",
    "composer.rs:200:\"Message the agent\"",
    "composer.rs:222:\"Message the agent… (Enter to send)\"",
    "composer.rs:227:\"Send\"",
    "composer.rs:227:\"Stop\"",
    "composer.rs:238:\"Queue\"",
    "composer.rs:241:\"Queue\"",
    "composer.rs:246:\"Queue\"",
    "composer.rs:272:\"Save\"",
    "composer.rs:277:\"Cancel\"",
    "composer.rs:292:\"Edit\"",
    "composer.rs:302:\"Remove\"",
    "dictation.rs:113:\" and \"",
    "dictation.rs:194:\"Dictation was interrupted\"",
    "dictation.rs:543:\"Speech recognition isn't available here\"",
    "dictation.rs:548:\"Microphone unavailable — check permission\"",
    "dictation.rs:550:\"Speech recognizer failed\"",
    "dictation.rs:655:\"Stop dictation\"",
    "dictation.rs:655:\"Start dictation\"",
    "dictation.rs:656:\"Stop dictation\"",
    "dictation.rs:656:\"Dictate\"",
    "dictation.rs:672:\"Stop\"",
    "dictation.rs:674:\"Paste\"",
    "dictation.rs:676:\"Dismiss dictation\"",
    "discovery.rs:55:\"Unpeel Host\"",
    "discovery.rs:108:\"discovery timed out\"",
    "discovery.rs:353:\"Nearby Hosts\"",
    "discovery.rs:354:\"Close\"",
    "discovery.rs:357:\"Searching the local network…\"",
    "find.rs:21:\"No results\"",
    "find.rs:21:\"3 of 17\"",
    "find.rs:85:\"No results\"",
    "find.rs:280:\"Find\"",
    "find.rs:283:\"Find in terminal\"",
    "find.rs:306:\"Previous match\"",
    "find.rs:312:\"Next match\"",
    "find.rs:318:\"Close find\"",
    "gallery.rs:73:\"Browser Gallery\"",
    "gallery.rs:78:\"Ask the Host to capture a screenshot into this gallery\"",
    "gallery.rs:79:\"Screenshot\"",
    "gallery.rs:81:\"Upload\"",
    "gallery.rs:82:\"Refresh\"",
    "gallery.rs:109:\"Delete\"",
    "gallery.rs:153:\"‹ Gallery\"",
    "gallery.rs:162:\"Share\"",
    "gallery.rs:170:\"Add to message\"",
    "gallery.rs:181:\"Confirm delete\"",
    "gallery.rs:186:\"Delete\"",
    "gallery.rs:238:\"Arrows\"",
    "gallery.rs:239:\"Draw\"",
    "gallery.rs:240:\"Crop\"",
    "notifier.rs:33:\"Mac notifies when the desktop isn't already showing the session\"",
    "notifier.rs:74:\"Needs your input\"",
    "notifier.rs:136:\"Notification\"",
    "organize.rs:103:\"Resume Session?\"",
    "organize.rs:104:\"Resume Agent?\"",
    "organize.rs:105:\"Stop Session?\"",
    "organize.rs:106:\"Stop and Archive Session?\"",
    "organize.rs:109:\"Remove Session?\"",
    "organize.rs:111:\"Remove From List?\"",
    "organize.rs:138:\"Move to\"",
    "organize.rs:217:\"Organize session\"",
    "organize.rs:218:\"Done\"",
    "organize.rs:221:\"Session name\"",
    "organize.rs:233:\"Pinned\"",
    "organize.rs:242:\"Notify when done\"",
    "organize.rs:245:\"Save\"",
    "organize.rs:247:\"Move to project\"",
    "organize.rs:254:\"Keep current location\"",
    "organize.rs:263:\"Archive library\"",
    "organize.rs:273:\"Tap again to confirm resume\"",
    "organize.rs:275:\"Resume session\"",
    "organize.rs:284:\"Tap again to confirm resume agent\"",
    "organize.rs:286:\"Resume agent\"",
    "organize.rs:297:\"Tap again to confirm stop\"",
    "organize.rs:299:\"Stop session\"",
    "organize.rs:308:\"Restore from archive\"",
    "organize.rs:315:\"Tap again to confirm archive\"",
    "organize.rs:317:\"Archive session\"",
    "organize.rs:326:\"Tap again to confirm remove\"",
    "organize.rs:328:\"Remove session\"",
    "organize.rs:330:\"Remove from list\"",
    "organize.rs:365:\"Done\"",
    "organize.rs:394:\"Restore\"",
    "organize.rs:403:\"Restore & Resume\"",
    "organize.rs:503:\"Done\"",
    "organize.rs:507:\"Group name\"",
    "organize.rs:519:\"Sort sessions by date\"",
    "organize.rs:522:\"Folder color\"",
    "organize.rs:540:\"None\"",
    "organize.rs:544:\"Save\"",
    "organize.rs:548:\"Archive library\"",
    "pairing.rs:28:\"Switch\"",
    "pairing.rs:29:\"Connect\"",
    "pairing.rs:43:\"Pair with an Unpeel Host\"",
    "pairing.rs:54:\"Pairing code from the Host\"",
    "pairing.rs:66:\"Pair\"",
    "pairing.rs:73:\"Previously paired\"",
    "pairing.rs:86:\"● viewing\"",
    "pairing.rs:95:\"Viewing\"",
    "pairing.rs:95:\"Switch\"",
    "pairing.rs:95:\"Connect\"",
    "pairing.rs:100:\"Forget\"",
    "presence.rs:12:\"Name (id)\"",
    "presence.rs:111:\"Name (id)\"",
    "presence.rs:124:\"Remote viewer\"",
    "presence.rs:127:\"Name (id)\"",
    "presence.rs:183:\"<name> connected\"",
    "presence.rs:229:\"no viewers\"",
    "presets.rs:1:\"New session\"",
    "presets.rs:42:\"New session\"",
    "presets.rs:45:\"New session\"",
    "presets.rs:97:\"New session\"",
    "presets.rs:104:\"Close presets\"",
    "project_tree.rs:89:\"Move to ▸\"",
    "push.rs:27:\"Not requested\"",
    "push.rs:29:\"Waiting for notification permission…\"",
    "push.rs:32:\"Notifications are denied in system Settings\"",
    "push.rs:34:\"Waiting for a push device token…\"",
    "push.rs:37:\"Ready (production)\"",
    "push.rs:39:\"Ready (sandbox)\"",
    "push.rs:57:\"Notifications are off\"",
    "push.rs:58:\"Notifications aren't working\"",
    "qr.rs:160:\"camera unavailable\"",
    "qr.rs:200:\"Requesting camera access…\"",
    "qr.rs:204:\"Camera access denied\"",
    "qr.rs:212:\"Scanner paused\"",
    "qr.rs:215:\"Point the camera at the pairing QR code\"",
    "settings.rs:67:\"Git worktrees\"",
    "settings.rs:76:\"Sessions use\"",
    "settings.rs:87:\"Workspaces\"",
    "settings.rs:96:\"Browser use\"",
    "settings.rs:105:\"Remote workspaces\"",
    "settings.rs:185:\"Agents\"",
    "settings.rs:188:\"Allow agent MCP access\"",
    "settings.rs:189:\"Ask before writes\"",
    "settings.rs:194:\"Browser\"",
    "settings.rs:197:\"Allow browser use\"",
    "settings.rs:198:\"Persist logins\"",
    "settings.rs:203:\"Sessions use\"",
    "settings.rs:206:\"Read other sessions\"",
    "settings.rs:207:\"Ask before writing to another session\"",
    "settings.rs:331:\"Features\"",
    "settings.rs:332:\"Agents\"",
    "settings.rs:333:\"Browser\"",
    "settings.rs:334:\"Sessions\"",
    "settings.rs:335:\"Plugins\"",
    "settings.rs:336:\"Workspaces\"",
    "settings.rs:337:\"Worktrees\"",
    "settings.rs:338:\"Developer\"",
    "settings.rs:389:\"Experimental\"",
    "ssh.rs:28:\"Standard SSH\"",
    "ssh.rs:29:\"Interactive shell\"",
    "ssh.rs:381:\"Forget\"",
    "terminal.rs:234:\"inherit the terminal default\"",
    "terminal.rs:467:\"Copy All\"",
    "terminal.rs:593:\"Select text\"",
    "terminal.rs:597:\"Copy All\"",
    "terminal.rs:602:\"Close\"",
    "terminal.rs:736:\"Select\"",
    "workspaces.rs:78:\"VS Code\"",
    "workspaces.rs:79:\"Cursor\"",
    "workspaces.rs:80:\"Zed\"",
    "workspaces.rs:81:\"IntelliJ\"",
    "workspaces.rs:82:\"WebStorm\"",
    "workspaces.rs:83:\"GitHub Desktop\"",
    "workspaces.rs:84:\"Fork\"",
    "workspaces.rs:85:\"Tower\"",
    "workspaces.rs:86:\"Sourcetree\"",
    "workspaces.rs:87:\"GitKraken\"",
    "workspaces.rs:88:\"Sublime Merge\"",
    "workspaces.rs:89:\"Finder\"",
    "workspaces.rs:90:\"Terminal\"",
    "workspaces.rs:92:\"Ghostty\"",
    "workspaces.rs:93:\"Warp\"",
    "workspaces.rs:94:\"WezTerm\"",
    "workspaces.rs:96:\"Alacritty\"",
    "workspaces.rs:97:\"Tabby\"",
    "workspaces.rs:98:\"Hyper\"",
    "workspaces.rs:99:\"Rio\"",
    "workspaces.rs:100:\"Wave\"",
    "workspaces.rs:101:\"Xcode\"",
    "workspaces.rs:203:\"PATH\"",
    "workspaces.rs:329:\"Git worktrees\"",
    "workspaces.rs:331:\"Require a clean tree before creating a worktree\"",
    "workspaces.rs:350:\"uncommitted changes\"",
    "workspaces.rs:358:\"Remove\"",
];

/// Heuristic: does this string literal look like user-facing text?
/// We flag literals with letters and spaces/punctuation that are longer
/// than a couple characters, excluding obvious technical strings.
fn looks_like_ui_text(literal: &str) -> bool {
    let inner = literal.trim_matches('"');
    if inner.len() < 3 {
        return false;
    }
    // Must contain at least one letter.
    if !inner.chars().any(|c| c.is_alphabetic()) {
        return false;
    }
    // Exclude technical strings: paths, URLs, IDs, class names, formats.
    let technical = inner.contains('/')
        || inner.contains('\\')
        || inner.contains('.')
        || inner.contains('_')
        || inner.contains('-')
        || inner.contains(':')
        || inner.contains('{')
        || inner.contains('%');
    if technical {
        return false;
    }
    // User-facing text usually has spaces or is a capitalized word.
    inner.contains(' ')
        || (inner.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            && inner.chars().all(|c| c.is_alphabetic()))
}

fn find_ui_src() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("src");
    dir
}

/// Remove `#[cfg(test)] mod tests { ... }` blocks so test fixtures
/// don't count as hardcoded UI strings.
fn strip_test_modules(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].contains("#[cfg(test)]")
            && i + 1 < lines.len()
            && lines[i + 1].contains("mod tests")
        {
            // Skip to the matching closing brace.
            let mut depth = 0i32;
            let mut j = i + 1;
            while j < lines.len() {
                depth += lines[j].matches('{').count() as i32
                    - lines[j].matches('}').count() as i32;
                if depth == 0 && lines[j].contains('{') {
                    break;
                }
                j += 1;
            }
            j += 1;
            while j < lines.len() && depth > 0 {
                depth += lines[j].matches('{').count() as i32
                    - lines[j].matches('}').count() as i32;
                j += 1;
            }
            i = j;
        } else {
            out.push(lines[i]);
            i += 1;
        }
    }
    out.join("\n")
}

#[test]
fn no_hardcoded_ui_strings() {
    let src_dir = find_ui_src();
    let mut violations = Vec::new();
    let allowlist: HashSet<&str> = ALLOWLIST.iter().copied().collect();

    let mut files: Vec<PathBuf> = std::fs::read_dir(&src_dir)
        .expect("read src dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "rs").unwrap_or(false))
        .collect();
    files.sort();

    for path in files {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        // Skip the i18n module itself (it defines the catalog).
        if filename == "i18n.rs" {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("read file");
        let content = strip_test_modules(&content);
        for (idx, line) in content.lines().enumerate() {
            let lineno = idx + 1;
            // Skip lines that already use the i18n function.
            if line.contains("t(\"") || line.contains("t_with_locale") {
                continue;
            }
            // Find string literals on this line.
            let mut in_str = false;
            let mut start = 0;
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
                    if !in_str {
                        in_str = true;
                        start = i;
                    } else {
                        in_str = false;
                        let literal: String = chars[start..=i].iter().collect();
                        if looks_like_ui_text(&literal) {
                            let key = format!("{}:{}:{}", filename, lineno, literal);
                            if !allowlist.contains(key.as_str()) {
                                violations.push(key);
                            }
                        }
                    }
                }
                i += 1;
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Hardcoded user-facing strings found (route through i18n::t() or add to ALLOWLIST with justification):\n{}",
        violations.join("\n")
    );
}

#[test]
fn allowlist_entries_are_still_valid() {
    // Every allowlist entry must match an actual hardcoded string.
    // If the string was migrated to t(), remove it from the allowlist.
    let src_dir = find_ui_src();
    let mut found = HashSet::new();

    for path in std::fs::read_dir(&src_dir)
        .expect("read src dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "rs").unwrap_or(false))
    {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        if filename == "i18n.rs" {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("read file");
        let content = strip_test_modules(&content);
        for (idx, line) in content.lines().enumerate() {
            let lineno = idx + 1;
            if line.contains("t(\"") {
                continue;
            }
            // Simple check: does the line contain the literal?
            for entry in ALLOWLIST {
                let parts: Vec<&str> = entry.splitn(3, ':').collect();
                if parts.len() == 3
                    && parts[0] == filename
                    && parts[1] == lineno.to_string()
                    && line.contains(parts[2].trim_matches('"'))
                {
                    found.insert(*entry);
                }
            }
        }
    }

    let missing: Vec<&&str> = ALLOWLIST.iter().filter(|e| !found.contains(*e)).collect();
    assert!(
        missing.is_empty(),
        "Allowlist entries no longer match (string was migrated? remove from ALLOWLIST):\n{:?}",
        missing
    );
}
