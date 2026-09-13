<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import type { UnlistenFn } from '@tauri-apps/api/event'
  import * as api from './api'
  import { splitPath, pathHue, elideLeft, formatSize, joinPath } from './api'
  import type { WindowInfo, WindowPaneInfo, Entry } from './api'
  import TransferBar from './TransferBar.svelte'

  export let windows: WindowInfo[] = []
  export let pinned: boolean = false
  export let onSelectWindow: (w: WindowInfo) => void
  export let onSelectPane: (w: WindowInfo, pane: WindowPaneInfo) => void
  export let onCloseWindow: (label: string) => void
  export let onNote: (msg: string) => void = () => {}

  // 俯瞰画面から一時除外（退避トレイへ移動）したウィンドウのラベル
  let hiddenLabels = new Set<string>()

  // 各ウィンドウのフォルダ内ファイル一覧キャッシュ
  let dirCache: Record<string, { entries: Entry[]; loading: boolean; error?: string }> = {}
  let selectedPaneIds: Record<string, number> = {}

  function selectedPane(win: WindowInfo): WindowPaneInfo | undefined {
    const selectedId = selectedPaneIds[win.label]
    return win.panes.find((pane) => pane.id === selectedId)
      ?? win.panes.find((pane) => pane.is_active)
      ?? win.panes[0]
  }

  function choosePane(win: WindowInfo, pane: WindowPaneInfo) {
    selectedPaneIds = { ...selectedPaneIds, [win.label]: pane.id }
    onSelectPane(win, pane)
  }

  // ドラッグ中のファイル情報
  let draggingItem: {
    paths: string[]
    srcWindowLabel: string
    srcPaneId?: number
    sourceDir?: string
    name: string
    fromTray: boolean
  } | null = null
  let dropTargetKey: string | null = null
  let isCopyMode = false
  let progress: api.ProgressEvent | null = null
  let activeTransfer: {
    id: number
    srcWindowLabel: string
    targetLabel: string
    targetPaneId: number
    targetDir: string
    sourceDir?: string
    name: string
    moveFiles: boolean
    fromTray: boolean
  } | null = null
  let startingTransfer = false
  let earlyProgress: api.ProgressEvent | null = null
  let earlyDone: api.DoneEvent | null = null

  // listen のコールバックが await 中に書き換える値。関数境界を置くことで、
  // TypeScript に「直前に null を入れたまま」と誤って狭められないようにする。
  function takeEarlyEvents() {
    const buffered: { progress: api.ProgressEvent | null; done: api.DoneEvent | null } = {
      progress: earlyProgress,
      done: earlyDone,
    }
    earlyProgress = null
    earlyDone = null
    return buffered
  }

  // 表示対象ウィンドウと退避中ウィンドウ
  $: visibleWindows = windows.filter((w) => !hiddenLabels.has(w.label))
  $: hiddenWindows = windows.filter((w) => hiddenLabels.has(w.label))
  /** オーバーレイを呼び出す直前に最前面だった窓。Registry は最終フォーカス順で返す。 */
  $: traySource = windows[0]?.tray_paths.length ? windows[0] : undefined

  // 表示カード数に応じたレイアウト区分
  $: layoutMode =
    visibleWindows.length === 1
      ? 'single'
      : visibleWindows.length === 2
      ? 'split-2'
      : visibleWindows.length === 3
      ? 'split-3'
      : 'grid'

  // 表示中ウィンドウのディレクトリ内容を非同期読み込み
  $: {
    selectedPaneIds
    for (const w of visibleWindows) {
      const pane = selectedPane(w)
      if (pane?.kind === 'directory' && !dirCache[pane.path] && pane.path) {
        loadDir(pane.path)
      }
    }
  }

  async function loadDir(path: string) {
    dirCache[path] = { entries: [], loading: true }
    try {
      const listing = await api.listDir(path)
      dirCache[path] = { entries: listing.entries, loading: false }
    } catch (e) {
      dirCache[path] = { entries: [], loading: false, error: String(e) }
    }
  }

  async function refreshDir(path: string) {
    try {
      const listing = await api.listDir(path)
      dirCache[path] = { entries: listing.entries, loading: false }
    } catch {
      // 再取得失敗時は前回の状態を維持
    }
  }

  // ウィンドウを俯瞰画面から一時除外（目隠し）
  function hideFromWorkbench(label: string, ev?: MouseEvent) {
    ev?.stopPropagation()
    hiddenLabels.add(label)
    hiddenLabels = new Set(hiddenLabels)
  }

  // 退避トレイから復帰
  function restoreToWorkbench(label: string) {
    hiddenLabels.delete(label)
    hiddenLabels = new Set(hiddenLabels)
  }

  // すべての退避ウィンドウを復帰
  function restoreAll() {
    hiddenLabels.clear()
    hiddenLabels = new Set(hiddenLabels)
  }

  // ファイルドラッグ開始
  function handleDragStart(
    entry: Entry,
    win: WindowInfo,
    pane: WindowPaneInfo,
    parentPath: string,
    ev: DragEvent
  ) {
    const fullPath = joinPath(parentPath, entry.name)

    draggingItem = {
      paths: [fullPath],
      srcWindowLabel: win.label,
      srcPaneId: pane.id,
      sourceDir: parentPath,
      name: entry.name,
      fromTray: false,
    }
    isCopyMode = !!ev.ctrlKey

    if (ev.dataTransfer) {
      ev.dataTransfer.effectAllowed = 'copyMove'
      ev.dataTransfer.setData('text/plain', fullPath)
    }
  }

  function handleTrayDragStart(win: WindowInfo, paths: string[], ev: DragEvent) {
    draggingItem = {
      paths: [...paths],
      srcWindowLabel: win.label,
      name: paths.length === 1 ? splitPath(paths[0]).tail || paths[0] : `${paths.length}件のトレイ`,
      fromTray: true,
    }
    isCopyMode = !ev.shiftKey
    if (ev.dataTransfer) {
      ev.dataTransfer.effectAllowed = 'copyMove'
      ev.dataTransfer.setData('text/plain', paths.join('\n'))
    }
  }

  function handleDragEnd() {
    draggingItem = null
    dropTargetKey = null
  }

  function paneKey(win: WindowInfo, pane: WindowPaneInfo): string {
    return `${win.label}:${pane.id}`
  }

  function canDropOn(win: WindowInfo, pane: WindowPaneInfo): boolean {
    if (activeTransfer || pane.kind !== 'directory' || !draggingItem) return false
    return draggingItem.fromTray
      || draggingItem.srcWindowLabel !== win.label
      || draggingItem.srcPaneId !== pane.id
  }

  function handlePaneDragOver(win: WindowInfo, pane: WindowPaneInfo, ev: DragEvent) {
    ev.preventDefault()
    ev.stopPropagation()
    if (!canDropOn(win, pane)) {
      if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'none'
      return
    }
    isCopyMode = draggingItem?.fromTray ? !ev.shiftKey : ev.ctrlKey
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = isCopyMode ? 'copy' : 'move'
    dropTargetKey = paneKey(win, pane)
  }

  function handleCardDragOver(win: WindowInfo, ev: DragEvent) {
    const pane = selectedPane(win)
    if (!pane) return
    handlePaneDragOver(win, pane, ev)
  }

  function handleCardDragLeave(win: WindowInfo, _ev: DragEvent) {
    if (dropTargetKey?.startsWith(`${win.label}:`)) dropTargetKey = null
  }

  // カードへのドロップ（ファイル移動・コピー実行）
  async function handleCardDrop(win: WindowInfo, ev: DragEvent) {
    const pane = selectedPane(win)
    if (!pane) return
    await handlePaneDrop(win, pane, ev)
  }

  async function handlePaneDrop(win: WindowInfo, pane: WindowPaneInfo, ev: DragEvent) {
    ev.preventDefault()
    ev.stopPropagation()
    dropTargetKey = null
    if (activeTransfer) {
      onNote('転送中です。完了または中断後にもう一度操作してください')
      return
    }
    const dragged = draggingItem
    if (!dragged) return
    if (!canDropOn(win, pane)) {
      if (pane.kind === 'search') onNote('検索ペインは転送先にはできません')
      return
    }

    const srcPaths = dragged.paths
    const srcWindowLabel = dragged.srcWindowLabel
    const targetDir = pane.path
    // 通常項目は従来どおり Ctrl でコピー。収集トレイは安全側のコピーを既定にし、Shift で移動。
    const moveFiles = dragged.fromTray ? ev.shiftKey : !ev.ctrlKey

    try {
      startingTransfer = true
      earlyProgress = null
      earlyDone = null
      const id = await api.startTransfer(srcPaths, targetDir, moveFiles)
      activeTransfer = {
        id,
        srcWindowLabel,
        targetLabel: win.label,
        targetPaneId: pane.id,
        targetDir,
        sourceDir: dragged.sourceDir,
        name: dragged.name,
        moveFiles,
        fromTray: dragged.fromTray,
      }
      onNote(`「${dragged.name}」の${moveFiles ? '移動' : 'コピー'}を開始しました`)
      startingTransfer = false
      const buffered = takeEarlyEvents()
      if (buffered.progress?.id === id) progress = buffered.progress
      if (buffered.done?.id === id) await finishTransfer(buffered.done)
    } catch (e) {
      startingTransfer = false
      onNote(`転送を開始できません: ${e}`)
    } finally {
      draggingItem = null
    }
  }

  let unlistenProgress: UnlistenFn | null = null
  let unlistenDone: UnlistenFn | null = null

  async function finishTransfer(payload: api.DoneEvent) {
    const job = activeTransfer
    if (!job || payload.id !== job.id) return

    const { cancelled, created, completedSources, error } = payload
    progress = null
    activeTransfer = null

    if (job.moveFiles && job.fromTray && completedSources.length) {
      const source = windows.find((w) => w.label === job.srcWindowLabel)
      const moved = new Set(completedSources.map(api.pathIdentity))
      const remaining = source
        ? source.tray_paths.filter((path) => !moved.has(api.pathIdentity(path)))
        : []
      await api.setWindowTray(job.srcWindowLabel, remaining)
      if (source) source.tray_paths = remaining
      windows = windows
    }

    if (job.sourceDir) await refreshDir(job.sourceDir)
    await refreshDir(job.targetDir)

    if (error) onNote(`転送に失敗: ${error}`)
    else if (cancelled) onNote(`転送を中断しました（完了 ${created}件）`)
    else onNote(`「${job.name}」を${job.moveFiles ? '移動' : 'コピー'}しました（${created}件）`)
  }

  onMount(async () => {
    unlistenProgress = await listen<api.ProgressEvent>(api.TRANSFER_PROGRESS, (ev) => {
      if (ev.payload.id === activeTransfer?.id) progress = ev.payload
      else if (startingTransfer) earlyProgress = ev.payload
    })
    unlistenDone = await listen<api.DoneEvent>(api.TRANSFER_DONE, async (ev) => {
      if (ev.payload.id === activeTransfer?.id) await finishTransfer(ev.payload)
      else if (startingTransfer) earlyDone = ev.payload
    })
  })

  onDestroy(() => {
    unlistenProgress?.()
    unlistenDone?.()
  })

  // ファイルの種類に応じた絵文字アイコン
  function fileIcon(entry: Entry): string {
    if (entry.is_dir) return '📁'
    const ext = entry.ext || ''
    if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp'].includes(ext)) return '🖼️'
    if (['zip', 'rar', '7z', 'tar', 'gz'].includes(ext)) return '📦'
    if (['txt', 'md', 'json', 'ts', 'js', 'rs', 'html', 'css'].includes(ext)) return '📄'
    if (['mp3', 'wav', 'flac', 'm4a', 'aac'].includes(ext)) return '🎵'
    if (['mp4', 'mkv', 'avi', 'mov', 'webm'].includes(ext)) return '🎬'
    if (['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx'].includes(ext)) return '📑'
    if (['exe', 'msi', 'bat', 'cmd', 'ps1'].includes(ext)) return '⚙️'
    return '📄'
  }
