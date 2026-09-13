# Filer implementation handover

## Objective

Bring the proven ChainFlow workflow—collecting items across folders with the left hand while pointing with the mouse—into the Tauri Filer, then connect that collection to the multi-window workbench.

The feature is intentionally an evolution rather than a direct copy:

- `Alt+Click` collects or removes a file/folder from a tab-local tray.
- The tray is visible in the normal sidebar and survives session restore.
- Collected items can be copied or moved to the active pane through the existing transfer/undo pipeline.
- The workbench can send the tray to another Filer window.
- Existing selection, drag/drop, shortcuts, and Space quick preview must keep working.

The next direction is to make panes role-switchable rather than treating every pane as a
directory browser. The first additional role is a persistent search pane: one side can
show an ordinary destination folder while another side keeps cross-directory search
results available as normal copy/move/preview/tray sources.

## Working rules

- Keep the Slint implementation untouched.
- Implement in small phases with a verified Git commit for each meaningful milestone.
- Do not replace `Space` quick preview with a shortcut layer.
- Prefer existing transfer, progress, cancellation, collision handling, and undo paths.
- Never remove a tray item merely because a transfer was attempted; update it from the actual result.
- Keep `SPEC.md` as the product-facing source for human/AI interviews; keep implementation chronology and engineering detail in this handover.

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
- [x] Phase 9 — introduce a backward-compatible pane-kind model (`directory` / `search`)
- [x] Phase 10 — add a search-pane shell and role-switching UI without changing directory-pane behavior
- [x] Phase 11 — implement cancellable, streaming filename search with explicit scope
- [x] Phase 12 — share selection, preview, tray, drag, copy, and move behavior with search results
- [x] Phase 13 — persist and restore search scope, query, options, and result presentation
- [x] Phase 14 — bring over advanced query/sort/history behavior from `File_Search_APP` selectively
- [x] Search history follow-up — expose search-location history as a first-class navigation surface, separate from ordinary folder history and per-pane query history
- [ ] Search follow-up — profile real-device performance before deciding whether parallel directory walking is beneficial

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
- Reviewed `E:\CODE\Antigravity\File_Search_APP` as the reference search implementation. Its useful seams are the query parser, parallel scanner/search worker, incremental result delivery, cancellation/pause behavior, and native multi-path drag support.
- Chosen direction: search is a pane role, not a temporary palette. A search pane remains beside ordinary directory panes and acts as a first-class source for existing file operations.
- Phase 9 implemented: pane roles and dormant search conditions now round-trip through session state. Sessions from older builds default safely to directory panes, and search results themselves are intentionally not serialized.
- Phase 10 implemented: a directory pane can switch to a clearly identified search shell and back while retaining its directory context and draft query. Split, close, hover targeting, and drop rejection have explicit search-pane behavior.
- Phase 11 implemented: search panes recursively scan an explicit root on a background thread, stream matching metadata in bounded batches, support cancellation/restart, and ignore events from superseded searches by client-generated request ID.
- The original MVP searched per query; it has since been replaced by a persistent Rust-side cache and dedicated search worker modeled on `File_Search_APP`.
- Phase 12 implemented: search results now use the shared virtualized file list with absolute-path identity, so same-named files in different directories remain independently selectable. Results support range/multi-selection, keyboard navigation, open/reveal, preview, tray toggling, configurable copy/cut/path-copy shortcuts, sorting, and native multi-path drag to normal panes or other applications.
- Double-clicking a result folder converts that search pane back to a directory pane at the chosen folder while retaining its dormant search conditions. Copying or dragging to a directory pane uses the existing transfer pipeline; cutting then pasting uses the existing cross-window clipboard move path.
- Phase 13 implemented: search scope, query, path matching, sort key/direction, directory-first preference, and preview visibility persist per pane. Restored panes register event listeners before automatically rerunning saved non-empty searches, so fast searches cannot lose their first events.
- Search result columns now collapse by available pane width, preserving the name/source-path column when a search pane sits beside a destination pane.
- Phase 14 implemented selectively: spaces are AND, `|` is OR, and a leading `!` or `-` excludes a term. Semicolon/full-width-semicolon separated roots are searched together, duplicate roots are collapsed, and each pane keeps ten deduplicated recent queries.
- Parallel directory walking remains a measured follow-up. Cancellation, pause, and resume are implemented; concurrency should be added only if real-device profiling shows that filesystem latency justifies the extra pressure.
- Search interaction now follows `File_Search_APP` internally as well as visibly: entering the search role starts a background inventory pass, Rust retains all cached entries plus pre-normalized name/path strings, and editing the query submits asynchronous filter requests to a dedicated worker. Enter is not required; it only records query history.
- The worker drains queued requests and computes only the latest pending condition. The WebView accepts only its latest request ID, receives at most 500 matching entries, and no longer owns the full inventory. Scan-driven refreshes are throttled to 150/300/500 ms as the cache grows.
- Changing roots or pressing reload creates a fresh cache and worker. During pause, cached data remains searchable; resume continues walking from the same point.
- Pane creation now exposes both meanings explicitly: `N` duplicates the current directory as a normal pane, `Shift+N` preserves the source pane and adds a search pane scoped to its current directory, and the existing `⌕` action converts the current pane itself to search.
- Sidebar tree folder names now toggle as well as navigate: the first click expands the folder and a second click collapses it. The separate disclosure arrow remains available for opening or closing without navigation.
- The directory-pane sidebar can now be split into two stacked information views. Each section independently shows tree, favorites, history, or tray; the divider adjusts their height, either section can collapse back to the original single view, and the composition is restored per pane.
- Search navigation remains parallel to directory navigation rather than being folded into it. Following `File_Search_APP`, recently searched roots (up to 15) are exposed so a flow such as “search here, then search the place used a moment ago” is one click away.
- Search panes now show a visible history rail by default. Search-location history is global, persistent, deduplicated case-insensitively on Windows, capped at 15 entries, and kept separate from ordinary navigation history. Selecting a previous location preserves the current search word and immediately re-indexes that location.
- The same rail exposes the pane-local search-word history as direct buttons and the normal Filer favorites as reusable search locations. A single current root can be added to or removed from favorites in place. The `履` toolbar action collapses the rail, and that choice persists with the search pane.
- Splitting from a search pane now clones its roots, query, sort, matching, preview/history presentation, and recent words into an independent new pane. This makes side-by-side variations possible without rebuilding the search context; directory-to-search splitting still starts with a clean search scoped to that directory.
- `SPEC.md` now provides a product-facing description of the Tauri Filer, separating implemented behavior, design principles, candidate directions, non-goals, interview questions, and reusable prompts for consulting other AI systems.
- The window registry now receives the active tab label, total tab count, and every visible pane's ID, kind, path, query, and active state. The compact overlay groups those panes beneath their owning window, includes pane context in filtering, and can focus a specific pane through a targeted cross-WebView event. Other tabs remain represented by count only to avoid flattening the hierarchy into an unreadable list.
- Workbench window cards now contain a pane map for the active tab. Selecting a pane changes both the real Filer target and the card body; directory panes load their own mini listing, while search panes show their query/context without pretending to be directories. Directory pane tiles are explicit drop targets, including transfers between two panes in the same window, and search panes reject destination drops.
- Workbench cards now receive every tab's ID, concrete display name, and active state. A horizontally scrollable tab strip replaces the vague “other N tabs” summary as the primary overview; selecting a name switches the real Filer tab, after which the card's pane map and mini listing refresh to that tab.
- The Trayce visual identity is now applied to the Tauri icon set. `assets/branding/trayce-icon-source.png` is the retained 1024px source, and the generated PNG/ICO/ICNS plus platform icon variants live under `src-tauri/icons`. The Windows title bar, taskbar, executable, and installer pick up the new mark on the next development restart or rebuild.

