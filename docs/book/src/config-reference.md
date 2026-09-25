# Config reference

Generated from the typed config schema (`unpeel config reference`).
Every setting has a documented default applied by its reader;
missing keys are never an issue. Unknown keys produce warnings;
invalid values produce errors (exit 2 from `unpeel config check`).

## `experimental_features.sessions_mcp`

- **Type:** boolean
- **Allowed:** true or false

## `experimental_features.browser_mcp`

- **Type:** boolean
- **Allowed:** true or false

## `experimental_features.computer_use`

- **Type:** boolean
- **Allowed:** true or false

## `browser_default_access`

- **Type:** string, one of: on, ask, off
- **Allowed:** on, ask, or off

## `mcp_nonchild_write_access`

- **Type:** string, one of: ask, allow, deny
- **Allowed:** ask, allow, or deny

## `computer_default_access`

- **Type:** string, one of: ask, allow, off
- **Allowed:** ask, allow, or off

## `computer_access`

- **Type:** string, one of: ask, allow, off
- **Allowed:** ask, allow, or off

## `mcp_worktree_access`

- **Type:** boolean
- **Allowed:** true or false

## `mcp_auto_add_browser_screenshots`

- **Type:** boolean
- **Allowed:** true or false

## `auto_stop_archive_minutes`

- **Type:** integer, one of: 0, 30, 60, 120, 240, 480, 1440
- **Allowed:** 0, 30, 60, 120, 240, 480, or 1440 (0 = off)

## `sidebar_stopped_limit`

- **Type:** integer, one of: 0, 3, 5, 10, 15, 25
- **Allowed:** 0, 3, 5, 10, 15, or 25

## `theme`

- **Type:** string, one of: system, light, dark
- **Allowed:** system, light, or dark

