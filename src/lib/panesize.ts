/**
 * ペインの幅の持ち方と、区切りを掴んで動かした時の計算。
 *
 * 幅はピクセルではなく**重み（割合）**で持つ。ピクセルで覚えると、ウィンドウの
 * 大きさを変えたり別のモニタへ移したりした時に、合計が画面と合わなくなる。
 * 重みなら「左を広く、右を狭く」という意図のほうが残る。
 *
 * 重みは `flex-grow` に渡すが、**そのままでは渡さない**。`flex-grow` が比として
 * 働くのは合計が 1 以上のときだけで、下回ると CSS は余白をその合計ぶんしか配らない。
 * 操作はどれも合計を保つものの、ペインを閉じると消えた重みぶん合計が減る。
 * 渡す直前に `shares()` で揃えること。
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

/**
 * `at` 番目のペインを `deltaPx` だけ太らせる（負なら痩せさせる）。
 *
 * 増減ぶんは他のペインが今の幅に比例して負担する。隣だけに押し付けると、
 * 3枚以上の時に隣がすぐ最低幅に当たって止まり、「回しても動かない」になる。
 * 他のペインは最低幅より細くしない。取れるぶんが足りなければ、取れたぶんだけ動く。
 */
export function growAt(
  weights: number[],
  at: number,
  deltaPx: number,
  totalPx: number
): number[] {
  if (at < 0 || at >= weights.length || weights.length < 2 || totalPx <= 0) return weights
  const sum = weights.reduce((total, weight) => total + weight, 0)
  if (sum <= 0) return weights
  const pxPerWeight = totalPx / sum
  const px = weights.map((weight) => weight * pxPerWeight)

  const others = px.map((_, index) => index).filter((index) => index !== at)
  let delta = deltaPx
  if (delta > 0) {
    // 他から取れるのは、最低幅を超えている余りまで。
    const spare = others.reduce((total, index) => total + Math.max(0, px[index] - MIN_PANE_PX), 0)
    delta = Math.min(delta, spare)
    if (delta <= 0) return weights
    for (const index of others) px[index] -= (delta * Math.max(0, px[index] - MIN_PANE_PX)) / spare
  } else {
    delta = Math.max(delta, MIN_PANE_PX - px[at])
    if (delta >= 0) return weights
    const othersPx = others.reduce((total, index) => total + px[index], 0)
    for (const index of others) px[index] -= (delta * px[index]) / othersPx
  }
  px[at] += delta
  return px.map((value) => value / pxPerWeight)
}

/**
 * `flex-grow` へ渡す前に、合計が 1 になるよう揃える。
 *
 * このファイルの先頭は「合計がいくつでも比だけが効く」と書いているが、これは
 * **合計が 1 以上のときだけ**正しい。`flex-grow` の合計が 1 を下回ると、CSS は
 * 余白をその合計ぶんしか配らず、残りを空けたままにする。
 *
 * 分割・区切りの移動・`Alt+ホイール`・幅を揃える、はどれも合計を保つので問題ない。
 * 崩れるのは**ペインを閉じた時**で、消えたペインの重みぶん合計が減る。2枚を
 * 0.5 ずつで並べてから片方を閉じると合計 0.5 になり、残った1枚が画面の半分の
 * ままで止まる。
 *
 * 閉じる側で配り直すのではなくここで揃えるのは、重みが減る経路が他にもあるため
 * （切り離し、配置の適用、保存されたセッションの復元）。渡す直前に一度揃えておけば、
 * どの経路から来ても同じ結果になる。
 */
export function shares(weights: number[]): number[] {
  if (weights.length === 0) return weights
  const sum = weights.reduce((total, weight) => total + weight, 0)
  // 重みが全部 0（壊れた保存データなど）なら、等分に倒す。
  if (!(sum > 0)) return weights.map(() => 1 / weights.length)
  return weights.map((weight) => weight / sum)
}