## Key integration points

- `src/lib/Filer.svelte`: owns tabs and session mapping.
- `src/lib/Pane.svelte`: owns selection and the existing transfer UI.
- `src/lib/FileList.svelte`: row click/selection behavior and marked-row rendering.
- `src/lib/Sidebar.svelte`: tree/favorites/history tabs; tray UI belongs here.
- `src/lib/Workbench.svelte`: existing single-item card-to-card transfer.
- `src/lib/api.ts`, `src-tauri/src/store.rs`: persisted session types.
- `src-tauri/src/transfer.rs`, `src-tauri/src/undo.rs`: progress/cancel/undo pipeline to reuse.
- `E:\CODE\Antigravity\File_Search_APP\src\query.rs`: reference semantics for AND/OR/NOT queries.
- `E:\CODE\Antigravity\File_Search_APP\src\scanner.rs`, `search_worker.rs`: reference architecture for incremental background search; adapt concepts instead of coupling the two executables.

## Search pane plan

### Product model

- Every pane owns a discriminated role. Phase 9 starts with `directory` and `search`; the model must allow later roles such as tray, recent files, or comparison without another state rewrite.
- Converting a directory pane to search uses its current folder as the initial scope. Converting back returns to the last browsed directory rather than guessing from a result.
- Search results are virtual listings, but every row retains its real absolute path. File actions therefore continue to operate on real sources through the existing transfer and undo paths.
- Pane role belongs to the saved tab session. Runtime-only results do not need to be serialized; saved search conditions rerun on restore.

