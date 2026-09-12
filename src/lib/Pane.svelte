<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import type { UnlistenFn } from '@tauri-apps/api/event'
  import PathBar from './PathBar.svelte'
  import FileList from './FileList.svelte'
  import Sidebar from './Sidebar.svelte'
  import TransferBar from './TransferBar.svelte'
  import Preview from './Preview.svelte'
  import ContextMenu from './ContextMenu.svelte'
  import type { MenuItem } from './ContextMenu.svelte'
  import FolderSummary from './FolderSummary.svelte'
  import { resolveKey, matchAction } from './shortcuts'
  import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener'
  import * as api from './api'
  import type { Entry, Listing, SortKey } from './api'

  export let initialPath: string
  export let dragIcon = ''
  /** ペインが1枚しかない時は閉じる操作も、アクティブ表示も要らない。 */
  export let closable = false
  export let active = false
  /** 2枚以上ある状態。アクティブ表示を出すかどうかの判断に使う。 */
  export let multi = false

  /** アプリ設定。窓レベルで読み込んで配る。 */
  export let settings: api.Settings

  export let onPathChange: (path: string) => void = () => {}
  /** 設定ダイアログを開く。窓レベルで1つだけ持つので親へ委ねる。 */
  export let onOpenSettings: () => void = () => {}
  export let onNote: (message: string) => void = () => {}
  export let onSplit: (path: string) => void = () => {}
  export let onClose: () => void = () => {}
  export let onDetach: (path: string) => void = () => {}
  export let onActivate: () => void = () => {}
  export let trayItems: string[] = []
  export let onTrayToggle: (path: string) => void = () => {}
  export let onTrayClear: () => void = () => {}

  let listing: Listing | null = null
  let error: string | null = null

  /**
   * サイドバー（ツリー / お気に入り / 履歴）の表示。
   *
   * 常設すると「1画面に詰め込む」従来型に寄ってしまい、
   * 特に分割時は横幅を圧迫する。既定では出すが、いつでも畳めるようにしておく。
   */
  let showTree = settings.showSidebar
  let treeWidth = 210

  let sidebar: Sidebar | null = null
  let isFavorite = false

  /**
   * プレビューパネル（右側）。
   *
   * ツリー同様、既定はオフにする。プレビューは補助機能であり、
   * 常設すると1画面に詰め込む従来型に寄ってしまう。単一選択時のみ中身を出す
   * （複数選択で「どれのプレビューか」が紛らわしくなるのを避ける）。
   */
  let showPreview = settings.showPreview
  let previewWidth = 260

  // 幅のドラッグ調整。狭すぎ・広すぎを防ぐ。
  const TREE_MIN = 120
  const TREE_MAX = 480
  const PREVIEW_MIN = 160
  const PREVIEW_MAX = 600
  let resizing = false
  let resizingPreview = false

  function onResizeMove(ev: PointerEvent) {
    const box = paneEl?.getBoundingClientRect()
    if (!box) return
    if (resizing) {
      treeWidth = Math.min(TREE_MAX, Math.max(TREE_MIN, ev.clientX - box.left))
    }
    if (resizingPreview) {
      previewWidth = Math.min(PREVIEW_MAX, Math.max(PREVIEW_MIN, box.right - ev.clientX))
    }
  }

  let paneEl: HTMLElement | null = null

  /** 初期の並び順は設定から。以降はこのペインの操作で変わる。 */
  let sort: api.SortSpec = {
    key: settings.sortKey,
    descending: settings.sortDescending,
    dirsFirst: settings.dirsFirst,
    showHidden: settings.showHidden,
  }
  let selection: string[] = []

  /**
   * 未読み込み時に配る空配列。
   *
   * `listing?.entries ?? []` と毎回書くと描画のたびに別の配列になり、
   * 子側の「変わったら〜」という反応が延々と再発火する。定数を配る。
   */
  const NO_ENTRIES: Entry[] = []
  $: allEntries = listing?.entries ?? NO_ENTRIES

  /** フォルダ内の絞り込み。名前に対する部分一致。 */
  let filter = ''
  $: entries = filter
    ? allEntries.filter((e) => e.name.toLowerCase().includes(filter.toLowerCase()))
    : allEntries

  /** いま監視を頼んでいる場所。移る時に必ず外して二重登録を防ぐ。 */
  let watched: string | null = null

  async function rewatch(path: string) {
    if (watched === path) return
    if (watched) await api.unwatchDir(watched).catch(() => {})
    watched = path
    await api.watchDir(path).catch(() => {})
  }

  /**
   * これを超えたら記録に残す。
   * 体感で引っかかる境目で、ストリーミング化が要るかの判断材料になる。
   */
  const SLOW_LOAD_MS = 120

  export async function open(path: string) {
    try {
      const t0 = performance.now()
      listing = await api.listDir(path, sort)
      const took = Math.round(performance.now() - t0)
      if (took >= SLOW_LOAD_MS) {
        api.logUi('warn', `読み込みに ${took}ms: ${listing.entries.length}件 ${path}`)
      }
      error = null
      filter = '' // 場所が変われば絞り込みも意味を失う
      onPathChange(listing.path)
      await rewatch(listing.path)
      // アドレスバーやツリーから来た場合、焦点がそこに残っている。
      // 一覧に戻さないと、移動直後にキーボードが効かない。
      fileList?.focusList()

      // 訪れた場所を履歴に積む。ここが「さっき見てたやつ」を辿る唯一の入口。
      await api.recordHistory(listing.path)
      await syncFavoriteState()
      sidebar?.refresh()
    } catch (e) {
      error = String(e)
    }
    await refreshUndoState()
  }

  /** 表示中の場所を保ったまま引き直す。 */
  export async function reload() {
    if (!listing) return
    try {
      listing = await api.listDir(listing.path, sort)
      error = null
    } catch (e) {
      error = String(e)
    }
    // 一覧が変わりうるタイミングは、大抵ここに集約される
    // （新規作成・削除・貼り付け・転送完了・undo…）。取りこぼしなく更新する。
    await refreshUndoState()
  }

  // ---- クリップボード ----
  //
  // 中身は Rust 側に置いてあるので、窓Aで切り取って窓Bで貼る、が成立する。

  async function copySelection(cut: boolean) {
    if (!selection.length) return
    await api.setClipboard(selection, cut)
    onNote(`${cut ? '切り取り' : 'コピー'} ${selection.length}件`)
  }

  async function paste() {
    if (!listing) return
    try {
      const data = await api.getClipboard()
      if (!data.paths.length) return
      await runTransfer(data.paths, listing.path, data.cut)
      // 切り取りは一度きり。二度目は元が無いので消しておく。
      if (data.cut) await api.setClipboard([], false)
    } catch (e) {
      onNote(`貼り付け失敗: ${e}`)
      error = String(e)
    }
  }

  /**
   * 取り消せる操作の有無とラベル。
   *
   * 履歴はアプリ全体で1本（どの窓・ペインの操作も同じ列に積まれる）なので、
   * このペインで何もしていなくても、他の窓での操作で状態が変わりうる。
   * ボタンにカーソルを乗せた時など、必要な時に問い合わせて表示を合わせる。
   */
  let undoLabel = ''
  let undoAvailable = false

  async function refreshUndoState() {
    const s = await api.undoState()
    undoAvailable = s.available
    undoLabel = s.label
  }

  /**
   * 直前の操作を取り消す。
   *
   * 取り消し履歴はアプリ全体で1本（どの窓・ペインの操作も同じ列に積まれる）。
   * ここでは「取り消しを起こす」だけで、影響を受けた他のペイン・窓の表示は
   * ファイル監視の自動更新に任せる。自分のペインだけ明示的に読み直す。
   */
  async function undo() {
    try {
      const label = await api.undoLast()
      onNote(`元に戻しました: ${label}`)
      await reload()
    } catch (e) {
      onNote(`元に戻せません: ${e}`)
    } finally {
      await refreshUndoState()
    }
  }

  // ---- 進捗付きの転送 ----
  //
  // 数GBのコピーを同期で走らせると UI が固まり「落ちた」ように見える。
  // 別スレッドに投げ、進み具合を受け取って出す。

  /** 走らせている転送。null なら何も動いていない。 */
  let progress: api.ProgressEvent | null = null
  let transferId: number | null = null

  async function runTransfer(paths: string[], dest: string, moveFiles: boolean) {
    try {
      transferId = await api.startTransfer(paths, dest, moveFiles)
    } catch (e) {
      error = String(e)
    }
  }

  /** 同じ列をもう一度押したら昇順/降順を反転する。 */
  function changeSort(key: SortKey) {
    sort =
      sort.key === key
        ? { ...sort, descending: !sort.descending }
        : // 日時とサイズは「大きい方・新しい方を先に見たい」ことが多いので降順から入る。
          { ...sort, key, descending: key === 'modified' || key === 'size' }
    reload()
  }

  function toggleHidden() {
    sort = { ...sort, showHidden: !sort.showHidden }
    reload()
  }

  async function launch(_entry: Entry, path: string) {
    try {
      await openPath(path)
    } catch (e) {
      onNote(`開けません: ${e}`)
    }
  }

  async function revealSelection() {
    const target = selection[0] ?? listing?.path
    if (!target) return
    try {
      await revealItemInDir(target)
    } catch (e) {
      onNote(`エクスプローラーで開けません: ${e}`)
    }
  }

  /**
   * クリップボード（OS の方）へ文字列を入れる。
   *
   * フルパスのコピーは「他のアプリへ渡す」ための操作なので、
   * アプリ内クリップボード（ファイル用）ではなく OS のテキストクリップボードを使う。
   */
  async function copyText(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text)
      onNote(`${what}をコピー: ${text}`)
    } catch (e) {
      onNote(`コピーできません: ${e}`)
    }
  }

  function copyFullPaths() {
    const targets = selection.length ? selection : listing ? [listing.path] : []
    if (!targets.length) return
    copyText(targets.join('\r\n'), targets.length > 1 ? `${targets.length}件のパス` : 'フルパス')
  }

  function copyNames() {
    if (!selection.length) return
    const names = selection.map((p) => p.split(/[\\/]/).pop() ?? p)
    copyText(names.join('\r\n'), names.length > 1 ? `${names.length}件の名前` : '名前')
  }

  async function zipSelection() {
    if (!selection.length) return
    onNote(`ZIP を作成中… ${selection.length}件`)
    try {
      const created = await api.compressToZip(selection)
      onNote(`ZIP を作成: ${created.split(/[\\/]/).pop()}`)
      await reload()
    } catch (e) {
      error = String(e)
      onNote(`ZIP を作成できません: ${e}`)
    }
  }

  async function unzipSelection() {
    const target = selection[0]
    if (!target) return
    onNote('展開中…')
    try {
      const dest = await api.extractZip(target)
      onNote(`展開しました: ${dest.split(/[\\/]/).pop()}`)
      await reload()
    } catch (e) {
      error = String(e)
      onNote(`展開できません: ${e}`)
    }
  }

  async function newFolder() {
    if (!listing) return
    try {
      const created = await api.createFolder(listing.path, '新しいフォルダー')
      await reload()
      // 作った直後は名前を変えたいはず。そのまま入力状態にする。
      renaming = { path: created, value: created.split(/[\\/]/).pop() ?? '' }
    } catch (e) {
      error = String(e)
    }
  }

  /** リネーム中の対象。null なら非表示。 */
  let renaming: { path: string; value: string } | null = null

  function startRename(entry: Entry) {
    if (!listing) return
    renaming = { path: api.joinPath(listing.path, entry.name), value: entry.name }
  }

  async function commitRename() {
    if (!renaming) return
    const { path, value } = renaming
    renaming = null
    try {
      await api.renameEntry(path, value)
      await reload()
    } catch (e) {
      error = String(e)
    }
  }

  /**
   * ゴミ箱へ送る。
   *
   * 確認を挟むのは、選択が意図とずれていた時の最後の砦になるため。
   * 完全削除は用意していないので、間違えても OS のゴミ箱から戻せる。
   */
  async function trashSelection(paths: string[]) {
    if (!paths.length) return
    const label = paths.length === 1 ? paths[0].split(/[\\/]/).pop() : `${paths.length}件`
    // 確認は設定で切れる。切っていても undo で戻せる（ゴミ箱送りなので）。
    if (settings.confirmTrash && !confirm(`${label} をゴミ箱へ送りますか？`)) return
    try {
      const n = await api.trashEntries(paths)
      onNote(`ゴミ箱へ ${n}件`)
      await reload()
    } catch (e) {
      error = String(e)
    }
  }

  async function syncFavoriteState() {
    const favorites = await api.listFavorites()
    isFavorite = !!listing && favorites.includes(listing.path)
  }

  async function toggleFavorite() {
    if (!listing) return
    isFavorite = await api.toggleFavorite(listing.path)
    sidebar?.refresh()
  }

  /** 落とされたファイルをこのペインの場所へ取り込む。 */
  export async function acceptDrop(paths: string[]) {
    if (!listing) return
    onNote(`drop in: ${paths.length}件`)
    // 既定はコピー。移動は取り返しがつかないので明示操作に限る。
    await runTransfer(paths, listing.path, false)
  }

  export function currentPath(): string | null {
    return listing?.path ?? null
  }

  // ---- 右クリックメニュー ----

  let menuAt: { x: number; y: number } | null = null
  let menuItems: MenuItem[] = []

  /** ショートカット表記をメニューに添える。操作を覚えてもらう導線になる。 */
  const hint = (id: Parameters<typeof resolveKey>[0]) => resolveKey(id, settings.shortcuts)

  function openContextMenu(ev: MouseEvent, entry: Entry | null) {
    const n = selection.length
    const one = n === 1 ? selection[0] : null
    const isZip = one?.toLowerCase().endsWith('.zip') ?? false
    const onEntry = entry !== null

    menuItems = onEntry
      ? [
          { kind: 'item', label: entry.is_dir ? '開く' : '既定のアプリで開く', run: () => {
              if (!listing) return
              const full = api.joinPath(listing.path, entry.name)
              entry.is_dir ? open(full) : launch(entry, full)
            } },
          { kind: 'item', label: 'エクスプローラーで表示', run: revealSelection },
          { kind: 'sep' },
          { kind: 'item', label: 'コピー', hint: hint('copy'), run: () => copySelection(false) },
          { kind: 'item', label: '切り取り', hint: hint('cut'), run: () => copySelection(true) },
          { kind: 'item', label: '貼り付け', hint: hint('paste'), run: paste },
          { kind: 'sep' },
          { kind: 'item', label: 'フルパスをコピー', hint: hint('copyPath'), run: copyFullPaths },
          { kind: 'item', label: '名前をコピー', run: copyNames },
          { kind: 'sep' },
          { kind: 'item', label: 'ZIP に圧縮', hint: hint('zip'), run: zipSelection },
          // ZIP を選んでいる時だけ出す。常に出して無効化するより一覧が短くなる。
          ...(isZip
            ? ([{ kind: 'item', label: 'ここに展開', run: unzipSelection }] as MenuItem[])
            : []),
          { kind: 'sep' },
          { kind: 'item', label: '名前を変更', hint: hint('rename'), disabled: n !== 1, run: () => startRename(entry) },
          { kind: 'item', label: 'ゴミ箱へ送る', hint: hint('trash'), danger: true, run: () => trashSelection(selection) },
        ]
      : [
          // 空き領域＝「この場所」に対する操作。
          { kind: 'item', label: '新しいフォルダー', hint: hint('newFolder'), run: newFolder },
          { kind: 'item', label: '貼り付け', hint: hint('paste'), run: paste },
          { kind: 'sep' },
          { kind: 'item', label: 'このフォルダのパスをコピー', hint: hint('copyPath'), run: copyFullPaths },
          { kind: 'item', label: 'エクスプローラーで表示', run: revealSelection },
          { kind: 'sep' },
          { kind: 'item', label: '再読み込み', hint: hint('reload'), run: reload },
          { kind: 'item', label: sort.showHidden ? '隠しファイルを隠す' : '隠しファイルを表示', run: toggleHidden },
          { kind: 'sep' },
          { kind: 'item', label: '設定…', hint: hint('settings'), run: onOpenSettings },
        ]

    menuAt = { x: ev.clientX, y: ev.clientY }
  }

  let pathBar: PathBar | null = null
  let filterInput: HTMLInputElement | null = null
  let fileList: FileList | null = null

  /**
   * ペイン全体のショートカット。
   *
   * 一覧の中の移動は FileList が持ち、ここは「場所を変える・物を動かす」系を扱う。
   * このペインが操作対象でない時は何もしない。複数ペインで同時に反応すると事故になる。
   */
  function onPaneKey(ev: KeyboardEvent) {
    if (multi && !active) return

    // 入力欄で打っている最中はショートカットを奪わない。
    const el = ev.target as HTMLElement | null
    if (el && (el.tagName === 'INPUT' || el.isContentEditable)) return

    // 割り当ては設定から引く。既定と設定の二重管理を避けるため、
    // ここでキーを直接書かない（shortcuts.ts が唯一の定義元）。
    const action = matchAction(ev, settings.shortcuts)
    if (!action) return

    switch (action) {
      case 'address':
        ev.preventDefault()
        pathBar?.beginEdit()
        break
      case 'filter':
        ev.preventDefault()
        filterInput?.focus()
        break
      case 'copy':
        ev.preventDefault()
        copySelection(false)
        break
      case 'cut':
        ev.preventDefault()
        copySelection(true)
        break
      case 'paste':
        ev.preventDefault()
        paste()
        break
      case 'undo':
        ev.preventDefault()
        undo()
        break
      case 'reload':
        ev.preventDefault()
        reload()
        break
      case 'newFolder':
        ev.preventDefault()
        newFolder()
        break
      case 'zip':
        ev.preventDefault()
        zipSelection()
        break
      case 'copyPath':
        ev.preventDefault()
        copyFullPaths()
        break
      case 'settings':
        ev.preventDefault()
        onOpenSettings()
        break
      // 一覧の中の操作（↑↓/Enter/F2/Delete/Ctrl+A）は FileList 側が持つ。
      // タブ操作は窓レベル（Filer.svelte）が持つ。
      default:
        break
    }
  }

  let unlistenFs: UnlistenFn | null = null
  let unlistenProgress: UnlistenFn | null = null
  let unlistenDone: UnlistenFn | null = null
  let reloadTimer: number | null = null

  onMount(async () => {
    await open(initialPath)

    // 転送イベントは窓全体に飛ぶので、自分が始めたものだけ拾う。
    unlistenProgress = await listen<api.ProgressEvent>(api.TRANSFER_PROGRESS, (ev) => {
      if (ev.payload.id !== transferId) return
      progress = ev.payload
    })

    unlistenDone = await listen<api.DoneEvent>(api.TRANSFER_DONE, async (ev) => {
      if (ev.payload.id !== transferId) return
      const { cancelled, created, error: err } = ev.payload
      progress = null
      transferId = null

      if (err) {
        error = err
        onNote(`転送に失敗: ${err}`)
      } else if (cancelled) {
        onNote('転送を中断しました')
      } else {
        onNote(`転送 ${created}件`)
      }
      await reload()
    })

    // 外で作られたファイルが見えないままだと、ファイラとして信用できない。
    // 変更通知は連続して飛んでくるので、少し溜めてから1回だけ読み直す。
    unlistenFs = await listen<string>(api.FS_CHANGED, (ev) => {
      if (ev.payload !== listing?.path) return
      if (reloadTimer !== null) clearTimeout(reloadTimer)
      reloadTimer = window.setTimeout(() => {
        reloadTimer = null
        reload()
      }, 250)
    })
  })

  onDestroy(() => {
    unlistenFs?.()
    unlistenProgress?.()
    unlistenDone?.()
    if (reloadTimer !== null) clearTimeout(reloadTimer)
    // 見張りを残したままペインを閉じると、監視が積み上がっていく。
    if (watched) api.unwatchDir(watched).catch(() => {})
  })
