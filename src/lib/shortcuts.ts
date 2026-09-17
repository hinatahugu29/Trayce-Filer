/**
 * アプリ内ショートカットの定義と照合。
 *
 * 定義を1箇所に集めることで、設定画面の一覧と実際のキー判定が食い違わないようにする。
 * （両方に書くと、片方だけ直して「設定したのに効かない」が起きる）
 */

export type ActionId =
  | 'address'
  | 'addressAlt'
  | 'filter'
  | 'copy'
  | 'cut'
  | 'paste'
  | 'undo'
  | 'reload'
  | 'newTab'
  | 'closeTab'
  | 'restoreClosedTab'
  | 'nextTab'
  | 'prevTab'
  | 'newFolder'
  | 'rename'
  | 'trash'
  | 'selectAll'
  | 'zip'
  | 'copyPath'
  | 'settings'
  | 'hoverParent'
  | 'hoverClosePane'
  | 'hoverFavorite'
  | 'hoverSplitPane'
  | 'hoverSplitSearchPane'
  | 'hoverPreview'
  | 'sortName'
  | 'sortSize'
  | 'sortExt'
  | 'sortModified'
  | 'sortReverse'
  | 'sortNameAll'
  | 'sortSizeAll'
  | 'sortExtAll'
  | 'sortModifiedAll'
  | 'toggleExpandAll'
  | 'trayToggle'
  | 'toggleHidden'
  | 'layouts'
  | 'swapWindow'

export type ActionDef = {
  id: ActionId
  label: string
  /** 組み込みの既定キー。設定で上書きできる。 */
  fallback: string
  /** 設定画面での並び分け。 */
  group: '移動' | '編集' | 'タブ' | 'ウィンドウ' | '配置' | '左手操作' | 'その他'
}

export const ACTIONS: ActionDef[] = [
  { id: 'address', label: 'アドレスバーへ', fallback: 'Ctrl+L', group: '移動' },
  // Explorer やブラウザで手に馴染んだ Alt+D も同じ動作にする。
  { id: 'addressAlt', label: 'アドレスバーへ（別キー）', fallback: 'Alt+D', group: '移動' },
  { id: 'filter', label: 'フォルダ内を絞り込み', fallback: 'Ctrl+F', group: '移動' },
  { id: 'reload', label: '再読み込み', fallback: 'F5', group: '移動' },

  { id: 'copy', label: 'コピー', fallback: 'Ctrl+C', group: '編集' },
  { id: 'cut', label: '切り取り', fallback: 'Ctrl+X', group: '編集' },
  { id: 'paste', label: '貼り付け', fallback: 'Ctrl+V', group: '編集' },
  { id: 'undo', label: '元に戻す', fallback: 'Ctrl+Z', group: '編集' },
  { id: 'selectAll', label: 'すべて選択', fallback: 'Ctrl+A', group: '編集' },
  { id: 'newFolder', label: '新しいフォルダー', fallback: 'Ctrl+Shift+N', group: '編集' },
  { id: 'rename', label: '名前を変更', fallback: 'F2', group: '編集' },
  { id: 'trash', label: 'ゴミ箱へ送る', fallback: 'Delete', group: '編集' },
  { id: 'zip', label: 'ZIP に圧縮', fallback: 'Ctrl+Shift+Z', group: '編集' },
  { id: 'copyPath', label: 'フルパスをコピー', fallback: 'Ctrl+Shift+C', group: '編集' },

  { id: 'newTab', label: '新しいタブ', fallback: 'Ctrl+T', group: 'タブ' },
  { id: 'closeTab', label: 'タブを閉じる', fallback: 'Ctrl+W', group: 'タブ' },
  { id: 'restoreClosedTab', label: '閉じたタブを開き直す', fallback: 'Ctrl+Shift+T', group: 'タブ' },
  { id: 'nextTab', label: '次のタブ', fallback: 'Ctrl+Tab', group: 'タブ' },
  { id: 'prevTab', label: '前のタブ', fallback: 'Ctrl+Shift+Tab', group: 'タブ' },

  // 素の Q/W はペインに効く。Alt を足すと同じ指のまま一段外側の「窓」に効く、
  // という規則にしてある。窓の層に破壊的な操作は置かない（切替キーの隣で
  // 指が滑ると、ペイン1枚ではなく窓ごと失うことになるため）。
  { id: 'swapWindow', label: '直前のウィンドウへ', fallback: 'Alt+Q', group: 'ウィンドウ' },

  { id: 'hoverParent', label: 'ポインター先で親へ', fallback: 'Q', group: '左手操作' },
  { id: 'hoverClosePane', label: 'ポインター先のペインを閉じる', fallback: 'W', group: '左手操作' },
  { id: 'hoverFavorite', label: 'ポインター先をお気に入り切替', fallback: 'F', group: '左手操作' },
  { id: 'hoverSplitPane', label: 'ポインター先を分割', fallback: 'N', group: '左手操作' },
  { id: 'hoverSplitSearchPane', label: '検索ペインを隣に追加', fallback: 'Shift+N', group: '左手操作' },
  { id: 'hoverPreview', label: 'ポインター先のプレビュー', fallback: 'Space', group: '左手操作' },

  // 並べ替えは左手のホームポジションに寄せる。列見出しへポインターを運ぶ往復が
  // 一番よく起きる無駄で、しかも並べ替えは「押して、見て、押し直す」試行錯誤に
  // なりやすい。同じキーをもう一度押すと昇順/降順が反転する。
  // フォルダが上に固定されることは変わらない（フォルダはフォルダ内、
  // ファイルはファイル内で並ぶ）。
  { id: 'sortName', label: '名前で並べ替え', fallback: 'A', group: '左手操作' },
  { id: 'sortSize', label: 'サイズで並べ替え', fallback: 'S', group: '左手操作' },
  { id: 'sortExt', label: '種類で並べ替え', fallback: 'X', group: '左手操作' },
  { id: 'sortModified', label: '更新日時で並べ替え', fallback: 'Z', group: '左手操作' },
  // 軸を変えずに向きだけ返す。押し直しでも反転できるが、それだと「いま何で並んでいるか」を
  // 思い出してから押すことになる。向きだけ変えたい時に軸を意識させない。
  { id: 'sortReverse', label: '並び順を反転', fallback: 'D', group: '左手操作' },
  // 転送元と転送先を同じ並びで見たい場面は多い。ペインごとに押して回るのは
  // 「同じ状態に揃える」という1つの意図に対して操作が増えすぎる。
  { id: 'sortNameAll', label: '全ペインを名前で並べ替え', fallback: 'Shift+A', group: '左手操作' },
  { id: 'sortSizeAll', label: '全ペインをサイズで並べ替え', fallback: 'Shift+S', group: '左手操作' },
  { id: 'sortExtAll', label: '全ペインを種類で並べ替え', fallback: 'Shift+X', group: '左手操作' },
  { id: 'sortModifiedAll', label: '全ペインを更新日時で並べ替え', fallback: 'Shift+Z', group: '左手操作' },

  // その場展開は → ← で1つずつしか触れない。深く広げた後に畳む手段が要る。
  { id: 'toggleExpandAll', label: 'その場展開をすべて開く/畳む', fallback: 'E', group: '左手操作' },
  // 「左手で集める」のが元々の思想なのに、集める操作だけマウスが要るのはおかしい。
  { id: 'trayToggle', label: '選択をトレイに入れる/外す', fallback: 'C', group: '左手操作' },
  { id: 'toggleHidden', label: '隠しファイルの表示切替', fallback: 'R', group: '左手操作' },

  // 保存した配置そのものは Ctrl+1..9 で直接呼ぶ。番号は一覧の並び順から決まる
  // 位置指定で、アクション1つ1つに割り当てるものではないのでここには並べない。
  { id: 'layouts', label: '配置を開く', fallback: 'Ctrl+E', group: '配置' },

  { id: 'settings', label: '設定を開く', fallback: 'Ctrl+,', group: 'その他' },
]

