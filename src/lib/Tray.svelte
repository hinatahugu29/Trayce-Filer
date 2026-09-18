<script lang="ts">
  import { onMount } from 'svelte'
  import { splitPath } from './api'
  import * as api from './api'
  import { armMiddleClick, onMiddleClick } from './middleclick'

  export let items: string[] = []
  export let onNavigate: (path: string) => void
  /** 中クリック用。いまのペインを動かさず、その場所を隣のペインで開く。 */
  export let onOpenBeside: (path: string) => void = () => {}
  export let onRemove: (path: string) => void = () => {}
  export let onRemoveMany: (paths: string[]) => void = () => {}
  export let onClear: () => void = () => {}
  export let onTransfer: (paths: string[], moveFiles: boolean) => void = () => {}
  /** 転送中でも押せる（ペイン側が順番待ちに入れる）。 */
  export let transferBusy = false
  export let destinationPath = ''
  /** タブの全トレイ。2つ以上ある時や、名前を付けたい時に切り替え欄を使う。 */
  export let trays: api.TraySummary[] = []
  export let activeTrayId = 0
  export let onSelect: (id: number) => void = () => {}
  export let onAdd: () => void = () => {}
  export let onRename: (id: number) => void = () => {}
  export let onDelete: (id: number) => void = () => {}

  $: activeTray = trays.find((tray) => tray.id === activeTrayId)

  /**
   * 各項目の実在と種別。
   *
   * フォルダは運ぶ荷物であると同時に「行き先」でもある。同じ一覧に混ざるので、
   * 種別を知らないと動作を出し分けられない。
   */
  let kinds: api.PathKind[] = []
  let refreshToken = 0
  let selectedKeys = new Set<string>()
  let anchor = 0
  let lastItemSignature = ''

  $: itemSignature = items.map(api.pathIdentity).join('\n')
  $: if (itemSignature !== lastItemSignature) {
    lastItemSignature = itemSignature
    const valid = new Set(items.map(api.pathIdentity))
    selectedKeys = new Set([...selectedKeys].filter((key) => valid.has(key)))
    anchor = Math.min(anchor, Math.max(0, items.length - 1))
  }

  $: selectedPaths = items.filter((path) => selectedKeys.has(api.pathIdentity(path)))
  $: transferPaths = selectedPaths.length ? selectedPaths : items

  async function refresh() {
    const token = ++refreshToken
    const result = await api.pathKinds(items)
    if (token === refreshToken) kinds = result
  }

  /** 内訳。混ざった状態で「すべてコピー」を押す前に、何を運ぶのかが見えるようにする。 */
  $: dirCount = kinds.filter((kind) => kind.isDir).length
  $: fileCount = kinds.filter((kind) => kind.exists && !kind.isDir).length

  $: if (items) refresh()
  onMount(refresh)

  /**
   * 項目を開く。
   *
   * フォルダはそこへ入る（一時的なブックマークとしての使い方）。
   * ファイルは、それ自体を開くのはトレイの仕事ではないので、含まれる場所を開く。
   */
  function reveal(path: string, isDir: boolean) {
    if (isDir) return onNavigate(path)
    const { lead } = splitPath(path)
    onNavigate(lead || path)
  }

  function selectItem(path: string, index: number, ev: MouseEvent) {
    const key = api.pathIdentity(path)
    if (ev.shiftKey) {
      const [from, to] = anchor <= index ? [anchor, index] : [index, anchor]
      selectedKeys = new Set(items.slice(from, to + 1).map(api.pathIdentity))
    } else if (ev.ctrlKey) {
      selectedKeys.has(key) ? selectedKeys.delete(key) : selectedKeys.add(key)
      selectedKeys = new Set(selectedKeys)
      anchor = index
    } else {
      selectedKeys = new Set([key])
      anchor = index
    }
  }

  function onKeyDown(ev: KeyboardEvent) {
    if (ev.ctrlKey && ev.key.toLowerCase() === 'a') {
      ev.preventDefault()
      selectedKeys = new Set(items.map(api.pathIdentity))
    } else if (ev.key === 'Escape') {
      selectedKeys = new Set()
    }
  }
