<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { getCurrentWebview } from '@tauri-apps/api/webview'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { listen, type UnlistenFn } from '@tauri-apps/api/event'
  import Pane from './Pane.svelte'
  import SearchPane from './SearchPane.svelte'
  import SettingsDialog from './SettingsDialog.svelte'
  import * as api from './api'
  import { splitPath } from './api'
  import { matchAction, resolveKey } from './shortcuts'

  const win = getCurrentWindow()
  const label = win.label

  type PaneState = {
    id: number
    /** Missing in old sessions; every newly created pane starts as a directory. */
    kind: api.PaneKind
    /** Directory context is retained even when a future pane role is active. */
    path: string
    search?: api.SavedSearchState
    sidebar?: api.SavedSidebarState
    /** 転送先として固定し、場所を変えないペイン。 */
    pinned?: boolean
    ref?: Pane | SearchPane
  }
  /**
   * タブ = 横並びペインの集合。
   *
   * ペイン分割は「1つの作業」の中の見え方の話で、タブは「別の作業」を
   * 切り替える話。混ぜると、分割中に別の場所へ移るたび今の分割が壊れる。
   */
  type TabState = { id: number; panes: PaneState[]; activeId: number; trayItems: string[] }

  let tabs: TabState[] = []
  let activeTabId = 0
  let nextTabId = 1
  /**
   * ペイン ID は全タブを通じて共有する採番にする。
   *
   * タブごとに 1 から採番すると、タブA・タブBのペインが同じ id を持ちうる。
   * `{#each activeTab.panes as pane (pane.id)}` はこの id をキーにしているため、
   * 同じ id なら別タブへ切り替えても Svelte が同一コンポーネントとみなして
   * 使い回してしまい、表示がタブ切り替え前のまま固まる。
   */
  let nextPaneId = 1
  let hoveredPaneId: number | null = null
  let hotkey = ''
  let dragIcon = ''
  let hovering = false
  let ready = false
  /** アプリ設定。ペインより先に読み、全ペインへ配る。 */
  let settings: api.Settings | null = null
  let settingsOpen = false

  function newSearchState(path: string): api.SavedSearchState {
    return {
      scopePaths: [path],
      query: '',
      matchPath: true,
      sortKey: settings?.sortKey ?? 'name',
      sortDescending: settings?.sortDescending ?? false,
      dirsFirst: settings?.dirsFirst ?? true,
      showPreview: settings?.showPreview ?? false,
      showHistory: true,
      recentQueries: [],
    }
  }

  $: activeTab = tabs.find((t) => t.id === activeTabId)
  $: if (ready && activeTab) api.setWindowTray(label, activeTab.trayItems).catch(() => {})
  $: if (ready && activeTab) syncWindowContext()

  let notes: string[] = []
  function note(message: string) {
    notes = [`${new Date().toLocaleTimeString()}  ${message}`, ...notes].slice(0, 8)
    api.logDnd(`[${label}] ${message}`)
  }

  /** タブに出す短い名前。アクティブなペインの末尾フォルダ名。 */
  function tabLabel(tab: TabState): string {
    const pane = tab.panes.find((p) => p.id === tab.activeId) ?? tab.panes[0]
    return pane ? splitPath(pane.path).tail || pane.path : '…'
  }

  /** 表に出ているタブの、表に出ているペインの現在パス。 */
  function activePanePath(): string | undefined {
    const tab = activeTab
    if (!tab) return undefined
    return tab.panes.find((p) => p.id === tab.activeId)?.ref?.currentPath() ?? undefined
  }

  /** 窓のタイトルと俯瞰表示へ、現在タブの全ペインを同期する。 */
  function syncWindowContext() {
    const tab = activeTab
    if (!tab) return
    const panes: api.WindowPaneInfo[] = tab.panes.map((pane) => ({
      id: pane.id,
      kind: pane.kind,
      path: pane.ref?.currentPath() || pane.path,
      query: pane.kind === 'search' ? pane.search?.query ?? '' : '',
      is_active: pane.id === tab.activeId,
    }))
    const windowTabs: api.WindowTabInfo[] = tabs.map((candidate) => ({
      id: candidate.id,
      label: tabLabel(candidate),
      is_active: candidate.id === activeTabId,
    }))
    api.setWindowContext(label, tabLabel(tab), tabs.length, panes, windowTabs).catch(() => {})
  }

  function newTab(path: string) {
    const paneId = nextPaneId++
    const tab: TabState = {
      id: nextTabId++,
      panes: [{ id: paneId, kind: 'directory', path }],
      activeId: paneId,
      trayItems: [],
    }
    tabs = [...tabs, tab]
    activeTabId = tab.id
  }

  let closedTabs: TabState[] = []
  let dragTabIndex: number | null = null

  function closeTab(id: number) {
    if (tabs.length <= 1) return // 最後の1つは残す
    const idx = tabs.findIndex((t) => t.id === id)
    const target = tabs[idx]
    if (target) {
      closedTabs = [target, ...closedTabs].slice(0, 20)
    }
    tabs = tabs.filter((t) => t.id !== id)
    if (activeTabId === id) {
      activeTabId = tabs[Math.min(idx, tabs.length - 1)].id
    }
  }

  function restoreClosedTab() {
    if (closedTabs.length === 0) return
    const [restored, ...rest] = closedTabs
    closedTabs = rest
    tabs = [...tabs, restored]
    activeTabId = restored.id
  }

  function handleTabDragStart(index: number, ev: DragEvent) {
    dragTabIndex = index
    if (ev.dataTransfer) {
      ev.dataTransfer.effectAllowed = 'move'
    }
  }

  function handleTabDragOver(index: number, ev: DragEvent) {
    if (dragTabIndex === null || dragTabIndex === index) return
    ev.preventDefault()
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = 'move'
  }

  function handleTabDrop(index: number, ev: DragEvent) {
    if (dragTabIndex === null || dragTabIndex === index) return
    ev.preventDefault()
    const moved = tabs[dragTabIndex]
    const next = [...tabs]
    next.splice(dragTabIndex, 1)
    next.splice(index, 0, moved)
    tabs = next
    dragTabIndex = null
  }

  function cycleTab(delta: number) {
    if (tabs.length <= 1) return
    const idx = tabs.findIndex((t) => t.id === activeTabId)
    activeTabId = tabs[(idx + delta + tabs.length) % tabs.length].id
  }

  function splitPane(
    afterId: number,
    path: string,
    kind: api.PaneKind = 'directory',
    searchSeed?: api.SavedSearchState,
  ) {
    const tab = activeTab
    if (!tab) return
    const idx = tab.panes.findIndex((p) => p.id === afterId)
    const created: PaneState = {
      id: nextPaneId++,
      kind,
      path,
      search: kind === 'search'
        ? searchSeed
          ? {
              ...searchSeed,
              scopePaths: [...searchSeed.scopePaths],
              recentQueries: [...(searchSeed.recentQueries ?? [])],
            }
          : newSearchState(path)
        : undefined,
    }
    tab.panes = [...tab.panes.slice(0, idx + 1), created, ...tab.panes.slice(idx + 1)]
    tab.activeId = created.id
    tabs = tabs // ネストした更新を描画に反映させる
  }

  function changePaneKind(id: number, kind: api.PaneKind) {
    const tab = activeTab
    const pane = tab?.panes.find((candidate) => candidate.id === id)
    if (!tab || !pane || pane.kind === kind) return

    const directoryPath = pane.ref?.currentPath() || pane.path
    pane.path = directoryPath
    pane.kind = kind
    if (kind === 'search' && !pane.search) {
      pane.search = newSearchState(directoryPath)
    }
    tabs = tabs
    syncWindowContext()
  }

  function updatePaneSearch(id: number, search: api.SavedSearchState) {
    const pane = activeTab?.panes.find((candidate) => candidate.id === id)
    if (!pane) return
    pane.search = search
    tabs = tabs
  }

  function openDirectoryFromSearch(id: number, path: string) {
    const pane = activeTab?.panes.find((candidate) => candidate.id === id)
    if (!pane) return
    pane.path = path
    pane.kind = 'directory'
    tabs = tabs
    syncWindowContext()
  }

  function toggleTrayItem(path: string) {
    const tab = activeTab
    if (!tab) return
    const key = api.pathIdentity(path)
    const exists = tab.trayItems.some((item) => api.pathIdentity(item) === key)
    tab.trayItems = exists
      ? tab.trayItems.filter((item) => api.pathIdentity(item) !== key)
      : [...tab.trayItems, path]
    tabs = tabs
    note(`${exists ? 'トレイから解除' : 'トレイへ追加'}: ${splitPath(path).tail || path}`)
  }

  function clearTray() {
    const tab = activeTab
    if (!tab || tab.trayItems.length === 0) return
    const count = tab.trayItems.length
    tab.trayItems = []
    tabs = tabs
    note(`トレイから ${count} 件を解除`)
  }

  function removeTrayItems(paths: string[]) {
    const tab = activeTab
    if (!tab || paths.length === 0) return
    const removed = new Set(paths.map(api.pathIdentity))
    tab.trayItems = tab.trayItems.filter((item) => !removed.has(api.pathIdentity(item)))
    tabs = tabs
  }

  function togglePanePin(id: number) {
    const pane = activeTab?.panes.find((candidate) => candidate.id === id)
    if (!pane) return
    pane.pinned = !pane.pinned
    tabs = tabs
    note(pane.pinned ? `固定: ${splitPath(pane.path).tail || pane.path}` : '固定を解除')
  }

  /**
   * 固定中のペインで移動しようとした先を、別のペインで開く。
   *
   * 固定していない通常ペインがあればそこを移動させてアクティブにし、
   * 無ければ固定ペインの隣に新しいペインを作る。固定した転送先は動かない。
   */
  function openFromPinned(fromId: number, path: string) {
    const tab = activeTab
    if (!tab) return
    const target = tab.panes.find((pane) => pane.id !== fromId && pane.kind === 'directory' && !pane.pinned)
    if (target?.ref && 'open' in target.ref) {
      tab.activeId = target.id
      tabs = tabs
      ;(target.ref as Pane).open(path)
      note(`固定中のため、別のペインで開きました: ${splitPath(path).tail || path}`)
      return
    }
    splitPane(fromId, path)
    note(`固定中のため、隣のペインで開きました: ${splitPath(path).tail || path}`)
  }

  function closePane(id: number) {
    const tab = activeTab
    if (!tab || tab.panes.length <= 1) return // 最後の1枚は残す
    tab.panes = tab.panes.filter((p) => p.id !== id)
    if (tab.activeId === id) tab.activeId = tab.panes[0].id
    tabs = tabs
    if (hoveredPaneId === id) hoveredPaneId = null
    syncWindowContext()
  }

  /** ペインを独立した窓へ切り離す。分割の逆操作。 */
  async function detachPane(id: number, path: string) {
    await api.openWindow(path)
    // 1枚しかない場合は、元のペインをそのまま残す（空にしない）。
    if ((activeTab?.panes.length ?? 0) > 1) closePane(id)
  }

  /**
   * 落とされた座標から、受け取るペインを決める。
   *
   * ドロップは窓単位で飛んでくるので、どのペインに落ちたかは自分で判定する必要がある。
   * ここを間違えると、意図しないフォルダにファイルが入る。
   */
  function paneAt(x: number, y: number): PaneState | undefined {
    const tab = activeTab
    if (!tab) return undefined
    const el = document.elementFromPoint(x, y)?.closest('[data-pane-id]')
    if (el) {
      const id = Number((el as HTMLElement).dataset.paneId)
      const hit = tab.panes.find((p) => p.id === id)
      if (hit) return hit
    }
    return tab.panes.find((p) => p.id === tab.activeId) ?? tab.panes[0]
  }

  function onWindowKey(ev: KeyboardEvent) {
    const el = ev.target as HTMLElement | null
    if (el && (el.tagName === 'INPUT' || el.isContentEditable)) return
    if (!settings) return

    switch (matchAction(ev, settings.shortcuts)) {
      case 'newTab':
        ev.preventDefault()
        newTab(activePanePath() ?? tabs[0]?.panes[0]?.path ?? '')
        break
      case 'closeTab':
        ev.preventDefault()
        closeTab(activeTabId)
        break
      case 'restoreClosedTab':
        ev.preventDefault()
        restoreClosedTab()
        break
      case 'nextTab':
        ev.preventDefault()
        cycleTab(1)
        break
      case 'prevTab':
        ev.preventDefault()
        cycleTab(-1)
        break
      default:
        break
    }
  }

  let unlistenDrop: UnlistenFn | null = null
  let unlistenFocus: UnlistenFn | null = null
  let unlistenTray: UnlistenFn | null = null
  let unlistenActivatePane: UnlistenFn | null = null
  let unlistenActivateTab: UnlistenFn | null = null

  /** 起動に失敗した理由。ここが埋まる時は画面が空のままになるので必ず見せる。 */
  let bootError: string | null = null

  onMount(async () => {
    try {
      await boot()
    } catch (e) {
      bootError = String(e)
      api.logUi('error', `起動に失敗: ${e}`)
    }
  })

  /**
   * 現在の全タブ・ペインの状態を永続化保存する。
   * 切り離した窓も保存し、次回起動時は最後に閉じた窓の状態が復元される。
   */
  function saveCurrentSession() {
    const sessionTabs = tabs.map((t) => {
      const activeIdx = Math.max(0, t.panes.findIndex((p) => p.id === t.activeId))
      const panes = t.panes.map((p) => ({
        path: p.ref?.currentPath() || p.path,
        kind: p.kind,
        pinned: p.pinned || undefined,
        search: p.search,
        sidebar: p.sidebar,
      }))
      return { panes, activePaneIndex: activeIdx, trayPaths: t.trayItems }
    })
    const activeTabIdx = Math.max(0, tabs.findIndex((t) => t.id === activeTabId))
    if (sessionTabs.length > 0) {
      api.saveSessionState(label, { tabs: sessionTabs, activeTabIndex: activeTabIdx }).catch(() => {})
    }
  }

  $: if (ready && tabs) {
    saveCurrentSession()
  }

  async function boot() {
    // 設定はペインを作る前に読む。ペインは初期状態（サイドバー等）をここから取る。
    settings = await api.getSettings()
    dragIcon = await api.dragPreviewIcon()
    hotkey = await api.overlayHotkey()

    // open_window で開かれた窓は、指定されたパスがレジストリに入っている。
    const initialWindowPath = await api.windowInitialPath(label)
    if (!initialWindowPath && label === 'main' && settings.restoreSession) {
      const savedSession = await api.getSessionState()
      if (savedSession && savedSession.tabs && savedSession.tabs.length > 0) {
        const restoredTabs: TabState[] = []
        for (const t of savedSession.tabs) {
          const tabId = nextTabId++
          const panes: PaneState[] = t.panes.map((p) => ({
            id: nextPaneId++,
            kind: p.kind ?? 'directory',
            path: p.path,
            pinned: p.pinned ?? false,
            search: p.search,
            sidebar: p.sidebar,
          }))
          if (panes.length === 0) {
            panes.push({ id: nextPaneId++, kind: 'directory', path: await api.homeDir() })
          }
          const activeId = panes[t.activePaneIndex]?.id ?? panes[0].id
          restoredTabs.push({ id: tabId, panes, activeId, trayItems: t.trayPaths ?? [] })
        }
        tabs = restoredTabs
        const activeTabObj = tabs[savedSession.activeTabIndex] ?? tabs[0]
        activeTabId = activeTabObj.id
      }
    }

    if (tabs.length === 0) {
      const start = initialWindowPath ?? (await api.homeDir())
      newTab(start)
    }
    // main 窓は Rust の open_window を通らないので自己申告で登録する。
    await api.registerWindow(label, activePanePath() ?? (await api.homeDir()))
    // 登録前にトレイ同期が走ると、空のレジストリへ送って失われるので最後に ready にする。
    ready = true

    unlistenDrop = await getCurrentWebview().onDragDropEvent(async (event) => {
      if (event.payload.type === 'over') {
        hovering = true
        return
      }
      hovering = false
      if (event.payload.type !== 'drop') return

      // 物理座標で来るので、CSS ピクセルへ直してから当たり判定する。
      const dpr = window.devicePixelRatio || 1
      const target = paneAt(event.payload.position.x / dpr, event.payload.position.y / dpr)
      await target?.ref?.acceptDrop(event.payload.paths)
    })

    unlistenFocus = await win.onFocusChanged(({ payload }) => {
      if (payload) api.touchWindow(label)
    })

    unlistenTray = await listen<api.WindowTrayChanged>(api.WINDOW_TRAY_CHANGED, (ev) => {
      if (ev.payload.label !== label || !activeTab) return
      const current = activeTab.trayItems.map(api.pathIdentity)
      const incoming = ev.payload.paths.map(api.pathIdentity)
      if (current.length === incoming.length && current.every((path, i) => path === incoming[i])) return
      activeTab.trayItems = ev.payload.paths
      tabs = tabs
    })
    unlistenActivatePane = await listen<number>(api.ACTIVATE_PANE_REQUEST, ({ payload }) => {
      const tab = activeTab
      if (!tab || !tab.panes.some((pane) => pane.id === payload)) return
      tab.activeId = payload
      hoveredPaneId = null
      tabs = tabs
      syncWindowContext()
    })
    unlistenActivateTab = await listen<number>(api.ACTIVATE_TAB_REQUEST, ({ payload }) => {
      if (!tabs.some((tab) => tab.id === payload)) return
      activeTabId = payload
      hoveredPaneId = null
      tabs = tabs
    })
  }

  onDestroy(() => {
    unlistenDrop?.()
    unlistenFocus?.()
    unlistenTray?.()
    unlistenActivatePane?.()
    unlistenActivateTab?.()
  })
