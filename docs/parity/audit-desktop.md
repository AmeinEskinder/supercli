# [DESKTOP] Parity Audit (70 items)

**Branch:** `track-b-audit-desktop` (from `track-b-parity-docs@de51bb2`)
**Scaffold reference:** `track-b-gpuidart-desktop@bbd8b70` (40 screens stubbed, NOT merged)
**Date:** 2026-09-26

**Method:** Each item checked against (1) the gpuidart scaffold on `bbd8b70` for UI structure,
(2) the original gpuidart app on `de51bb2` for working features with tests/screenshots.
Per Amein's rule: `done` requires a rendered screenshot + a test exercising the behaviour.
A 4-line re-export is `missing`, not `partial`. A scaffold with UI structure but no
behaviour is `partial`.

**Summary:** done 1 · partial 27 · missing 42 · improved 0

| # | Tag | Status | supercli Path | Test | Notes |
|---|-----|--------|---------------|------|-------|
| 150 | [DESKTOP] | partial | clients/supercli-app/lib/screens/rootview.dart, chrome.dart (scaffold bbd8b70) | screens_test.dart (RootView layout) | Scaffold: sidebar+content layout only. No vibrancy, no custom titlebar, no ⌘B toggle. |
| 151 | [DESKTOP] | partial | clients/supercli-app/lib/screens/sidebarview.dart, projectsidebarview.dart (scaffold bbd8b70) | screens_test.dart (SidebarView) | Scaffold: sections, filter input, tables. No pins, groups, worktree folders, attention dots, or busy spinners. |
| 152 | [DESKTOP] | missing | clients/supercli-app/lib/screens/sidebarsessiondrag.dart (4-line re-export, bbd8b70) | - | Re-export only. No drag-and-drop (P0-13). |
| 153 | [DESKTOP] | missing | - | - | No session context menu in scaffold. |
| 154 | [DESKTOP] | missing | - | - | No project context menu in scaffold. |
| 155 | [DESKTOP] | missing | - | - | No folder color palette in scaffold. |
| 156 | [DESKTOP] | partial | clients/supercli-app/lib/screens/archivedsessionsview.dart (34 lines, bbd8b70) | - | Scaffold: basic archived list UI. No search, no Restore & Resume behaviour. |
| 157 | [DESKTOP] | partial | clients/supercli-app/lib/screens/workspaceopenmenu.dart (85 lines, bbd8b70) | - | Scaffold: workspace open menu structure. Dots/selector are 4-line re-exports; no trackpad swipe. |
| 158 | [DESKTOP] | missing | - | - | No multi-window support (P0-14). |
| 159 | [DESKTOP] | missing | - | - | No move-project-between-workspaces in scaffold. |
| 160 | [DESKTOP] | partial | clients/supercli-app/lib/screens/projectsidebarview.dart (111 lines, bbd8b70) | - | Scaffold: project sidebar structure. No pinned-session stack behaviour. |
| 161 | [DESKTOP] | missing | clients/supercli-app/lib/screens/localsitemenu.dart (4-line re-export, bbd8b70) | - | Re-export only. No globe button / local-server menu. |
| 162 | [DESKTOP] | missing | - | - | No "Open in" menu in scaffold. |
| 163 | [DESKTOP] | partial | clients/supercli-app/lib/screens/sessionlauncherview.dart (35 lines, bbd8b70) | - | Scaffold: launcher UI structure. No tool-pick behaviour. |
| 164 | [DESKTOP] | partial | clients/supercli-app/lib/screens/commandpaletteview.dart (61 lines, bbd8b70) | screens_test.dart (palette) | Scaffold: palette UI with actions. No ⌘K wiring, no real filtering. |
| 165 | [DESKTOP] | missing | - | - | No ⌃Tab MRU switcher in scaffold. |
| 166 | [DESKTOP] | missing | - | - | Actions defined but no ⌘1–9 / ⌃1–9 switching behaviour. |
| 167 | [DESKTOP] | partial | clients/supercli-app/lib/screens/recentactivityview.dart, globalactivitymenu.dart (bbd8b70) | - | Scaffold: activity UI structure. No ⇧⌘R page behaviour. |
| 168 | [DESKTOP] | partial | clients/supercli-app/lib/screens/toastcenter.dart (25 lines, bbd8b70) | - | Scaffold: toast UI structure. No notification plumbing. |
| 169 | [DESKTOP] | partial | clients/supercli-app/lib/screens/terminalpaneview.dart (54 lines, bbd8b70) | - | Scaffold renders terminal as UiText rows. No ghostty surface, no ANSI, no cursor (P0-8/P0-10). |
| 170 | [DESKTOP] | missing | - | - | No remote-host pane rendering. Same gap as 169. |
| 171 | [DESKTOP] | partial | clients/supercli-app/lib/screens/terminalpaneview.dart (Split H/V buttons, bbd8b70) | - | Buttons exist; no split-tree logic, no 8-pane limit. |
| 172 | [DESKTOP] | missing | - | - | No zoom/equalize/spatial-focus in scaffold. |
| 173 | [DESKTOP] | missing | - | - | No detach/exit-multi-pane in scaffold. |
| 174 | [DESKTOP] | missing | - | - | No pane header menu in scaffold. |
| 175 | [DESKTOP] | partial | clients/supercli-app/lib/screens/sessionlauncherview.dart (35 lines, bbd8b70) | - | See 163. Transient launcher pane UI only. |
| 176 | [DESKTOP] | missing | - | - | No pane-layout persistence in scaffold. |
| 177 | [DESKTOP] | partial | clients/supercli-app/lib/screens/terminalfindbar.dart (35 lines, bbd8b70) | - | Scaffold: find bar UI (input + Next/Prev). No search behaviour. |
| 178 | [DESKTOP] | missing | - | - | No font-size controls in scaffold. |
| 179 | [DESKTOP] | partial | clients/supercli-app/lib/screens/clickablepath.dart (23 lines, bbd8b70) | - | Scaffold: clickable path UI. No URL/OSC 8 parsing. |
| 180 | [DESKTOP] | partial | clients/supercli-app/lib/screens/clickablepath.dart (23 lines, bbd8b70) | - | See 179. No ⌘-click behaviour. |
| 181 | [DESKTOP] | missing | - | - | No file drag-and-drop (P0-13). |
| 182 | [DESKTOP] | missing | - | - | No scroll-to-bottom / exited-session bar in scaffold. |
| 183 | [DESKTOP] | missing | - | - | No restart-recommendation banner in scaffold. |
| 184 | [DESKTOP] | missing | - | - | No TUI background matching in scaffold. |
| 185 | [DESKTOP] | partial | clients/supercli-app/lib/screens/vieweravatarsview.dart (20 lines, bbd8b70) | - | Scaffold: avatar UI. No presence data, no Fit-to-desktop. |
| 186 | [DESKTOP] | done | clients/supercli-app/lib/app.dart (de51bb2) | clients/supercli-app/test/app_test.dart | Rendered screenshots: docs/internal/proofs/proof-screenshots/approve-before.png, approve-after.png, deny-before.png, deny-after.png. Tests exercise approve/deny flow. |
| 187 | [DESKTOP] | partial | clients/supercli-app/lib/screens/sessiongallerypanel.dart (50 lines, bbd8b70) | - | Scaffold: gallery panel UI. No screenshot/download/upload behaviour. |
| 188 | [DESKTOP] | missing | clients/supercli-app/lib/screens/sessiongallerymarkup.dart (6-line re-export, bbd8b70) | - | Re-export only. No markup tools. |
| 189 | [DESKTOP] | partial | clients/supercli-app/lib/screens/sessionscreenshotcapture.dart (21 lines, bbd8b70) | - | Scaffold: capture UI. No ⇧⌘S behaviour. |
| 190 | [DESKTOP] | missing | - | - | No main menu set in gpuidart app. |
| 191 | [DESKTOP] | missing | - | - | Menu-bar status item exists only in unmerged track-b-objc2-glue@a03ddb8. Nothing on this branch. |
| 192 | [DESKTOP] | missing | - | - | No menu-bar agent mode. |
| 193 | [DESKTOP] | missing | - | - | No Finder service (macOS-only, needs native glue). |
| 194 | [DESKTOP] | missing | - | - | Sparkle updater interface only in unmerged track-b-objc2-glue@a03ddb8. Nothing on this branch. |
| 195 | [DESKTOP] | missing | - | - | Notification bindings only in unmerged track-b-objc2-glue@a03ddb8. Nothing on this branch. |
| 196 | [DESKTOP] | missing | - | - | Keychain reuse only in unmerged track-b-objc2-glue@a03ddb8. Nothing on this branch. |
| 197 | [DESKTOP] | missing | - | - | Host-side concern; no desktop UI. |
| 198 | [DESKTOP] | missing | - | - | No Bonjour discovery in scaffold. |
| 199 | [DESKTOP] | missing | clients/supercli-app/lib/screens/remotefolderpicker.dart (4-line re-export, bbd8b70) | - | Re-export only. |
| 200 | [DESKTOP] | partial | clients/supercli-app/lib/screens/settingsview.dart (64 lines, bbd8b70) | - | Scaffold: settings shell UI. No scope picker behaviour. |
| 201 | [DESKTOP] | partial | clients/supercli-app/lib/screens/workspaceopenmenu.dart (85 lines, bbd8b70) | - | See 157. Settings ▸ Workspaces panel itself is a 4-line re-export. |
| 202 | [DESKTOP] | missing | - | - | No Settings ▸ Agents in scaffold. |
| 203 | [DESKTOP] | missing | clients/supercli-app/lib/screens/pluginsettingspanel.dart (4-line re-export, bbd8b70) | - | Re-export only. |
| 204 | [DESKTOP] | partial | clients/supercli-app/lib/screens/sessionsaccesssections.dart (65 lines, bbd8b70) | - | Scaffold: access sections UI. Panel wrapper is a 4-line re-export. |
| 205 | [DESKTOP] | missing | clients/supercli-app/lib/screens/browseraccesssections.dart (6-line re-export, bbd8b70) | - | Re-export only. |
| 206 | [DESKTOP] | partial | clients/supercli-app/lib/screens/settingspanels.dart (168 lines, bbd8b70) | - | Scaffold: appearance panel UI structure. No behaviour. |
| 207 | [DESKTOP] | partial | clients/supercli-app/lib/screens/hostpickerview.dart (84 lines, bbd8b70) | - | Scaffold: host picker UI (QR placeholder, P0-11). No pairing behaviour. |
| 208 | [DESKTOP] | missing | clients/supercli-app/lib/screens/remotehostworkspaceview.dart (4-line re-export, bbd8b70) | - | Re-export only. |
| 209 | [DESKTOP] | missing | clients/supercli-app/lib/screens/licensesettingspanel.dart (4-line re-export, bbd8b70) | - | Re-export only. |
| 210 | [DESKTOP] | partial | clients/supercli-app/lib/screens/settingspanels.dart (168 lines, bbd8b70) | - | See 206. Transcripts panel UI structure only. |
| 211 | [DESKTOP] | partial | clients/supercli-app/lib/screens/settingspanels.dart (168 lines, bbd8b70) | - | See 206. Notifications panel UI structure only. |
| 212 | [DESKTOP] | missing | clients/supercli-app/lib/screens/worktreessettingspanel.dart (4-line re-export, bbd8b70) | - | Re-export only. |
| 213 | [DESKTOP] | partial | clients/supercli-app/lib/screens/settingspanels.dart (168 lines, bbd8b70) | - | See 206. Features panel UI structure only. |
| 214 | [DESKTOP] | partial | clients/supercli-app/lib/screens/settingspanels.dart (168 lines, bbd8b70) | - | See 206. Advanced panel UI structure only. |
| 215 | [DESKTOP] | missing | - | - | No default-editor preference in scaffold. |
| 216 | [DESKTOP] | missing | clients/supercli-app/lib/screens/presetssettingspanel.dart (4-line re-export, bbd8b70) | - | Re-export only. |
| 217 | [DESKTOP] | missing | - | - | No remote-host scope UI parity. |
| 218 | [DESKTOP] | missing | - | - | No worktree discovery UI. |
| 219 | [DESKTOP] | missing | clients/supercli-app/lib/screens/sidebarskeleton.dart (4-line re-export, bbd8b70) | - | Re-export only. |

## Notes

- The only `done` item (186, in-pane MCP approval overlay) comes from the original
  gpuidart app on `de51bb2`, which has rendered screenshots
  (`docs/internal/proofs/proof-screenshots/approve-*.png`, `deny-*.png`) and
  behaviour tests (`clients/supercli-app/test/app_test.dart`).
- All `partial` items are scaffolds on the unmerged `track-b-gpuidart-desktop@bbd8b70`:
  UI structure exists (widgets compose, some unit tests assert tree shape) but no
  behaviour, no Host wiring, no rendered screenshots.
- Items 191, 194, 195, 196 have Rust implementations on the unmerged
  `track-b-objc2-glue@a03ddb8`, but nothing on this branch — marked `missing`
  per the branch under audit.
- The 15 four-line re-exports and 2 six-line re-exports are marked `missing`
  per Amein's rule (a re-export is not an implementation).
