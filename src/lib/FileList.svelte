<script lang="ts" context="module">
  /** 型は module context に置く。インスタンス側では `export type` が使えない。 */

  /** スクロール位置・選択・カーソルの組。場所を離れる時に持ち出す単位。 */
  export type ViewState = { scrollTop: number; selected: string[]; cursor: number }
</script>

<script lang="ts">
  import { tick } from 'svelte'
  import { startDrag } from '@crabnebula/tauri-plugin-drag'
  import { fileIcon, formatSize, formatModified, joinPath, pathIdentity } from './api'
  import * as prefetch from './prefetch'
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
  /**
   * 情報を落とした表示。
   *
   * 注目していないペインで種類・サイズ・更新日時をフルに出しても、実際に読まれるのは
   * 名前だけで、その名前が幅を奪われて省略される。見られていない列を畳んで、
   * 残った幅を名前へ回す。
   */
  export let dense = false
  /**
   * フォルダ行をその場で展開できるようにする。
   *
   * 「表示フォルダを完全に切り替える」のをやめるための機能。子を見るのに移動すると
   * 現在地・選択・スクロールが破棄されるが、ここで開けば親子が同じ平面に並ぶ。
   * 複数階層から続けてトレイへ拾う使い方もできる。
   *
   * 検索結果のような仮想一覧では切っておく。行ごとに親が違うため、
   * 「その場で開く」が階層の話にならない。
   */
  export let expandable = false

  const ROW_H = 24
  /** 1段ぶんの字下げ。深くしすぎると名前の幅が消えるので控えめに。 */
  const INDENT_PX = 13
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

  /** いまの見え方を取り出す。場所を離れる直前に呼び、戻ってきた時に restoreView へ渡す。 */
  export function captureView(): ViewState {
    return { scrollTop, selected: [...selected], cursor }
  }

  /**
   * 以前の見え方へ戻す。
   *
   * `path` が変わると resetView() が先に走って選択もスクロールも消えるので、
   * 呼び出し側は一覧が新しい場所で描き終わってからここへ来ること。
   */
  export async function restoreView(view: ViewState) {
    selected = new Set(view.selected)
    // 件数が減っていることがある。範囲外のカーソルは末尾へ寄せる。
    cursor = Math.min(Math.max(0, view.cursor), Math.max(0, rows.length - 1))
    anchor = cursor
    await tick()
    if (viewport) viewport.scrollTop = view.scrollTop
    scrollTop = viewport?.scrollTop ?? view.scrollTop
    onSelectionChange([...selected])
  }

  /**
   * その場で開いているフォルダ。値は読み込んだ子（読み込み中は null）。
   * 鍵は `pathIdentity`。大文字小文字が違うだけで二重に開くのを防ぐ。
   */
  let expanded = new Map<string, Entry[] | null>()

  async function toggleExpand(target: string) {
    const key = pathIdentity(target)
    if (expanded.has(key)) {
      expanded.delete(key)
      expanded = expanded
      return
    }
    expanded.set(key, null)
    expanded = expanded
    try {
      const listing = await prefetch.listDir(target, sort)
      // 読んでいる間に畳まれていたら捨てる。
      if (expanded.has(key)) expanded.set(key, listing.entries)
    } catch {
      // 権限が無いフォルダなど。空として開き、印だけ出す。
      if (expanded.has(key)) expanded.set(key, [])
    }
    expanded = expanded
  }

  // 「..」を先頭の仮想行として混ぜる。展開した子は親のすぐ下へ、深さを持たせて並べる。
  type Row =
    | { kind: 'up'; path: string }
    | { kind: 'entry'; entry: Entry; path: string; depth: number }
    /** 読み込み中・空・読めなかった展開先を示すだけの行。 */
    | { kind: 'note'; path: string; depth: number; text: string }

  $: rows = buildRows(entries, expanded, path, parent, resolvePath)

  /** 実体のある行だけを取り出す型の絞り込み。展開した子も含む。 */
  type EntryRow = Extract<Row, { kind: 'entry' }>
  const isEntryRow = (row: Row): row is EntryRow => row.kind === 'entry'

  function buildRows(
    list: Entry[],
    open: Map<string, Entry[] | null>,
    dir: string,
    up: string | null,
    resolve: (entry: Entry) => string
  ): Row[] {
    const out: Row[] = []
    if (up) out.push({ kind: 'up', path: up })

    const walk = (items: Entry[], base: string, depth: number) => {
      for (const entry of items) {
        // 一番上の段だけは呼び出し側の解決規則に従う（検索結果など仮想一覧のため）。
        const abs = depth === 0 ? resolve(entry) : joinPath(base, entry.name)
        out.push({ kind: 'entry', entry, path: abs, depth })
        if (!entry.is_dir) continue
        const key = pathIdentity(abs)
        if (!open.has(key)) continue
        const children = open.get(key) ?? null
        if (children === null) {
          out.push({ kind: 'note', path: abs, depth: depth + 1, text: '読み込み中…' })
        } else if (children.length === 0) {
          out.push({ kind: 'note', path: abs, depth: depth + 1, text: '（空、または読めません）' })
        } else {
          walk(children, abs, depth + 1)
        }
      }
    }

    walk(list, dir, 0)
    return out
  }

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
    // 別の場所へ移ったら、その場で開いていた枝も畳む。
    // 前の場所の枝を残すと、行の並びと現在地が食い違う。
    expanded = new Map()
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
        .filter(isEntryRow)
        .map((r) => r.path)
    )
    emitSelection()
  }

  function onRowClick(index: number, row: Row, ev: MouseEvent) {
    cursor = index
    if (row.kind !== 'entry') return

    if (ev.altKey) {
      ev.preventDefault()
      onTrayToggle(row.path)
      return
    }

    if (ev.shiftKey) {
      selectRange(anchor, index)
      return
    }
    anchor = index
    const name = row.path
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
  function onRowContext(index: number, row: EntryRow, ev: MouseEvent) {
    ev.preventDefault()
    cursor = index
    if (!selected.has(row.path)) {
      anchor = index
      selected = new Set([row.path])
      emitSelection()
    }
    onContext(ev, row.entry)
  }

  // ---- 矩形選択（ドラッグ選択） ----
  //
  // 空き領域から引いた時だけ働く。行の上から引くのは外部アプリへのドラッグなので、
  // 起点で用途が決まる（Explorer と同じ規則）。
  //
  // 座標は viewport ではなく「内容」基準で持つ。スクロールしながら引いても
  // 矩形が紙にくっついたまま伸びるようにするため。
  type Marquee = { x0: number; y0: number; x1: number; y1: number }
  let marquee: Marquee | null = null
  /** 引き始めた時点の選択。Ctrl / Shift 併用ならここへ足していく。 */
  let marqueeBase = new Set<string>()
  let autoScrollTimer: number | null = null
  /** 端からこの距離まで来たら自動でスクロールする。 */
  const EDGE_PX = 24
  const EDGE_SPEED = 12

  function contentPoint(ev: PointerEvent): { x: number; y: number } | null {
    if (!viewport) return null
    const box = viewport.getBoundingClientRect()
    return {
      x: ev.clientX - box.left + viewport.scrollLeft,
      y: ev.clientY - box.top + viewport.scrollTop,
    }
  }

  function startMarquee(ev: PointerEvent) {
    const el = ev.target as HTMLElement | null
    // 行の上から引いた場合は外部ドラッグ。ここでは扱わない。
    if (ev.button !== 0 || el?.closest('.row')) return
    // スクロールバーも viewport の一部なので、掴んだだけで矩形が始まってしまう。
    // 内容の幅より右で押された場合はバーとみなす。
    if (viewport && ev.clientX - viewport.getBoundingClientRect().left >= viewport.clientWidth) return
    const at = contentPoint(ev)
    if (!at) return
    marquee = { x0: at.x, y0: at.y, x1: at.x, y1: at.y }
    marqueeBase = ev.ctrlKey || ev.shiftKey ? new Set(selected) : new Set()
    if (!ev.ctrlKey && !ev.shiftKey) {
      selected = new Set()
      emitSelection()
    }
    viewport?.setPointerCapture(ev.pointerId)
  }

  function applyMarquee() {
    if (!marquee) return
    const top = Math.min(marquee.y0, marquee.y1)
    const bottom = Math.max(marquee.y0, marquee.y1)
    // 行は固定高なので、範囲は割り算だけで出る（仮想化していても正しい）。
    const from = Math.max(0, Math.floor(top / ROW_H))
    const to = Math.min(rows.length - 1, Math.floor(bottom / ROW_H))

    const next = new Set(marqueeBase)
    for (let i = from; i <= to; i++) {
      const row = rows[i]
      if (isEntryRow(row)) next.add(row.path)
    }
    selected = next
    emitSelection()
  }

  function stopAutoScroll() {
    if (autoScrollTimer !== null) {
      clearInterval(autoScrollTimer)
      autoScrollTimer = null
    }
  }

  /**
   * 端まで引いた時に送り続ける。
   *
   * pointermove は指が止まると来なくなるので、端に置いたまま待つ動きを
   * 成立させるには別に回し続ける必要がある。
   */
  function updateAutoScroll(clientY: number) {
    if (!viewport) return
    const box = viewport.getBoundingClientRect()
    const up = clientY - box.top < EDGE_PX
    const down = box.bottom - clientY < EDGE_PX
    if (!up && !down) return stopAutoScroll()
    if (autoScrollTimer !== null) return
    autoScrollTimer = window.setInterval(() => {
      if (!viewport || !marquee) return stopAutoScroll()
      const before = viewport.scrollTop
      viewport.scrollTop += up ? -EDGE_SPEED : EDGE_SPEED
      const moved = viewport.scrollTop - before
      if (moved === 0) return stopAutoScroll()
      scrollTop = viewport.scrollTop
      // 指は動いていないので、伸びた先はスクロール量ぶんだけ進める。
      marquee = { ...marquee, y1: marquee.y1 + moved }
      applyMarquee()
    }, 16)
  }

  function endMarquee(ev: PointerEvent) {
    if (!marquee) return
    marquee = null
    stopAutoScroll()
    if (viewport?.hasPointerCapture(ev.pointerId)) viewport.releasePointerCapture(ev.pointerId)
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
    if (row.kind === 'note') return
    if (row.entry.is_dir) return onOpen(row.path)
    onLaunch(row.entry, row.path)
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
      selected = isEntryRow(row) ? new Set([row.path]) : new Set()
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
      // 移動と区別する。→ はその場で開くだけで、現在地は変えない。
      case 'ArrowRight': {
        if (!expandable) break
        const row = rows[cursor]
        if (!isEntryRow(row) || !row.entry.is_dir) break
        ev.preventDefault()
        if (!expanded.has(pathIdentity(row.path))) await toggleExpand(row.path)
        else await moveCursor(1, false)
        break
      }
      case 'ArrowLeft': {
        if (!expandable) break
        const row = rows[cursor]
        if (!isEntryRow(row)) break
        if (expanded.has(pathIdentity(row.path))) {
          ev.preventDefault()
          await toggleExpand(row.path)
        } else if (row.depth > 0) {
          // 子にいるなら、まず自分を含んでいる行まで戻る。
          ev.preventDefault()
          for (let i = cursor - 1; i >= 0; i--) {
            const candidate = rows[i]
            if (isEntryRow(candidate) && candidate.depth < row.depth) {
              await moveCursor(i - cursor, false)
              break
            }
          }
        }
        break
      }
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
          // 展開した子も対象にする。見えているものが選ばれないと辻褄が合わない。
          selected = new Set(rows.filter(isEntryRow).map((r) => r.path))
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
  let pending: { path: string; x: number; y: number } | null = null

  function onPointerDown(row: EntryRow, ev: PointerEvent) {
    if (ev.button !== 0) return
    pending = { path: row.path, x: ev.clientX, y: ev.clientY }
  }

  async function onPointerMove(ev: PointerEvent) {
    if (marquee) {
      const at = contentPoint(ev)
      if (at) {
        marquee = { ...marquee, x1: at.x, y1: at.y }
        applyMarquee()
      }
      updateAutoScroll(ev.clientY)
      return
    }
    if (!pending) return
    if (Math.hypot(ev.clientX - pending.x, ev.clientY - pending.y) < DRAG_THRESHOLD_PX) return

    const grabbed = pending.path
    pending = null // startDrag は制御を OS に渡すので、先に掴み状態を解く

    // 掴んだものが選択に含まれていなければ、それ単体を運ぶ。
    const items = selected.has(grabbed) ? [...selected] : [grabbed]

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

<div class="head" class:dense>
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
  class:dense
  tabindex="0"
  role="listbox"
  aria-label="ファイル一覧"
  bind:this={viewport}
  bind:clientHeight={viewportH}
  on:scroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  on:keydown={onKeyDown}
  on:pointerdown={startMarquee}
  on:pointermove={onPointerMove}
  on:pointerup={(e) => {
    pending = null
    endMarquee(e)
  }}
  on:pointercancel={endMarquee}
  on:pointerleave={() => (pending = null)}
  on:contextmenu={onEmptyContext}
>
  <!-- 実件数ぶんの高さを確保して、スクロールバーの長さを正しく見せる。 -->
  <div class="spacer" style="height: {rows.length * ROW_H}px">
    {#if marquee}
      <div
        class="marquee"
        style="
          left: {Math.min(marquee.x0, marquee.x1)}px;
          top: {Math.min(marquee.y0, marquee.y1)}px;
          width: {Math.abs(marquee.x1 - marquee.x0)}px;
          height: {Math.abs(marquee.y1 - marquee.y0)}px;
        "
      />
    {/if}
    <div class="rows" style="transform: translateY({first * ROW_H}px)">
      {#each visible as row, vi (row.kind === 'up' ? '..' : row.kind + row.path)}
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
        {:else if row.kind === 'note'}
          <div class="row note" style="height: {ROW_H}px">
            <span class="col c-name" style="padding-left: {10 + row.depth * INDENT_PX}px">
              <span class="note-text">{row.text}</span>
            </span>
          </div>
        {:else}
          {@const entry = row.entry}
          {@const open = expanded.has(pathIdentity(row.path))}
          <div
            class="row"
            role="option"
            aria-selected={selected.has(row.path)}
            tabindex="-1"
            class:dir={entry.is_dir}
            class:selected={selected.has(row.path)}
            class:cursor={index === cursor}
            class:hidden={entry.hidden}
            class:nested={row.depth > 0}
            class:in-tray={trayKeys.has(pathIdentity(row.path))}
            style="height: {ROW_H}px"
            on:click={(e) => onRowClick(index, row, e)}
            on:dblclick={() => activate(row)}
            on:keydown={(e) => e.key === 'Enter' && activate(row)}
            on:contextmenu={(e) => onRowContext(index, row, e)}
            on:pointerdown={(e) => onPointerDown(row, e)}
          >
            <span class="col c-name" style="padding-left: {10 + row.depth * INDENT_PX}px">
              {#if expandable && entry.is_dir}
                <!-- 移動せずに中を出す。ここを押しても現在地は変わらない。 -->
                <button
                  type="button"
                  class="twist"
                  class:open
                  aria-expanded={open}
                  title={open ? 'ここで畳む' : 'ここで開く（移動しません）'}
                  on:click|stopPropagation={() => toggleExpand(row.path)}
                >
                  ▸
                </button>
              {:else if expandable}
                <span class="twist-space" />
              {/if}
              <span class="icon">{fileIcon(entry.name, entry.is_dir)}</span>
              <span class="name-stack"><span class="name">{entry.name}</span>{#if secondaryLabel(entry)}<small>{secondaryLabel(entry)}</small>{/if}</span>
              {#if trayKeys.has(pathIdentity(row.path))}<span class="tray-mark" title="トレイに登録済み">◈</span>{/if}
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
  /* 引いている矩形。半透明で行の上に重ねる（Explorer と同じ見え方）。
     下に敷くと、背景を持たない行の上でしか見えず、掴んだ範囲が読めない。 */
  .marquee {
    position: absolute;
    z-index: 2;
    pointer-events: none;
    border: 1px solid #5a8fc4;
    background: #5a8fc433;
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
  /* その場で開くつまみ。押しても現在地は変わらないので、移動の印とは形を分ける。 */
  .twist {
    flex: none;
    width: 12px;
    padding: 0;
    border: 0;
    font-size: 8px;
    line-height: 1;
    color: #6a6a6a;
    background: none;
    cursor: pointer;
    transition: transform 90ms linear;
  }

  .twist:hover {
    color: #ddd;
  }

  .twist.open {
    transform: rotate(90deg);
    color: #b0b0b0;
  }

  /* つまみを持たない行も、名前の開始位置を揃える。 */
  .twist-space {
    flex: none;
    width: 12px;
  }

  /* 展開した子。親より一段沈ませて、同じ平面でも階層が読めるようにする。 */
  .row.nested {
    background: #1b1b1b;
  }

  .row.nested.selected {
    background: #2d4a63;
  }

  .row.note {
    align-items: center;
  }

  .note-text {
    font-size: 10px;
    color: #6a6a6a;
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

  /* 注目していないペイン。幅ではなく注意の量に応じて情報を落とす。
     サイズだけは残す：転送先を選ぶ時に「入るかどうか」の手掛かりになる。 */
  .dense .c-ext,
  .dense .c-time {
    display: none;
  }
  .dense .c-size {
    width: 62px;
  }
</style>
