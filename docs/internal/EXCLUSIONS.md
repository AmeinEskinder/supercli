# supercli-main-v2 exclusion list

When assembling the `supercli-main-v2` tree from `supercli-next`, the following
paths MUST NOT be carried over. They are historical or transient working notes,
not part of the product.

| Path | Reason |
|------|--------|
| `docs/internal/buildlog.md` | Internal build journal (agent working log), not user documentation. |
| `handoff.md` | Transient agent handoff notes. |
| `phases/` | Historical phase working notes from the pre-rename tree. |
| `pr-draft.md` | Draft PR text, not a deliverable. |

`scripts/fresh-clone-verify.sh` deletes these from the fresh clone and then
asserts none of them exist; the verify run fails if any exclusion is present.
