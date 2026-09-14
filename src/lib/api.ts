import { invoke } from '@tauri-apps/api/core'

/**
 * 一覧の1項目。
 *
 * **絶対パスは持たない。** 親ディレクトリと name から `joinPath` で組み立てる。
 * 全項目に持たせると転送量が倍近くになるため（WinSxS 27,498件で計測:
 * 文字列 5.4MB のうち 54% が path だった）。
 */
export type Entry = {
  name: string
  is_dir: boolean
  size: number
  /** 最終更新(ms)。0 は取得できなかったことを表す。 */
  modified: number
  /** 小文字の拡張子（`.` なし）。 */
  ext: string
  hidden: boolean
}

/** 親ディレクトリと名前から絶対パスを作る。区切りの重複を避ける。 */
export function joinPath(dir: string, name: string): string {
  const sep = dir.includes('/') && !dir.includes('\\') ? '/' : '\\'
  return dir.replace(/[\\/]+$/, '') + sep + name
}

/** Windows パスをトレイ内の同一性比較に使える形へ揃える。表示用の元文字列は変えない。 */
export function pathIdentity(path: string): string {
  const normalized = path.replace(/\//g, '\\')
  const withoutTrailing = /^[a-zA-Z]:\\$/.test(normalized)
    ? normalized
    : normalized.replace(/\\+$/, '')
  return withoutTrailing.toLocaleLowerCase('en-US')
}
/** U+FF61〜U+FF9F の半角カナに対応する全角文字。Rust 側 search.rs と同じ表。 */
const HALFWIDTH_KANA = '。「」、・ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン゛゜'

/**
 * 絞り込み用に表記の揺れを畳む。Rust の `search::normalize` と同じ規則。
 *
 * 英字の大文字小文字、全角英数記号と半角、全角空白、半角カナ（濁点合成を含む）を同一視する。
 * 表示用の文字列には使わず、比較の両辺にだけ掛ける。
 */
export function foldForSearch(value: string): string {
  // NFKC は全角英数・半角カナ・濁点合成をまとめて扱える。
  // ただし `①`→`1` や `㍻`→`平成` まで畳むので、Rust 側と結果を揃えるため範囲を限る。
  let out = ''
  for (const ch of value) {
    const code = ch.codePointAt(0)!
    if (code === 0x3000) out += ' '
    else if (code >= 0xff01 && code <= 0xff5e) out += String.fromCodePoint(code - 0xfee0)
    else if ((code === 0xff9e || code === 0xff9f) && out) {
      const composed = (out.slice(-1) + (code === 0xff9e ? '゙' : '゚')).normalize('NFC')
      out = composed.length === 1 ? out.slice(0, -1) + composed : out + HALFWIDTH_KANA[code - 0xff61]
    } else if (code >= 0xff61 && code <= 0xff9f) out += HALFWIDTH_KANA[code - 0xff61]
    else out += ch
  }
  return out.toLowerCase()
}

export type Listing = { path: string; parent: string | null; entries: Entry[] }

export type SortKey = 'name' | 'size' | 'modified' | 'ext'
export type SortSpec = {
  key: SortKey
  descending: boolean
  dirsFirst: boolean
  showHidden: boolean
}

export const defaultSort = (): SortSpec => ({
  key: 'name',
  descending: false,
  dirsFirst: true,
  showHidden: false,
})
export type WindowPaneInfo = {
  id: number
  kind: PaneKind
  path: string
  query: string
  is_active: boolean
}
export type WindowTabInfo = { id: number; label: string; is_active: boolean }
export type WindowInfo = {
  label: string
  path: string
  last_focused: number
  tray_paths: string[]
  active_tab_label: string
  tab_count: number
  panes: WindowPaneInfo[]
  tabs: WindowTabInfo[]
}
export type HistoryEntry = { path: string; at: number }

export const listFavorites = () => invoke<string[]>('list_favorites')
/** 登録済みなら解除、未登録なら登録。戻り値は操作後に登録されているか。 */
export const toggleFavorite = (path: string) => invoke<boolean>('toggle_favorite_cmd', { path })
export const removeFavorite = (path: string) => invoke<void>('remove_favorite', { path })
export const reorderFavorite = (from: number, to: number) =>
  invoke<void>('reorder_favorite', { from, to })

export const listHistory = () => invoke<HistoryEntry[]>('list_history')
export const recordHistory = (path: string) => invoke<void>('record_history', { path })
export const clearHistory = () => invoke<void>('clear_history')
export type SearchLocationEntry = { paths: string[]; at: number }
export const listSearchLocations = () => invoke<SearchLocationEntry[]>('list_search_locations')
export const recordSearchLocation = (paths: string[]) => invoke<void>('record_search_location', { paths })
export const removeSearchLocation = (paths: string[]) => invoke<void>('remove_search_location', { paths })
export const clearSearchLocations = () => invoke<void>('clear_search_locations')

/** 保存されている場所がまだ存在するか。消えたフォルダを灰色にするのに使う。 */
export const pathsExist = (paths: string[]) => invoke<boolean[]>('paths_exist', { paths })

export const homeDir = () => invoke<string>('home_dir')
export const listDir = (path: string, sort?: SortSpec) => invoke<Listing>('list_dir', { path, sort })

export const createFolder = (parent: string, name: string) =>
  invoke<string>('create_folder', { parent, name })
export const createFile = (parent: string, name: string) =>
  invoke<string>('create_file', { parent, name })
/** ダブルクリックでそのまま動いてしまう種類。開く前に一度だけ確かめる。 */
const EXECUTABLE = /\.(exe|bat|cmd|com|msi|ps1|vbs|js|lnk|scr)$/i
export function confirmLaunch(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? path
  return !EXECUTABLE.test(name) || confirm(`${name} を実行しますか？`)
}
export const openWith = (path: string) => invoke<void>('open_with', { path })
export const showProperties = (path: string) => invoke<void>('show_properties', { path })
/** Windows 本来の右クリックメニューをマウス位置に出す。 */
export const showShellMenu = (paths: string[]) => invoke<void>('show_shell_menu', { paths })
export const openTerminal = (path: string) => invoke<void>('open_terminal', { path })
export const renameEntry =(path: string, newName: string) =>
  invoke<string>('rename_entry', { path, newName })
/**
 * 一括名前変更の規則。拡張子は常にそのまま残る。
 * `template` の `{name}` は置換後の元の名前、`{n}` は連番（`start` から、`digits` 桁で 0 埋め）。
 */
export type RenameRule = {
  find: string
  replace: string
  matchCase: boolean
  template: string
  start: number
  digits: number
  case: '' | 'lower' | 'upper'
}
export const defaultRenameRule = (): RenameRule => ({
  find: '',
  replace: '',
  matchCase: false,
  template: '{name}',
  start: 1,
  digits: 2,
  case: '',
})
export type RenamePreview = {
  path: string
  name: string
  newName: string
  status: 'ok' | 'unchanged' | 'invalid' | 'conflict'
  message: string
}
/** 規則を当てた結果の一覧。実際には何も変えない。`paths` の並び順で連番を振る。 */
export const planBulkRename = (paths: string[], rule: RenameRule) =>
  invoke<RenamePreview[]>('plan_bulk_rename', { paths, rule })
/** 規則を当てて名前を変える。1件でも問題があれば何も変えない。戻り値は変更した件数。 */
export const applyBulkRename = (paths: string[], rule: RenameRule) =>
  invoke<number>('apply_bulk_rename', { paths, rule })

/** ゴミ箱へ送る。完全削除は用意しない（誤操作で戻せないのを避けるため）。 */
export const trashEntries = (paths: string[]) => invoke<number>('trash_entries', { paths })

/** フォルダの中身の合計。大きな木では時間がかかる（別スレッドで数える）。 */
export const measureFolder = (path: string) =>
  invoke<{ files: number; bytes: number }>('measure_folder', { path })

/** アドレスバーの補完候補。 */
export const completePath = (input: string, showHidden = false) =>
  invoke<string[]>('complete_path', { input, showHidden })

export type Preview =
  | { kind: 'text'; text: string; truncated: boolean }
  | { kind: 'image' }
  | { kind: 'unsupported'; reason: string }

/** ファイルの中身をどう見せられるか判定する。画像は実データを返さず、フロントが直接読む。 */
export const previewEntry = (path: string) => invoke<Preview>('preview_entry', { path })

export type ClipboardData = { paths: string[]; cut: boolean }

/** クリップボードは Rust 側に持つ。窓Aで切って窓Bで貼る、を成立させるため。 */
export const setClipboard = (paths: string[], cut: boolean) =>
  invoke<void>('set_clipboard', { paths, cut })
export const getClipboard = () => invoke<ClipboardData>('get_clipboard')

export type ProgressEvent = {
  id: number
  /** 総量を数えている最中。大きな木ではここで待たされる。 */
  scanning: boolean
  filesDone: number
  filesTotal: number
  bytesDone: number
  bytesTotal: number
  bytesPerSec?: number
  etaSecs?: number | null
  current: string
}

export function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return '0 B/s'
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s']
  let i = 0
  let speed = bytesPerSec
  while (speed >= 1024 && i < units.length - 1) {
    speed /= 1024
    i++
  }
  return `${speed.toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

export function formatEta(seconds: number): string {
  if (seconds <= 0) return 'まもなく完了'
  if (seconds < 60) return `残り 約${seconds}秒`
  const mins = Math.floor(seconds / 60)
  const secs = seconds % 60
  if (mins < 60) return `残り 約${mins}分${secs > 0 ? `${secs}秒` : ''}`
  const hours = Math.floor(mins / 60)
  const remMins = mins % 60
  return `残り 約${hours}時間${remMins > 0 ? `${remMins}分` : ''}`
}
export type DoneEvent = {
  id: number
  dest: string
  created: number
  cancelled: boolean
  error: string | null
  completedSources: string[]
}

/**
 * 転送先に同名があった時の扱い。
 * rename は `name (2)` で両方残す。overwrite は既存をゴミ箱へ送ってから置く。skip は転送しない。
 */
export type ConflictPolicy = 'rename' | 'overwrite' | 'skip'

/** 転送先で名前が衝突する項目名。空なら選択肢を出さずに転送してよい。 */
export const transferConflicts = (paths: string[], dest: string) =>
  invoke<string[]>('transfer_conflicts', { paths, dest })

/** 別スレッドでコピー/移動を始める。戻り値は中断に使う ID。 */
export const startTransfer = (
  paths: string[],
  dest: string,
  moveFiles: boolean,
  conflict: ConflictPolicy = 'rename',
) => invoke<number>('start_transfer', { paths, dest, moveFiles, conflict })
export const cancelTransfer = (id: number) => invoke<void>('cancel_transfer', { id })
export const TRANSFER_PROGRESS = 'transfer-progress'
export const TRANSFER_DONE = 'transfer-done'

export type SearchEntry = {
  path: string
  name: string
  isDir: boolean
  size: number
  modified: number
  ext: string
}
export type SearchProgressEvent = { id: string; indexed: number; paused: boolean }
export type SearchResultsEvent = {
  id: string
  requestId: number
  entries: SearchEntry[]
  matched: number
  indexed: number
  truncated: boolean
}
export type SearchDoneEvent = {
  id: string
  indexed: number
  cancelled: boolean
  warningCount: number
  error: string | null
}
export type SearchFilterOptions = {
  query: string
  matchPath: boolean
  sortKey: SortKey | 'path'
  descending: boolean
  dirsFirst: boolean
  limit?: number
}
export const SEARCH_PROGRESS = 'search-progress'
export const SEARCH_RESULTS = 'search-results'
export const SEARCH_DONE = 'search-done'
export const startSearch = (id: string, roots: string[]) => invoke<void>('start_search', { id, roots })
export const filterSearch = (id: string, requestId: number, options: SearchFilterOptions) =>
  invoke<void>('filter_search', { id, requestId, options })
/** 表示中の結果が実在するか確かめ直す。移動・削除された項目は結果から消える。 */
export const recheckSearch = (id: string) => invoke<void>('recheck_search', { id })
export const cancelSearch = (id: string) => invoke<void>('cancel_search', { id })
export const pauseSearch = (id: string) => invoke<void>('pause_search', { id })
export const resumeSearch = (id: string) => invoke<void>('resume_search', { id })

/** 選択を1つの ZIP にまとめる。戻り値は作られた ZIP のパス。 */
export const compressToZip = (paths: string[]) => invoke<string>('compress_to_zip', { paths })
/** ZIP を同じ場所のフォルダへ展開する。戻り値は展開先。 */
export const extractZip = (path: string) => invoke<string>('extract_zip', { path })

export type Settings = {
  showHidden: boolean
  showSidebar: boolean
  showPreview: boolean
  confirmTrash: boolean
  sortKey: SortKey
  sortDescending: boolean
  dirsFirst: boolean
  restoreSession: boolean
  overlayHotkey: string
  shortcuts: Record<string, string>
  /** 検索で中へ潜らないフォルダ名（大文字小文字は区別しない）。 */
  searchExcludes: string[]
}

export const getSettings = () => invoke<Settings>('get_settings')
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings })
export const resetSettings = () => invoke<Settings>('reset_settings')

/**
 * A pane's role is persisted independently from its current filesystem context.
 * Missing `kind` means directory so sessions written by older builds remain valid.
 */
export type PaneKind = 'directory' | 'search'
export type SidebarTab = 'tree' | 'favorites' | 'history' | 'tray'
export type SavedSidebarState = {
  primary: SidebarTab
  secondary?: SidebarTab
  splitRatio?: number
}
export type SavedSearchState = {
  scopePaths: string[]
  query: string
  matchPath: boolean
  sortKey?: SortKey
  sortDescending?: boolean
  dirsFirst?: boolean
  showPreview?: boolean
  showHistory?: boolean
  recentQueries?: string[]
}
export type SavedPaneState = {
  path: string
  kind?: PaneKind
  /** 固定中のペインは移動しない。移動しようとした先は別のペインで開く。 */
  pinned?: boolean
  search?: SavedSearchState
  sidebar?: SavedSidebarState
  selectedEntry?: string
  scrollTop?: number
}
/** 目的別に分けた収集トレイ。 */
export type SavedTray = { name: string; paths: string[] }
export type SavedTabState = {
  panes: SavedPaneState[]
  activePaneIndex: number
  /** 選んでいるトレイの中身。複数トレイより前の保存データとの互換のため残す。 */
  trayPaths?: string[]
  /** 名前付きの全トレイ。無ければ trayPaths を1つのトレイとして扱う。 */
  trays?: SavedTray[]
  activeTrayIndex?: number
}
/** トレイ切り替え欄に出す概要。 */
export type TraySummary = { id: number; name: string; count: number }
export type SessionState = { tabs: SavedTabState[]; activeTabIndex: number }

/** どの窓も保存する。次回起動時は最後に閉じた窓のセッションが main に復元される。 */
export const saveSessionState = (label: string, session: SessionState) =>
  invoke<void>('save_session_state', { label, session })
export const getSessionState = () => invoke<SessionState | null>('get_session_state')

export type UndoState = { available: boolean; label: string }

/** いま取り消せる操作があるか、あれば何をするか。 */
export const undoState = () => invoke<UndoState>('undo_state')
/** 直前の操作を取り消す。取り消した内容の説明を返す。 */
export const undoLast = () => invoke<string>('undo_last')

export const watchDir = (path: string) => invoke<void>('watch_dir', { path })
export const unwatchDir = (path: string) => invoke<void>('unwatch_dir', { path })
/** ディレクトリ変更の通知イベント名。ペイロードは変更のあったパス。 */
export const FS_CHANGED = 'fs-changed'
export const listSubdirs = (path: string, showHidden = false) =>
  invoke<Entry[]>('list_subdirs', { path, showHidden })
export const drives = () => invoke<string[]>('drives')
export const dragPreviewIcon = () => invoke<string>('drag_preview_icon')
/**
 * ホットキーの表示用文字列。
 *
 * Rust 側は登録に使う `CmdOrCtrl+...` 形式を返すが、それをそのまま出すと
 * 内部表記が画面に漏れる。Windows 専用と決めているので Ctrl に読み替える。
 */
export const overlayHotkey = () =>
  invoke<string>('overlay_hotkey').then((s) => s.replace('CmdOrCtrl', 'Ctrl'))

export const openWindow = (path: string) => invoke<string>('open_window', { path })
export const listWindows = () => invoke<WindowInfo[]>('list_windows')
export const focusWindow = (label: string) => invoke<void>('focus_window', { label })
export const focusPane = (label: string, paneId: number) =>
  invoke<void>('focus_pane', { label, paneId })
export const focusTab = (label: string, tabId: number) =>
  invoke<void>('focus_tab', { label, tabId })
/** `open_window` で指定された初期パス。main 窓など未登録の場合は null。 */
export const windowInitialPath = (label: string) =>
  invoke<string | null>('window_initial_path', { label })

export const registerWindow = (label: string, path: string) =>
  invoke<void>('register_window', { label, path })
export const setWindowPath = (label: string, path: string) =>
  invoke<void>('set_window_path', { label, path })
export const setWindowContext = (
  label: string,
  activeTabLabel: string,
  tabCount: number,
  panes: WindowPaneInfo[],
  tabs: WindowTabInfo[],
) => invoke<void>('set_window_context', { label, activeTabLabel, tabCount, panes, tabs })
export const setWindowTray = (label: string, paths: string[]) =>
  invoke<void>('set_window_tray', { label, paths })
export const WINDOW_TRAY_CHANGED = 'window-tray-changed'
export const ACTIVATE_PANE_REQUEST = 'activate-pane-request'
export const ACTIVATE_TAB_REQUEST = 'activate-tab-request'
export type WindowTrayChanged = { label: string; paths: string[] }
export const touchWindow = (label: string) => invoke<void>('touch_window', { label })
export const hideOverlay = () => invoke<void>('hide_overlay')
export const setOverlayMode = (mode: 'compact' | 'workbench') =>
  invoke<void>('set_overlay_mode', { mode })
export const closeWindow = (label: string) => invoke<void>('close_window', { label })

/** 診断用。画面内ログはウィンドウを閉じると消えるので Rust 側にも残す。 */
export const logDnd = (message: string) => invoke<void>('log_dnd', { message }).catch(() => {})

/** フロントの異常を Rust のログへ。WebView のコンソールは外から見えない。 */
export const logUi = (level: 'error' | 'warn' | 'info', message: string) =>
  invoke<void>('log_ui', { level, message }).catch(() => {})

/**
 * パスを「末尾のフォルダ名」と「その手前」に割る。
 *
 * 窓が重なった状態で探す時、人が手掛かりにするのは末尾のフォルダ名。
 * そこだけ大きく出し、上位階層は文脈として小さく添える。
 */
export function splitPath(path: string): { lead: string; tail: string } {
  const normalized = path.replace(/[\\/]+$/, '')
  const idx = Math.max(normalized.lastIndexOf('\\'), normalized.lastIndexOf('/'))
  if (idx < 0) return { lead: '', tail: normalized }
  // lead に末尾の区切りは残さない。残すと表示側で余計な `\` が見える。
  return { lead: normalized.slice(0, idx), tail: normalized.slice(idx + 1) || normalized }
}

/**
 * 長いパスを左側から削る。
 *
 * 上位階層は末尾（＝直近の親）ほど手掛かりになるので、あふれる時は先頭を捨てる。
 * CSS の `direction: rtl` でも似たことはできるが、区切り文字の並びが崩れるので
 * ここで確定させる。
 */
export function elideLeft(text: string, max = 42): string {
  if (text.length <= max) return text
  return '…' + text.slice(text.length - max + 1)
}

/**
 * ルートから自分自身までの各階層を並べる。
 *
 * ツリーを現在地まで自動で開くのに使う。先頭はドライブ根で、
 * `drives()` が返す `C:\` と同じ形になるよう区切りを付けたまま返す。
 */
export function ancestorsOf(path: string): string[] {
  const normalized = path.replace(/[\\/]+$/, '')
  if (!normalized) return []

  const parts = normalized.split(/[\\/]/)
  const out: string[] = []
  parts.forEach((part, i) => {
    if (i === 0) {
      // ドライブ根だけは区切りを残す。`C:` では実在するパスにならない。
      out.push(part + '\\')
    } else {
      out.push(out[out.length - 1].replace(/\\$/, '') + '\\' + part)
    }
  })
  return out
}

/**
 * 履歴の時刻を「さっき見た」感覚に近い表記にする。
 *
 * 絶対時刻より「5分前」「昨日」の方が記憶と結び付く。
 */
export function relativeTime(at: number, now = Date.now()): string {
  const sec = Math.max(0, Math.floor((now - at) / 1000))
  if (sec < 60) return 'たった今'
  const min = Math.floor(sec / 60)
  if (min < 60) return `${min}分前`
  const hour = Math.floor(min / 60)
  if (hour < 24) return `${hour}時間前`
  const day = Math.floor(hour / 24)
  if (day === 1) return '昨日'
  if (day < 30) return `${day}日前`
  const month = Math.floor(day / 30)
  if (month < 12) return `${month}か月前`
  return `${Math.floor(month / 12)}年前`
}

/**
 * バイト数を読める形にする。
 *
 * 桁が揃っていないと一覧で大小を目で比べられないので、
 * 1000未満は小数1桁に揃える。
 */
export function formatSize(bytes: number, isDir = false): string {
  if (isDir) return ''
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB', 'PB']
  let value = bytes / 1024
  let i = 0
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024
    i++
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[i]}`
}

/**
 * 更新日時。今日のものは時刻だけにして、日付の羅列に埋もれさせない。
 */
export function formatModified(ms: number, now = Date.now()): string {
  if (!ms) return ''
  const d = new Date(ms)
  const n = new Date(now)
  const p2 = (v: number) => String(v).padStart(2, '0')
  const time = `${p2(d.getHours())}:${p2(d.getMinutes())}`

  const sameDay =
    d.getFullYear() === n.getFullYear() &&
    d.getMonth() === n.getMonth() &&
    d.getDate() === n.getDate()
  if (sameDay) return time

  if (d.getFullYear() === n.getFullYear()) {
    return `${d.getMonth() + 1}/${p2(d.getDate())} ${time}`
  }
  return `${d.getFullYear()}/${p2(d.getMonth() + 1)}/${p2(d.getDate())}`
}

/**
 * パスから安定した色相を作る。
 *
 * 同じフォルダは常に同じ色になるので、窓が重なっていても
 * 色の帯だけで「あのプロジェクトの窓だ」と当たりが付く。
 */
export function pathHue(path: string): number {
  let h = 0
  for (let i = 0; i < path.length; i++) {
    h = (h * 31 + path.charCodeAt(i)) % 360
  }
  return h
}
