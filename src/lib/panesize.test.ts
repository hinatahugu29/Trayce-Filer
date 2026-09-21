import { describe, it, expect } from 'vitest'
import { resizeAt, splitWeight, MIN_PANE_PX } from './panesize'

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
