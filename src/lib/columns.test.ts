import { describe, it, expect } from 'vitest'
import {
  pickColumns,
  representativeName,
  COLUMN_WIDTH,
  NAME_CHROME,
  NAME_FLOOR,
} from './columns'

/** 名前の文字幅から、欲しい欄の幅を作る。 */
const wanting = (textWidth: number) => textWidth + NAME_CHROME

describe('pickColumns', () => {
  // これが直したかった場面そのもの。1920px に3ペイン、一覧部はおよそ 420px。
  // 名前は `2026-09-16 22-31-18.mp4` 程度で 175px ほどしか要らない。
  it('名前が短ければ、420px でも更新日時まで出る', () => {
    expect(pickColumns(420, wanting(175))).toEqual(['size', 'modified'])
  })

  it('広ければ全部出る', () => {
    expect(pickColumns(1200, wanting(175))).toEqual(['size', 'modified', 'ext'])
  })

  // 名前が長いフォルダ（WinSxS のマニフェストなど）。名前に譲り続けると
  // 列が1本も出なくなるので、上限で止めて省略に任せる。
  it('名前が長くてもサイズは残る', () => {
    expect(pickColumns(420, wanting(500))).toEqual(['size'])
  })

  // 狭いペインでサイズまで消すと、転送先に入るかどうかが分からなくなる。
  it('狭くても、名前が最低幅を保てる限りサイズは残る', () => {
    expect(pickColumns(250, wanting(400))).toEqual(['size'])
    expect(250 - COLUMN_WIDTH.size).toBeGreaterThanOrEqual(NAME_FLOOR)
  })

  it('名前の最低幅を割るところで、ついにサイズも消える', () => {
    expect(pickColumns(NAME_FLOOR + COLUMN_WIDTH.size - 1, wanting(400))).toEqual([])
  })

  // 種類より更新日時が先に消えることはない（優先順が入れ替わらないこと）。
  it('更新日時を畳んだのに種類だけ出る、という並びにはならない', () => {
    for (let available = 0; available <= 1400; available += 7) {
      const kept = pickColumns(available, wanting(200))
      if (kept.includes('ext')) expect(kept).toContain('modified')
      if (kept.includes('modified')) expect(kept).toContain('size')
    }
  })
})

describe('representativeName', () => {
  it('空なら空', () => {
    expect(representativeName([])).toBe('')
  })

  // 桁外れに長い1件に版面を決めさせない。
  it('飛び抜けて長い1件は代表にしない', () => {
    const names = [...Array(99).fill('short.txt'), 'x'.repeat(200)]
    expect(representativeName(names)).toBe('short.txt')
  })

  it('大半が長ければ、その長さが代表になる', () => {
    const long = 'x'.repeat(120)
    const names = [...Array(99).fill(long), 'a.txt']
    expect(representativeName(names).length).toBe(120)
  })

  it('分位を上げると長い側へ寄る', () => {
    const names = [...Array(90).fill('a'.repeat(10)), ...Array(10).fill('b'.repeat(50))]
    expect(representativeName(names, 0.5).length).toBe(10)
    expect(representativeName(names, 0.95).length).toBe(50)
  })

  it('1件しかなければそれが代表', () => {
    expect(representativeName(['only.txt'])).toBe('only.txt')
  })
})
