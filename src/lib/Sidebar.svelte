<script lang="ts">
  import { onMount } from 'svelte'
  import Tree from './Tree.svelte'
  import Favorites from './Favorites.svelte'
  import History from './History.svelte'
  import Tray from './Tray.svelte'
  import * as api from './api'
  import type { HistoryEntry } from './api'

  export let currentPath: string
  export let onNavigate: (path: string) => void
  /** 一覧の設定をツリーにも反映させる。 */
  export let showHidden = false
  export let trayItems: string[] = []
  export let onTrayRemove: (path: string) => void = () => {}
  export let onTrayRemoveMany: (paths: string[]) => void = () => {}
  export let onTrayClear: () => void = () => {}
  export let onTrayTransfer: (paths: string[], moveFiles: boolean) => void = () => {}
  export let trays: api.TraySummary[] = []
  export let activeTrayId = 0
  export let onTraySelect: (id: number) => void = () => {}
  export let onTrayAdd: () => void = () => {}
  export let onTrayRename: (id: number) => void = () => {}
  export let onTrayDelete: (id: number) => void = () => {}

  /** どのタブを開くか。ペインごとに独立して覚える。 */
  export let tab: api.SidebarTab = 'tree'
  /** 上下分割時は、狭い領域を通常の4タブで消費しない。 */
  export let compact = false
  export let onSplit: () => void = () => {}
  export let onClose: () => void = () => {}
  export let onTabChange: () => void = () => {}

  function chooseTab(next: api.SidebarTab) {
    tab = next
    onTabChange()
  }

  let favorites: string[] = []
  let favoritesExist: boolean[] = []
  let history: HistoryEntry[] = []
  let historyExist: boolean[] = []

  /**
   * 保存されている場所の実在確認。
   *
   * お気に入りも履歴も、消えたフォルダを指したまま残る。
   * 掴んでから「開けません」と言われるより、先に灰色で示す方が親切。
   */
  export async function refresh() {
    favorites = await api.listFavorites()
    history = await api.listHistory()
    const [favoriteKinds, historyKinds] = await Promise.all([
      api.pathKinds(favorites),
      api.pathKinds(history.map((h) => h.path)),
    ])
    // ここは場所だけを並べる欄なので、ファイルに置き換わっていたら「無い」と同じに扱う。
    favoritesExist = favoriteKinds.map((kind) => kind.isDir)
    historyExist = historyKinds.map((kind) => kind.isDir)
  }

  // 表示中のタブが変わった時と、場所が変わった時に取り直す。
  // 履歴は移動のたびに増えるので、開きっぱなしでも古びないようにする。
  $: if (tab && currentPath) refresh()

  onMount(refresh)
</script>

<div class="sidebar">
  {#if compact}
    <div class="compact-head">
      <select bind:value={tab} aria-label="表示する情報" on:change={onTabChange}>
        <option value="tree">ツリー</option>
        <option value="favorites">★ お気に入り</option>
        <option value="history">履歴</option>
        <option value="tray">トレイ</option>
      </select>
      <button type="button" title="この段を閉じる" aria-label="この段を閉じる" on:click={onClose}>×</button>
    </div>
  {:else}
    <div class="tabs" role="tablist">
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'tree'}
      class:on={tab === 'tree'}
      on:click={() => chooseTab('tree')}>ツリー</button
    >
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'favorites'}
      class:on={tab === 'favorites'}
      on:click={() => chooseTab('favorites')}>★ {favorites.length || ''}</button
    >
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'history'}
      class:on={tab === 'history'}
      on:click={() => chooseTab('history')}>履歴</button
    >
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'tray'}
      class:on={tab === 'tray'}
      on:click={() => chooseTab('tray')}>トレイ {trayItems.length || ''}</button
    >
      <button class="split" type="button" title="左欄を上下に分割" aria-label="左欄を上下に分割" on:click={onSplit}>↕</button>
    </div>
  {/if}

  <div class="body">
    {#if tab === 'tree'}
      <Tree {currentPath} {onNavigate} {showHidden} />
    {:else if tab === 'favorites'}
      <Favorites
        items={favorites}
        missing={favoritesExist}
        {currentPath}
        {onNavigate}
        onChanged={refresh}
      />
    {:else if tab === 'history'}
      <History
        items={history}
        missing={historyExist}
        {currentPath}
        {onNavigate}
        onChanged={refresh}
      />
    {:else}
      <Tray
        items={trayItems}
        {onNavigate}
        onRemove={onTrayRemove}
        onRemoveMany={onTrayRemoveMany}
        onClear={onTrayClear}
        onTransfer={onTrayTransfer}
        destinationPath={currentPath}
        {trays}
        {activeTrayId}
        onSelect={onTraySelect}
        onAdd={onTrayAdd}
        onRename={onTrayRename}
        onDelete={onTrayDelete}
      />
    {/if}
  </div>
</div>

<style>
  .sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #191919;
  }

  .tabs {
    display: flex;
    flex: none;
    border-bottom: 1px solid #2c2c2c;
  }
  .tabs button {
    flex: 1;
    min-width: 0;
    padding: 6px 2px;
    background: none;
    border: 0;
    border-bottom: 2px solid transparent;
    color: #777;
    font: inherit;
    font-size: 10.5px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
  }
  .tabs button:hover {
    color: #bbb;
  }
  .tabs button.on {
    color: #fff;
    border-bottom-color: #4c9aff;
  }
  .tabs button.split { flex: none; width: 24px; color: #aaa; }

  .compact-head { display: flex; flex: none; height: 27px; border-bottom: 1px solid #2c2c2c; }
  .compact-head select { flex: 1; min-width: 0; border: 0; background: #202020; color: #ddd; padding: 0 6px; font: inherit; font-size: 10.5px; }
  .compact-head button { width: 26px; border: 0; background: transparent; color: #888; cursor: pointer; }
  .compact-head button:hover { color: #fff; background: #333; }

  .body {
    flex: 1;
    min-height: 0;
  }
</style>
