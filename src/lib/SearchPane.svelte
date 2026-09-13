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
  export let onSplitSearch: (path: string, search?: SavedSearchState) => void = () => {}
  export let onClose: () => void = () => {}
  export let onActivate: () => void = () => {}
  export let onHoverChange: (hovered: boolean) => void = () => {}
  export let onNote: (message: string) => void = () => {}

  $: scopes = search.scopePaths.length ? search.scopePaths : [directoryPath]
  $: scope = scopes.join('; ')
  $: scopeLabel = scopes.length > 1 ? `${scopes.length}か所` : splitPath(scopes[0]).tail || scopes[0]
  let results: api.SearchEntry[] = []
  let selection: string[] = []
  let showPreview = search.showPreview ?? settings.showPreview
  let showHistory = search.showHistory ?? true
  let locationHistory: api.SearchLocationEntry[] = []
  let favoriteLocations: string[] = []
  $: currentSingleScopeFavorite = scopes.length === 1 && favoriteLocations.some((path) => api.pathIdentity(path) === api.pathIdentity(scopes[0]))
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
  let filterRequestId = 0
  let running = false
  let paused = false
  let scanned = 0
  let matched = 0
  let truncated = false
  let status = '検索対象を読み込んでいます…'
  let unlistenProgress: UnlistenFn | null = null
  let unlistenResults: UnlistenFn | null = null
  let unlistenDone: UnlistenFn | null = null

  export function currentPath(): string {
    return directoryPath
  }

  export async function reload() {
    await runIndex()
  }

  export async function acceptDrop(_paths: string[]) {
    onNote('検索ペインはコピー・移動先にはできません')
  }

  function updateQuery(query: string) {
    onSearchChange({ ...search, query })
    requestFilter({ query })
  }

  function updateScope(scopeText: string) {
    const scopePaths = scopeText
      .split(/[;；\n]/)
      .map((path) => path.trim())
      .filter(Boolean)
    onSearchChange({ ...search, scopePaths })
    if (scopePaths.length) runIndex(scopePaths)
  }

  function toggleMatchPath() {
    onSearchChange({ ...search, matchPath: !search.matchPath })
    requestFilter({ matchPath: !search.matchPath })
  }

  function togglePreview() {
    showPreview = !showPreview
    onSearchChange({ ...search, showPreview })
  }

  function toggleHistory() {
    showHistory = !showHistory
    onSearchChange({ ...search, showHistory })
  }

  async function refreshLocationHistory() {
    ;[locationHistory, favoriteLocations] = await Promise.all([
      api.listSearchLocations(),
      api.listFavorites(),
    ])
  }

  function locationLabel(paths: string[]): string {
    const first = paths[0] ?? ''
    const name = splitPath(first).tail || first
    return paths.length > 1 ? `${name} ほか${paths.length - 1}か所` : name
  }

  async function revisitLocation(paths: string[]) {
    onSearchChange({ ...search, scopePaths: paths })
    await runIndex(paths)
  }

  async function forgetLocation(paths: string[]) {
    await api.removeSearchLocation(paths)
    await refreshLocationHistory()
  }

  async function clearLocationHistory() {
    await api.clearSearchLocations()
    locationHistory = []
  }

  async function toggleCurrentScopeFavorite() {
    if (scopes.length !== 1) {
      onNote('複数の検索場所はまとめてお気に入り登録できません')
      return
    }
    await api.toggleFavorite(scopes[0])
    favoriteLocations = await api.listFavorites()
  }

  function nextRequestId(): string {
    return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random()}`
  }

  function rememberQuery() {
    const query = search.query.trim()
    if (!query) return
    const recentQueries = [query, ...(search.recentQueries ?? []).filter((item) => item.toLocaleLowerCase() !== query.toLocaleLowerCase())].slice(0, 10)
    onSearchChange({ ...search, query, recentQueries })
  }

  async function runIndex(overrideRoots?: string[]) {
    const roots = (overrideRoots ?? search.scopePaths).filter((path) => path.trim())
    if (roots.length === 0) {
      status = '検索対象を入力してください'
      return
    }
    if (requestId) await api.cancelSearch(requestId).catch(() => {})
    const id = nextRequestId()
    requestId = id
    results = []
    scanned = 0
    running = true
    status = 'ファイルを読み込んでいます…'
    try {
      await api.startSearch(id, roots)
      await api.recordSearchLocation(roots)
      await refreshLocationHistory()
      requestFilter()
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

  async function togglePause() {
    if (!requestId) return
    if (paused) await api.resumeSearch(requestId)
    else await api.pauseSearch(requestId)
    paused = !paused
    status = paused ? '一時停止中（検索は使えます）' : '読み込みを再開しています…'
  }

  function requestFilter(override: Partial<api.SearchFilterOptions> = {}) {
    if (!requestId) return
    const id = ++filterRequestId
    api.filterSearch(requestId, id, {
      query: search.query,
      matchPath: search.matchPath,
      sortKey: sort.key,
      descending: sort.descending,
      dirsFirst: sort.dirsFirst,
      limit: 500,
      ...override,
    }).catch((error) => {
      if (id === filterRequestId) status = String(error)
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
    requestFilter({ sortKey: sort.key, descending: sort.descending })
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
    await refreshLocationHistory()
    unlistenProgress = await listen<api.SearchProgressEvent>(api.SEARCH_PROGRESS, ({ payload }) => {
      if (payload.id !== requestId) return
      scanned = payload.indexed
      status = `${scanned.toLocaleString()}件を読み込み中`
    })
    unlistenResults = await listen<api.SearchResultsEvent>(api.SEARCH_RESULTS, ({ payload }) => {
      if (payload.id !== requestId || payload.requestId !== filterRequestId) return
      results = payload.entries
      scanned = payload.indexed
      matched = payload.matched
      truncated = payload.truncated
    })
    unlistenDone = await listen<api.SearchDoneEvent>(api.SEARCH_DONE, ({ payload }) => {
      if (payload.id !== requestId) return
      running = false
      scanned = payload.indexed
      if (payload.error) {
        status = payload.error
      } else if (payload.cancelled) {
        status = `停止しました — ${payload.indexed.toLocaleString()}件を読み込み済み`
      } else {
        const warning = payload.warningCount ? `・読めない場所 ${payload.warningCount}件` : ''
        status = `${payload.indexed.toLocaleString()}件を読み込み済み${warning}`
      }
    })
    // ペインへ切り替えた時点で走査を開始する。検索語は開始条件にしない。
    if (search.scopePaths.some((path) => path.trim())) await runIndex()
  })

  onDestroy(() => {
    if (requestId) api.cancelSearch(requestId).catch(() => {})
    unlistenProgress?.()
    unlistenResults?.()
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
      case 'hoverSplitSearchPane':
        if (ev.repeat) break
        ev.preventDefault()
        onSplitSearch(directoryPath, search)
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
      <button type="button" title={showHistory ? '検索履歴を隠す' : '検索履歴を表示'} class:on={showHistory} on:click={toggleHistory}>履</button>
      <button type="button" title={showPreview ? 'プレビューを隠す' : 'プレビューを出す'} class:on={showPreview} on:click={togglePreview}>◐</button>
      <button type="button" title="選択項目をトレイへ追加・解除" disabled={selection.length === 0} on:click={() => selection.forEach(onTrayToggle)}>◈</button>
      <button type="button" title="選択項目の場所を表示" disabled={selection.length === 0} on:click={revealSelection}>⧉</button>
      <button type="button" title="通常のフォルダペインに戻す" on:click={() => onKindChange('directory')}>▣</button>
      <button type="button" title="このペインを左右に分割" on:click={() => onSplit(directoryPath)}>⫿</button>
      <button class="search-split" type="button" title="同じ検索条件のペインを隣に追加 (Shift+N)" on:click={() => onSplitSearch(directoryPath, search)}>⫿⌕</button>
      {#if closable}<button type="button" title="このペインを閉じる" on:click={onClose}>✕</button>{/if}
    </div>
  </header>

  <div class="scope-row">
    <span>対象</span>
    <input class="scope-input" value={scope} placeholder="複数指定は ; で区切る" spellcheck="false" aria-label="検索対象" on:change={(event) => updateScope(event.currentTarget.value)} />
  </div>
  <div class="query-row">
    <input
      value={search.query}
      placeholder="検索する名前やパス"
      spellcheck="false"
      aria-label="検索語"
      on:input={(event) => updateQuery(event.currentTarget.value)}
      on:blur={rememberQuery}
      on:keydown={(event) => {
        if (event.key === 'Enter') rememberQuery()
        if (event.key === 'Escape' && running) stopSearch()
      }}
    />
    <button type="button" class:on={search.matchPath} title="ファイル名だけでなくフォルダのパスも検索" on:click={toggleMatchPath}>パス</button>
    {#if running}
      <button type="button" class:on={paused} on:click={togglePause}>{paused ? '再開' : '一時停止'}</button>
      <button type="button" class="stop" on:click={stopSearch}>停止</button>
    {:else}
      <button type="button" disabled={!scope.trim()} title="検索対象をもう一度読み込む" on:click={() => runIndex()}>再読込</button>
    {/if}
  </div>

  <div class="syntax"><span>空白: AND</span><span>|: OR</span><span>! または -: 除外</span></div>

  <div class="workspace">
    {#if showHistory}
      <aside class="history-rail">
        <section>
          <div class="rail-heading">
            <strong>★ よく使う場所</strong>
            <button
              type="button"
              class:starred={currentSingleScopeFavorite}
              title={currentSingleScopeFavorite ? '現在の検索場所をお気に入りから外す' : '現在の検索場所をお気に入りに追加'}
              on:click={toggleCurrentScopeFavorite}
            >{currentSingleScopeFavorite ? '★' : '☆'}</button>
          </div>
          {#if favoriteLocations.length === 0}
            <p class="rail-empty">ファイラーで登録したお気に入りも、ここに並びます。</p>
          {:else}
            <div class="rail-list">
              {#each favoriteLocations as path}
                <div class="rail-item" class:current={scopes.length === 1 && api.pathIdentity(path) === api.pathIdentity(scopes[0])}>
                  <button class="rail-main" type="button" title={path} on:click={() => revisitLocation([path])}>
                    <span>★ {splitPath(path).tail || path}</span>
                    <small>{path}</small>
                  </button>
                </div>
              {/each}
            </div>
          {/if}
        </section>

        <section>
          <div class="rail-heading">
            <strong>検索した場所</strong>
            {#if locationHistory.length}<button type="button" title="検索場所の履歴を消去" on:click={clearLocationHistory}>消去</button>{/if}
          </div>
          {#if locationHistory.length === 0}
            <p class="rail-empty">検索すると、ここから同じ場所をもう一度調べられます。</p>
          {:else}
            <div class="rail-list">
              {#each locationHistory as entry}
                <div class="rail-item" class:current={entry.paths.join(';') === scopes.join(';')}>
                  <button class="rail-main" type="button" title={entry.paths.join('\n')} on:click={() => revisitLocation(entry.paths)}>
                    <span>⌕ {locationLabel(entry.paths)}</span>
                    <small>{entry.paths.join(' ; ')}</small>
                  </button>
                  <button class="rail-remove" type="button" title="履歴から外す" aria-label="履歴から外す" on:click={() => forgetLocation(entry.paths)}>×</button>
                </div>
              {/each}
            </div>
          {/if}
        </section>

        <section class="query-history">
          <div class="rail-heading"><strong>検索した言葉</strong></div>
          {#if (search.recentQueries?.length ?? 0) === 0}
            <p class="rail-empty">検索語の履歴はまだありません。</p>
          {:else}
            <div class="query-chips">
              {#each search.recentQueries ?? [] as query}
                <button type="button" title={`「${query}」で絞り込む`} on:click={() => updateQuery(query)}>{query}</button>
              {/each}
            </div>
          {/if}
        </section>
      </aside>
    {/if}

    <div class="results-column">
      <div class="status" class:searching={running}>
        {status} · {results.length.toLocaleString()}件表示{#if truncated} / 一致 {matched.toLocaleString()}件{/if}
      </div>
      {#if results.length === 0}
        <div class="empty">
          <span class="mark">⌕</span>
          <strong>{running ? '読み込み中…' : '一致する項目はありません'}</strong>
          <p>{running ? `${scanned.toLocaleString()}件を読み込みました` : '検索語を変えると即座に絞り込みます。'}</p>
          <small>入力中にリアルタイム絞り込み、Escapeで走査を停止</small>
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
    </div>
  </div>
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
  .actions button.search-split { width: 32px; }
  .actions button.on { border-color: #4c9aff; background: #35506f; color: #fff; }
  .scope-row, .query-row { display: flex; align-items: center; gap: 6px; padding: 7px 10px; }
  .scope-row { border-bottom: 1px solid #292d30; color: #89949c; font-size: 11px; }
  .query-row { border-bottom: 1px solid #2b2f32; padding-top: 0; }
  input { min-width: 0; flex: 1; border: 1px solid #3c444a; border-radius: 4px; outline: none; background: #22262a; color: #eee; padding: 6px 9px; }
  input:focus { border-color: #63cfad; box-shadow: 0 0 0 1px rgba(99, 207, 173, 0.2); }
  .scope-input { border-color: transparent; background: transparent; color: #aab2b8; padding: 4px 6px; }
  .stop { border-color: #80504e; color: #f0aaa4; }
  .syntax { display: flex; gap: 12px; border-bottom: 1px solid #292d30; color: #67727a; font-size: 9px; padding: 2px 11px 5px; }
  .status { min-height: 18px; border-bottom: 1px solid #292d30; color: #89949c; font-size: 11px; padding: 4px 11px; }
  .status.searching { color: #63cfad; }
  .workspace { display: flex; flex: 1; min-width: 0; min-height: 0; }
  .results-column { display: flex; flex: 1; min-width: 0; min-height: 0; flex-direction: column; }
  .history-rail { width: 220px; flex: none; overflow-y: auto; border-right: 1px solid #303438; background: #17191b; }
  .history-rail section { padding: 10px 8px; }
  .history-rail section + section { border-top: 1px solid #303438; }
  .rail-heading { display: flex; align-items: center; justify-content: space-between; margin-bottom: 7px; color: #b9c2c9; font-size: 11px; }
  .rail-heading button { height: 20px; border: 0; background: transparent; color: #77828a; font-size: 9px; }
  .rail-heading button.starred { color: #f1c75b; }
  .rail-empty { margin: 6px 2px; color: #69737a; font-size: 10px; line-height: 1.5; }
  .rail-list { display: flex; flex-direction: column; gap: 2px; }
  .rail-item { display: flex; min-width: 0; border-radius: 4px; }
  .rail-item:hover { background: #252a2e; }
  .rail-item.current { background: #253a38; }
  .rail-main { display: flex; height: auto; min-width: 0; flex: 1; align-items: flex-start; flex-direction: column; gap: 2px; border: 0; background: transparent; padding: 5px 6px; text-align: left; }
  .rail-main span, .rail-main small { width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rail-main span { color: #d1d6da; font-size: 11px; }
  .rail-main small { color: #68737b; font-size: 9px; }
  .rail-remove { width: 22px; height: auto; flex: none; border: 0; background: transparent; color: #667078; opacity: 0; }
  .rail-item:hover .rail-remove { opacity: 1; }
  .query-chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .query-chips button { max-width: 100%; height: 23px; overflow: hidden; border-color: #3b4247; background: #24282b; padding: 0 7px; color: #aeb8bf; text-overflow: ellipsis; white-space: nowrap; font-size: 10px; }
  .empty { display: grid; flex: 1; place-content: center; justify-items: center; color: #7e8992; text-align: center; }
  .empty .mark { margin-bottom: 8px; color: #63cfad; font-size: 36px; }
  .empty strong { color: #c8ced3; }
  .empty p { margin: 8px 0 3px; }
  .empty small { color: #68727a; }
  .result-area { display: flex; flex: 1; min-height: 0; }
  .list-slot { display: flex; flex: 1; min-width: 0; min-height: 0; flex-direction: column; container-type: inline-size; }
  .preview-slot { width: 260px; min-width: 160px; border-left: 1px solid #2c2c2c; }
  @media (max-width: 700px) { .history-rail { width: 180px; } }
</style>
