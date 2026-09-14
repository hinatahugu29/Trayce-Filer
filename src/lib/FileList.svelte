<script lang="ts">
  import { tick } from 'svelte'
  import { startDrag } from '@crabnebula/tauri-plugin-drag'
  import { fileIcon, formatSize, formatModified, joinPath, pathIdentity } from './api'
  import type { Entry, SortKey, SortSpec } from './api'

  export let entries: Entry[] = []
  export let parent: string | null = null
  /** 表示中のディレクトリ。選択を解除する判断はこれの変化で行う。 */
  export let path = ''
  export let dragIcon = ''
  export let sort: SortSpec
  export let onOpen: (path: string) => void
  /** ファイルを既定のアプリで開く。パスは組み立て済みのものを渡す。 */
  export let onLaunch: (entry: Entry, path: string) => void = () => {}
  export let onSort: (key: SortKey) => void = () => {}
  export let onSelectionChange: (paths: string[]) => void = () => {}
  export let onNote: (message: string) => void = () => {}
  export let onDelete: (paths: string[]) => void = () => {}
  export let onRename: (entry: Entry) => void = () => {}
  export let trayItems: string[] = []
  export let onTrayToggle: (path: string) => void = () => {}
  /**
   * 右クリック。`entry` が null なら空き領域での右クリック。
   * 呼び出し側はこれを見て「選択に対する操作」と「この場所に対する操作」を出し分ける。
   */
  export let onContext: (ev: MouseEvent, entry: Entry | null) => void = () => {}
  /** Virtual listings such as search results already own an absolute path. */
  export let resolvePath: (entry: Entry) => string = (entry) => joinPath(path, entry.name)
  /** Optional second line used by virtual listings to show the source directory. */
  export let secondaryLabel: (entry: Entry) => string = () => ''

  const ROW_H = 24
  /** 画面外に少し余分に描いて、速いスクロールでも空白が見えないようにする。 */
  const OVERSCAN = 8

  let viewport: HTMLDivElement | null = null
  let scrollTop = 0
  let viewportH = 0

  /**
   * 一覧にキーボードの焦点を戻す。
   *
   * アドレスバーやツリーから移動した直後は、そこに焦点が残ったままだと
   * ↑↓ も Ctrl+A も一覧に届かない。移動した直後こそキーボードを使いたい。
   */
  export function focusList() {
    viewport?.focus({ preventScroll: true })
  }

  // 「..」を先頭の仮想行として混ぜる。
  type Row = { kind: 'up'; path: string } | { kind: 'entry'; entry: Entry }
  $: rows = [
    ...(parent ? [{ kind: 'up', path: parent }] : []),
    ...entries.map((entry) => ({ kind: 'entry', entry })),
  ] as Row[]

  // 件数が数千でも軽く保つため、見えている範囲だけ描く。
  $: first = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN)
  $: last = Math.min(rows.length, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN)
  $: visible = rows.slice(first, last)

  // 別のフォルダへ移ったら先頭に戻し、選択も切る。
  // 前の選択を残したまま別フォルダで削除すると事故になる。
  //
  // 判定に entries の同一性を使ってはいけない。呼び出し側が
  // `listing?.entries ?? []` のように毎描画で新しい配列を渡すと、
  // ここが毎回走って onSelectionChange → 親の再描画 → また新しい配列、
  // という無限ループになる（Svelte の dirty_components が膨れて
  // "Invalid array length" で落ちる）。実際に変わった時だけ反応させる。
  let lastPath: string | null = null
  $: if (path !== lastPath) {
    lastPath = path
    resetView()
  }

  function resetView() {
    if (viewport) viewport.scrollTop = 0
    scrollTop = 0
    selected = new Set()
    cursor = 0
    anchor = 0
    onSelectionChange([])
  }

  /** Absolute paths also keep same-named results from different folders distinct. */
  let selected = new Set<string>()
  /** キーボードの選択位置。rows の添字。 */
  let cursor = 0
  /** Shift+クリック / Shift+↑↓ の起点。 */
  let anchor = 0

  const fullPath = (entry: Entry) => resolvePath(entry)
  $: trayKeys = new Set(trayItems.map(pathIdentity))

  function emitSelection() {
    onSelectionChange([...selected])
  }

  /** 起点から現在位置までを選択する。 */
  function selectRange(from: number, to: number) {
    const [lo, hi] = from <= to ? [from, to] : [to, from]
    selected = new Set(
      rows
        .slice(lo, hi + 1)
        .filter((r): r is { kind: 'entry'; entry: Entry } => r.kind === 'entry')
        .map((r) => fullPath(r.entry))
    )
    emitSelection()
  }

  function onRowClick(index: number, row: Row, ev: MouseEvent) {
    cursor = index
    if (row.kind !== 'entry') return

    if (ev.altKey) {
      ev.preventDefault()
      onTrayToggle(fullPath(row.entry))
      return
    }

    if (ev.shiftKey) {
      selectRange(anchor, index)
      return
    }
    anchor = index
    const name = fullPath(row.entry)
    if (ev.ctrlKey) {
      selected.has(name) ? selected.delete(name) : selected.add(name)
      selected = selected
    } else {
      selected = new Set([name])
    }
    emitSelection()
  }

  /**
   * 行の右クリック。
   *
   * 未選択の行を右クリックしたら、その行だけを選択してからメニューを出す。
   * そうしないと「別の物を選んだままメニューを開いて、見えていない対象を消す」事故が起きる。
   */
  function onRowContext(index: number, entry: Entry, ev: MouseEvent) {
    ev.preventDefault()
    cursor = index
    if (!selected.has(fullPath(entry))) {
      anchor = index
      selected = new Set([fullPath(entry)])
      emitSelection()
    }
    onContext(ev, entry)
  }

  /** 一覧の空き領域での右クリック。行の上なら行側が既に処理しているので何もしない。 */
  function onEmptyContext(ev: MouseEvent) {
    const el = ev.target as HTMLElement | null
    if (el?.closest('.row')) return
    ev.preventDefault()
    onContext(ev, null)
  }

  /** カーソル位置の行を開く。フォルダなら移動、ファイルなら既定のアプリ。 */
  function activate(row: Row) {
    if (row.kind === 'up') return onOpen(row.path)
    if (row.entry.is_dir) return onOpen(fullPath(row.entry))
    onLaunch(row.entry, fullPath(row.entry))
  }

  async function moveCursor(delta: number, extend: boolean) {
    const next = Math.max(0, Math.min(rows.length - 1, cursor + delta))
    if (next === cursor) return
    cursor = next

    if (extend) {
      selectRange(anchor, cursor)
    } else {
      anchor = cursor
      const row = rows[cursor]
      selected = row.kind === 'entry' ? new Set([fullPath(row.entry)]) : new Set()
      emitSelection()
    }
    await scrollCursorIntoView()
  }

  async function scrollCursorIntoView() {
    await tick()
    if (!viewport) return
    const top = cursor * ROW_H
    const bottom = top + ROW_H
    if (top < viewport.scrollTop) {
      viewport.scrollTop = top
    } else if (bottom > viewport.scrollTop + viewport.clientHeight) {
      viewport.scrollTop = bottom - viewport.clientHeight
    }
    scrollTop = viewport.scrollTop
  }

  /**
   * 一覧のキーボード操作。
   *
   * ファイラの速度はここで決まる。マウスに手を伸ばさずに
   * 移動・選択・開く・上へ戻る が完結すること。
   */
  async function onKeyDown(ev: KeyboardEvent) {
    const page = Math.max(1, Math.floor(viewportH / ROW_H) - 1)

    switch (ev.key) {
      case 'ArrowDown':
        ev.preventDefault()
        await moveCursor(1, ev.shiftKey)
        break
      case 'ArrowUp':
        ev.preventDefault()
        await moveCursor(-1, ev.shiftKey)
        break
      case 'PageDown':
        ev.preventDefault()
        await moveCursor(page, ev.shiftKey)
        break
      case 'PageUp':
        ev.preventDefault()
        await moveCursor(-page, ev.shiftKey)
        break
      case 'Home':
        ev.preventDefault()
        await moveCursor(-rows.length, ev.shiftKey)
        break
      case 'End':
        ev.preventDefault()
        await moveCursor(rows.length, ev.shiftKey)
        break
      case 'Enter':
        ev.preventDefault()
        if (rows[cursor]) activate(rows[cursor])
        break
      case 'Backspace':
        ev.preventDefault()
        if (parent) onOpen(parent)
        break
      case 'Delete':
        ev.preventDefault()
        if (selected.size) onDelete([...selected])
        break
      case 'F2': {
        ev.preventDefault()
        const row = rows[cursor]
        if (row?.kind === 'entry') onRename(row.entry)
        break
      }
      case 'a':
        if (ev.ctrlKey) {
          ev.preventDefault()
          selected = new Set(entries.map(fullPath))
          emitSelection()
        }
        break
      case 'Escape':
        selected = new Set()
        emitSelection()
        break
    }
  }

  // pointerdown で即 startDrag すると、クリックやダブルクリックまで
  // ネイティブドラッグに化けてフォルダ移動ができなくなる。一定距離動いてから始める。
  const DRAG_THRESHOLD_PX = 5
  let pending: { entry: Entry; x: number; y: number } | null = null

  function onPointerDown(entry: Entry, ev: PointerEvent) {
    if (ev.button !== 0) return
    pending = { entry, x: ev.clientX, y: ev.clientY }
  }

  async function onPointerMove(ev: PointerEvent) {
    if (!pending) return
    if (Math.hypot(ev.clientX - pending.x, ev.clientY - pending.y) < DRAG_THRESHOLD_PX) return

    const { entry } = pending
    pending = null // startDrag は制御を OS に渡すので、先に掴み状態を解く

    // 掴んだものが選択に含まれていなければ、それ単体を運ぶ。
    const items = selected.has(fullPath(entry)) ? [...selected] : [fullPath(entry)]

    onNote(`drag out 開始: ${items.length}件`)
    try {
      await startDrag({ item: items, icon: dragIcon }, (payload) =>
        onNote(`drag out 結果: ${payload.result}`)
      )
    } catch (e) {
      onNote(`drag out 失敗: ${e}`)
    }
  }

  const columns: { key: SortKey; label: string; cls: string }[] = [
    { key: 'name', label: '名前', cls: 'c-name' },
    { key: 'ext', label: '種類', cls: 'c-ext' },
    { key: 'size', label: 'サイズ', cls: 'c-size' },
    { key: 'modified', label: '更新日時', cls: 'c-time' },
  ]
