# Filer implementation handover

## Objective

Bring the proven ChainFlow workflow—collecting items across folders with the left hand while pointing with the mouse—into the Tauri Filer, then connect that collection to the multi-window workbench.

The feature is intentionally an evolution rather than a direct copy:

- `Alt+Click` collects or removes a file/folder from a tab-local tray.
- The tray is visible in the normal sidebar and survives session restore.
- Collected items can be copied or moved to the active pane through the existing transfer/undo pipeline.
- The workbench can send the tray to another Filer window.
- Existing selection, drag/drop, shortcuts, and Space quick preview must keep working.

## Working rules

- Keep the Slint implementation untouched.
- Implement in small phases with a verified Git commit for each meaningful milestone.
- Do not replace `Space` quick preview with a shortcut layer.
- Prefer existing transfer, progress, cancellation, collision handling, and undo paths.
- Never remove a tray item merely because a transfer was attempted; update it from the actual result.

## Planned phases

- [x] Phase 1 — tab-local tray state, `Alt+Click`, marked-row UI, sidebar tray, session persistence
- [x] Phase 2 — copy/move tray contents to the active pane using the existing transfer pipeline
- [x] Phase 3 — expose the active tray in Workbench and support multi-item drop to a window
- [x] Phase 4 — initial left-hand/hover interaction set; further refinement follows real use
- [x] Phase 5 — partial tray selection, partial transfer, and per-item Workbench drag
- [x] Phase 6 — integrate hover/left-hand controls into configurable shortcut infrastructure
- [x] Phase 7 — clarify tray destination and add selected-only removal
- [x] Phase 8 — asynchronous Workbench transfer with progress, cancellation, and race-safe completion
- [x] Lifecycle fix — exit the process when the last normal Filer window closes

## Current status

- Repository inspected; starting point is clean `master` at `3cdcfbf`.
- Existing frontend tests: 46 passing.
- Existing Rust tests: 70 passing, 1 ignored benchmark.
- Svelte diagnostics and both Tauri/Slint compilation checks passed before implementation.
- Phase 1 implemented: each tab owns a tray shared by its panes; `Alt+Click` toggles membership without changing normal selection.
- Added marked-row treatment, a sidebar tray with missing-path status/removal, and backward-compatible session persistence.
- Phase 2 implemented: sidebar actions copy/move the tray into that pane through the existing progress, cancellation, and undo pipeline.
- Copy keeps the tray intact; move removes only source paths reported as completed by the backend.
- Phase 3 implemented: active-window tray state is synchronized through the Rust window registry and shown in Workbench as a draggable collection.
- Workbench tray drag defaults to copy; holding Shift while dropping moves and clears the successfully transferred tray.
- Phase 4 initial interaction set implemented: the hovered pane becomes the left-hand keyboard target, with active-pane fallback and a distinct green target outline.
- Unmodified `Q/W/F/N/Space` perform parent/close/favorite/split/preview on that target; text inputs and existing modified shortcuts retain priority.
- Phase 5 implemented: the tray supports click/Ctrl+Click/Shift+Click selection, Ctrl+A, Esc, selected-only transfer, and per-item Workbench drag.
- With no explicit tray selection, transfer actions retain the convenient all-items behavior. Individual successful moves remove only that item from the source tray.
- Phase 6 implemented: hover controls now use the same configurable shortcut definitions, settings UI, live footer labels, and duplicate-key detection as the rest of the app.
- Space key events are normalized to the readable `Space` setting name and covered by regression tests.
- Phase 7 implemented: each pane's tray names its receiving folder and selected entries can be removed without affecting files or unselected tray entries.
- Lifecycle fix implemented: the hidden overlay no longer keeps `app.exe` alive after the last normal Filer window closes. Closing one of several Filer windows still leaves the process running.
- Phase 8 implemented: Workbench drops use the shared background transfer/undo engine, render progress on the destination card and bottom bar, allow cancellation, and block overlapping drops.
- Completion handling removes only actually moved tray sources, refreshes both sides, distinguishes success/cancel/error, and buffers events that can arrive before the start call returns.

## Key integration points

- `src/lib/Filer.svelte`: owns tabs and session mapping.
- `src/lib/Pane.svelte`: owns selection and the existing transfer UI.
- `src/lib/FileList.svelte`: row click/selection behavior and marked-row rendering.
- `src/lib/Sidebar.svelte`: tree/favorites/history tabs; tray UI belongs here.
- `src/lib/Workbench.svelte`: existing single-item card-to-card transfer.
- `src/lib/api.ts`, `src-tauri/src/store.rs`: persisted session types.
- `src-tauri/src/transfer.rs`, `src-tauri/src/undo.rs`: progress/cancel/undo pipeline to reuse.

## Decisions still to validate during implementation

- Path identity must be case-insensitive on Windows and ignore trailing separators without damaging drive roots.
- Copy keeps items in the tray. Successful moves remove only the items actually moved.
- Missing paths remain visible with a warning until explicitly removed.
- The overlay uses the most recently focused registered Filer window as its source context. Only that window's active tab tray is shown.

## Progress log

- 2026-09-12: Created handover before implementation and established phased scope.
- 2026-09-12: Completed Phase 1 implementation. Verification passed: Svelte diagnostics, 48 frontend tests, 71 Rust tests (plus 1 ignored benchmark), production web build, and `git diff --check`.
- 2026-09-12: Completed Phase 2. Extended transfer completion events with completed source paths so tray state follows actual results rather than attempted inputs. Full frontend/Rust/build verification passed.
- 2026-09-12: Completed Phase 3. Added cross-WebView tray synchronization, a Workbench collection strip, and safe-default multi-item card drops. Full automated verification passed.
- 2026-09-12: Development app launched successfully. Runtime reported that the global hotkey was already owned by another running Filer instance; the new process itself reached the UI event loop and was then stopped.
- 2026-09-12: Completed the initial Phase 4 hover-key layer. Svelte diagnostics, 48 frontend tests, 71 Rust tests (1 ignored benchmark), production build, and diff checks passed.
- 2026-09-12: Completed Phase 5 partial tray operations and individual Workbench drag. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 72 Rust tests (1 ignored benchmark), and a production build.
- 2026-09-12: Completed Phase 6 configurable hover controls and Space normalization. Full frontend/Rust/build verification passed.
- 2026-09-12: Completed Phase 7 destination clarity and selected-only tray removal. Full verification passed.
- 2026-09-12: Fixed application shutdown semantics after real-device testing revealed that the hidden reusable overlay kept the release executable locked.
- 2026-09-12: Completed Phase 8 asynchronous Workbench transfers with progress, cancellation, overlap prevention, race-safe completion, and actual-result tray updates. Full verification passed.

## Commit log

- `3f37899` — `docs: add tray workbench implementation handover`
- `f9ec3a7` — `feat: add tab-local collection tray`
- `fb97296` — `feat: transfer collected tray items`
- `307a746` — `feat: send collection tray from workbench`
- `3839536` — `feat: add hover-target left-hand controls`
- `e8ebb3c` — `docs: record hover-control milestone`
- `f000175` — `fix: exit after closing the last filer window`
- `d351b39` — `docs: record last-window exit fix`
- `a98ba0d` — `feat: support partial tray operations`
- `94d0b99` — `docs: record partial tray milestone`
- `c59745b` — `feat: make hover controls configurable`
- `4e9e910` — `feat: clarify tray transfer targets`
- `a5ca2af` — `docs: record shortcut and tray polish`
- `6d96ace` — `feat: add async workbench transfers`
