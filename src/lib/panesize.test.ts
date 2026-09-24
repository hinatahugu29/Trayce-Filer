import { describe, it, expect } from 'vitest'
import { resizeAt, splitWeight, growAt, shares, MIN_PANE_PX } from './panesize'

/** 重みを、その幅での実ピクセルへ直す。読みやすさのためのテスト用ヘルパー。 */
const toPx = (weights: number[], totalPx: number) => {
  const sum = weights.reduce((total, weight) => total + weight, 0)
  return weights.map((weight) => Math.round((weight / sum) * totalPx))
}

describe('resizeAt', () => {
  it('掴んだ境目の2枚だけが動く', () => {
    const next = resizeAt([1, 1, 1], 0, 100, 1800)
    expect(toPx(next, 1800)).toEqual([700, 500, 600])
  })

  it('逆向きにも動く', () => {
    const next = resizeAt([1, 1, 1], 0, -100, 1800)
    expect(toPx(next, 1800)).toEqual([500, 700, 600])
  })

  // 3ペインを 900px に並べると1枚300pxで、最低幅までの余裕は80pxしかない。
  // 掴んで引いた時にそこで止まることを、実際に起きる形で押さえておく。
  it('3ペインの狭い窓では、すぐ最低幅に当たる', () => {
    const next = resizeAt([1, 1, 1], 0, -200, 900)
    expect(toPx(next, 900)).toEqual([MIN_PANE_PX, 600 - MIN_PANE_PX, 300])
  })

  it('最低幅で止まる', () => {
    const next = resizeAt([1, 1], 0, -1000, 900)
    expect(toPx(next, 900)).toEqual([MIN_PANE_PX, 900 - MIN_PANE_PX])
  })

  it('右端側の最低幅でも止まる', () => {
    const next = resizeAt([1, 1], 0, 1000, 900)
    expect(toPx(next, 900)).toEqual([900 - MIN_PANE_PX, MIN_PANE_PX])
  })

  // 2枚で最低幅の2倍も無い時に clamp すると、掴んだ向きと逆へ跳ねる。
  it('2枚分の最低幅すら無ければ動かさない', () => {
    const weights = [1, 1]
    expect(resizeAt(weights, 0, 50, MIN_PANE_PX)).toBe(weights)
  })

  it('端や範囲外の境目は無視する', () => {
    const weights = [1, 1]
    expect(resizeAt(weights, 1, 50, 900)).toBe(weights)
    expect(resizeAt(weights, -1, 50, 900)).toBe(weights)
  })

  it('合計が0でも壊れない', () => {
    const weights = [0, 0]
    expect(resizeAt(weights, 0, 50, 900)).toBe(weights)
  })

  // 重みは割合なので、幅が変わっても比が保たれる。
  it('ウィンドウの幅が変わっても比は変わらない', () => {
    const next = resizeAt([1, 1], 0, 150, 900)
    expect(toPx(next, 900)).toEqual([600, 300])
    expect(toPx(next, 1800)).toEqual([1200, 600])
  })
})

describe('splitWeight', () => {
  it('割られたペインの中だけで分け合う', () => {
    expect(splitWeight([2, 1], 0)).toEqual([1, 1, 1])
  })

  it('隣のペインの幅は動かない', () => {
    const next = splitWeight([1, 3], 1)
    expect(toPx(next, 800)).toEqual([200, 300, 300])
  })

  it('範囲外は何もしない', () => {
    const weights = [1, 1]
    expect(splitWeight(weights, 5)).toBe(weights)
  })
})

describe('growAt', () => {
  const px = (weights: number[], total: number) => {
    const sum = weights.reduce((a, b) => a + b, 0)
    return weights.map((w) => (w / sum) * total)
  }

  it('widens the pane and takes the room from the others in proportion', () => {
    const next = px(growAt([1, 1, 1], 0, 90, 1200), 1200)
    expect(next[0]).toBeCloseTo(490)
    expect(next[1]).toBeCloseTo(355)
    expect(next[2]).toBeCloseTo(355)
  })

  it('narrows the pane and hands the room back', () => {
    const next = px(growAt([1, 1], 1, -100, 1000), 1000)
    expect(next).toEqual([expect.closeTo(600), expect.closeTo(400)])
  })

  it('never pushes another pane below the minimum', () => {
    const next = px(growAt([1, 1, 1], 1, 10000, 1200), 1200)
    expect(next[0]).toBeCloseTo(MIN_PANE_PX)
    expect(next[2]).toBeCloseTo(MIN_PANE_PX)
    expect(next[1]).toBeCloseTo(1200 - MIN_PANE_PX * 2)
  })

  it('never shrinks the pane itself below the minimum', () => {
    const next = px(growAt([1, 1], 0, -10000, 1000), 1000)
    expect(next[0]).toBeCloseTo(MIN_PANE_PX)
  })

  it('leaves a single pane alone', () => {
    const weights = [1]
    expect(growAt(weights, 0, 50, 800)).toBe(weights)
  })
})

describe('shares', () => {
  // 報告された症状そのもの。2枚を 0.5 ずつで並べ、右を W で閉じると
  // 残りの重みは 0.5。flex-grow 0.5 は余白を半分しか配らないので、
  // 1枚になっても画面の半分のままで止まっていた。
  it('ペインを閉じて合計が 1 を下回っても、残りが画面を埋める', () => {
    const afterSplit = splitWeight([1], 0)
    expect(afterSplit).toEqual([0.5, 0.5])

    const afterClose = [afterSplit[0]] // 右を閉じた
    expect(shares(afterClose)).toEqual([1])
  })

  it('合計を 1 に揃える', () => {
    const got = shares([0.25, 0.25])
    expect(got[0] + got[1]).toBeCloseTo(1)
    expect(got).toEqual([0.5, 0.5])
  })

  // 幅を揃えると全部 1 になる（合計 3）。比が変わってはいけない。
  it('比は変えない', () => {
    expect(shares([1, 1, 1]).map((v) => Number(v.toFixed(4)))).toEqual([0.3333, 0.3333, 0.3333])
    expect(shares([3, 1])).toEqual([0.75, 0.25])
  })

  // 壊れた保存データで全部 0 だと 0 除算になり、幅が NaN になって消える。
  it('重みが全部 0 なら等分に倒す', () => {
    expect(shares([0, 0])).toEqual([0.5, 0.5])
  })

  it('空でも落ちない', () => {
    expect(shares([])).toEqual([])
  })
})