### Interaction model

- Add a compact role switch in the pane header/menu. Splitting still creates another directory pane first; the user can then change either pane's role.
- Search pane header contains scope, query, running state, and a compact options affordance. Results occupy the existing main list area rather than introducing a separate modal.
- Default scope is the directory from which the pane was converted. Later scope choices are current folder, drive, and multiple selected roots.
- Selecting, multi-selecting, previewing, adding to tray, opening, revealing, and dragging results should feel identical to directory rows.
- Dropping search results on a directory pane means copy/move as today. Dropping them on the tray means collect only. A search pane is not initially a transfer destination.
- Hover-target left-hand shortcuts continue to target the pane under the pointer. Parent navigation is directory-only; search gets a dedicated focus/start-stop action only after real-device use establishes the best key.
- Keep three concepts distinct in the UI and state: ordinary folder-navigation history, searched-location history, and search-word history. Selecting a searched location should immediately retarget/reindex the search pane while retaining the current search word, matching the reference app's rapid “search here, then revisit another root” flow.

### Architecture

- Keep `Filer.svelte` responsible for tab/pane identity and persisted role state.
- Split the current monolithic `Pane.svelte` gradually: retain it as the directory role initially, add `SearchPane.svelte`, and extract only proven shared row operations instead of performing a large rewrite upfront.
- Define a common pane-facing capability surface for current path/context, focus, reload, selected paths, drop acceptance, and keyboard actions. Unsupported actions must be explicit no-ops or disabled UI, not role checks scattered through `Filer.svelte`.
- Implement search in Rust as its own background job registry with request IDs, cancellation, incremental result events, and stale-event rejection. Do not run a full recursive scan on the WebView thread.
- Reuse the existing `Entry` shape where it remains truthful; extend it with an absolute/source path when virtual results require it. Avoid fabricating a single parent path for mixed search results.
- Borrow behavior from `File_Search_APP`, but do not make Filer launch or depend on that executable. Code can be adapted into Filer's Rust backend once its boundaries and tests are understood.

### Delivery slices and acceptance checks

1. **State foundation:** old sessions restore as directory panes; new role state round-trips; all current tests and behavior remain unchanged.
2. **Visible shell:** a pane can switch directory → search → directory and preserve its last directory. Empty and not-yet-searched states are clear.
3. **Search MVP:** one root, filename/path matching, incremental results, cancel/restart, stale searches never overwrite newer queries.
4. **Operational parity:** open/reveal, selection, preview, tray toggle, and drag from results work; copy/move uses existing progress, cancellation, collision, and undo behavior.
5. **Session and polish:** search conditions restore and rerun safely; status shows scanning/result counts; narrow split layouts remain usable.
6. **Advanced search:** AND/OR/NOT, normalization, multiple roots, date/name/path sort, history/favorites, pause/resume—added only after the MVP is exercised on the real machine.