</script>

<!-- ペインのショートカットは窓全体で受け、操作対象のペインだけが反応する。
     ペイン要素に付けると、一覧やサイドバーに焦点がある時に届かない。 -->
<svelte:window on:keydown={onPaneKey} />

<section
  class="pane"
  class:active
  class:multi
  bind:this={paneEl}
  on:pointerdown={onActivate}
  on:pointermove={onResizeMove}
  on:pointerup={() => {
    resizing = false
    resizingPreview = false
  }}
  on:pointerleave={() => {
    resizing = false
    resizingPreview = false
  }}
>
  <PathBar
    bind:this={pathBar}
    path={listing?.path ?? '…'}
    onNavigate={open}
    showHidden={sort.showHidden}
  >
    <div class="actions">
      <button
        type="button"
        title={isFavorite ? 'お気に入りから外す' : 'お気に入りに登録'}
        class:star={isFavorite}
        on:click={toggleFavorite}
      >
        {isFavorite ? '★' : '☆'}
      </button>
      <button
        type="button"
        title={showTree ? 'サイドバーを隠す' : 'サイドバーを出す'}
        class:on={showTree}
        on:click={() => (showTree = !showTree)}
      >
        ☰
      </button>
      <button
        type="button"
        title={showPreview ? 'プレビューを隠す' : 'プレビューを出す'}
        class:on={showPreview}
        on:click={() => (showPreview = !showPreview)}
      >
        ◐
      </button>
      <button
        type="button"
        title={undoAvailable ? `元に戻す: ${undoLabel} (Ctrl+Z)` : '元に戻せる操作はありません'}
        disabled={!undoAvailable}
        on:pointerenter={refreshUndoState}
        on:click={undo}>↩</button
      >
      <button type="button" title="新しいフォルダー" on:click={newFolder}>＋</button>
      <button
        type="button"
        title="選択をゴミ箱へ"
        disabled={selection.length === 0}
        on:click={() => trashSelection(selection)}>🗑</button
      >
      <button type="button" title="エクスプローラーで表示" on:click={revealSelection}>⧉</button>
      <button
        type="button"
        title={sort.showHidden ? '隠しファイルを隠す' : '隠しファイルを表示'}
        class:on={sort.showHidden}
        on:click={toggleHidden}>◌</button
      >
      <button type="button" title="このペインを左右に分割" on:click={() => listing && onSplit(listing.path)}>
        ⫿
      </button>
      <button type="button" title="独立した窓として切り離す" on:click={() => listing && onDetach(listing.path)}>
        ⇱
      </button>
      {#if closable}
        <button type="button" title="このペインを閉じる" on:click={onClose}>✕</button>
      {/if}
    </div>
  </PathBar>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  <div class="body">
    {#if showTree}
      <div class="tree-slot" style="width: {treeWidth}px">
        <Sidebar
          bind:this={sidebar}
          currentPath={listing?.path ?? ''}
          onNavigate={open}
          showHidden={sort.showHidden}
          {trayItems}
          onTrayRemove={onTrayToggle}
          {onTrayClear}
        />
      </div>
      <!-- 幅の調整つまみ。掴んでいる間だけ pointermove を効かせる。 -->
      <div
        class="resizer"
        class:dragging={resizing}
        role="separator"
        aria-orientation="vertical"
        aria-label="ツリーの幅"
        tabindex="-1"
        on:pointerdown|stopPropagation={() => (resizing = true)}
      />
    {/if}

    <div class="list-slot">
      <div class="filter">
        <input
          bind:this={filterInput}
          bind:value={filter}
          placeholder="このフォルダ内を絞り込み (Ctrl+F)"
          spellcheck="false"
          aria-label="絞り込み"
          on:keydown={(e) => {
            if (e.key === 'Escape') {
              filter = ''
              filterInput?.blur()
            }
          }}
        />
        {#if filter}
          <span class="hits">{entries.length} / {allEntries.length}</span>
          <button type="button" title="絞り込みを解除" on:click={() => (filter = '')}>✕</button>
        {/if}
      </div>

      <FileList
        bind:this={fileList}
        {entries}
        parent={listing?.parent ?? null}
        path={listing?.path ?? ''}
        {dragIcon}
        {sort}
        onOpen={open}
        onLaunch={launch}
        onSort={changeSort}
        onSelectionChange={(paths) => (selection = paths)}
        onDelete={trashSelection}
        onRename={startRename}
        onContext={openContextMenu}
        {trayItems}
        {onTrayToggle}
        {onNote}
      />

      <!-- 一覧の下の空きを情報で埋める。ファイルが少ないほど余白が大きくなるが、
           そこは本来「このフォルダが何なのか」を掴むのに使える面積。 -->
      {#if listing}
        <FolderSummary entries={allEntries} />
      {/if}

      {#if renaming}
        <!-- 一覧の上に重ねる。行の位置に合わせるより、迷いようのない場所に出す。 -->
        <div class="rename">
          <label for="rename-input">新しい名前</label>
          <!-- svelte-ignore a11y-autofocus -->
          <input
            id="rename-input"
            autofocus
            bind:value={renaming.value}
            on:keydown={(e) => {
              if (e.key === 'Enter') commitRename()
              if (e.key === 'Escape') renaming = null
            }}
          />
          <button type="button" on:click={commitRename}>変更</button>
          <button type="button" on:click={() => (renaming = null)}>やめる</button>
        </div>
      {/if}
    </div>

    {#if showPreview}
      <!-- 幅の調整つまみ。左側（ツリー）と同じ作法。 -->
      <div
        class="resizer"
        class:dragging={resizingPreview}
        role="separator"
        aria-orientation="vertical"
        aria-label="プレビューの幅"
        tabindex="-1"
        on:pointerdown|stopPropagation={() => (resizingPreview = true)}
      />
      <div class="preview-slot" style="width: {previewWidth}px">
        <Preview path={selection.length === 1 ? selection[0] : null} />
      </div>
    {/if}
  </div>

  <TransferBar {progress} />

  <div class="count">
    {listing?.entries.length ?? 0} 件{#if selection.length}<span class="sel"
        >／{selection.length} 選択</span
      >{/if}
  </div>
</section>

<ContextMenu items={menuItems} at={menuAt} onClose={() => (menuAt = null)} />

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    flex: 1;
    background: #1b1b1b;
  }

  /* 複数ペインの時、どれが操作対象かを分からせる。
     落としたファイルがどこへ入るかを決める情報なので、一目で分かる差を付ける。
     非アクティブ側を沈ませることで、アクティブ側が自然に浮き上がる。 */
  .pane.multi:not(.active) {
    background: #161616;
  }
  .pane.multi:not(.active) :global(header) {
    background: #1a1a1a;
  }
  .pane.multi:not(.active) :global(.tail) {
    color: #8a8a8a;
  }
  .pane.multi:not(.active) :global(.band) {
    filter: saturate(0.3) brightness(0.6);
  }

  /* アクティブ側は上端に明示的な線を引く。背景差だけだと環境によっては潰れる。 */
  .pane.multi.active {
    box-shadow: inset 0 2px 0 0 #4c9aff;
  }

  .actions {
    display: flex;
    gap: 4px;
    margin: 10px 8px 0 0;
  }
  .actions button {
    width: 24px;
    height: 22px;
    padding: 0;
    background: #2f2f2f;
    border: 1px solid #454545;
    border-radius: 4px;
    color: #aaa;
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
  }
  .actions button:hover {
    background: #3a3a3a;
    color: #fff;
  }
  /* サイドバーが出ている状態を分からせる。 */
  .actions button.on {
    background: #35506f;
    border-color: #4c9aff;
    color: #fff;
  }
  /* 登録済みは一目で分かるように。 */
  .actions button.star {
    color: #ffc94d;
    border-color: #6a5a2a;
  }

  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .tree-slot {
    flex: none;
    min-width: 0;
    border-right: 1px solid #2c2c2c;
  }

  .resizer {
    width: 4px;
    flex: none;
    margin-left: -2px;
    cursor: col-resize;
    background: transparent;
    z-index: 1;
  }
  .resizer:hover,
  .resizer.dragging {
    background: #4c9aff;
  }

  .preview-slot {
    flex: none;
    min-width: 0;
    border-left: 1px solid #2c2c2c;
  }

  .count {
    flex: none;
    padding: 4px 14px;
    border-top: 1px solid #2c2c2c;
    font-size: 10px;
    color: #666;
  }
  .sel {
    color: #7fb0e8;
  }

  .list-slot {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
  }

  .filter {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
    padding: 4px 8px;
    border-bottom: 1px solid #2c2c2c;
    background: #1d1d1d;
  }
  .filter input {
    flex: 1;
    min-width: 0;
    padding: 3px 7px;
    background: #262626;
    border: 1px solid #383838;
    border-radius: 4px;
    color: #ddd;
    font: inherit;
    font-size: 11px;
    outline: none;
  }
  .filter input:focus {
    border-color: #4c9aff;
  }
  .filter .hits {
    flex: none;
    font-size: 10px;
    color: #7fb0e8;
    font-variant-numeric: tabular-nums;
  }
  .filter button {
    flex: none;
    width: 18px;
    height: 18px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 3px;
    color: #888;
    font-size: 10px;
    cursor: pointer;
  }
  .filter button:hover {
    background: #333;
    color: #fff;
  }

  .rename {
    position: absolute;
    top: 34px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    background: #2a2a2a;
    border: 1px solid #4c9aff;
    border-radius: 6px;
    box-shadow: 0 6px 20px #0009;
    z-index: 2;
  }
  .rename label {
    font-size: 10.5px;
    color: #999;
    white-space: nowrap;
  }
  .rename input {
    width: 220px;
    padding: 4px 7px;
    background: #1e1e1e;
    border: 1px solid #444;
    border-radius: 4px;
    color: #eee;
    font: inherit;
    font-size: 12px;
    outline: none;
  }
  .rename input:focus {
    border-color: #4c9aff;
  }
  .rename button {
    padding: 4px 9px;
    background: #333;
    border: 1px solid #4a4a4a;
    border-radius: 4px;
    color: #ccc;
    font-size: 11px;
    cursor: pointer;
  }
  .rename button:hover {
    background: #3d3d3d;
    color: #fff;
  }

  .actions button:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .error {
    margin: 0;
    flex: none;
    padding: 8px 16px;
    background: #3a1d1d;
    border-left: 3px solid #e05252;
    font-family: Consolas, monospace;
    font-size: 12px;
    color: #ff9b9b;
  }
</style>