</script>

<div class="workbench">
  <!-- メインカード領域（可視ウィンドウ） -->
  <div class="cards-viewport {layoutMode}">
    {#each visibleWindows as w (w.label)}
      {@const selected = selectedPane(w)}
      {@const displayPath = selected?.path ?? w.path}
      {@const parts = splitPath(displayPath)}
      {@const cache = selected?.kind === 'directory' ? dirCache[displayPath] : undefined}
      {@const isDropTarget = dropTargetKey?.startsWith(`${w.label}:`) ?? false}
      <div
        class="card"
        class:drop-target={isDropTarget}
        class:transferring={activeTransfer?.targetLabel === w.label}
        style="--hue: {pathHue(displayPath)}"
        on:click={() => onSelectWindow(w)}
        on:keydown={(e) => e.key === 'Enter' && onSelectWindow(w)}
        on:dragover={(e) => handleCardDragOver(w, e)}
        on:dragleave={(e) => handleCardDragLeave(w, e)}
        on:drop={(e) => handleCardDrop(w, e)}
        role="button"
        tabindex="0"
        aria-label="{parts.tail} フォルダカード"
      >
        <!-- カードヘッダー -->
        <div class="card-header">
          <div class="header-left">
            <span class="band" />
            <div class="titles">
              <span class="folder-name" title={displayPath}>{parts.tail || displayPath}</span>
              <span class="folder-path" title={displayPath}>{w.active_tab_label}{w.tab_count > 1 ? ` · 他${w.tab_count - 1}タブ` : ''}</span>
            </div>
          </div>
          <div class="header-actions">
            <button
              type="button"
              class="action-btn hide-btn"
              title="この窓を一時除外（トレイへ退避）"
              on:click={(e) => hideFromWorkbench(w.label, e)}
            >
              👁️
            </button>
            <button
              type="button"
              class="action-btn focus-btn"
              title="この窓へジャンプ"
              on:click|stopPropagation={() => onSelectWindow(w)}
            >
              ↗️
            </button>
            <button
              type="button"
              class="action-btn close-btn"
              title="この窓を閉じる"
              on:click|stopPropagation={() => onCloseWindow(w.label)}
            >
              ✕
            </button>
          </div>
        </div>

        <div class="pane-map" aria-label="現在のタブのペイン">
          {#each w.panes as pane, paneIndex (`${w.label}-${pane.id}`)}
            {@const paneParts = splitPath(pane.path)}
            <button
              type="button"
              class:selected={pane.id === selected?.id}
              class:active-pane={pane.is_active}
              class:search-pane={pane.kind === 'search'}
              class:drop-target={dropTargetKey === paneKey(w, pane)}
              class:transferring={activeTransfer?.targetLabel === w.label && activeTransfer?.targetPaneId === pane.id}
              title={pane.path}
              on:click|stopPropagation={() => choosePane(w, pane)}
              on:dragover={(event) => handlePaneDragOver(w, pane, event)}
              on:dragleave|stopPropagation={() => {
                if (dropTargetKey === paneKey(w, pane)) dropTargetKey = null
              }}
              on:drop={(event) => handlePaneDrop(w, pane, event)}
            >
              <span>{pane.kind === 'search' ? '⌕' : '▣'} {paneIndex + 1}</span>
              <strong>{pane.kind === 'search' && pane.query ? pane.query : paneParts.tail || pane.path}</strong>
              <small>{pane.kind === 'search' ? '検索ペイン' : elideLeft(pane.path, 28)}</small>
            </button>
          {/each}
        </div>

        <!-- ドロップオーバー時のオーバーレイバッジ -->
        {#if isDropTarget}
          <div class="drop-banner">
            <span>{isCopyMode ? '📥 ここへコピー' : '📦 ここへ移動'}</span>
          </div>
        {/if}

        {#if activeTransfer?.targetLabel === w.label}
          <div class="transfer-badge">
            {progress?.scanning ? '転送量を集計中…' : `${progress?.filesDone ?? 0} / ${progress?.filesTotal ?? '…'} 件`}
          </div>
        {/if}

        <!-- カードボディ：ファイル一覧（ミニファイラー） -->
        <div class="card-body">
          {#if selected?.kind === 'search'}
            <div class="search-state">
              <span>⌕</span>
              <strong>{selected.query || '検索語なし'}</strong>
              <small title={selected.path}>{selected.path}</small>
              <p>検索ペインは転送元として使い、転送先にはしません。</p>
            </div>
          {:else if !cache || cache.loading}
            <div class="loading-state">読み込み中…</div>
          {:else if cache.error}
            <div class="error-state">フォルダを読めません: {cache.error}</div>
          {:else if cache.entries.length === 0}
            <div class="empty-state">（空のフォルダー）</div>
          {:else}
            <div class="file-grid">
              {#each cache.entries as entry (entry.name)}
                <div
                  class="file-item"
                  class:is-dir={entry.is_dir}
                  draggable="true"
                  on:dragstart={(e) => selected && handleDragStart(entry, w, selected, displayPath, e)}
                  on:dragend={handleDragEnd}
                  title="{entry.name} ({entry.is_dir ? 'フォルダ' : formatSize(entry.size)})"
                  role="presentation"
                >
                  <span class="item-icon">{fileIcon(entry)}</span>
                  <span class="item-name">{entry.name}</span>
                  {#if !entry.is_dir}
                    <span class="item-size">{formatSize(entry.size)}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>

        <!-- カードフッター -->
        <div class="card-footer">
          <span class="entry-count">
            {selected?.kind === 'search' ? '検索ペイン' : cache?.entries ? `${cache.entries.length} 項目` : '…'}
          </span>
          <span class="dnd-hint">
            {pinned ? '📌固定中 · ' : ''}他カードへD&Dで移動 / Ctrl+D&Dでコピー
          </span>
        </div>
      </div>
    {/each}

    {#if visibleWindows.length === 0}
      <div class="all-hidden-state">
        <p>すべてのウィンドウが退避トレイに入っています</p>
        <button type="button" class="btn-restore-all" on:click={restoreAll}>
          すべてのウィンドウを復帰する
        </button>
      </div>
    {/if}
  </div>

  {#if traySource}
    <div class="collection-tray">
      <div class="collection-copy">
        <strong>◈ 収集トレイ</strong>
        <span>{traySource.tray_paths.length}件</span>
        <small>カードへドラッグでコピー・Shiftを押しながらで移動</small>
      </div>
      <div
        class="collection-items"
      >
        {#each traySource.tray_paths.slice(0, 5) as path (api.pathIdentity(path))}
          <button
            type="button"
            class="collection-pill"
            draggable="true"
            on:dragstart={(e) => handleTrayDragStart(traySource, [path], e)}
            on:dragend={handleDragEnd}
            title="この項目だけ送り先カードへドラッグ"
          >{splitPath(path).tail || path}</button>
        {/each}
        {#if traySource.tray_paths.length > 5}
          <span class="collection-more">+{traySource.tray_paths.length - 5}</span>
        {/if}
        <button
          type="button"
          class="drag-all"
          draggable="true"
          on:dragstart={(e) => handleTrayDragStart(traySource, traySource.tray_paths, e)}
          on:dragend={handleDragEnd}
          title="トレイ全体を送り先カードへドラッグ"
        >全件を運ぶ ↗</button>
      </div>
    </div>
  {/if}

  <!-- 下部：一時退避トレイ（除外されたウィンドウ群） -->
  {#if hiddenWindows.length > 0}
    <div class="hidden-tray">
      <div class="tray-label">
        <span>📥 一時退避中 ({hiddenWindows.length})</span>
        <button type="button" class="tray-restore-all" on:click={restoreAll}>
          すべて戻す
        </button>
      </div>
      <div class="tray-items">
        {#each hiddenWindows as hw (hw.label)}
          {@const p = splitPath(hw.path)}
          <button
            type="button"
            class="tray-pill"
            style="--hue: {pathHue(hw.path)}"
            title="{hw.path} を俯瞰画面に戻す"
            on:click={() => restoreToWorkbench(hw.label)}
          >
            <span class="pill-band" />
            <span class="pill-name">{p.tail || hw.path}</span>
            <span class="pill-add">＋</span>
          </button>
        {/each}
      </div>
    </div>
  {/if}
  <TransferBar {progress} />
</div>

<style>
  .workbench {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    gap: 8px;
  }

  .collection-tray { display: flex; align-items: center; gap: 12px; flex: none; padding: 8px 12px; border: 1px solid #285847; border-radius: 7px; background: #172a25; color: #cdebe1; }
  .collection-copy { display: flex; align-items: baseline; gap: 7px; flex: none; }
  .collection-copy strong { color: #65d0ad; font-size: 12px; }
  .collection-copy span { font-size: 11px; }
  .collection-copy small { color: #75988d; font-size: 9.5px; }
  .collection-items { display: flex; align-items: center; gap: 5px; flex: 1; min-width: 0; overflow: hidden; cursor: grab; }
  .collection-items:active { cursor: grabbing; }
  .collection-pill { max-width: 150px; padding: 4px 7px; border: 1px solid #376e5c; border-radius: 12px; background: #203b33; color: inherit; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font: inherit; font-size: 10px; cursor: grab; }
  .collection-more { color: #69c6a8; font-size: 10px; }
  .drag-all { margin-left: auto; flex: none; padding: 4px 8px; border: 0; border-radius: 4px; background: #2a594a; color: #d9f5eb; font: inherit; font-size: 10px; cursor: grab; }

  .cards-viewport {
    flex: 1;
    min-height: 0;
    padding: 10px 14px;
    display: grid;
    gap: 12px;
    overflow-y: auto;
  }

  /* オートスプリット レイアウト */
  .cards-viewport.single {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr;
  }
  .cards-viewport.split-2 {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr;
  }
  .cards-viewport.split-3 {
    grid-template-columns: 1fr 1fr 1fr;
    grid-template-rows: 1fr;
  }
  .cards-viewport.grid {
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    grid-auto-rows: minmax(280px, 1fr);
  }

  /* カードデザイン */
  .card {
    display: flex;
    flex-direction: column;
    background: #202022;
    border: 1px solid #36363a;
    border-radius: 9px;
    overflow: hidden;
    transition: border-color 0.15s, box-shadow 0.15s, transform 0.1s;
    position: relative;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35);
    outline: none;
    text-align: left;
  }
  .card:hover,
  .card:focus-visible {
    border-color: #4f4f58;
  }
  .card.drop-target {
    border-color: #3b82f6 !important;
    box-shadow: 0 0 16px rgba(59, 130, 246, 0.45);
    transform: scale(1.005);
  }
  .card.transferring { border-color: #4c9aff; box-shadow: 0 0 14px rgba(76, 154, 255, .3); }
  .transfer-badge { position: absolute; z-index: 11; top: 51px; right: 9px; padding: 4px 8px; border-radius: 4px; background: #214a73; color: #d7eaff; font-size: 10px; pointer-events: none; }

  .drop-banner {
    position: absolute;
    top: 96px;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(14, 28, 54, 0.88);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    font-weight: bold;
    color: #60a5fa;
    z-index: 10;
    pointer-events: none;
    border: 2px dashed #3b82f6;
    border-radius: 0 0 8px 8px;
  }

  /* カードヘッダー */
  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    background: #18181a;
    border-bottom: 1px solid #2d2d31;
    gap: 8px;
  }
  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    flex: 1;
  }
  .band {
    width: 4px;
    height: 24px;
    border-radius: 2px;
    background: hsl(var(--hue) 75% 55%);
    flex: none;
  }
  .titles {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .folder-name {
    font-size: 14px;
    font-weight: 600;
    color: #f3f3f3;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folder-path {
    font-size: 11px;
    color: #8a8a93;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .pane-map { display: flex; gap: 5px; padding: 6px 8px; border-bottom: 1px solid #303035; background: #1b1b1e; }
  .pane-map button { display: grid; min-width: 0; flex: 1; grid-template-columns: auto 1fr; gap: 1px 5px; border: 1px solid #363940; border-radius: 5px; background: #242529; padding: 5px 7px; color: #89939c; text-align: left; cursor: pointer; }
  .pane-map button:hover { border-color: #525965; background: #2b2d32; }
  .pane-map button.selected { border-color: #4c9aff; background: #26384d; color: #8fc1f5; }
  .pane-map button.active-pane { box-shadow: inset 2px 0 0 #63cfad; }
  .pane-map button.search-pane { border-style: dashed; }
  .pane-map button.drop-target { border-color: #60a5fa; background: #173f68; box-shadow: 0 0 10px rgba(59, 130, 246, .45); }
  .pane-map button.transferring { border-color: #63cfad; }
  .pane-map span { grid-row: 1 / 3; align-self: center; font-size: 10px; }
  .pane-map strong, .pane-map small { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pane-map strong { color: #d5d9dd; font-size: 10px; font-weight: 600; }
  .pane-map small { color: #707983; font-size: 8.5px; }
  .action-btn {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: #2a2a2e;
    border: 1px solid #3c3c42;
    border-radius: 5px;
    color: #bbb;
    font-size: 12px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .action-btn:hover {
    background: #38383e;
    color: #fff;
  }
  .close-btn:hover {
    background: #b91c1c;
    color: #fff;
    border-color: #ef4444;
  }

  /* カードボディ */
  .card-body {
    flex: 1;
    min-height: 120px;
    overflow-y: auto;
    padding: 8px;
    background: #1e1e21;
  }

  .loading-state,
  .error-state,
  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: #777;
    font-size: 12px;
    padding: 20px;
  }
  .error-state {
    color: #f87171;
  }
  .search-state { display: flex; height: 100%; align-items: center; justify-content: center; flex-direction: column; color: #74818b; text-align: center; }
  .search-state > span { color: #63cfad; font-size: 30px; }
  .search-state strong { max-width: 90%; margin-top: 5px; overflow: hidden; color: #d8dcdf; text-overflow: ellipsis; white-space: nowrap; }
  .search-state small { max-width: 90%; margin-top: 4px; overflow: hidden; color: #74818b; text-overflow: ellipsis; white-space: nowrap; }
  .search-state p { margin: 12px 0 0; color: #59636b; font-size: 10px; }

  .file-grid {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: 5px;
    cursor: grab;
    user-select: none;
    transition: background 0.1s;
    font-size: 12px;
  }
  .file-item:hover {
    background: #2a2a30;
  }
  .file-item:active {
    cursor: grabbing;
    background: #35353d;
  }
  .item-icon {
    font-size: 14px;
    flex: none;
  }
  .item-name {
    flex: 1;
    min-width: 0;
    color: #ddd;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file-item.is-dir .item-name {
    font-weight: 500;
    color: #93c5fd;
  }
  .item-size {
    font-size: 11px;
    color: #71717a;
    flex: none;
  }

  /* カードフッター */
  .card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 12px;
    background: #18181b;
    border-top: 1px solid #28282c;
    font-size: 11px;
    color: #71717a;
  }
  .dnd-hint {
    font-size: 10px;
    color: #52525b;
  }

  /* 全退避時 */
  .all-hidden-state {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 60px 20px;
    color: #888;
    gap: 12px;
  }
  .btn-restore-all {
    padding: 8px 16px;
    background: #3b82f6;
    border: none;
    border-radius: 6px;
    color: #fff;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }
  .btn-restore-all:hover {
    background: #2563eb;
  }

  /* 下部退避トレイ */
  .hidden-tray {
    display: flex;
    flex-direction: column;
    padding: 8px 14px 10px;
    background: #141416;
    border-top: 1px solid #2b2b30;
    gap: 6px;
    flex: none;
  }
  .tray-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    color: #8a8a93;
    font-weight: 500;
  }
  .tray-restore-all {
    background: none;
    border: none;
    color: #60a5fa;
    cursor: pointer;
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .tray-restore-all:hover {
    background: rgba(96, 165, 250, 0.12);
  }
  .tray-items {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .tray-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px 4px 6px;
    background: #232328;
    border: 1px solid #383840;
    border-radius: 20px;
    color: #ddd;
    font-size: 12px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s;
  }
  .tray-pill:hover {
    background: #2c2c33;
    border-color: #555;
    color: #fff;
  }
  .pill-band {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: hsl(var(--hue) 75% 55%);
  }
  .pill-name {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pill-add {
    color: #60a5fa;
    font-weight: bold;
    font-size: 13px;
  }
</style>