Each slice gets its own implementation commit followed by verification and a handover update. Minimum verification remains Svelte diagnostics, frontend tests, Rust tests, production web build, and `git diff --check`; search worker logic also needs cancellation, stale-result, query, and session-compatibility tests.

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
- 2026-09-12: Planned the next track as role-switchable panes, beginning with a persistent search pane informed by `File_Search_APP`. No search implementation has started yet.
- 2026-09-13: Completed Phases 9 and 10: persistent pane roles plus the directory/search switching shell. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 73 Rust tests (1 ignored benchmark), and a production build.
- 2026-09-13: Completed Phase 11 background search and incremental result display. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 76 Rust tests (1 ignored benchmark), and a production build.
- 2026-09-13: Completed Phase 12 actionable search results and generalized file-list path identity for virtual listings. Full verification passed with the same 48 frontend and 76 Rust tests plus production build.
- 2026-09-13: Completed Phase 13 search-session restoration and narrow-pane presentation. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 77 Rust tests (1 ignored benchmark), and a production build.
- 2026-09-13: Completed the selected Phase 14 advanced-search set: AND/OR/NOT, multiple roots, duplicate-root suppression, and per-pane query history. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 79 Rust tests (1 ignored benchmark), and a production build.
- 2026-09-13: Reworked search initiation around a scan-on-pane-open, filter-as-you-type model. Added shared frontend query semantics and regression tests; verification passed with 0 Svelte diagnostics, 51 frontend tests, 79 Rust tests (1 ignored benchmark), and a production build.
- 2026-09-13: Replaced the temporary WebView-side inventory with a `File_Search_APP`-style Rust cache and coalescing search worker. Added throttled scan refresh, 500-result IPC limit, pause/resume, and stale request rejection. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 77 Rust tests (1 ignored benchmark), and a production build.
- 2026-09-13: Added direct search-pane splitting without replacing the source pane. The toolbar action and configurable `Shift+N` shortcut work from both directory and search panes. Full verification passed.
- 2026-09-13: Made sidebar tree folder-name activation bidirectional. Repeated clicks now expand and collapse the folder while retaining normal navigation. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 77 Rust tests (1 ignored benchmark), production build, and diff checks.
- 2026-09-13: Added an optional stacked sidebar. The normal four-tab header remains unchanged until the `↕` action is used; split sections use compact independent selectors and a draggable horizontal divider. Per-pane layout persistence is backward compatible. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 78 Rust tests (1 ignored benchmark), production build, and diff checks.
- 2026-09-13: Completed the accessible search-history track in four isolated changes: persistent search-location storage, a visible search history rail, favorite-folder reuse from search, and context-preserving search splits. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 80 Rust tests (1 ignored benchmark), production build, and diff checks.
- 2026-09-13: Added `SPEC.md` as the product and interview document. It deliberately avoids treating roadmap ideas as implemented requirements and points engineering follow-ups back to this handover.
- 2026-09-13: Brought active pane topology into the compact window list in three commits: registry schema and tests, live Filer publication, then grouped child rows with targeted pane activation. Verification passed with 0 Svelte diagnostics, 48 frontend tests, 81 Rust tests (1 ignored benchmark), production build, and diff checks.
- 2026-09-13: Extended the same topology into Workbench cards. Added a selectable pane map, selected-pane content switching, search-pane context presentation, pane-specific drop targets, and same-window cross-pane transfers. Frontend diagnostics, 48 tests, production build, and diff checks passed; the unchanged Rust layer retains its 81 passing tests plus 1 ignored benchmark.
- 2026-09-13: Replaced Workbench's opaque “other N tabs” description with all concrete tab names in a compact scrollable strip. Tab IDs and active state are shared through the registry, and choosing a tab from the card activates it in the owning Filer. Frontend diagnostics, 48 tests, production build, five focused window-registry tests, and diff checks passed.

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
- `91174a9` — `docs: record async transfer milestone`
- `bb2ec10` — `docs: plan role-switchable search panes`
- `7fad931` — `feat: add persistent pane roles`
- `b02616d` — `feat: add search pane shell`
- `a59751e` — `feat: stream search pane results`
- `f849e8d` — `docs: record search pane MVP`
- `fd36884` — `feat: make search results actionable`
- `3b3dca0` — `docs: record actionable search results`
- `28ea183` — `feat: restore search pane sessions`
- `c6b3423` — `docs: record search session milestone`
- `cee3f60` — `feat: add advanced pane search`
- `3fcc742` — `docs: record advanced search milestone`
- `9c77101` — `feat: filter search panes as you type`
- `57be84c` — `docs: record live search interaction`
- `0b6b416` — `refactor: move live search into Rust worker`
- `6e9154d` — `docs: record Rust search worker architecture`
- `48a89a4` — `feat: split directly into search panes`
- `a7c3402` — `docs: record search split action`
- `5829b11` — `fix: toggle tree folders from their names`
- `8fb89aa` — `docs: record tree toggle behavior`
- `19d4bc0` — `feat: split sidebar into stacked views`
- `68f49da` — `docs: record stacked sidebar milestone`
- `ddb480a` — `docs: plan accessible search location history`
- `b70aaa1` — `feat: persist search location history`
- `2fdd448` — `feat: expose search history in search panes`
- `cb9bbf2` — `feat: search favorite locations from history rail`
- `d1f4d2f` — `fix: preserve search context when splitting`
- `fcbd1dd` — `docs: record search history workflow`
- `e2f0164` — `docs: add product specification`
- `d14aef2` — `docs: link product spec from handover`
- `94bbdbd` — `feat: share active window pane context`
- `85b0fbc` — `feat: publish live pane layout from filer windows`
- `da1f4fa` — `feat: show pane children in window list`
- `d46d976` — `feat: activate panes from window list`
- `43ad7b7` — `feat: map panes inside workbench cards`
- `ecc0709` — `feat: target workbench transfers by pane`
- `393525e` — `docs: clarify pane-level workbench actions`
- `05861b7` — `feat: share window tab summaries`
- `e2f7fef` — `feat: switch named tabs from workbench cards`