</script>

<div class="head">
  {#each columns as col}
    <button
      type="button"
      class="col {col.cls}"
      class:active={sort.key === col.key}
      on:click={() => onSort(col.key)}
    >
      {col.label}
      {#if sort.key === col.key}<span class="arrow">{sort.descending ? '▾' : '▴'}</span>{/if}
    </button>
  {/each}
</div>

<!-- svelte-ignore a11y-no-noninteractive-tabindex -->
<div
  class="viewport"
  tabindex="0"
  role="listbox"
  aria-label="ファイル一覧"
  bind:this={viewport}
  bind:clientHeight={viewportH}
  on:scroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  on:keydown={onKeyDown}
  on:pointermove={onPointerMove}
  on:pointerup={() => (pending = null)}
  on:pointerleave={() => (pending = null)}
  on:contextmenu={onEmptyContext}
>
  <!-- 実件数ぶんの高さを確保して、スクロールバーの長さを正しく見せる。 -->
  <div class="spacer" style="height: {rows.length * ROW_H}px">
    <div class="rows" style="transform: translateY({first * ROW_H}px)">
      {#each visible as row, vi (row.kind === 'up' ? '..' : fullPath(row.entry))}
        {@const index = first + vi}
        {#if row.kind === 'up'}
          <div
            class="row up"
            role="option"
            aria-selected="false"
            tabindex="-1"
            class:cursor={index === cursor}
            style="height: {ROW_H}px"
            on:click={(e) => onRowClick(index, row, e)}
            on:dblclick={() => onOpen(row.path)}
            on:keydown={(e) => e.key === 'Enter' && onOpen(row.path)}
          >
            <span class="col c-name"><span class="icon">↰</span><span class="name">..</span></span>
          </div>
        {:else}
          {@const entry = row.entry}
          <div
            class="row"
            role="option"
            aria-selected={selected.has(fullPath(entry))}
            tabindex="-1"
            class:dir={entry.is_dir}
            class:selected={selected.has(fullPath(entry))}
            class:cursor={index === cursor}
            class:hidden={entry.hidden}
            class:in-tray={trayKeys.has(pathIdentity(fullPath(entry)))}
            style="height: {ROW_H}px"
            on:click={(e) => onRowClick(index, row, e)}
            on:dblclick={() => activate(row)}
            on:keydown={(e) => e.key === 'Enter' && activate(row)}
            on:contextmenu={(e) => onRowContext(index, entry, e)}
            on:pointerdown={(e) => onPointerDown(entry, e)}
          >
            <span class="col c-name">
              <span class="icon">{fileIcon(entry.name, entry.is_dir)}</span>
              <span class="name-stack"><span class="name">{entry.name}</span>{#if secondaryLabel(entry)}<small>{secondaryLabel(entry)}</small>{/if}</span>
              {#if trayKeys.has(pathIdentity(fullPath(entry)))}<span class="tray-mark" title="トレイに登録済み">◈</span>{/if}
            </span>
            <span class="col c-ext">{entry.is_dir ? '' : entry.ext}</span>
            <span class="col c-size">{formatSize(entry.size, entry.is_dir)}</span>
            <span class="col c-time">{formatModified(entry.modified)}</span>
          </div>
        {/if}
      {/each}
    </div>
  </div>
</div>

<style>
  .head {
    display: flex;
    flex: none;
    border-bottom: 1px solid #333;
    background: #212121;
  }
  .head .col {
    background: none;
    border: 0;
    border-right: 1px solid #2c2c2c;
    padding: 4px 8px;
    font: inherit;
    font-size: 10.5px;
    color: #888;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
  }
  .head .col:hover {
    background: #2a2a2a;
    color: #ccc;
  }
  .head .col.active {
    color: #fff;
  }
  .arrow {
    font-size: 8px;
    margin-left: 2px;
  }

  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    outline: none;
  }
  .viewport:focus-visible {
    box-shadow: inset 0 0 0 1px #3a5f8a;
  }
  .spacer {
    position: relative;
  }
  .rows {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    will-change: transform;
  }

  .row {
    display: flex;
    align-items: center;
    font-size: 12.5px;
    cursor: default;
    user-select: none;
    box-sizing: border-box;
  }
  .row:hover {
    background: #2b2b2b;
  }
  .row.selected {
    background: #2d4a6b;
  }
  .row.in-tray { box-shadow: inset 3px 0 #58c6a5; }
  .row.in-tray:not(.selected) { background: #20332f; }
  .tray-mark { margin-left: auto; padding-right: 5px; color: #66d1ae; font-size: 10px; }
  .name-stack { display: flex; min-width: 0; flex: 1; flex-direction: column; line-height: 10px; }
  .name-stack .name, .name-stack small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name-stack small { color: #6f7880; font-size: 8px; }
  /* キーボードの位置。選択とは別に示さないと、Shift 選択中に迷子になる。 */
  .row.cursor {
    box-shadow: inset 0 0 0 1px #4c9aff;
  }
  .row.dir .name {
    color: #9cd0ff;
  }
  .row.up .name {
    color: #888;
  }
  .row.hidden {
    opacity: 0.55;
  }

  /* 列幅はヘッダと本体で同じ指定にする。 */
  .col {
    box-sizing: border-box;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .c-name {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding-left: 10px;
  }
  .c-ext {
    width: 62px;
    flex: none;
    padding: 0 6px;
    color: #8a8a8a;
    font-size: 11px;
  }
  .c-size {
    width: 78px;
    flex: none;
    padding: 0 8px;
    text-align: right;
    color: #9a9a9a;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .c-time {
    width: 108px;
    flex: none;
    padding: 0 8px;
    color: #8a8a8a;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .icon {
    width: 16px;
    flex: none;
    text-align: center;
    font-size: 11px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* 検索元と作業先を並べた狭い幅では、重要度の低い列から畳む。 */
  @container (max-width: 520px) {
    .c-time { display: none; }
  }
  @container (max-width: 380px) {
    .c-ext { display: none; }
    .c-size { width: 58px; }
  }
</style>
