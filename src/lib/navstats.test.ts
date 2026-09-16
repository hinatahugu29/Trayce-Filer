import { describe, expect, it } from 'vitest'
import { classifyMove, newTally, record, summarize, QUICK_RETURN_MS } from './navstats'

describe('classifyMove', () => {
  const here = 'C:\\work\\project'

  it('has nothing to classify without a starting point', () => {
    expect(classifyMove(null, here)).toBeNull()
  })

  it('ignores staying in the same place', () => {
    expect(classifyMove(here, 'c:\\work\\project\\')).toBeNull()
  })

  it('recognises going up one level', () => {
    expect(classifyMove(here, 'C:\\work')).toBe('parent')
  })

  it('separates a direct child from a deeper one', () => {
    expect(classifyMove(here, 'C:\\work\\project\\src')).toBe('child')
    expect(classifyMove(here, 'C:\\work\\project\\src\\lib')).toBe('descendant')
  })

  it('recognises a sibling', () => {
    expect(classifyMove(here, 'C:\\work\\other')).toBe('sibling')
  })

  // 俯瞰のような跳躍向けの装置が回収できるかは、この割合で決まる。
  it('treats an unrelated place as a jump', () => {
    expect(classifyMove(here, 'D:\\archive\\2026')).toBe('other')
    expect(classifyMove(here, 'C:\\')).toBe('other')
  })
})

describe('record', () => {
  it('counts each move under its kind', () => {
    const tally = newTally()
    record(tally, 'C:\\work\\project', 'C:\\work', null, null)
    record(tally, 'C:\\work', 'C:\\work\\project', null, null)
    expect(tally.parent).toBe(1)
    expect(tally.child).toBe(1)
  })

  it('counts a fast turnaround as a quick return', () => {
    const tally = newTally()
    const arrived = 1_000
    // C:\work から project へ入り、5秒で C:\work へ戻った。
    record(tally, 'C:\\work\\project', 'C:\\work', 'C:\\work', arrived, arrived + 5_000)
    expect(tally.quickReturns).toBe(1)
  })

  it('does not count a slow turnaround', () => {
    const tally = newTally()
    const arrived = 1_000
    record(tally, 'C:\\work\\project', 'C:\\work', 'C:\\work', arrived, arrived + QUICK_RETURN_MS + 1)
    expect(tally.quickReturns).toBe(0)
  })

  it('does not count moving on somewhere else as a return', () => {
    const tally = newTally()
    record(tally, 'C:\\work\\project', 'C:\\work\\other', 'C:\\work', 1_000, 2_000)
    expect(tally.quickReturns).toBe(0)
  })
})

describe('summarize', () => {
  it('says so when nothing has been recorded', () => {
    expect(summarize(newTally())).toBe('移動なし')
  })

  it('reports shares, not just counts', () => {
    const tally = newTally()
    tally.parent = 3
    tally.other = 1
    expect(summarize(tally)).toContain('parent=3(75%)')
    expect(summarize(tally)).toContain('other=1(25%)')
  })
})
