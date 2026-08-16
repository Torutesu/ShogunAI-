# Unpushed changes

- Option-key drafts now adapt tone and wording to the active app and screen context.
- Inline context refreshes before generation and treats captured screen content as untrusted prompt data.
- Stale inline context is invalidated before it can affect later generations.
- Option-context code is consistently formatted.
- Completed option-context work is recorded in project documentation.
- Inline rewriting only changes the focused text target.
- Visual Recall retention is user-configurable, with storage estimates for each preset.
- Scribe mode supports fast double-Option activation while notch activation stays tightly scoped.
- Scribe editing state now completes cleanly, restores focus, and closes correctly.
- Scribe can compose into empty editable fields.
- Scribe preserves source text while generation runs and clears completed edit requests safely.
- Voice dictation now inserts at the captured caret without selecting, replacing, or rewriting existing text.
