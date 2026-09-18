<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import type { UnlistenFn } from '@tauri-apps/api/event'
  import PathBar from './PathBar.svelte'
  import FileList from './FileList.svelte'
  import Sidebar from './Sidebar.svelte'
  import TransferBar from './TransferBar.svelte'
  import Preview from './Preview.svelte'
  import ContextMenu from './ContextMenu.svelte'
  import type { MenuItem } from './ContextMenu.svelte'
  import ConflictDialog from './ConflictDialog.svelte'
  import BulkRenameDialog from './BulkRenameDialog.svelte'
  import { createConflictPrompt, type ConflictRequest } from './conflicts'
  import FolderSummary from './FolderSummary.svelte'
  import { resolveKey, matchAction } from './shortcuts'
  import { relativeTo } from './layouts'
  import { splitPath } from './api'
  import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener'
  import * as api from './api'
  import * as prefetch from './prefetch'
  import * as navstats from './navstats'
  import type { Entry, Listing, SortKey } from './api'

  export let initialPath: string
  export let dragIcon = ''
  /** ペインが1枚しかない時は閉じる操作も、アクティブ表示も要らない。 */
  export let closable = false
  export let active = false
  /** 2枚以上ある状態。アクティブ表示を出すかどうかの判断に使う。 */
  export let multi = false
  /** 左手単独キーの作用先。ホバー中ペインを優先し、無ければアクティブペイン。 */
  export let keyboardTarget = false

  /** アプリ設定。窓レベルで読み込んで配る。 */
  export let settings: api.Settings

  export let onPathChange: (path: string) => void = () => {}
  /** 設定ダイアログを開く。窓レベルで1つだけ持つので親へ委ねる。 */
  export let onOpenSettings: () => void = () => {}
  /** 配置（ペインの並びの型）を開く。タブ全体の話なので親が持つ。 */
  export let onOpenLayouts: () => void = () => {}
  /**
   * すべてのペインを同じ並びにする。ペインを跨ぐ話なので親へ委ねる。
   * 向きまで渡すのは、各ペインが自分の向きへ反転すると揃わないため。
   */
  export let onSortAll: (key: SortKey, descending: boolean) => void = () => {}
  export let onNote: (message: string) => void = () => {}
  export let onSplit: (path: string) => void = () => {}
  /** 隣に検索ペインを足す。query を渡すと、その語を入れた状態で開く。 */
  export let onSplitSearch: (path: string, query?: string) => void = () => {}
  export let onClose: () => void = () => {}
  export let onDetach: (path: string) => void = () => {}
  /**
   * 転送先として固定しているか。固定中は場所を変えず、移動しようとした先は
   * `onPinnedNavigate` で別のペインに開いてもらう（探索側だけを動かす使い方のため）。
   */
  export let pinned = false
  export let onTogglePin: () => void = () => {}
  export let onPinnedNavigate: (path: string) => void = () => {}
  export let onKindChange: (kind: api.PaneKind) => void = () => {}
  export let onActivate: () => void = () => {}
  export let onHoverChange: (hovered: boolean) => void = () => {}
  export let trayItems: string[] = []
  export let onTrayToggle: (path: string) => void = () => {}
  export let onTrayClear: () => void = () => {}
  export let onTrayRemoveMany: (paths: string[]) => void = () => {}
  /** タブの全トレイと、選んでいるトレイ。切り替え・追加・名前変更・削除はタブ側が持つ。 */
  export let trays: api.TraySummary[] = []
  export let activeTrayId = 0
  export let onTraySelect: (id: number) => void = () => {}
  export let onTrayAdd: () => void = () => {}
  export let onTrayRename: (id: number) => void = () => {}
  export let onTrayDelete: (id: number) => void = () => {}
  /** セッションから復元する、場所ごとの作業状態。新しいものが先頭。 */
  export let savedPathStates: api.SavedPathState[] = []
  export let sidebarState: api.SavedSidebarState = { primary: 'tree' }
  export let onSidebarChange: (state: api.SavedSidebarState) => void = () => {}

  let listing: Listing | null = null
  let error: string | null = null

  // ---- 場所ごとの作業状態 ----
  //
  // ペインの「現在地」は1つしかないので、移動すると前の場所のスクロール位置・
  // 選択・絞り込み・並べ替えが消える。これが「親へ戻るのも、無関係な場所へ跳ぶのも
  // 同じコスト」の正体なので、場所をキーにして覚えておき、戻った時に作業の続きから
  // 再開できるようにする。
  //
  // Map は挿入順を保つので、これだけで LRU になる（触るたびに delete → set）。
  const PATH_STATE_CAP = 32
  const pathStates = new Map<string, api.SavedPathState>(
    savedPathStates.map((st) => [api.pathIdentity(st.path), st] as const).reverse()
  )

  /** 離れる直前の場所の見え方をしまう。 */
  function stashPathState() {
    if (!listing || !fileList) return
    const key = api.pathIdentity(listing.path)
    const view = fileList.captureView()
    pathStates.delete(key)
    pathStates.set(key, {
      path: listing.path,
      scrollTop: view.scrollTop,
      selected: view.selected,
      cursor: view.cursor,
      filter,
      sortKey: sort.key,
      sortDescending: sort.descending,
    })
    // 上限を超えたら最も古いものから捨てる。
    while (pathStates.size > PATH_STATE_CAP) {
      const oldest = pathStates.keys().next().value
      if (oldest === undefined) break
      pathStates.delete(oldest)
    }
  }

  /** セッション保存用。新しいものが先頭になるよう逆順で渡す。 */
  export function capturePathStates(): api.SavedPathState[] {
    stashPathState()
    return [...pathStates.values()].reverse()
  }

  // ---- 進む（分岐の記憶） ----
  //
  // ブラウザの「進む」は一本道だが、フォルダは木なので「A・B・C を見て回り、
  // B に戻ってから C へ」という動きになる。どの場所からどこへ潜っていたかを
  // 場所ごとに覚えておけば、そこへの再突入が1操作で済む。
  const forwardMemory = new Map<string, string>()
  /** 1つ前にいた場所と、いまの場所へ着いた時刻。出戻りの速さを測るのに使う。 */
  let previousPath: string | null = null
  let arrivedAt: number | null = null
  /** いまの場所から、直前に潜っていた子。無ければ null。 */
  let forwardTo: string | null = null

  $: forwardLabel = forwardTo ? splitPath(forwardTo).tail || forwardTo : ''

  function rememberBranch(from: string | null, to: string) {
    if (!from) return
    // 上がった時だけ覚える。潜った先は今まさに見ているので覚える意味がない。
    if (relativeTo(to, from)) forwardMemory.set(api.pathIdentity(to), from)
  }

  /**
   * 絞り込みを、そのまま下位フォルダの検索へ引き継ぐ。
   *
   * 絞り込みは1階層しか見ないので、「この辺にあるはず」が外れると手が止まる。
   * ここで検索ペインへ渡せば、階層を降りる作業そのものを検索で置き換えられる。
   * 検索の土台（ルート指定・条件式）は既にあるので、繋ぐだけで済む。
   */
  function promoteFilterToSearch() {
    if (!listing || !filter.trim()) return
    onSplitSearch(listing.path, filter)
  }

  /**
   * 絞り込み語を復元した時に立てる印。
   *
   * 語だけ黙って戻すと「ファイルが消えている」と誤解する。復元したことが
   * 分かるように入力欄を強調し、一度でも触ったら印は下ろす。
   */
  let filterRestored = false

  /**
   * サイドバー（ツリー / お気に入り / 履歴）の表示。
   *
   * 常設すると「1画面に詰め込む」従来型に寄ってしまい、
   * 特に分割時は横幅を圧迫する。既定では出すが、いつでも畳めるようにしておく。
   */
  let showTree = settings.showSidebar
  let treeWidth = 210
  let sidebarPrimary = sidebarState.primary ?? 'tree'
  let sidebarSecondary: api.SidebarTab | null = sidebarState.secondary ?? null
  let sidebarSplitRatio = sidebarState.splitRatio ?? 0.55
  let sidebarResizing = false
  let treeSlot: HTMLElement | null = null

  function sidebarChanged() {
    onSidebarChange({
      primary: sidebarPrimary,
      secondary: sidebarSecondary ?? undefined,
      splitRatio: sidebarSecondary ? sidebarSplitRatio : undefined,
    })
  }

  function splitSidebar() {
    sidebarSecondary = sidebarPrimary === 'history' ? 'favorites' : 'history'
    sidebarChanged()
  }

  function closeSidebarSection(which: 'primary' | 'secondary') {
    if (which === 'primary' && sidebarSecondary) sidebarPrimary = sidebarSecondary
    sidebarSecondary = null
    sidebarChanged()
  }

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

  /**
   * プレビューの対象。単一選択のみ扱う（複数だと「どれのことか」が紛らわしい）。
   *
   * フォルダも対象にする。「入ってみたが違ったので戻る」という一番多い往復を、
   * 移動せずに済ませるため。
   */
  $: previewTarget = selection.length === 1 ? selection[0] : null
  $: previewIsDir = previewTarget
    ? (allEntries.find(
        (e) =>
          api.pathIdentity(api.joinPath(listing?.path ?? '', e.name)) ===
          api.pathIdentity(previewTarget!)
      )?.is_dir ?? false)
    : false

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
    if (sidebarResizing && treeSlot) {
      const side = treeSlot.getBoundingClientRect()
      sidebarSplitRatio = Math.min(0.8, Math.max(0.2, (ev.clientY - side.top) / side.height))
      sidebarChanged()
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

  /**
   * フォルダ内の絞り込み。名前に対する部分一致。
   * 大文字小文字・全角半角・半角カナは区別しない（検索ペインと同じ規則）。
   */
  let filter = ''
  $: filterKey = api.foldForSearch(filter)
  /**
   * 畳んだ名前を一覧ごとに1度だけ作る。
   *
   * 打鍵のたびに全件を畳み直すと、3万件のフォルダで1打鍵40〜55msかかり、
   * 打つほど遅れが積もる。畳むのは一覧が変わった時だけでよい。
   *
   * 絞り込みが空の間は作らない。ほとんどの表示では一度も絞り込まないので、
   * 先に作ると読み込みのたびに使われない計算を足すことになる。
   */
  let foldedFor: api.Entry[] | null = null
  let foldedNames: string[] = []
  $: if (filterKey && foldedFor !== allEntries) {
    foldedFor = allEntries
    foldedNames = allEntries.map((e) => api.foldForSearch(e.name))
  }
  $: entries = filterKey
    ? allEntries.filter((_, i) => foldedNames[i]?.includes(filterKey))
    : allEntries

  /** いま監視を頼んでいる場所。移る時に必ず外して二重登録を防ぐ。 */
  let watched: string | null = null

  async function rewatch(path: string) {
    if (watched === path) return
    // 見張り先は待つ前に確定させる。await の間にもう一度呼ばれると、
    // どちらも同じ場所を外しに行き（他のペインの監視まで止まる）、
    // その間に張った監視は誰も外さないまま残る。
    const previous = watched
    watched = path
    if (previous) await api.unwatchDir(previous).catch(() => {})
    await api.watchDir(path).catch(() => {})
  }

  /**
   * これを超えたら記録に残す。
   * 体感で引っかかる境目で、ストリーミング化が要るかの判断材料になる。
   */
  const SLOW_LOAD_MS = 120

  /**
   * この場所を開く。
   *
   * `counted: false` は移動の集計から外す。★に入れたファイルの「含まれる場所へ」の
   * ように、この機能を足したこと自体が生んだ跳躍を数えると、`other` の割合で
   * 俯瞰画面の要否を決める計測（Settings → 移動の集計）が自分の足で歪む。
   */
  /**
   * 移動の世代。古い読み込みの結果を捨てるために持つ。
   *
   * ディスクを読むコマンドはメインスレッドの外で走るので、**先に始めた読み込みが
   * 後から終わることがある**。大きいフォルダを開いてすぐ小さいフォルダへ移ると、
   * 小さい方が先に描かれ、その後から大きい方が到着して画面を奪う。
   *
   * 上書きされるのは一覧だけではない。履歴・監視対象・「戻る」先・移動の集計まで
   * 別の場所のもので埋まるので、見た目が直っても中身が食い違ったままになる。
   */
  let openGeneration = 0

  export async function open(path: string, { counted = true }: { counted?: boolean } = {}) {
    // パスバー・一覧・ツリー・履歴・Q キーはすべてここを通るので、固定の判定はここだけでよい。
    // 表示前（起動直後）は固定していても最初の場所を開く。
    if (pinned && listing && api.pathIdentity(path) !== api.pathIdentity(listing.path)) {
      onPinnedNavigate(path)
      return
    }
    // 離れる前に、いまの場所の見え方をしまう。戻ってきた時にここから再開する。
    stashPathState()

    const previous = listing?.path ?? null
    const leftAt = arrivedAt
    const before = previousPath

    // 並べ替えは一覧を引く前に決める必要がある。前回この場所を見た時の並びで引く。
    const remembered = pathStates.get(api.pathIdentity(path))
    if (remembered) {
      sort = { ...sort, key: remembered.sortKey, descending: remembered.sortDescending }
    }

    const generation = ++openGeneration
    try {
      const t0 = performance.now()
      // 直前まで見ていた場所へ戻る時は控えが効く。往復の体感を消すのが狙い。
      const fetched = await prefetch.listDir(path, sort)
      // この間に次の移動が始まっていたら、ここから先は何も触らない。
      if (generation !== openGeneration) return
      listing = fetched
      const took = Math.round(performance.now() - t0)
      if (took >= SLOW_LOAD_MS) {
        api.logUi('warn', `読み込みに ${took}ms: ${listing.entries.length}件 ${path}`)
      }
      error = null
      // 覚えていればその場所での絞り込みを戻し、初めての場所なら空にする。
      filter = remembered?.filter ?? ''
      filterRestored = !!filter

      // 記録は移動が成立してから。読めずに終わった場所を「行った」と数えない。
      rememberBranch(previous, listing.path)
      // どの種別の移動が多いかを数える。どこへ投資すべきかを推測ではなく実データで決めるため。
      if (counted) navstats.track(previous, listing.path, before, leftAt)

      forwardTo = forwardMemory.get(api.pathIdentity(listing.path)) ?? null
      previousPath = previous
      arrivedAt = Date.now()
      onPathChange(listing.path)
      await rewatch(listing.path)
      // アドレスバーやツリーから来た場合、焦点がそこに残っている。
      // 一覧に戻さないと、移動直後にキーボードが効かない。
      fileList?.focusList()

      // 一覧が新しい場所で描き終わってから戻す。FileList は path が変わった時点で
      // スクロールと選択を捨てるので、その後でなければ上書きされる。
      if (remembered) {
        await tick()
        await fileList?.restoreView({
          scrollTop: remembered.scrollTop,
          selected: remembered.selected,
          cursor: remembered.cursor,
        })
      }

      // 見え方を戻す間にも移動は挟まる。履歴と監視は取り消しの効かない副作用なので、
      // ここでもう一度確かめてから進む。
      if (generation !== openGeneration) return

      // 訪れた場所を履歴に積む。ここが「さっき見てたやつ」を辿る唯一の入口。
      await api.recordHistory(listing.path)
      await syncFavoriteState()
      sidebar?.refresh()

      // 親と、見えている子フォルダを裏で読んでおく。次の一手はほぼこのどれか。
      // ここ自体が遅かった場所（ネットワークドライブなど）では対象を絞る。
      prefetch.warm(listing, sort, took)
    } catch (e) {
      // 古い読み込みの失敗で、いま見えている場所にエラーを出さない。
      if (generation !== openGeneration) return
      error = String(e)
    }
    await refreshUndoState()
  }

  /** 表示中の場所を保ったまま引き直す。 */
  export async function reload() {
    if (!listing) return
    // 引き直しは今いる場所が前提なので、世代は進めず確かめるだけにする。進めると、
    // 監視通知で始まった引き直しが、利用者の移動を追い越して古い場所を描いてしまう。
    const generation = openGeneration
    try {
      // 引き直しが目的なので控えは使わない。読めた内容で控えを差し替える。
      prefetch.invalidate(listing.path)
      const fetched = await api.listDir(listing.path, sort)
      if (generation !== openGeneration) return
      listing = fetched
      prefetch.remember(listing, sort)
      error = null
    } catch (e) {
      if (generation !== openGeneration) return
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
      const started = await runTransfer(data.paths, listing.path, data.cut)
      // 切り取りは一度きり。二度目は元が無いので消しておく。
      // 衝突の確認で取りやめた場合は、まだ何も動いていないので残す。
      if (data.cut && started) await api.setClipboard([], false)
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
  let trayTransfer: { moveFiles: boolean } | null = null

  /** 衝突の確認ダイアログ。転送先に同名があれば選んでもらう。 */
  let conflictRequest: ConflictRequest | null = null
  const conflicts = createConflictPrompt((request) => (conflictRequest = request))

  /**
   * 転送の順番待ち。
   *
   * 以前は転送中に次を始めると、動いている転送の ID を上書きして進捗も完了も見失っていた。
   * 衝突の確認は依頼した時点で済ませ、選んだ扱いを持ったまま順に流す。
   */
  type TransferJob = { paths: string[]; dest: string; moveFiles: boolean; conflict: api.ConflictPolicy; fromTray: boolean }
  let queue: TransferJob[] = []
  /** 開始を依頼して ID が返るまでの間。完了がこの間に届くことがある（小さな転送）。 */
  let startingTransfer = false
  let earlyProgress: api.ProgressEvent | null = null
  let earlyDone: api.DoneEvent | null = null

  $: transferBusy = transferId !== null || startingTransfer

  /** 戻り値は転送を始めた（または順番待ちに入れた）か。衝突の確認で取りやめた場合は false。 */
  async function runTransfer(paths: string[], dest: string, moveFiles: boolean, fromTray = false): Promise<boolean> {
    if (conflicts.open) return false
    try {
      const conflict = await conflicts.ask(paths, dest, moveFiles)
      if (!conflict) {
        onNote(`${moveFiles ? '移動' : 'コピー'}を取りやめました`)
        return false
      }
      const job: TransferJob = { paths, dest, moveFiles, conflict, fromTray }
      if (transferBusy) {
        queue = [...queue, job]
        onNote(`順番待ちに追加しました（${queue.length}件目）`)
        return true
      }
      await startJob(job)
      return true
    } catch (e) {
      error = String(e)
      return false
    }
  }

  async function startJob(job: TransferJob) {
    startingTransfer = true
    earlyProgress = null
    earlyDone = null
    trayTransfer = job.fromTray ? { moveFiles: job.moveFiles } : null
    try {
      transferId = await api.startTransfer(job.paths, job.dest, job.moveFiles, job.conflict)
    } catch (e) {
      trayTransfer = null
      error = String(e)
      onNote(`転送を開始できません: ${e}`)
      startingTransfer = false
      await startNextJob()
      return
    }
    startingTransfer = false
    // await の間にイベントの受け口が書き換えるので、ここで読み直す
    // （関数の先頭で null を入れたままだと型の上では null に絞り込まれてしまう）。
    const bufferedProgress = earlyProgress as api.ProgressEvent | null
    const bufferedDone = earlyDone as api.DoneEvent | null
    earlyProgress = null
    earlyDone = null
    if (bufferedProgress && bufferedProgress.id === transferId) progress = bufferedProgress
    if (bufferedDone && bufferedDone.id === transferId) await finishTransfer(bufferedDone)
  }

  async function startNextJob() {
    const [next, ...rest] = queue
    if (!next) return
    queue = rest
    await startJob(next)
  }

  function clearQueue() {
    const count = queue.length
    queue = []
    if (count) onNote(`順番待ちの ${count}件 を取り消しました`)
  }

  async function finishTransfer(payload: api.DoneEvent) {
    const { cancelled, created, completedSources, linksSkipped, error: err } = payload
    const completedTrayMove = trayTransfer?.moveFiles ? completedSources : []
    progress = null
    transferId = null
    trayTransfer = null

    if (completedTrayMove.length) onTrayRemoveMany(completedTrayMove)

    if (err) {
      error = err
      onNote(`転送に失敗: ${err}`)
    } else if (cancelled) {
      onNote('転送を中断しました')
    } else {
      // 飛ばしたリンクを黙って落とすと「コピーしたはずのものが無い」になる。
      const skipped = linksSkipped
        ? `（リンク ${linksSkipped}件は中へ降りずに飛ばしました）`
        : ''
      onNote(`転送 ${created}件${skipped}`)
    }
    await reload()
    await startNextJob()
  }

  async function transferTray(paths: string[], moveFiles: boolean) {
    if (!listing || !paths.length) return
    await runTransfer(paths, listing.path, moveFiles, true)
  }

  /**
   * その列を押した時に行き着く並び。
   *
   * 同じ列をもう一度なら昇順/降順を反転。別の列なら、その列で普通に見たい向きから入る。
   * 「全ペインを揃える」側もここを通す。揃えるのに各ペインが自分の向きへ反転してしまうと、
   * 押した結果が揃わない。
   */
  function nextSort(key: SortKey): { key: SortKey; descending: boolean } {
    if (sort.key === key) return { key, descending: !sort.descending }
    // 日時とサイズは「大きい方・新しい方を先に見たい」ことが多いので降順から入る。
    return { key, descending: key === 'modified' || key === 'size' }
  }

  /** 同じ列をもう一度押したら昇順/降順を反転する。 */
  function changeSort(key: SortKey) {
    const next = nextSort(key)
    sort = { ...sort, key: next.key, descending: next.descending }
    reload()
  }

  /**
   * 指定された並びをそのまま当てる。全ペインを揃える時に外から呼ぶ。
   * 既に同じ並びなら何もしない（揃える操作で全ペインを読み直すのは無駄）。
   */
  export function applySort(key: SortKey, descending: boolean) {
    if (sort.key === key && sort.descending === descending) return
    sort = { ...sort, key, descending }
    reload()
  }

  /** 軸は変えずに向きだけ返す。いま何で並んでいるかを思い出さずに押せる。 */
  function reverseSort() {
    sort = { ...sort, descending: !sort.descending }
    reload()
  }

  /**
   * その場展開をまとめて開く／畳む。
   *
   * 打ち切った時は黙らない。開かなかったフォルダが「中身が無い」ように見える。
   */
  async function expandAll() {
    const result = await fileList?.toggleExpandAll()
    if (!result) return
    if (result.action === 'collapsed') {
      onNote(`展開していた ${result.count} 件を畳みました`)
    } else if (result.count < result.total) {
      onNote(`${result.total} 件中 先頭 ${result.count} 件を開きました`)
    }
  }

  function toggleHidden() {
    sort = { ...sort, showHidden: !sort.showHidden }
    reload()
  }

  async function launch(_entry: Entry, path: string) {
    if (!api.confirmLaunch(path)) return
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

  async function newFile() {
    if (!listing) return
    try {
      const created = await api.createFile(listing.path, '新しいテキスト ドキュメント.txt')
      await reload()
      renaming = { path: created, value: created.split(/[\\/]/).pop() ?? '' }
    } catch (e) {
      error = String(e)
    }
  }

  async function openTerminalHere() {
    if (!listing) return
    try {
      await api.openTerminal(listing.path)
    } catch (e) {
      onNote(String(e))
    }
  }

  /** リネーム中の対象。null なら非表示。 */
  let renaming: { path: string; value: string } | null = null

  /** 一括名前変更の対象（一覧の表示順）。null なら閉じている。 */
  let bulkRenamePaths: string[] | null = null

  function startBulkRename() {
    if (!listing || !selection.length) return
    // 連番は見えている並び順で振りたい。選択した順ではなく一覧の順に並べ直す。
    const chosen = new Set(selection.map(api.pathIdentity))
    const dir = listing.path
    bulkRenamePaths = entries.map((entry) => api.joinPath(dir, entry.name)).filter((path) => chosen.has(api.pathIdentity(path)))
  }

  async function finishBulkRename(count: number) {
    bulkRenamePaths = null
    onNote(`${count}件の名前を変更しました（Ctrl+Z で戻せます）`)
    await reload()
  }

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

  /** 集計に数えない移動。★のファイル行から、含まれる場所を開く時に使う。 */
  function goToUncounted(path: string) {
    open(path, { counted: false })
  }

  async function syncFavoriteState() {
    const favorites = await api.listFavorites()
    // 大文字小文字や末尾の区切りが違うだけで ☆ が消えないよう、Rust 側と同じ同一性で見る。
    const here = listing ? api.pathIdentity(listing.path) : null
    isFavorite = here !== null && favorites.some((path) => api.pathIdentity(path) === here)
  }

  async function toggleFavorite() {
    if (!listing) return
    isFavorite = await api.toggleFavorite(listing.path)
    // 上段の Sidebar しか bind していないので、自前で呼ぶと下段が古いまま残る。
    api.favoritesChanged()
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

  /**
   * フォルダの中身の合計を数えて知らせる。
   * 一覧ではフォルダの大きさを 0 にしている（開くだけで木全体を読まないため）ので、必要な時だけ数える。
   */
  async function measureFolder(path: string) {
    const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path
    onNote(`「${name}」の大きさを数えています…`)
    try {
      const { files, bytes } = await api.measureFolder(path)
      onNote(`「${name}」: ${files.toLocaleString()} ファイル・${api.formatSize(bytes)}`)
    } catch (e) {
      onNote(`大きさを数えられません: ${e}`)
    }
  }

  /**
   * 選択をトレイに入れる／外す項目。
   * 選択が全部トレイにあれば「外す」、1つでも無ければ「入れる」（混在時に一部だけ外れる事故を避ける）。
   */
  /** いまの選択が全部トレイに入っているか。入れる/外すのどちらに倒すかを決める。 */
  function selectionAllInTray(): boolean {
    const keys = new Set(trayItems.map(api.pathIdentity))
    return selection.length > 0 && selection.every((path) => keys.has(api.pathIdentity(path)))
  }

  /** 選択をトレイに入れる、または全部入っていれば外す。メニューとキーの共通の入口。 */
  function toggleTray() {
    if (selection.length === 0) return
    const keys = new Set(trayItems.map(api.pathIdentity))
    if (selectionAllInTray()) onTrayRemoveMany(selection)
    else selection.filter((path) => !keys.has(api.pathIdentity(path))).forEach(onTrayToggle)
  }

  function trayMenuItem(): MenuItem {
    const allIn = selectionAllInTray()
    return {
      kind: 'item',
      label: allIn ? 'トレイから外す' : 'トレイに入れる',
      hint: `Alt+クリック / ${hint('trayToggle')}`,
      disabled: selection.length === 0,
      run: toggleTray,
    }
  }

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
          ...(entry.is_dir
            ? []
            : ([{ kind: 'item', label: 'プログラムから開く…', run: () => {
                if (listing) api.openWith(api.joinPath(listing.path, entry.name)).catch((e) => onNote(String(e)))
              } }] as MenuItem[])),
          { kind: 'item', label: 'エクスプローラーで表示', run: revealSelection },
          { kind: 'item', label: 'その他のオプション（Windows）', run: () => {
              api.showShellMenu(selection).catch((e) => onNote(String(e)))
            } },
          { kind: 'item', label: 'プロパティ', disabled: n !== 1, run: () => {
              if (one) api.showProperties(one).catch((e) => onNote(String(e)))
            } },
          // フォルダを見つけた後に「この中から探す」へ一手で移る。元のペインは残す。
          ...(entry.is_dir
            ? ([
                { kind: 'item', label: 'このフォルダ内を検索', hint: hint('hoverSplitSearchPane'), run: () => {
                  if (listing) onSplitSearch(api.joinPath(listing.path, entry.name))
                } },
                { kind: 'item', label: '大きさを数える', run: () => {
                  if (listing) measureFolder(api.joinPath(listing.path, entry.name))
                } },
              ] as MenuItem[])
            : []),
          { kind: 'sep' },
          { kind: 'item', label: 'コピー', hint: hint('copy'), run: () => copySelection(false) },
          { kind: 'item', label: '切り取り', hint: hint('cut'), run: () => copySelection(true) },
          { kind: 'item', label: '貼り付け', hint: hint('paste'), run: paste },
          { kind: 'sep' },
          { kind: 'item', label: 'フルパスをコピー', hint: hint('copyPath'), run: copyFullPaths },
          { kind: 'item', label: '名前をコピー', run: copyNames },
          { kind: 'sep' },
          // Alt+クリックを知らなくても集められるように。選択全体に対して入れる／外す。
          trayMenuItem(),
          { kind: 'sep' },
          { kind: 'item', label: 'ZIP に圧縮', hint: hint('zip'), run: zipSelection },
          // ZIP を選んでいる時だけ出す。常に出して無効化するより一覧が短くなる。
          ...(isZip
            ? ([{ kind: 'item', label: 'ここに展開', run: unzipSelection }] as MenuItem[])
            : []),
          { kind: 'sep' },
          { kind: 'item', label: '名前を変更', hint: hint('rename'), disabled: n !== 1, run: () => startRename(entry) },
          // 置換・連番・大文字小文字をまとめて。適用前に一覧で確かめられ、Ctrl+Z で戻せる。
          { kind: 'item', label: n > 1 ? `${n}件の名前をまとめて変更…` : 'まとめて名前を変更…', disabled: n === 0, run: startBulkRename },
          { kind: 'item', label: 'ゴミ箱へ送る', hint: hint('trash'), danger: true, run: () => trashSelection(selection) },
        ]
      : [
          // 空き領域＝「この場所」に対する操作。
          { kind: 'item', label: '新しいフォルダー', hint: hint('newFolder'), run: newFolder },
          { kind: 'item', label: '新しいテキストファイル', run: newFile },
          { kind: 'item', label: '貼り付け', hint: hint('paste'), run: paste },
          { kind: 'sep' },
          { kind: 'item', label: 'このフォルダのパスをコピー', hint: hint('copyPath'), run: copyFullPaths },
          { kind: 'item', label: 'エクスプローラーで表示', run: revealSelection },
          { kind: 'item', label: 'ここでターミナルを開く', run: openTerminalHere },
          { kind: 'item', label: 'このフォルダ内を検索', hint: hint('hoverSplitSearchPane'), run: () => {
              if (listing) onSplitSearch(listing.path)
            } },
          { kind: 'item', label: 'このフォルダの大きさを数える', run: () => {
              if (listing) measureFolder(listing.path)
            } },
          { kind: 'sep' },
          { kind: 'item', label: '再読み込み', hint: hint('reload'), run: reload },
          { kind: 'item', label: sort.showHidden ? '隠しファイルを隠す' : '隠しファイルを表示', run: toggleHidden },
          { kind: 'sep' },
          { kind: 'item', label: '配置…', hint: hint('layouts'), run: onOpenLayouts },
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
    if (!keyboardTarget) return

    // 入力欄で打っている最中はショートカットを奪わない。
    const el = ev.target as HTMLElement | null
    if (el && (el.tagName === 'INPUT' || el.isContentEditable)) return

    // 割り当ては設定から引く。既定と設定の二重管理を避けるため、
    // ここでキーを直接書かない（shortcuts.ts が唯一の定義元）。
    const action = matchAction(ev, settings.shortcuts)
    if (!action) return

    switch (action) {
      case 'address':
      case 'addressAlt':
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
      case 'layouts':
        ev.preventDefault()
        onOpenLayouts()
        break
      case 'hoverParent':
        if (ev.repeat || !listing?.parent) break
        ev.preventDefault()
        open(listing.parent)
        break
      case 'hoverClosePane':
        if (ev.repeat || !closable) break
        ev.preventDefault()
        onClose()
        break
      case 'hoverFavorite':
        if (ev.repeat) break
        ev.preventDefault()
        toggleFavorite()
        break
      case 'hoverSplitPane':
        if (ev.repeat || !listing) break
        ev.preventDefault()
        onSplit(listing.path)
        break
      case 'hoverSplitSearchPane':
        if (ev.repeat || !listing) break
        ev.preventDefault()
        onSplitSearch(listing.path)
        break
      case 'hoverPreview':
        if (ev.repeat) break
        ev.preventDefault()
        showPreview = !showPreview
        break
      // 並べ替え。押した列がもう一度来たら昇順/降順が反転するので、
      // 単キーを叩き続けるだけで「どちらから見たいか」まで決められる。
      // 押しっぱなしは無視する。連射すると昇順/降順が高速に入れ替わるだけで、
      // そのたびに一覧を引き直すことになる。
      case 'sortName':
        if (ev.repeat) break
        ev.preventDefault()
        changeSort('name')
        break
      case 'sortSize':
        if (ev.repeat) break
        ev.preventDefault()
        changeSort('size')
        break
      case 'sortExt':
        if (ev.repeat) break
        ev.preventDefault()
        changeSort('ext')
        break
      case 'sortModified':
        if (ev.repeat) break
        ev.preventDefault()
        changeSort('modified')
        break
      case 'sortReverse':
        if (ev.repeat) break
        ev.preventDefault()
        reverseSort()
        break
      // 全ペインを揃える。押した結果の並びをここで決めてから配る。
      case 'sortNameAll':
      case 'sortSizeAll':
      case 'sortExtAll':
      case 'sortModifiedAll': {
        if (ev.repeat) break
        ev.preventDefault()
        const key: SortKey =
          action === 'sortNameAll'
            ? 'name'
            : action === 'sortSizeAll'
              ? 'size'
              : action === 'sortExtAll'
                ? 'ext'
                : 'modified'
        const next = nextSort(key)
        onSortAll(next.key, next.descending)
        break
      }
      case 'toggleExpandAll':
        if (ev.repeat) break
        ev.preventDefault()
        expandAll()
        break
      case 'trayToggle':
        if (ev.repeat) break
        ev.preventDefault()
        toggleTray()
        break
      case 'toggleHidden':
        if (ev.repeat) break
        ev.preventDefault()
        toggleHidden()
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
  let firstFsChangeAt: number | null = null
  const FS_RELOAD_DEBOUNCE = 250
  const FS_RELOAD_MAX_WAIT = 1000

  let unlistenFavorites: (() => void) | null = null

  onMount(async () => {
    await open(initialPath)

    // 別のペインへ★を落とされた時にも、ここの ☆ が追従するようにする。
    unlistenFavorites = api.onFavoritesChanged(() => {
      syncFavoriteState()
    })

    // 転送イベントは窓全体に飛ぶので、自分が始めたものだけ拾う。
    // 開始の応答（ID）より先に届いた分は、ID が分かるまで預かっておく。
    // 捨てると、小さな転送で完了を見逃し「転送中」のまま次の順番待ちも動かなくなる。
    unlistenProgress = await listen<api.ProgressEvent>(api.TRANSFER_PROGRESS, (ev) => {
      if (ev.payload.id === transferId) progress = ev.payload
      else if (startingTransfer) earlyProgress = ev.payload
    })

    unlistenDone = await listen<api.DoneEvent>(api.TRANSFER_DONE, async (ev) => {
      if (ev.payload.id === transferId) await finishTransfer(ev.payload)
      else if (startingTransfer) earlyDone = ev.payload
    })

    // 外で作られたファイルが見えないままだと、ファイラとして信用できない。
    // 変更通知は連続して飛んでくるので、少し溜めてから1回だけ読み直す。
    // ただし通知が途切れない間（大量コピーの受け側など）待ち続けると一覧が全く更新されないので、
    // 最初の通知から一定時間経ったら溜まっていても読み直す。
    unlistenFs = await listen<string>(api.FS_CHANGED, (ev) => {
      // 自分が見ていない場所でも、控えたままだと古い内容を見せてしまう。
      prefetch.invalidate(ev.payload)
      if (ev.payload !== listing?.path) return
      if (reloadTimer !== null) clearTimeout(reloadTimer)
      const now = Date.now()
      if (firstFsChangeAt === null) firstFsChangeAt = now
      const delay = now - firstFsChangeAt >= FS_RELOAD_MAX_WAIT ? 0 : FS_RELOAD_DEBOUNCE
      reloadTimer = window.setTimeout(() => {
        reloadTimer = null
        firstFsChangeAt = null
        reload()
      }, delay)
    })
  })

  onDestroy(() => {
    unlistenFs?.()
    unlistenProgress?.()
    unlistenDone?.()
    unlistenFavorites?.()
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
  class:keyboard-target={keyboardTarget}
  bind:this={paneEl}
  on:pointerdown={onActivate}
  on:pointerenter={() => onHoverChange(true)}
  on:pointermove={onResizeMove}
  on:pointerup={() => {
    resizing = false
    resizingPreview = false
    sidebarResizing = false
  }}
  on:pointerleave={() => {
    resizing = false
    resizingPreview = false
    sidebarResizing = false
    onHoverChange(false)
  }}
>
  <PathBar
    bind:this={pathBar}
    path={listing?.path ?? '…'}
    onNavigate={open}
    onOpenBeside={onSplit}
    showHidden={sort.showHidden}
  >
    <div class="actions">
      {#if forwardTo}
        <button
          type="button"
          class="forward"
          title={`さっき潜っていた「${forwardLabel}」へ戻る`}
          on:click={() => forwardTo && open(forwardTo)}
        >
          ↘{forwardLabel}
        </button>
      {/if}
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
        title={pinned
          ? '固定を解除（このペインで移動できるようにする）'
          : 'このペインを固定（移動しようとした先は別のペインで開く）'}
        class:on={pinned}
        aria-pressed={pinned}
        on:click={onTogglePin}
      >
        📌
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
      <button
        type="button"
        title={`配置（ペインの並びを保存・呼び出し） (${hint('layouts')})`}
        on:click={onOpenLayouts}
      >
        ⊞
      </button>
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
      <button type="button" title="このペインを検索ペインにする" on:click={() => onKindChange('search')}>
        ⌕
      </button>
      <button class="search-split" type="button" title="この場所を検索するペインを隣に追加 (Shift+N)" on:click={() => listing && onSplitSearch(listing.path)}>
        ⫿⌕
      </button>
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
      <div class="tree-slot" bind:this={treeSlot} style="width: {treeWidth}px">
        <div class="sidebar-section" style:flex={sidebarSecondary ? `0 0 ${sidebarSplitRatio * 100}%` : '1'}>
          <Sidebar
          bind:this={sidebar}
          bind:tab={sidebarPrimary}
          compact={sidebarSecondary !== null}
          onSplit={splitSidebar}
          onClose={() => closeSidebarSection('primary')}
          onTabChange={sidebarChanged}
          currentPath={listing?.path ?? ''}
onNavigate={open}
          onOpenBeside={onSplit}
          onNavigateUncounted={goToUncounted}
          showHidden={sort.showHidden}
          {trayItems}
          onTrayRemove={onTrayToggle}
          {onTrayRemoveMany}
          {onTrayClear}
          onTrayTransfer={transferTray}
          {trays}
              {activeTrayId}
              {onTraySelect}
              {onTrayAdd}
              {onTrayRename}
              {onTrayDelete}
          />
        </div>
        {#if sidebarSecondary}
          <div class="sidebar-row-resizer" role="separator" aria-orientation="horizontal" aria-label="左欄の上下比率" on:pointerdown|stopPropagation={() => (sidebarResizing = true)} />
          <div class="sidebar-section lower">
            <Sidebar
              bind:tab={sidebarSecondary}
              compact
              onClose={() => closeSidebarSection('secondary')}
              onTabChange={sidebarChanged}
              currentPath={listing?.path ?? ''}
              onNavigate={open}
              onOpenBeside={onSplit}
              onNavigateUncounted={goToUncounted}
              showHidden={sort.showHidden}
              {trayItems}
              onTrayRemove={onTrayToggle}
              {onTrayRemoveMany}
              {onTrayClear}
              onTrayTransfer={transferTray}
              {trays}
              {activeTrayId}
              {onTraySelect}
              {onTrayAdd}
              {onTrayRename}
              {onTrayDelete}
            />
          </div>
        {/if}
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
      <div class="filter" class:restored={filterRestored}>
        <input
          bind:this={filterInput}
          bind:value={filter}
          placeholder="このフォルダ内を絞り込み (Ctrl+F)"
          spellcheck="false"
          aria-label="絞り込み"
          title={filterRestored ? '前回この場所で使っていた絞り込みを復元しました' : ''}
          on:input={() => (filterRestored = false)}
          on:keydown={(e) => {
            filterRestored = false
            if (e.key === 'Escape') {
              filter = ''
              filterInput?.blur()
            } else if (e.key === 'Enter' && e.ctrlKey) {
              e.preventDefault()
              promoteFilterToSearch()
            }
          }}
        />
        {#if filterRestored}
          <span class="restored-tag" title="前回この場所で使っていた絞り込みを復元しました">復元</span>
        {/if}
        {#if filter}
          <button
            type="button"
            class="promote"
            class:urge={entries.length === 0}
            title="この語のまま、ここより下を検索する (Ctrl+Enter)"
            on:click={promoteFilterToSearch}
          >
            ⌕下位も
          </button>
          <span class="hits">{entries.length} / {allEntries.length}</span>
          <button type="button" title="絞り込みを解除" on:click={() => (filter = '')}>✕</button>
        {/if}
      </div>

      <FileList
        bind:this={fileList}
        dense={multi && !active}
        expandable={true}
        {entries}
        parent={listing?.parent ?? null}
        path={listing?.path ?? ''}
        {dragIcon}
        {sort}
        onOpen={open}
        onOpenBeside={onSplit}
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
        <Preview
          path={previewTarget}
          isDir={previewIsDir}
          onNavigate={(target) => open(target)}
        />
      </div>
    {/if}
  </div>

  <TransferBar {progress} queued={queue.length} onClearQueue={clearQueue} />
  <ConflictDialog request={conflictRequest} onChoose={conflicts.choose} />
  <BulkRenameDialog paths={bulkRenamePaths} onClose={() => (bulkRenamePaths = null)} onApplied={finishBulkRename} />

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

  /* 青はクリック上のアクティブ、緑は「今キーを押すと作用する場所」。 */
  .pane.multi.keyboard-target {
    box-shadow: inset 0 2px 0 0 #63cfad, inset 0 0 0 1px rgba(99, 207, 173, 0.22);
  }

  /* 下の段へ回った時は右寄せで並べ、それでも入らなければさらに折り返す。 */
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
    margin: 10px 8px 8px auto;
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
  .actions button.search-split { width: 32px; }
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
    display: flex;
    flex-direction: column;
    flex: none;
    min-width: 0;
    border-right: 1px solid #2c2c2c;
  }
  .sidebar-section { min-height: 0; }
  .sidebar-section.lower { flex: 1; }
  .sidebar-row-resizer { height: 5px; flex: none; margin-top: -2px; margin-bottom: -2px; background: #303030; cursor: row-resize; z-index: 2; }
  .sidebar-row-resizer:hover { background: #4c9aff; }

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

  /* FileList の列の畳み方（@container）の基準。検索ペインと同じく、ここを幅の容器にする。
     指定が無いと規則が効かず、狭いペインで固定幅の列に押されて名前の列が消えていた。 */
  .list-slot {
    container-type: inline-size;
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
  /* 絞り込みを復元した場所では、黙って項目が減っているように見えないよう印を出す。 */
  /* 直前に潜っていた子への再突入。名前を出さないと、どこへ進むのか分からない。 */
  .actions .forward {
    max-width: 130px;
    padding: 1px 6px;
    font-size: 10px;
    color: #9fb8cc;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .actions .forward:hover {
    color: #d8e8f5;
  }

  .filter.restored input {
    border-color: #6b5a2a;
    background: #262115;
  }

  /* 絞り込みで見つからない時こそ押してほしいので、0件では目立たせる。 */
  .promote {
    flex: none;
    padding: 1px 6px;
    border: 1px solid #3a3a3a;
    border-radius: 3px;
    font-size: 9.5px;
    color: #9a9a9a;
    background: #242424;
    cursor: pointer;
  }

  .promote:hover {
    border-color: #4f4f4f;
    color: #ddd;
  }

  .promote.urge {
    border-color: #4a6b8a;
    color: #cfe2f2;
    background: #2f4257;
  }

  .restored-tag {
    flex: none;
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 9.5px;
    color: #d8b75e;
    background: #3a3117;
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
