/* supercli_client_ffi.h
 *
 * C ABI for the Supercli client logic (crates/supercli-client-ffi).
 *
 * This header is hand-maintained to match src/lib.rs. If you add, remove,
 * or change an exported symbol, update this file in the same commit and
 * bump SUPERCLI_FFI_ABI_VERSION.
 *
 * Memory contract: functions returning `char *` hand ownership to the
 * caller; release with supercli_string_free(). Input pointers are borrowed
 * for the call.
 *
 * Panic contract: no exported function ever unwinds. On panic it returns
 * its failure value (NULL / 0) and records a message retrievable with
 * supercli_last_error().
 *
 * Error contract: NULL input pointers and invalid UTF-8 are errors, not
 * silent empty strings. On a failure return, call supercli_last_error()
 * for the message (NULL when no error was recorded).
 */

#ifndef SUPERCLI_CLIENT_FFI_H
#define SUPERCLI_CLIENT_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ABI version. Dart checks this at load time and fails fast on mismatch. */
#define SUPERCLI_FFI_ABI_VERSION 2
uint32_t supercli_ffi_abi_version(void);

/* Last recorded error message (caller-owned, free with
 * supercli_string_free), or NULL when none. */
char *supercli_last_error(void);
/* Clear the last recorded error. */
void supercli_clear_error(void);

/* Release a string returned by this library. NULL-safe. */
void supercli_string_free(char *s);

/* JSON array of all runtime descriptors. Never NULL on success. */
char *supercli_runtime_catalog_json(void);
/* JSON object for a stable id or legacy slug, or NULL when unknown. */
char *supercli_runtime_by_id_json(const char *id);
/* Legacy slug of the runtime launching `command`, or NULL. */
char *supercli_runtime_detect_tool(const char *command);

/* 1 when `command` resolves to a quick-launchable runtime, else 0. */
uint8_t supercli_preset_tool_is_quick_launchable(const char *command);
/* Display name for a tool's legacy slug, or NULL when unknown. */
char *supercli_preset_tool_display_name(const char *legacy_slug);

/* Exponential backoff delay in ms for consecutive_failures (>= 1). */
uint64_t supercli_pool_backoff_delay_ms(uint32_t consecutive_failures);
/* Pool policy tunables as JSON. Never NULL on success. */
char *supercli_pool_policy_json(void);

/* URL-safe slug for a workspace display name. */
char *supercli_registry_slugify(const char *name);
/* `local:` order key for a workspace home path. */
char *supercli_registry_local_key(const char *home);
/* `host:` order key for a paired host id. */
char *supercli_registry_paired_key(const char *host_id);

/* Parse one presence feed; JSON {session_id: [{id, device_id,
 * display_name, last_seen_ms}]}. Malformed input -> "{}". */
char *supercli_presence_parse(const uint8_t *data, size_t len,
                              const char *source);
/* Display name for a device ("Name (id)") / ip pair. */
char *supercli_presence_display_name(const char *device, const char *ip);
/* 1 when device ("Name (id)") carries a stable device id, else 0. */
uint8_t supercli_presence_has_device_id(const char *device);

/* 1 when the drop-target map JSON accepts a drop at (row, column) at
 * now_ms, else 0. Malformed input -> 0. */
uint8_t supercli_drop_map_accepts(const uint8_t *json, size_t len,
                                  uint32_t row, uint32_t column,
                                  uint64_t now_ms);
/* Host-local path for the path-drag map at (row, column) at now_ms,
 * or NULL when unmapped/stale/malformed. */
char *supercli_path_drag_map_path_at(const uint8_t *json, size_t len,
                                     uint32_t row, uint32_t column,
                                     uint64_t now_ms);

/* Pane layout (JSON snapshots). All take a snapshot JSON string; mutation
 * results return a JSON object with the updated "snapshot" plus relevant
 * ids ("pane_id", "group_id"). NULL on failure; check
 * supercli_last_error(). */

/* Create a single-pane layout for session_id with the requested pane id
 * (used when UUID-shaped; otherwise a generated stable id is returned in
 * "pane_id"). Returns {snapshot, pane_id, group_id}. */
char *supercli_pane_layout_single(const char *session_id, const char *pane_id);
/* Insert session_id at edge of target pane. Returns
 * {snapshot, pane_id, group_id}. */
char *supercli_pane_layout_insert(const char *snapshot, const char *session_id,
                                  const char *target_pane_id, const char *edge);
/* Close a pane. Returns {snapshot, pane_id, group_id} or NULL when it would
 * leave zero panes (last pane cannot be closed). */
char *supercli_pane_layout_close(const char *snapshot, const char *pane_id);
/* Set the split ratio of the split containing pane_id (0.1..0.9, clamped).
 * Returns {snapshot, group_id}. */
char *supercli_pane_layout_resize(const char *snapshot, const char *group_id,
                                  const char *pane_id, double ratio);
/* Equalize all split ratios in the group. Returns {snapshot, group_id}. */
char *supercli_pane_layout_equalize(const char *snapshot, const char *group_id);
/* Swap two panes. Returns {snapshot, group_id}. */
char *supercli_pane_layout_swap(const char *snapshot, const char *pane_a,
                                const char *pane_b);
/* Pane id adjacent to pane_id toward edge, or JSON null when none. */
char *supercli_pane_layout_neighbor(const char *snapshot, const char *pane_id,
                                    const char *edge);
/* Drop sessions not in eligible_ids (JSON array). A lone surviving session
 * is re-homed as a single-pane group; zero eligible sessions empties the
 * groups. Returns {snapshot, pane_id, group_id}. */
char *supercli_pane_layout_reconcile(const char *snapshot,
                                     const char *eligible_ids_json);
/* Per-leaf geometry for hit-testing: JSON array of
 * {pane_id, x, y, width, height} over (x, y, width, height). */
char *supercli_pane_layout_leaf_boxes(const char *snapshot, const char *group_id,
                                      double x, double y, double width,
                                      double height);

#ifdef __cplusplus
}
#endif

#endif /* SUPERCLI_CLIENT_FFI_H */