</script>

<svelte:window on:keydown={onWindowKey} />

<main class:hovering>
  {#if tabs.length > 1}
    <div class="tabbar" role="tablist">
      {#each tabs as tab, idx (tab.id)}
        <button
          type="button"
          role="tab"
          aria-selected={tab.id === activeTabId}
          class="tab"
          class:on={tab.id === activeTabId}
          draggable="true"
          on:dragstart={(ev) => handleTabDragStart(idx, ev)}
          on:dragover={(ev) => handleTabDragOver(idx, ev)}
          on:drop={(ev) => handleTabDrop(idx, ev)}
          on:click={() => (activeTabId = tab.id)}
        >
          <span class="tab-label">{tabLabel(tab)}</span>
          <span
            class="tab-close"
            role="button"
            tabindex="-1"
            title="タブを閉じる (Ctrl+W)"
            on:click|stopPropagation={() => closeTab(tab.id)}
            on:keydown|stopPropagation={(e) => e.key === 'Enter' && closeTab(tab.id)}
          >
            ✕
          </span>
        </button>
      {/each}
      <button
        type="button"
        class="tab-new"
        title="新しいタブ (Ctrl+T)"
        on:click={() => newTab(activePanePath() ?? tabs[0]?.panes[0]?.path ?? '')}
      >
        ＋
      </button>
    </div>
  {/if}

  <div class="panes">
    {#if activeTab && settings}
      {#each activeTab.panes as pane, i (pane.id)}
        {#if i > 0}
          <div class="divider" />
        {/if}
        <div class="slot" data-pane-id={pane.id}>
          {#if pane.kind === 'search'}
            <SearchPane
              bind:this={pane.ref}
              directoryPath={pane.path}
              search={pane.search ?? newSearchState(pane.path)}
              {settings}
              {dragIcon}
              trayItems={activeTab.trayItems}
              onTrayToggle={toggleTrayItem}
              active={pane.id === activeTab.activeId}
              keyboardTarget={pane.id === (hoveredPaneId ?? activeTab.activeId)}
              multi={activeTab.panes.length > 1}
              closable={activeTab.panes.length > 1}
              onSearchChange={(search) => updatePaneSearch(pane.id, search)}
              onKindChange={(kind) => changePaneKind(pane.id, kind)}
              onOpenDirectory={(path) => openDirectoryFromSearch(pane.id, path)}
              onHoverChange={(hovered) => {
                if (hovered) hoveredPaneId = pane.id
                else if (hoveredPaneId === pane.id) hoveredPaneId = null
              }}
              onActivate={() => {
                activeTab.activeId = pane.id
                tabs = tabs
    syncWindowContext()
              }}
              onSplit={(path) => splitPane(pane.id, path)}
              onSplitSearch={(path, search) => splitPane(pane.id, path, 'search', search)}
              onClose={() => closePane(pane.id)}
              onNote={note}
            />
          {:else}
            <Pane
              bind:this={pane.ref}
              initialPath={pane.path}
              {settings}
              onOpenSettings={() => (settingsOpen = true)}
              {dragIcon}
              active={pane.id === activeTab.activeId}
              keyboardTarget={pane.id === (hoveredPaneId ?? activeTab.activeId)}
              multi={activeTab.panes.length > 1}
              closable={activeTab.panes.length > 1}
              trayItems={activeTab.trayItems}
              onTrayToggle={toggleTrayItem}
              onTrayClear={clearTray}
              onTrayRemoveMany={removeTrayItems}
              sidebarState={pane.sidebar ?? { primary: 'tree' }}
              onSidebarChange={(sidebar) => {
                pane.sidebar = sidebar
                tabs = tabs
              }}
              onHoverChange={(hovered) => {
                if (hovered) hoveredPaneId = pane.id
                else if (hoveredPaneId === pane.id) hoveredPaneId = null
              }}
              onActivate={() => {
                activeTab.activeId = pane.id
                tabs = tabs
                syncWindowContext()
              }}
              onPathChange={(path) => {
                // タブラベルは末尾フォルダ名を出すので、移動のたびに更新しないと
                // 「hinat」のまま固まって見える（実際のパスバーとタブ名が食い違う）。
                pane.path = path
                tabs = tabs
                syncWindowContext()
              }}
              onNote={note}
              onSplit={(path) => splitPane(pane.id, path)}
              onSplitSearch={(path) => splitPane(pane.id, path, 'search')}
              onClose={() => closePane(pane.id)}
              onDetach={(path) => detachPane(pane.id, path)}
              onKindChange={(kind) => changePaneKind(pane.id, kind)}
              pinned={!!pane.pinned}
              onTogglePin={() => togglePanePin(pane.id)}
              onPinnedNavigate={(path) => openFromPinned(pane.id, path)}
            />
          {/if}
        </div>
      {/each}
    {/if}

    {#if bootError}
      <div class="boot-error">
        <strong>起動に失敗しました</strong>
        <code>{bootError}</code>
      </div>
    {:else if !ready}
      <div class="loading">読み込み中…</div>
    {/if}
  </div>

  <footer>
    <span>{activeTab?.panes.length ?? 0} ペイン{tabs.length > 1 ? ` / ${tabs.length} タブ` : ''}</span>
    <span class="spacer" />
    {#if notes[0]}<span class="note">{notes[0]}</span>{/if}
    {#if settings}
      <span class="left-keys">
        {resolveKey('hoverParent', settings.shortcuts)} 親へ ·
        {resolveKey('hoverClosePane', settings.shortcuts)} 閉じる ·
        {resolveKey('hoverFavorite', settings.shortcuts)} お気に入り ·
        {resolveKey('hoverSplitPane', settings.shortcuts)} 分割 ·
        {resolveKey('hoverSplitSearchPane', settings.shortcuts)} 検索分割 ·
        {resolveKey('hoverPreview', settings.shortcuts)} プレビュー
      </span>
    {/if}
    <span class="hotkey">{hotkey} で窓一覧 · Ctrl+T 新規タブ</span>
  </footer>
</main>

<!-- 設定は窓に1つ。ペインごとに持つと同じものが複数開きうる。 -->
<SettingsDialog
  open={settingsOpen}
  onClose={() => (settingsOpen = false)}
  onSaved={(s) => {
    settings = s
    // ホットキーの変更は再起動が必要なので、ここでは表示だけ合わせる。
    api.overlayHotkey().then((h) => (hotkey = h))
  }}
/>

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
    border: 2px solid transparent;
  }
  /* 落とせる状態が分かるように枠を光らせる。 */
  main.hovering {
    border-color: #4c9aff;
  }

  .tabbar {
    display: flex;
    align-items: stretch;
    flex: none;
    background: #191919;
    border-bottom: 1px solid #333;
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
    max-width: 200px;
    padding: 6px 8px 6px 12px;
    background: none;
    border: 0;
    border-right: 1px solid #2c2c2c;
    border-bottom: 2px solid transparent;
    color: #888;
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }
  .tab:hover {
    background: #222;
    color: #ccc;
  }
  .tab.on {
    background: #232323;
    border-bottom-color: #4c9aff;
    color: #fff;
  }
  .tab-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tab-close {
    flex: none;
    width: 15px;
    height: 15px;
    border-radius: 3px;
    text-align: center;
    line-height: 15px;
    font-size: 10px;
    color: #777;
  }
  .tab-close:hover {
    background: #4a2020;
    color: #ff9b9b;
  }
  .tab-new {
    flex: none;
    width: 30px;
    background: none;
    border: 0;
    color: #777;
    font-size: 13px;
    cursor: pointer;
  }
  .tab-new:hover {
    background: #222;
    color: #ccc;
  }

  .panes {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .slot {
    display: flex;
    flex: 1;
    min-width: 0;
  }

  .divider {
    width: 1px;
    flex: none;
    background: #3a3a3a;
  }

  .loading {
    margin: auto;
    color: #666;
    font-size: 12px;
  }

  .boot-error {
    margin: auto;
    max-width: 70%;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px 18px;
    background: #3a1d1d;
    border-left: 3px solid #e05252;
    border-radius: 4px;
    font-size: 12px;
    color: #ffb4b4;
  }
  .boot-error code {
    font-family: Consolas, monospace;
    font-size: 11px;
    color: #ff9b9b;
    word-break: break-all;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: none;
    padding: 5px 14px;
    border-top: 1px solid #333;
    background: #1d1d1d;
    font-size: 11px;
    color: #777;
  }
  .spacer {
    flex: 1;
  }
  .note {
    color: #6bd968;
    font-family: Consolas, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 45%;
  }
  .hotkey {
    color: #666;
  }
  .left-keys {
    color: #6f988b;
    white-space: nowrap;
  }
</style>
