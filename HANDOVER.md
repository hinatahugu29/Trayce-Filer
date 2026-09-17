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
- [x] Performance/correctness review — transfer cancellation reporting, search path identity, same-volume moves, incremental search filtering, timestamp preservation, debounced persistence, watcher starvation, listing sort allocations
- [x] Functional review — preview head reads, no-overwrite on name exhaustion, single instance, width/case-insensitive filtering, collision choice dialog, last-closed-window session restore, stale search results, search/tray context menus
- [ ] Functional follow-up — restore every open window (not only the last closed one) if multi-window restore becomes necessary
- [x] Hardening and feature round — shared normalization fixture, Windows CI, overwrite undo, skip-aware totals, dev/release instance separation, shared collision prompt, removal of unused sync commands, search attribute filters, search folder exclusions, pane pinning, bulk rename, transfer queue (with early-event race fix), on-demand folder size, named trays
- [x] GUI verification (release build, automated) — collision skip and keep-both, bulk rename preview/apply/undo, search attribute filters and folder exclusions, pinned-pane redirection, session restore of a directory plus search pane; found and fixed the narrow-pane layout (`9d7293f`)
- [ ] GUI follow-up — not exercised automatically: overwrite followed by Ctrl+Z (would touch the real Recycle Bin), queued transfers (fixture copies finish too fast to overlap), named tray add/rename (native prompt dialogs)
- [x] Review follow-up — make trash undo lookup cheaper (deletes now record original paths; the recycle bin is enumerated only when undo runs)
- [x] Everyday-use round — opener scope fix, Alt+D, new text file, open terminal here, confirmation before running executables, ZIP off the UI thread, Explorer clipboard interop, open with / properties / native shell menu, file-type icons, Trayce product naming
- [ ] Everyday-use follow-up — not exercised in the running app: Explorer clipboard round trips, native shell menu (submenus such as Send To may be empty because IContextMenu2/3 messages are not forwarded), properties dialog, Alt+D
- [ ] Review follow-up — reduce per-entry search cache memory (four owned strings per entry) if multi-million-entry roots become common
- [ ] Review follow-up — split `fs_ops.rs` (listing / transfer / clipboard / preview) and continue the gradual `Pane.svelte` extraction
- [x] Navigation-cost round — per-place view state, folder peek, in-place expansion, breadcrumb siblings, branch memory, filter-to-search promotion, adaptive prefetch, named pane layouts, window identity, attention-based column gradation, box selection, move-kind measurement, tray waypoints
- [x] Left-hand keyboard round — single-key sorting (`A`/`S`/`X`/`Z`), direction-only reverse (`D`), sort every pane at once (`Shift+`), expand/collapse all (`E`), collect to tray (`C`), hidden files (`R`); search pane made honest about what stopping discards and wired to the same keys
- [x] Favorites drop round (phase A) — register folders by dragging them onto the ★ panel, from a listing, another pane, or Explorer; add-only backend command, and one notification path that keeps every open ★ panel in step
- [ ] Favorites drop round (phase B) — let files into ★ as a light launcher: stop treating a file as missing, give file rows an open verb plus `→` to reveal, filter the search-root reuse to folders, and decide how a jump from a file favourite is counted while the measurement window runs
- [ ] **Measurement window (next action)** — use the app normally for 1–2 weeks, then read Settings → 移動の集計 and decide the next investment from the data rather than from argument. Thresholds are printed next to the numbers. **The tally has not started yet**: `state.json` carried no `navTally` as of 2026-09-16, so the window begins at first use of a build from `caaa30f` onwards
- [ ] Deferred until the measurement says so — an overview/teleport surface for Trayce, tray cycling (next/previous waypoint on one key), tray folder rows as drop targets
- [ ] Search follow-up, deferred by the owner on 2026-09-16 — a search pane split from a search pane builds a second index of the same roots (accepted: duplicating is the user's own choice), and files created after the scan need a reload to appear. Both documented in `SPEC.md` §4.4; revisit only if real use makes either painful

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
- A speed/implementation review fixed eight issues without changing product behavior:
  - Cancelled transfers are no longer reported as completed. `copy_file`/`copy_dir_all` return whether they finished; interrupted items never reach tray removal or undo, and a partial directory tree is removed from its freshly allocated destination.
  - Search roots are stripped of the Windows `\\?\` prefix, so result paths share identity with directory panes, tray, and favorites.
  - Moves try `rename` for directories as well as files, and total sizing skips same-volume move sources. Sizing no longer follows symlinks/junctions.
  - The search worker keeps a local copy of the append-only cache, evaluates only newly indexed entries for an unchanged request, selects the top `limit` before sorting, suppresses no-op refresh events, and parses queries into allocation-free include/exclude lists. The scanner uses `DirEntry::metadata()` instead of an extra `fs::metadata` per file.
  - Streaming copies apply the source modified time.
  - `state.json` writes are coalesced by a writer thread (300 ms quiet period) and flushed on `RunEvent::Exit`.
  - Directory watch reloads keep the 250 ms debounce but fire after at most 1 s of continuous changes.
  - Listing sorts lowercase each name once instead of per comparison.
- A functional review followed, with direction from the user on three product decisions (collisions ask, last closed window restores, searches ignore case/width):
  - Text preview reads at most 64 KB via `Read::take` and trims an incomplete trailing UTF-8 sequence.
  - `unique_target` returns an error when no free name remains instead of returning the existing path, which the streaming copy would have overwritten.
  - A std-only single-instance guard (`instance.rs`) takes an exclusive temp lock file; later launches send a focus token to the first process over localhost and exit. `tauri-plugin-single-instance` was not used because crates could not be downloaded in this environment. `rust-version` is 1.89 for `File::try_lock`.
  - `search::normalize` and `api.foldForSearch` share one folding rule set: ASCII case, full-width ASCII, ideographic space, and half-width katakana with composed marks. It applies to search queries/cache, the in-folder filter, history filter, and window list filter; full-width `！`/`－`/`｜` act as operators.
  - Before any pane, paste, tray, or Workbench transfer, `transfer_conflicts` is checked. `ConflictDialog.svelte` offers keep both (default), overwrite (existing item to Recycle Bin; a target containing the source is refused), skip (not reported as completed), or cancel (cut clipboard kept). `start_transfer` takes an optional `conflict` policy.
  - Every Filer window saves its session with its label; the store keeps one per window and promotes a window's session to `last_session` on `Destroyed`, before unregistering can exit the app. Main restores the last closed window next launch.
  - The search worker verifies existence only for rows it is about to show, drops and backfills missing ones, and remembers them. `recheck_search` forces a re-emit; the pane calls it on pointer enter and window focus (throttled to 1 s).
  - Search results gained a context menu (open, open containing folder beside, reveal, copy/cut/path/name, tray toggle, narrow search to folder, search folder beside). Directory panes gained "search inside this folder" and a tray toggle.
  - Checked: pane paste already used the background transfer pipeline. The unused synchronous `paste_clipboard`/`accept_dropped` commands were later removed (`48962b0`).
- The Trayce visual identity is now applied to the Tauri icon set. `assets/branding/trayce-icon-source.png` is the retained 1024px source, and the generated PNG/ICO/ICNS plus platform icon variants live under `src-tauri/icons`. `build.rs` explicitly tracks the executable and window-icon sources so artwork-only changes rebuild the Windows resource instead of leaving Tauri's previously embedded default icon in development and release executables.

## Key integration points

- `src/lib/Filer.svelte`: owns tabs and session mapping.
- `src/lib/Pane.svelte`: owns selection and the existing transfer UI.
- `src/lib/FileList.svelte`: row click/selection behavior and marked-row rendering.
- `src/lib/Sidebar.svelte`: tree/favorites/history tabs; tray UI belongs here.
- `src/lib/Workbench.svelte`: existing single-item card-to-card transfer.
- `src/lib/api.ts`, `src-tauri/src/store.rs`: persisted session types.
- `src/lib/layouts.ts`: pure anchor-relative layout maths (capture / resolve / describe). Also exports `parentOf` and `relativeTo`, reused by `navstats.ts` and `Pane.svelte`.
- `src/lib/prefetch.ts`: short-lived listing cache, adaptive warming, and the generation token that drops work for a place already left.
- `src/lib/navstats.ts`: move classification and the persisted tally. Deliberately only produces evidence; it never changes behaviour.
- `src/lib/LayoutPalette.svelte`: the `Ctrl+E` palette. `Ctrl+1`..`9` are positional and handled in `Filer.svelte`, not registered as individual actions.
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

- 2026-09-13: Completed a speed and implementation review in eight focused commits (see Current status). Verification passed with 0 Svelte diagnostics, 48 frontend tests, 89 Rust tests (1 ignored benchmark), production build, and diff checks. Not yet exercised in the running app: the exit-time state flush and the incremental search worker on a large real root.
- 2026-09-13: Completed the functional review in eight commits (see Current status) and updated `SPEC.md`. Verification passed with 0 Svelte diagnostics, 52 frontend tests, 102 Rust tests (1 ignored benchmark), production build, and diff checks. Not yet exercised in the running app: the conflict dialog flow end to end, second-launch focus hand-off, and session restore after closing a detached window last.
- 2026-09-13: Ran automated GUI verification against a release build (PrintWindow capture plus synthetic input, with `state.json` backed up and later restored by hash). Confirmed in the running app: a second launch exits with code 0 and leaves one process, full-width katakana typed into the folder filter matches a half-width katakana file, and the collision dialog appears with the colliding name. The dialog showed focused and hovered choices identically, fixed in `519006c`. The run was stopped when later clicks and a paste landed on another maximized application (the Claude desktop app) because the desktop was in use; Filer had exited normally (exit-time state flush observed, no crash events). Skip/keep-both results from that run are not treated as verified.
- 2026-09-13: Completed the hardening and feature round in fourteen commits: shared Rust/TypeScript normalization fixture and Windows CI; overwrite undo restoring Recycle Bin originals; skip-aware progress totals and removal of unused synchronous commands; separate dev/release single-instance locks; shared collision prompt; search `ext:`/`size:`/`modified:`/`type:` filters; configurable search folder exclusions; pane pinning with navigation redirection; bulk rename with preview, two-phase apply, rollback, and single-step undo; per-pane transfer queue, including a fix for completion events that arrived before `start_transfer` returned; on-demand folder size; named trays per tab with backward-compatible sessions. Verification passed with 0 Svelte diagnostics, 57 frontend tests, 115 Rust tests (1 ignored benchmark), production build, and diff checks.
- 2026-09-13: With the desktop explicitly free, resumed automated GUI verification on a fresh release build. Input helpers now abort (instead of warning) when the target point is not Filer or Filer is not the foreground window, and Filer is brought forward through its own single-instance focus hand-off. Confirmed on disk and on screen: Skip leaves the destination untouched; Keep both creates `report (2).txt`; the dialog distinguishes focused and hovered choices; bulk rename with `trip_{n}` numbers files in display order and Ctrl+Z restores the original names; `ext:png size:>1mb` returns only the 3 MB image and `type:dir` only the folder; `node_modules` is not indexed; a pinned pane stays put and a folder opened from it appears in a new pane with a note; a directory pane plus a search pane with its query restore after restart. The run exposed a real layout defect in narrow directory panes (toolbar crushing the breadcrumb, file names hidden because the list lacked a size container), fixed in `9d7293f` and re-verified. `state.json` was backed up before the run and restored by hash afterwards.

- 2026-09-16: Completed the navigation-cost round in `caaa30f`. The problem addressed: like every other file manager, switching folders replaced the whole view, so returning to the parent cost exactly as much as jumping to an unrelated drive — nothing made near places cheaper than far ones. Landed on three fronts (make going back free, avoid going down at all, make the move itself faster) plus named pane layouts, window identity, attention-based column gradation, box selection, and move-kind measurement.

  Three real defects were found by tests written alongside the code, not by review:
  - `relativeTo` matched `C:workbench` as a child of `C:work`, because `pathIdentity` strips the trailing separator before the prefix comparison.
  - Prefetch counted the parent slot against the child budget, so a drive root with no parent still fetched one child on a slow source — exactly the case the slow-source guard exists for.
  - `paths_exist` answered with `is_dir`, so **every file in the tray had always been shown as missing**. This was pre-existing and unrelated to the round; it surfaced while giving tray folders their own verb.

  Prefetch is adaptive rather than path-based: a mapped network drive (`T:...` here, observed at 711ms) cannot be told from a local one by its path, so the decision uses the measured time of the preceding read. At most two requests are in flight and queued work for an abandoned place is dropped.

  Logging had been registered only under `cfg!(debug_assertions)`, so the measurement built this round would have produced nothing in the builds actually used day to day. It now writes to the log directory in both profiles, and to stdout additionally in debug.

  Verification: 0 Svelte diagnostics, 103 frontend tests, 119 Rust tests (1 ignored benchmark), production web build, debug and release Tauri builds. Confirmed in the running app: both builds start, restore the session, and write to `logsTrayce.log`; no errors logged. Not exercised automatically: box selection and auto-scroll, in-place expansion, layout capture/apply, and the tray `→` button — all were checked by the author in the running app instead.

## Resuming this work elsewhere

Everything needed to continue is in the repository; nothing lives only on the machine it was written on.

```bash
git clone https://github.com/hinatahugu29/Trayce-Filer.git
cd Trayce-Filer
npm install
npm run check && npx vitest run && cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri:build -- --no-bundle   # exe only; drop the flag for installers
```

Two things are **not** in the repository and do not transfer:

- `%APPDATA%dev.filer.appstate.json` — favourites, history, sessions, saved layouts, and the move tally. A new machine starts empty, so its measurement starts from zero.
- `%LOCALAPPDATA%dev.filer.applogsTrayce.log` — the `[nav]` summaries. Copy it if the numbers from a particular machine matter.

Consequently the measurement window should be run on **one** machine. Mixing two machines' tallies is not possible, and neither is meaningful on its own if the work is split across both.

### Where the current design reasoning lives

- `SPEC.md` §3.6, §3.7 — the two principles this round added, written product-facing.
- `SPEC.md` §9 — what is deferred and the numeric thresholds that decide it.
- Commit `caaa30f` — the reasoning per change, in the message body.

### The decision this round was built to enable

An overview/teleport surface (a map of ancestors and descendants) is attractive but pays for itself only if it replaces three or more operations; reading a map costs 1–2 seconds where `Q` costs 0.2. So it must not take on short moves, and whether it is worth building at all depends on how often long moves actually happen.

Settings → 移動の集計 answers that:

- jumps (`other`) **≥ 30%** — build it
- jumps **≤ 10%** — do not; it cannot repay the reading cost
- quick turnarounds high — improve peeking instead of movement

Trayce and ChainFlow Filer are separate products on separate axes (many windows versus one screen) and are not converging. A mechanism that wins on one axis can lose on the other: chaining panes buys back pixels in ChainFlow, where pixels are scarce, but in Trayce it would add constraint with nothing bought. Move the idea, not the form.

## The left-hand keyboard round

- 2026-09-16: Merged the navigation-cost round as [#3](https://github.com/hinatahugu29/Trayce-Filer/pull/3) after a review found three defects, fixed in `76dfe69`: the breadcrumb sibling list closed on the window's `pointerdown` before its own `click` could fire, so picking a sibling never navigated and the `▾` could not be toggled shut; marquee auto-scroll captured its direction when the interval was created, so dragging from one edge to the opposite one kept scrolling the original way; and `savedPathStates={pane.pathStates ?? []}` wrote `?? []` straight into a prop — the pattern that once froze this app with `RangeError: Invalid array length`. It was harmless only because `Pane.svelte` reads that prop once at init, which is not a property the next edit is obliged to preserve. Stable constants now, and the neighbouring `sidebar ?? { primary: 'tree' }` with it.

- 2026-09-16: Put sorting and collecting under the left hand (`e082846`). The request was single-key sorting; almost all of it turned out to exist already. `dirs_first` was **already** partitioning before comparing, so "folders pinned on top, each kind sorted within itself" needed no Rust change — only a test, because under a single key the two orderings both become load-bearing on every press. `changeSort` already reversed on a repeated column. There was no type-ahead, so the letter keys were free, and `Q`/`W`/`F`/`N`/`Space` had already established the idiom (hovered pane first, active pane otherwise, never while typing in a field). The work was wiring, plus four judgements: hold-to-repeat is ignored (it would flip the direction back and forth, re-listing each time); `Shift+` fans a sort out to every directory pane with the direction decided **once** at the initiating pane, since letting each pane reverse its own would defeat the point of levelling them; search panes are excluded because their ordering belongs to the search, not to the window; and `E` stops after 40 folders, saying so, because expansion costs one read per folder.

  Asked afterwards for "add the current folder to favourites", which `F` had done since the hover-control round. Worth remembering that this codebase is now large enough that its owner can want something it already has — check before building.

- 2026-09-16: Read the search subsystem end to end on request and fixed what it was lying about (`5c66281`). **Stop is not a pause**: `cancel_search` removes the session, which drops the worker's `Sender`, ends the thread and frees the whole `CachedEntry` cache — yet the status line read `停止しました — N件を読み込み済み`, which reads as though those N are still searchable. Worse, `requestId` was never cleared (it had exactly one assignment, in `runIndex`), so typing afterwards sent a filter that came back `検索が開始されていません`, and that string overwrote the notice that the search had been stopped. The pane was then inert with nothing on screen explaining why. Stopping now releases the id, settles `running` and the status itself — it has to, because the done event is routed by that same id — and a filter attempted while stopped says a reload is needed instead of failing silently.

  Also connected the new sort/tray keys to the search pane (a key that works in one pane kind and is silently dead in another makes you check the pane kind before pressing), and made new search panes start at modified-descending: searching is mostly for something touched recently, so name order costs a guaranteed second press.

  Deferred by the owner, deliberately: a search pane split from a search pane re-scans the same roots into a second index (their call — it is the user's choice to duplicate), and files created after the scan do not appear until a reload. Both are now written down in `SPEC.md` §4.4 rather than left to be rediscovered.

## The favourites drop round

- 2026-09-17: Asked for files in ★ as a simple launcher. The registration path was settled first, because dragging onto the panel removes the question that file support would otherwise force: today `F` means "the folder I am standing in", and making it mean "the selected item when there is exactly one" would have put a condition on a single key that has none. Dragging is the right hand pointing at a specific thing; `F` is the left hand naming where it already is. §2.1 of the spec says keyboard-only completeness is **not** a goal while mouse-only completeness **is**, so a file path that exists only under the mouse is the sanctioned gap, and folders keep both.

  Three things had to be true and two of them already were. Row drags hand control to the OS via `startDrag`, so no DOM drop event ever arrives — but `onDragDropEvent` in `Filer.svelte` already catches the drag coming back into its own window and already hit-tests it by coordinate, which is why pane-to-pane drops work. Extending that meant one more element lookup. Dropping from Explorer came free, because Tauri does not distinguish where the drag started.

  The one real trap: `paneAt` **falls back to the active pane** when the point hits nothing, so a drop on the sidebar today copies into that pane's folder. The ★ test therefore has to run **before** `paneAt`, not after, or it can never be reached.

  `toggle_favorite` could not be reused — it reverses membership, so dropping something already registered would have removed it. `add_favorites` is add-only and returns how many it actually added, which is what lets the notice distinguish registered from already-there. Its key function is deliberately separate from the neighbouring `search_location_key`, which trims `C:\` down to `C:`; harmless as a search-location key, wrong for a favourite, since a drive root is a plausible entry. A test pins that.

  Also fixed while passing through: `bind:this={sidebar}` only ever bound the **upper** Sidebar, so `F` left the lower panel of a split sidebar stale. Favourite changes now go through one notifier in `api.ts` that every ★ panel and every path-bar ☆ subscribes to, which covers the new drop path and that old gap at once.

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
- `ae50d77` — `docs: record named workbench tabs`
- `c260a50` — `feat: apply Trayce application icon`
- `e3b1f4d` — `fix: rebuild Windows icon resources`
- `aaba3d6` — `fix: stop reporting cancelled transfers as completed`
- `4d1a6c7` — `fix: strip the \\?\ prefix from search result paths`
- `2dec3d7` — `perf: rename directories when moving within a volume`
- `fac11e9` — `perf: make search filtering incremental and allocation-free`
- `b1ec657` — `fix: preserve modified time when copying files`
- `9a37d9e` — `perf: debounce state.json writes and flush on exit`
- `669d92f` — `fix: refresh directory listings during sustained file changes`
- `f12411f` — `perf: lowercase names once when sorting listings`
- `35eda8b` — `docs: record performance and correctness review`
- `1bc82ee` — `fix: read only the head of files for text preview`
- `9d3366b` — `feat: ignore case and character width when searching and filtering`
- `b50e7d5` — `fix: prevent a second Filer process from starting`
- `69b822c` — `fix: fail instead of overwriting when no free name remains`
- `55ca77e` — `feat: restore the last closed window's session on startup`
- `316f10e` — `feat: ask how to handle name collisions before transferring`
- `c8cc2ab` — `fix: drop search results that no longer exist`
- `d3dd024` — `feat: add search and tray actions to context menus`
- `bd23bba` — `docs: record functional review and updated product behavior`
- `5ef37c5` — `test: share normalization cases and add CI`
- `519006c` — `fix: distinguish focused and hovered collision choices`
- `5e3f860` — `fix: let development and release builds run side by side`
- `f1ae187` — `refactor: share the transfer collision prompt`
- `c88683d` — `fix: restore overwritten items when undoing a transfer`
- `48962b0` — `fix: count only written items in transfer totals; drop unused sync commands`
- `b52533e` — `feat: filter search results by extension, size, date, and kind`
- `35f4c44` — `feat: skip configurable folders when indexing searches`
- `b5ee38c` — `feat: pin a pane as a fixed transfer destination`
- `3e842a7` — `feat: rename many files at once with a live preview`
- `71030ac` — `feat: queue pane transfers instead of losing the running one`
- `9405951` — `feat: measure folder sizes on demand`
- `be9e2bb` — `feat: keep several named collection trays per tab`
- `fd60d00` — `docs: record hardening round and new features`
- `9d7293f` — `fix: keep narrow directory panes readable`
- `caaa30f` — `feat: make returning to a place cheaper than travelling to a new one`
- `76dfe69` — `fix: make breadcrumb siblings, marquee auto-scroll, and pane props behave`
- `e082846` — `feat: put sorting and collecting under the left hand`
- `5c66281` — `fix: tell the truth when a search is stopped, and match the pane keys`
- `PENDING` — `feat: register favourite folders by dropping them`
