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
  /** 中クリック用。いまのペインを動かさず、その場所を隣のペインで開く。 */
  export let onOpenBeside: (path: string) => void = () => {}
  /** 移動の集計に数えない移動。既定では普通の移動と同じ。 */
  export let onNavigateUncounted: (path: string) => void = (path) => onNavigate(path)
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
  let favoriteKinds: api.PathKind[] = []
  let history: HistoryEntry[] = []
  let historyExist: boolean[] = []

  /**
   * 保存されている場所の実在確認。
   *
   * お気に入りも履歴も、消えたフォルダを指したまま残る。
   * 掴んでから「開けません」と言われるより、先に灰色で示す方が親切。
   */
  /**
   * ★の一覧。見出しに件数を出すので、開いていなくても持っておく。
   *
   * 種別（実在・ファイルかフォルダか）は一覧を描く時にしか要らない。
   * 1件につき最大2回 stat するため、★が切断されたネットワークドライブを
   * 指していると、見えてもいない欄のためにそこへ問い合わせに行くことになる。
   */
  /**
   * 取り直しの世代。一覧とその種別を必ず同じ回のもので揃えるために持つ。
   *
   * `listFavorites` と `pathKinds` の2回に分かれていて、種別は添字で一覧と
   * 対応する。取り直しが重なると、新しい一覧に古い種別が乗ることがある。
   * ずれた種別は見た目だけの問題では済まない——★のファイルがフォルダと
   * 判定され、クリックでそこへ「移動」しようとする。
   */
  let favoriteGeneration = 0
  let historyGeneration = 0

  async function refreshFavorites() {
    const generation = ++favoriteGeneration
    const list = await api.listFavorites()
    const kinds = tab === 'favorites' ? await api.pathKinds(list) : []
    if (generation !== favoriteGeneration) return
    favorites = list
    favoriteKinds = kinds
  }

  async function refreshHistory() {
    const generation = ++historyGeneration
    const list = await api.listHistory()
    const kinds = await api.pathKinds(list.map((h) => h.path))
    if (generation !== historyGeneration) return
    history = list
    // 履歴は場所だけを並べる欄なので、ファイルに置き換わっていたら「無い」と同じに扱う。
    historyExist = kinds.map((kind) => kind.isDir)
  }

  /** 外から促された時の入口。表示している欄のぶんだけ取り直す。 */
  export async function refresh() {
    if (tab === 'history') await refreshHistory()
    else if (tab === 'favorites') await refreshFavorites()
  }

  /**
   * 取り直す条件を欄ごとに分ける。
   *
   * 以前はどの欄を開いていても、移動のたびに★と履歴の両方を取り直していた。
   * サイドバーはペインごとに1つあり、上下分割ではさらに増えるので、
   * 3ペイン構成では `Q` 1回で10件以上のIPCと数十回の stat が走っていた。
   * しかも既定の欄はツリーで、★も履歴も描かれてすらいない。
   */
  // ★は移動では変わらない。変わるのは登録・解除の時だけで、それは下で拾う。
  $: if (tab === 'favorites') refreshFavorites()
  // 履歴は移動のたびに増えるので、開いている間は場所に追従させる。
  $: if (tab === 'history' && currentPath) refreshHistory()

  // お気に入りは全ペイン共有なので、どこで変わっても取り直す。自分で登録した時しか
  // 直さないと、隣のペインと上下分割の下段が古いまま残る。
  onMount(() => {
    refreshFavorites()
    return api.onFavoritesChanged(refreshFavorites)
  })
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
      <Tree {currentPath} {onNavigate} {onOpenBeside} {showHidden} />
    {:else if tab === 'favorites'}
      <Favorites
        items={favorites}
        kinds={favoriteKinds}
        {currentPath}
        {onNavigate}
        {onOpenBeside}
        {onNavigateUncounted}
        onChanged={api.favoritesChanged}
      />
    {:else if tab === 'history'}
      <History
        items={history}
        missing={historyExist}
        {currentPath}
        {onNavigate}
        {onOpenBeside}
        onChanged={refresh}
      />
    {:else}
      <Tray
        items={trayItems}
        {onNavigate}
        {onOpenBeside}
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
