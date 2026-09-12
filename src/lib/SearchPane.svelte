<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { listen, type UnlistenFn } from '@tauri-apps/api/event'
  import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener'
  import * as api from './api'
  import type { PaneKind, SavedSearchState, Settings } from './api'
  import FileList from './FileList.svelte'
  import Preview from './Preview.svelte'
  import { splitPath } from './api'
  import { matchAction } from './shortcuts'

  export let directoryPath: string
  export let search: SavedSearchState
  export let settings: Settings
  export let dragIcon = ''
  export let trayItems: string[] = []
  export let closable = false
  export let active = false
  export let multi = false
  export let keyboardTarget = false
  export let onSearchChange: (search: SavedSearchState) => void = () => {}
  export let onKindChange: (kind: PaneKind) => void = () => {}
  export let onOpenDirectory: (path: string) => void = () => {}
  export let onTrayToggle: (path: string) => void = () => {}
  export let onSplit: (path: string) => void = () => {}
  export let onClose: () => void = () => {}
  export let onActivate: () => void = () => {}
  export let onHoverChange: (hovered: boolean) => void = () => {}
  export let onNote: (message: string) => void = () => {}

  $: scope = search.scopePaths[0] || directoryPath
  $: scopeLabel = splitPath(scope).tail || scope
  let results: api.SearchEntry[] = []
  let selection: string[] = []
  let showPreview = search.showPreview ?? settings.showPreview
  let sort: api.SortSpec = {
    key: search.sortKey ?? settings.sortKey,
    descending: search.sortDescending ?? settings.sortDescending,
    dirsFirst: search.dirsFirst ?? settings.dirsFirst,
    showHidden: true,
  }
  type SearchListEntry = api.Entry & { path: string }
  $: listEntries = results.map((entry) => ({
    ...entry,
    is_dir: entry.isDir,
    hidden: false,
  })) as SearchListEntry[]

  function resultPath(entry: api.Entry): string {
    return (entry as SearchListEntry).path
  }

  function resultParent(entry: api.Entry): string {
    return splitPath(resultPath(entry)).lead
  }
  let requestId: string | null = null
  let running = false
  let scanned = 0
  let status = '検索語を入力してください'
  let unlistenBatch: UnlistenFn | null = null
  let unlistenDone: UnlistenFn | null = null

  export function currentPath(): string {
    return directoryPath
  }

  export async function reload() {
    if (search.query.trim()) await runSearch()
  }

  export async function acceptDrop(_paths: string[]) {
    onNote('検索ペインはコピー・移動先にはできません')
  }

  function updateQuery(query: string) {
    onSearchChange({ ...search, query })
  }

  function updateScope(scopePath: string) {
    onSearchChange({ ...search, scopePaths: [scopePath] })
  }

  function toggleMatchPath() {
    onSearchChange({ ...search, matchPath: !search.matchPath })
  }

  function togglePreview() {
    showPreview = !showPreview
    onSearchChange({ ...search, showPreview })
  }

  function nextRequestId(): string {
    return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`
  }

  async function runSearch() {
    const query = search.query.trim()
    const roots = search.scopePaths.filter((path) => path.trim())
    if (!query || roots.length === 0) {
      status = !query ? '検索語を入力してください' : '検索対象を入力してください'
      return
    }
    if (requestId) await api.cancelSearch(requestId).catch(() => {})
    const id = nextRequestId()
    requestId = id
    results = []
    scanned = 0
    running = true
    status = '検索しています…'
    try {
      await api.startSearch(id, roots, query, search.matchPath)
    } catch (error) {
      if (requestId !== id) return
      running = false
      status = String(error)
    }
  }

  async function stopSearch() {
    if (!requestId) return
    await api.cancelSearch(requestId).catch(() => {})
    status = '停止しています…'
  }

  function orderedResults(values: api.SearchEntry[]): api.SearchEntry[] {
    const direction = sort.descending ? -1 : 1
    return [...values].sort((a, b) => {
      if (sort.dirsFirst && a.isDir !== b.isDir) return a.isDir ? -1 : 1
      let compared = 0
      if (sort.key === 'size') compared = a.size - b.size
      else if (sort.key === 'modified') compared = a.modified - b.modified
      else if (sort.key === 'ext') compared = a.ext.localeCompare(b.ext)
      else compared = a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
      return compared * direction || a.path.localeCompare(b.path)
    })
  }

  function sortResults(key: api.SortKey) {
    if (sort.key === key) sort = { ...sort, descending: !sort.descending }
    else sort = { ...sort, key, descending: false }
    onSearchChange({
      ...search,
      sortKey: sort.key,
      sortDescending: sort.descending,
      dirsFirst: sort.dirsFirst,
    })
    results = orderedResults(results)
  }

  async function launch(_entry: api.Entry, path: string) {
    try {
      await openPath(path)
    } catch (error) {
      onNote(`開けません: ${error}`)
    }
  }

  async function revealSelection() {
    if (!selection[0]) return
    try {
      await revealItemInDir(selection[0])
    } catch (error) {
      onNote(`場所を表示できません: ${error}`)
    }
  }

  async function copySelection(cut: boolean) {
    if (!selection.length) return
    await api.setClipboard(selection, cut)
    onNote(`${cut ? '切り取り' : 'コピー'} ${selection.length}件`)
  }

  async function copyPaths() {
    if (!selection.length) return
    try {
      await navigator.clipboard.writeText(selection.join('\r\n'))
      onNote(`${selection.length}件のパスをコピー`)
    } catch (error) {
      onNote(`コピーできません: ${error}`)
    }
  }

  onMount(async () => {
    unlistenBatch = await listen<api.SearchBatchEvent>(api.SEARCH_BATCH, ({ payload }) => {
      if (payload.id !== requestId) return
      results = orderedResults([...results, ...payload.entries])
      scanned = payload.scanned
      status = `${scanned.toLocaleString()}件を確認中`
    })
    unlistenDone = await listen<api.SearchDoneEvent>(api.SEARCH_DONE, ({ payload }) => {
      if (payload.id !== requestId) return
      running = false
      scanned = payload.scanned
      if (payload.error) {
        status = payload.error
      } else if (payload.cancelled) {
        status = `停止しました — ${results.length.toLocaleString()}件`
      } else {
        const warning = payload.warningCount ? `・読めない場所 ${payload.warningCount}件` : ''
        const limited = payload.truncated ? '・上限に達しました' : ''
        status = `${payload.matched.toLocaleString()}件・${payload.scanned.toLocaleString()}件を確認${warning}${limited}`
      }
    })
    // 保存するのは条件だけ。復元時はリスナー準備後に新しい結果を作り直す。
    if (search.query.trim() && search.scopePaths.some((path) => path.trim())) await runSearch()
  })

  onDestroy(() => {
    if (requestId) api.cancelSearch(requestId).catch(() => {})
    unlistenBatch?.()
    unlistenDone?.()
  })

  function onPaneKey(ev: KeyboardEvent) {
    const el = ev.target as HTMLElement | null
    if (!keyboardTarget || (el && (el.tagName === 'INPUT' || el.isContentEditable))) return
    switch (matchAction(ev, settings.shortcuts)) {
      case 'copy':
        ev.preventDefault()
        copySelection(false)
        break
      case 'cut':
        ev.preventDefault()
        copySelection(true)
        break
      case 'copyPath':
        ev.preventDefault()
        copyPaths()
        break
      case 'reload':
        ev.preventDefault()
        reload()
        break
      case 'hoverClosePane':
        if (!closable) return
        ev.preventDefault()
        onClose()
        break
      case 'hoverSplitPane':
        ev.preventDefault()
        onSplit(directoryPath)
        break
      case 'hoverPreview':
        ev.preventDefault()
        togglePreview()
        break
      default:
        break
    }
  }
</script>

<svelte:window on:keydown={onPaneKey} />

<section
  class="search-pane"
  class:active
  class:multi
  class:keyboard-target={keyboardTarget}
  on:pointerdown={onActivate}
  on:pointerenter={() => onHoverChange(true)}
  on:pointerleave={() => onHoverChange(false)}
>
  <header>
    <div class="identity">
      <span class="kind">検索</span>
      <span class="scope" title={scope}>対象: {scopeLabel}</span>
    </div>
    <div class="actions">
      <button type="button" title={showPreview ? 'プレビューを隠す' : 'プレビューを出す'} class:on={showPreview} on:click={togglePreview}>◐</button>
      <button type="button" title="選択項目をトレイへ追加・解除" disabled={selection.length === 0} on:click={() => selection.forEach(onTrayToggle)}>◈</button>
      <button type="button" title="選択項目の場所を表示" disabled={selection.length === 0} on:click={revealSelection}>⧉</button>
      <button type="button" title="通常のフォルダペインに戻す" on:click={() => onKindChange('directory')}>▣</button>
      <button type="button" title="このペインを左右に分割" on:click={() => onSplit(directoryPath)}>⫿</button>
      {#if closable}<button type="button" title="このペインを閉じる" on:click={onClose}>✕</button>{/if}
    </div>
  </header>

  <div class="scope-row">
    <span>対象</span>
    <input class="scope-input" value={scope} spellcheck="false" aria-label="検索対象" on:change={(event) => updateScope(event.currentTarget.value.trim())} />
  </div>
  <div class="query-row">
    <input
      value={search.query}
      placeholder="検索する名前やパス"
      spellcheck="false"
      aria-label="検索語"
      on:input={(event) => updateQuery(event.currentTarget.value)}
      on:keydown={(event) => {
        if (event.key === 'Enter') runSearch()
        if (event.key === 'Escape' && running) stopSearch()
      }}
    />
    <button type="button" class:on={search.matchPath} title="ファイル名だけでなくフォルダのパスも検索" on:click={toggleMatchPath}>パス</button>
    {#if running}
      <button type="button" class="stop" on:click={stopSearch}>停止</button>
    {:else}
      <button type="button" disabled={!search.query.trim() || !scope.trim()} on:click={runSearch}>検索</button>
    {/if}
  </div>

  <div class="status" class:searching={running}>{status}</div>
  {#if results.length === 0}
    <div class="empty">
      <span class="mark">⌕</span>
      <strong>{running ? '検索中…' : '検索ペイン'}</strong>
      <p>{running ? `${scanned.toLocaleString()}件を確認しました` : '名前やパスを横断して探します。'}</p>
      <small>Enterで検索、検索中はEscapeで停止</small>
    </div>
  {:else}
    <div class="result-area">
      <div class="list-slot">
        <FileList
          entries={listEntries}
          parent={null}
          path={`search:${requestId ?? 'idle'}`}
          {dragIcon}
          {sort}
          {trayItems}
          {onTrayToggle}
          resolvePath={resultPath}
          secondaryLabel={resultParent}
          onOpen={onOpenDirectory}
          onLaunch={launch}
          onSort={sortResults}
          onSelectionChange={(paths) => (selection = paths)}
          {onNote}
        />
      </div>
      {#if showPreview}
        <div class="preview-slot"><Preview path={selection.length === 1 ? selection[0] : null} /></div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .search-pane { display: flex; flex: 1; min-width: 0; min-height: 0; flex-direction: column; background: #191b1d; color: #ddd; }
  .search-pane.multi:not(.active) { background: #151719; }
  .search-pane.multi.active { box-shadow: inset 0 2px 0 #4c9aff; }
  .search-pane.multi.keyboard-target { box-shadow: inset 0 2px 0 #63cfad, inset 0 0 0 1px rgba(99, 207, 173, 0.22); }
  header { display: flex; min-height: 64px; align-items: center; justify-content: space-between; border-bottom: 1px solid #303438; background: #202326; padding: 0 9px 0 14px; }
  .identity { display: flex; min-width: 0; flex-direction: column; gap: 4px; }
  .kind { color: #f0f0f0; font-size: 18px; font-weight: 700; }
  .scope { overflow: hidden; color: #8f9ba5; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
  .actions { display: flex; gap: 4px; }
  button { height: 26px; border: 1px solid #454b50; border-radius: 4px; background: #303438; color: #b8c0c7; cursor: pointer; }
  button:disabled { cursor: default; opacity: 0.45; }
  .actions button { width: 26px; height: 24px; padding: 0; }
  .actions button.on { border-color: #4c9aff; background: #35506f; color: #fff; }
  .scope-row, .query-row { display: flex; align-items: center; gap: 6px; padding: 7px 10px; }
  .scope-row { border-bottom: 1px solid #292d30; color: #89949c; font-size: 11px; }
  .query-row { border-bottom: 1px solid #2b2f32; padding-top: 0; }
  input { min-width: 0; flex: 1; border: 1px solid #3c444a; border-radius: 4px; outline: none; background: #22262a; color: #eee; padding: 6px 9px; }
  input:focus { border-color: #63cfad; box-shadow: 0 0 0 1px rgba(99, 207, 173, 0.2); }
  .scope-input { border-color: transparent; background: transparent; color: #aab2b8; padding: 4px 6px; }
  .stop { border-color: #80504e; color: #f0aaa4; }
  .status { min-height: 18px; border-bottom: 1px solid #292d30; color: #89949c; font-size: 11px; padding: 4px 11px; }
  .status.searching { color: #63cfad; }
  .empty { display: grid; flex: 1; place-content: center; justify-items: center; color: #7e8992; text-align: center; }
  .empty .mark { margin-bottom: 8px; color: #63cfad; font-size: 36px; }
  .empty strong { color: #c8ced3; }
  .empty p { margin: 8px 0 3px; }
  .empty small { color: #68727a; }
  .result-area { display: flex; flex: 1; min-height: 0; }
  .list-slot { display: flex; flex: 1; min-width: 0; min-height: 0; flex-direction: column; container-type: inline-size; }
  .preview-slot { width: 260px; min-width: 160px; border-left: 1px solid #2c2c2c; }
</style>
