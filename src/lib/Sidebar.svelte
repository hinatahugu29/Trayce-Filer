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
  export let onTrayClear: () => void = () => {}

  /** どのタブを開くか。ペインごとに独立して覚える。 */
  export let tab: 'tree' | 'favorites' | 'history' | 'tray' = 'tree'

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
    ;[favoritesExist, historyExist] = await Promise.all([
      api.pathsExist(favorites),
      api.pathsExist(history.map((h) => h.path)),
    ])
  }

  // 表示中のタブが変わった時と、場所が変わった時に取り直す。
  // 履歴は移動のたびに増えるので、開きっぱなしでも古びないようにする。
  $: if (tab && currentPath) refresh()

  onMount(refresh)
</script>

<div class="sidebar">
  <div class="tabs" role="tablist">
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'tree'}
      class:on={tab === 'tree'}
      on:click={() => (tab = 'tree')}>ツリー</button
    >
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'favorites'}
      class:on={tab === 'favorites'}
      on:click={() => (tab = 'favorites')}>★ {favorites.length || ''}</button
    >
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'history'}
      class:on={tab === 'history'}
      on:click={() => (tab = 'history')}>履歴</button
    >
    <button
      role="tab"
      type="button"
      aria-selected={tab === 'tray'}
      class:on={tab === 'tray'}
      on:click={() => (tab = 'tray')}>トレイ {trayItems.length || ''}</button
    >
  </div>

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
      <Tray items={trayItems} {onNavigate} onRemove={onTrayRemove} onClear={onTrayClear} />
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

  .body {
    flex: 1;
    min-height: 0;
  }
</style>
