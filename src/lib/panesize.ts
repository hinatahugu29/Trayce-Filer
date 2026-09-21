/**
 * ペインの幅の持ち方と、区切りを掴んで動かした時の計算。
 *
 * 幅はピクセルではなく**重み（割合）**で持つ。ピクセルで覚えると、ウィンドウの
 * 大きさを変えたり別のモニタへ移したりした時に、合計が画面と合わなくなる。
 * 重みなら「左を広く、右を狭く」という意図のほうが残る。
 *
 * 重みは `flex-grow` にそのまま渡す。合計がいくつでも比だけが効くので、
 * 正規化して回る必要がない。
 */

/** ペインをこれ以上は狭くしない。サイドバーと一覧が両方潰れる手前で止める。 */
export const MIN_PANE_PX = 220

/** 区切り線の幅。ペインが使える幅を出す時に差し引く。 */
export const DIVIDER_PX = 1

/**
 * `left` 番目と、その右隣のペインの境目を `deltaPx` だけ動かす。
 *
 * 動かすのは隣り合う2枚の間だけで、他のペインの幅には触れない。全体を配り直すと、
 * 掴んでいない側が勝手に動いて「どこを掴んだか」が分からなくなる。
 */
export function resizeAt(
  weights: number[],
  left: number,
  deltaPx: number,
  totalPx: number
): number[] {
  const right = left + 1
  if (left < 0 || right >= weights.length || totalPx <= 0) return weights

  const sum = weights.reduce((total, weight) => total + weight, 0)
  if (sum <= 0) return weights
  const pxPerWeight = totalPx / sum

  const leftPx = weights[left] * pxPerWeight
  const rightPx = weights[right] * pxPerWeight
  const pairPx = leftPx + rightPx

  // 2枚分の最低幅すら無い時は動かさない。ここで clamp すると、
  // 掴んだ方向と逆へ跳ねる。
  if (pairPx < MIN_PANE_PX * 2) return weights

  const nextLeftPx = Math.min(Math.max(leftPx + deltaPx, MIN_PANE_PX), pairPx - MIN_PANE_PX)

  const next = [...weights]
  next[left] = nextLeftPx / pxPerWeight
  next[right] = (pairPx - nextLeftPx) / pxPerWeight
  return next
}

/**
 * `at` 番目を2つに割った時の重み。分割で生まれるペインぶんを、割られた側だけで負担する。
 *
 * 全体を配り直すと、隣で幅を決めておいたペインまで動く。分割は「このペインを2つにする」
 * 操作なので、影響もそのペインの中に収める。
 */
export function splitWeight(weights: number[], at: number): number[] {
  if (at < 0 || at >= weights.length) return weights
  const half = weights[at] / 2
  const next = [...weights]
  next[at] = half
  next.splice(at + 1, 0, half)
  return next
}
