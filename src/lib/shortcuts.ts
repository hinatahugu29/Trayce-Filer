/**
 * アプリ内ショートカットの定義と照合。
 *
 * 定義を1箇所に集めることで、設定画面の一覧と実際のキー判定が食い違わないようにする。
 * （両方に書くと、片方だけ直して「設定したのに効かない」が起きる）
 */

export type ActionId =
  | 'address'
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

export type ActionDef = {
  id: ActionId
  label: string
  /** 組み込みの既定キー。設定で上書きできる。 */
  fallback: string
  /** 設定画面での並び分け。 */
  group: '移動' | '編集' | 'タブ' | '左手操作' | 'その他'
}

export const ACTIONS: ActionDef[] = [
  { id: 'address', label: 'アドレスバーへ', fallback: 'Ctrl+L', group: '移動' },
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

  { id: 'hoverParent', label: 'ポインター先で親へ', fallback: 'Q', group: '左手操作' },
  { id: 'hoverClosePane', label: 'ポインター先のペインを閉じる', fallback: 'W', group: '左手操作' },
  { id: 'hoverFavorite', label: 'ポインター先をお気に入り切替', fallback: 'F', group: '左手操作' },
  { id: 'hoverSplitPane', label: 'ポインター先を分割', fallback: 'N', group: '左手操作' },
  { id: 'hoverSplitSearchPane', label: '検索ペインを隣に追加', fallback: 'Shift+N', group: '左手操作' },
  { id: 'hoverPreview', label: 'ポインター先のプレビュー', fallback: 'Space', group: '左手操作' },

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
