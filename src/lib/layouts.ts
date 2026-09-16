/**
 * 配置テンプレートの組み立てと展開。
 *
 * 多画面で毎日払っているのは移動コストではなく「前に作った並びを作り直すコスト」で、
 * 元・先・参照・検索を並べるのに十数操作かかり、それが毎回霧散する。
 * ここはその並びを型として持ち、任意の場所へ当て直すための計算だけを置く。
 *
 * 絶対パスで持つと単なるタブのブックマークにしかならないので、基準フォルダからの
 * 相対（current / parent / child）で覚えられるようにするのが要点。
 */
import type { Layout, LayoutPane, LayoutPathMode, PaneKind, SavedSidebarState } from './api'
import { pathIdentity } from './api'

/** 区切りを `\` に揃え、末尾の区切りを落とす。ドライブ直下（`C:\`）だけは残す。 */
function normalize(path: string): string {
  const slashed = path.replace(/\//g, '\\')
  return /^[a-zA-Z]:\\$/.test(slashed) ? slashed : slashed.replace(/\\+$/, '')
}

/**
 * 親フォルダ。これ以上遡れない場合は null。
 *
 * `C:\` の親は無い。`\\server\share` も共有名より上は場所として意味を持たないので
 * 遡らせない（UNC を削ると `\\server` という開けない場所になる）。
 */
export function parentOf(path: string): string | null {
  const normalized = normalize(path)
  if (/^[a-zA-Z]:\\?$/.test(normalized)) return null

  const unc = normalized.match(/^\\\\[^\\]+\\[^\\]+$/)
  if (unc) return null

  const idx = normalized.lastIndexOf('\\')
  if (idx < 0) return null
  const lead = normalized.slice(0, idx)
  if (/^[a-zA-Z]:$/.test(lead)) return lead + '\\'
  // `\\server` まで削れてしまう形は親として返さない。
  if (/^\\\\[^\\]*$/.test(lead)) return null
  return lead || null
}

/** `base` の下にあるなら、そこからの相対名を返す。同じ場所や外側なら null。 */
export function relativeTo(base: string, target: string): string | null {
  const b = normalize(base)
  const t = normalize(target)
  const bId = pathIdentity(b)
  const tId = pathIdentity(t)
  if (bId === tId) return null
  // 区切りまで含めて比べる。`pathIdentity` は末尾の区切りを落とすので、それを通した
  // `C:\work\` で前方一致を見ると `C:\workbench` まで子として拾ってしまう。
  const prefix = bId.endsWith('\\') ? bId : bId + '\\'
  if (!tId.startsWith(prefix)) return null
  return t.slice(b.endsWith('\\') ? b.length : b.length + 1)
}

/** 基準からの相対名を実際のパスへ戻す。 */
export function joinRelative(base: string, relative: string): string {
  const b = normalize(base)
  return (b.endsWith('\\') ? b : b + '\\') + relative.replace(/^[\\/]+/, '')
}

/**
 * ペインの場所を、基準フォルダから見た表し方へ落とす。
 *
 * 相対で表せるものは相対にする。そうしないと「この形を今いる場所に当てる」が
 * できず、保存した時の場所でしか使えない配置になる。
 */
export function describePath(
  path: string,
  anchor: string
): { pathMode: LayoutPathMode; path: string } {
  if (pathIdentity(normalize(path)) === pathIdentity(normalize(anchor))) {
    return { pathMode: 'current', path: '' }
  }
  const parent = parentOf(anchor)
  if (parent && pathIdentity(normalize(path)) === pathIdentity(normalize(parent))) {
    return { pathMode: 'parent', path: '' }
  }
  const relative = relativeTo(anchor, path)
  if (relative) return { pathMode: 'child', path: relative }
  return { pathMode: 'absolute', path }
}

/**
 * 表し方を実際のパスへ戻す。
 *
 * 基準に親が無い（ドライブ直下など）のに `parent` を求められた場合は、
 * 黙って別の場所を開くより基準そのものを使う。配置が1枚減るより無難。
 */
export function resolvePath(pane: LayoutPane, anchor: string): string {
  switch (pane.pathMode) {
    case 'current':
      return anchor
    case 'parent':
      return parentOf(anchor) ?? anchor
    case 'child':
      return pane.path ? joinRelative(anchor, pane.path) : anchor
    case 'absolute':
    default:
      return pane.path || anchor
  }
}

/** 配置テンプレートへ落とす元になる、いまのペイン1枚分。 */
export type PaneSnapshot = {
  kind: PaneKind
  path: string
  pinned?: boolean
  sidebar?: SavedSidebarState
  query?: string
}

/** いまの並びを、基準フォルダから見た配置テンプレートにする。 */
export function captureLayout(name: string, panes: PaneSnapshot[], anchor: string): Layout {
  return {
    name,
    panes: panes.map((pane) => {
      const described = describePath(pane.path, anchor)
      return {
        kind: pane.kind,
        pathMode: described.pathMode,
        path: described.path,
        pinned: pane.pinned ?? false,
        sidebar: pane.sidebar,
        query: pane.query ?? '',
      }
    }),
  }
}

/** 配置テンプレートを、基準フォルダに当てて実際のペイン一覧にする。 */
export function applyLayout(layout: Layout, anchor: string): PaneSnapshot[] {
  return layout.panes.map((pane) => ({
    kind: pane.kind,
    path: resolvePath(pane, anchor),
    pinned: pane.pinned,
    sidebar: pane.sidebar,
    query: pane.query,
  }))
}

/**
 * 配置が「どんな形か」を1行で表す。保存済み一覧で、名前だけでは思い出せないため。
 * 例: `親 | ここ | 🔍検索`
 */
export function describeLayout(layout: Layout): string {
  return layout.panes
    .map((pane) => {
      if (pane.kind === 'search') return pane.query ? `🔍${pane.query}` : '🔍検索'
      switch (pane.pathMode) {
        case 'current':
          return 'ここ'
        case 'parent':
          return '親'
        case 'child':
          return pane.path
        default:
          return pane.path
      }
    })
    .join(' │ ')
}
