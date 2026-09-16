/**
 * 移動の種別を数える。
 *
 * 「親子のアクセスコストに差が無い」をどう解くかは、俯瞰・鎖・覗き見のどれが
 * 効くかで答えが変わるが、それは議論では決まらない。実際の移動がどの種別に
 * 偏っているかが分かれば決まる。
 *
 * - `other` の割合が小さいなら、俯瞰のような跳躍向けの装置に投資しても回収できない
 * - すぐ元へ戻る回数が多いなら、移動を速くするより「入らずに覗く」ほうが効く
 *
 * ここは判断材料を作るだけで、集めた数を使って何かを変えることはしない。
 */
import { parentOf, relativeTo } from './layouts'
import * as api from './api'
import { logUi, pathIdentity } from './api'

export type MoveKind =
  /** 1つ上。 */
  | 'parent'
  /** 直下の子。 */
  | 'child'
  /** 2階層以上下。 */
  | 'descendant'
  /** 同じ親を持つ隣。 */
  | 'sibling'
  /** それ以外（別ドライブ、無関係な場所への跳躍）。 */
  | 'other'

/** 移動元から見た移動先の位置関係。移動元が無い（起動直後）なら null。 */
export function classifyMove(from: string | null, to: string): MoveKind | null {
  if (!from) return null
  if (pathIdentity(from) === pathIdentity(to)) return null

  const fromParent = parentOf(from)
  if (fromParent && pathIdentity(fromParent) === pathIdentity(to)) return 'parent'

  const below = relativeTo(from, to)
  if (below) return below.includes('\\') ? 'descendant' : 'child'

  const toParent = parentOf(to)
  if (fromParent && toParent && pathIdentity(fromParent) === pathIdentity(toParent)) return 'sibling'

  return 'other'
}

/** 「入ってみたが違った」とみなす滞在時間。これより短い出戻りを覗き見の需要として数える。 */
export const QUICK_RETURN_MS = 30_000

export type Tally = Record<MoveKind, number> & {
  /** 30秒以内に元の場所へ戻った回数。入らずに覗ければ要らなかった往復。 */
  quickReturns: number
  /** 数え始めた時刻(ms)。0 なら未開始。何日ぶんの数字かが分からないと判断できない。 */
  since: number
}

export function newTally(): Tally {
  return { parent: 0, child: 0, descendant: 0, sibling: 0, other: 0, quickReturns: 0, since: 0 }
}

/**
 * 1回の移動を数える。
 *
 * `arrivedAt` は移動元に着いた時刻。いま離れるまでの滞在が短く、かつ戻る先が
 * 直前にいた場所なら「入ってみたが違った」と数える。
 */
export function record(
  tally: Tally,
  from: string | null,
  to: string,
  previous: string | null,
  arrivedAt: number | null,
  now = Date.now()
): MoveKind | null {
  const kind = classifyMove(from, to)
  if (!kind) return null
  tally[kind] += 1
  if (
    previous &&
    arrivedAt !== null &&
    now - arrivedAt < QUICK_RETURN_MS &&
    pathIdentity(previous) === pathIdentity(to)
  ) {
    tally.quickReturns += 1
  }
  return kind
}

/** 数える対象の種別。割合を出す時の分母はこれだけ（即戻りは別枠）。 */
export const MOVE_KINDS: MoveKind[] = ['parent', 'child', 'descendant', 'sibling', 'other']

export function total(tally: Tally): number {
  return MOVE_KINDS.reduce((sum, kind) => sum + tally[kind], 0)
}

/** 数えた結果を1行にする。割合まで出さないと、件数だけでは判断できない。 */
export function summarize(tally: Tally): string {
  const sum = total(tally)
  if (sum === 0) return '移動なし'
  const parts = MOVE_KINDS.map(
    (kind) => `${kind}=${tally[kind]}(${Math.round((tally[kind] / sum) * 100)}%)`
  )
  return `移動 ${sum}件 ${parts.join(' ')} 即戻り=${tally.quickReturns}`
}

/** これだけ移動したらログへ1行出す。毎回書くとログが移動記録で埋まる。 */
const REPORT_EVERY = 25

let tally = newTally()
let sinceReport = 0

/**
 * 保存されている累計を読み込む。窓を開いた時に1度だけ呼ぶ。
 *
 * 1〜2週間ぶんを見たいので、窓を閉じるたびに0へ戻っては判断できない。
 */
export async function load() {
  try {
    tally = { ...(await api.getNavTally()) }
  } catch {
    // 読めなくても計測を止める理由にはならない。0から数え直す。
  }
}

function persist() {
  api.saveNavTally({ ...tally }).catch(() => {})
}

/** 数え直す。設定画面から呼ぶ。 */
export async function reset() {
  tally = newTally()
  persist()
}

/**
 * 窓全体で数える。ペインごとに分けても判断は変わらないので、1本にまとめる。
 * 一定回数ごとにログへ要約を出し、後から読み返せるようにする。
 */
export function track(
  from: string | null,
  to: string,
  previous: string | null,
  arrivedAt: number | null
) {
  if (!record(tally, from, to, previous, arrivedAt)) return
  sinceReport += 1
  if (sinceReport >= REPORT_EVERY) {
    sinceReport = 0
    logUi('info', `[nav] ${summarize(tally)}`)
    // IPC は移動ごとではなくここでまとめて。落ちても直近25件を失うだけ。
    persist()
  }
}

/** いまの集計。設定画面などから読む。 */
export function snapshot(): Tally {
  return { ...tally }
}