</script>

<div class="tray">
  {#if trays.length}
    <div class="switcher">
      <select
        aria-label="使うトレイ"
        value={activeTrayId}
        on:change={(event) => onSelect(Number(event.currentTarget.value))}
      >
        {#each trays as tray (tray.id)}
          <option value={tray.id}>{tray.name}{tray.count ? `（${tray.count}）` : ''}</option>
        {/each}
      </select>
      <button type="button" title="トレイを追加（目的別に分けて集める）" aria-label="トレイを追加" on:click={onAdd}>＋</button>
      <button type="button" title="このトレイの名前を変える" aria-label="トレイの名前を変える" disabled={!activeTray} on:click={() => onRename(activeTrayId)}>✎</button>
      <button type="button" title="このトレイを削除（ファイルは消えません）" aria-label="トレイを削除" disabled={trays.length <= 1} on:click={() => onDelete(activeTrayId)}>🗑</button>
    </div>
  {/if}
  <div class="summary">
    <span>
      {#if !items.length}
        Alt+Click で項目を集める
      {:else}
        {items.length} 件を収集中{#if dirCount && fileCount}<small class="breakdown"
            >（フォルダ {dirCount} / ファイル {fileCount}）</small
          >{/if}
      {/if}
    </span>
    {#if items.length}
      <button type="button" on:click={onClear}>すべて外す</button>
    {/if}
  </div>

  {#if items.length}
    <div class="destination" title={destinationPath}>
      <span>送り先</span>
      <strong>{splitPath(destinationPath).tail || destinationPath}</strong>
    </div>
    <div class="actions">
      <button type="button" disabled={transferBusy} on:click={() => onTransfer(transferPaths, false)}>
        {selectedPaths.length ? `選択 ${selectedPaths.length}件をコピー` : 'すべてコピー'}
      </button>
      <button type="button" disabled={transferBusy} on:click={() => onTransfer(transferPaths, true)}>
        {selectedPaths.length ? `選択 ${selectedPaths.length}件を移動` : 'すべて移動'}
      </button>
    </div>
    {#if selectedPaths.length}
      <button class="remove-selected" type="button" on:click={() => onRemoveMany(selectedPaths)}>
        選択 {selectedPaths.length}件をトレイから外す
      </button>
    {/if}
  {/if}

  <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
  <div class="items" tabindex="0" role="listbox" aria-label="収集トレイ" on:keydown={onKeyDown}>
    {#each items as path, i (api.pathIdentity(path))}
      {@const parts = splitPath(path)}
      {@const kind = kinds[i]}
      {@const missing = kind?.exists === false}
      {@const isDir = kind?.isDir === true}
      <div
        class="item"
        class:selected={selectedKeys.has(api.pathIdentity(path))}
        class:missing
        class:dir={isDir}
        title={path}
        role="option"
        tabindex="-1"
        aria-selected={selectedKeys.has(api.pathIdentity(path))}
        on:mousedown={isDir ? armMiddleClick : null}
        on:auxclick={isDir ? onMiddleClick(() => onOpenBeside(path)) : null}
      >
        {#if isDir}
          <!-- フォルダだけが持つ動詞。選択とは当たり判定を分けるので、部分選択はそのまま使える。 -->
          <button
            class="go"
            type="button"
            title="ここへ移動"
            on:click|stopPropagation={() => onNavigate(path)}
          >
            →
          </button>
        {:else}
          <span class="go-space" />
        {/if}
        <button
          class="open"
          type="button"
          on:click={(e) => selectItem(path, i, e)}
          on:dblclick={() => reveal(path, isDir)}
        >
          <span class="icon">{missing ? '⚠' : isDir ? '📁' : '◈'}</span>
          <span class="text">
            <strong>{parts.tail || path}</strong>
            <small>{missing ? '見つかりません' : parts.lead}</small>
          </span>
        </button>
        <button class="remove" type="button" title="トレイから外す" on:click|stopPropagation={() => onRemove(path)}>×</button>
      </div>
    {/each}
  </div>
</div>

<style>
  .tray { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .switcher { display: flex; align-items: center; gap: 2px; padding: 5px 6px; border-bottom: 1px solid #2c2c2c; }
  .switcher select { flex: 1; min-width: 0; padding: 3px 4px; border: 1px solid #333; border-radius: 3px; background: #202020; color: #ddd; font: inherit; font-size: 10.5px; }
  .switcher button { width: 22px; height: 22px; border-radius: 3px; color: #8f9aaa; font-size: 11px; }
  .switcher button:hover:not(:disabled) { background: #2a2f35; color: #dfe7ee; }
  .switcher button:disabled { opacity: .35; cursor: default; }
  .summary { display: flex; align-items: center; gap: 6px; padding: 7px 8px; border-bottom: 1px solid #2c2c2c; color: #8f9aaa; font-size: 10.5px; }
  .summary span { flex: 1; }
  .breakdown { color: #67747c; font-size: 9.5px; }
  button { border: 0; background: none; color: inherit; font: inherit; cursor: pointer; }
  .summary button { color: #8fbce8; }
  .items { flex: 1; min-height: 0; overflow: auto; padding: 4px; }
  .actions { display: flex; gap: 5px; padding: 6px 7px; border-bottom: 1px solid #2c2c2c; }
  .actions button { flex: 1; padding: 5px 3px; border: 1px solid #345266; border-radius: 3px; color: #b9d9ee; background: #1d303c; font-size: 10px; }
  .actions button:hover:not(:disabled) { background: #25475a; }
  .actions button:disabled { opacity: .45; cursor: default; }
  .destination { display: flex; gap: 6px; align-items: center; padding: 5px 8px 0; color: #67747c; font-size: 9.5px; }
  .destination strong { color: #9bb9c8; font-size: 10.5px; font-weight: 500; }
  .remove-selected { margin: 0 7px 6px; padding: 3px; border: 1px solid #4c3b3b; border-radius: 3px; color: #bd8f8f; font-size: 9.5px; }
  .remove-selected:hover { background: #382727; color: #efb2b2; }
  .item { display: flex; align-items: center; border-radius: 4px; color: #ccc; }
  .item:hover { background: #252b31; }
  .item.selected { background: #294b55; box-shadow: inset 2px 0 #67c8da; }
  .item.missing { opacity: .58; }
  /* フォルダ行だけが持つ「行く」。普段は薄く、行に触れると濃くなる。
     一覧のインライン展開（▸）と同じ作法で、新しく覚えることを増やさない。 */
  .go { flex: none; width: 16px; height: 28px; color: #3f4650; font-size: 10px; }
  .item:hover .go { color: #8fbce8; }
  .go:hover { color: #cfe6fb; }
  /* つまみを持たないファイル行も、名前の開始位置を揃える。 */
  .go-space { flex: none; width: 16px; }
  .open { display: flex; align-items: center; gap: 7px; flex: 1; min-width: 0; padding: 6px 6px 6px 0; text-align: left; }
  .icon { flex: none; color: #65c5a5; }
  /* 行き先になれるものは、運ぶだけのものと色で分ける。 */
  .item.dir .icon { color: #8fbce8; }
  .missing .icon { color: #d7a65b; }
  .text { display: flex; flex-direction: column; min-width: 0; }
  strong, small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  strong { font-size: 11.5px; font-weight: 500; }
  small { color: #6f7882; font-size: 9.5px; direction: rtl; text-align: left; }
  .remove { flex: none; width: 24px; height: 28px; color: #68717a; font-size: 14px; }
  .remove:hover { color: #ff9b9b; }
</style>