/** キーイベントを `Ctrl+Shift+A` のような正規表記へ直す。 */
export function keyToString(ev: KeyboardEvent): string {
  const parts: string[] = []
  if (ev.ctrlKey) parts.push('Ctrl')
  if (ev.altKey) parts.push('Alt')
  if (ev.shiftKey) parts.push('Shift')

  let key = ev.key
  // 修飾キー単体は表記に含めない（`Ctrl+Control` になってしまう）。
  if (['Control', 'Alt', 'Shift', 'Meta'].includes(key)) return parts.join('+')

  // KeyboardEvent はスペースを文字 ` ` で返すが、設定画面では読める名前にする。
  if (key === ' ') key = 'Space'

  // 1文字キーは大文字に揃える。Shift 併用で `Ctrl+Shift+z` と `Ctrl+Shift+Z` に
  // 割れると、設定と実際の判定が一致しなくなる。
  if (key.length === 1) key = key.toUpperCase()
  parts.push(key)
  return parts.join('+')
}

/** 設定（未設定なら既定）から、実際に使うキー表記を引く。 */
export function resolveKey(id: ActionId, shortcuts: Record<string, string>): string {
  const def = ACTIONS.find((a) => a.id === id)
  return shortcuts[id] || def?.fallback || ''
}

/** このキーイベントがどのアクションに当たるか。該当なしなら null。 */
export function matchAction(
  ev: KeyboardEvent,
  shortcuts: Record<string, string>
): ActionId | null {
  const pressed = keyToString(ev)
  if (!pressed || ['Ctrl', 'Alt', 'Shift', 'Ctrl+Shift'].includes(pressed)) return null

  for (const def of ACTIONS) {
    if (resolveKey(def.id, shortcuts) === pressed) return def.id
  }
  return null
}
