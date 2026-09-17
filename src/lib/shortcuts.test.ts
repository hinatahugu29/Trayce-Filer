import { describe, it, expect } from 'vitest'
import { ACTIONS, keyToString, resolveKey, matchAction } from './shortcuts'

/** KeyboardEvent の必要な部分だけ用意する。 */
function ev(init: Partial<KeyboardEvent> & { key: string }): KeyboardEvent {
  return {
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...init,
  } as KeyboardEvent
}

describe('keyToString', () => {
  it('修飾キーを決まった順で並べる', () => {
    expect(keyToString(ev({ key: 'a', ctrlKey: true }))).toBe('Ctrl+A')
    expect(keyToString(ev({ key: 'z', ctrlKey: true, shiftKey: true }))).toBe('Ctrl+Shift+Z')
    expect(keyToString(ev({ key: 'x', ctrlKey: true, altKey: true, shiftKey: true }))).toBe(
      'Ctrl+Alt+Shift+X'
    )
  })

  // Shift 併用で大小が割れると、設定と判定が一致しなくなる。
  it('1文字キーは大文字に揃える', () => {
    expect(keyToString(ev({ key: 'Z', ctrlKey: true }))).toBe('Ctrl+Z')
    expect(keyToString(ev({ key: 'z', ctrlKey: true }))).toBe('Ctrl+Z')
  })

  it('機能キーはそのままの表記', () => {
    expect(keyToString(ev({ key: 'F5' }))).toBe('F5')
    expect(keyToString(ev({ key: 'Delete' }))).toBe('Delete')
    expect(keyToString(ev({ key: 'Tab', ctrlKey: true }))).toBe('Ctrl+Tab')
    expect(keyToString(ev({ key: ' ' }))).toBe('Space')
  })

  // `Ctrl+Control` のような表記になるのを防ぐ。
  it('修飾キー単体は自分自身を含めない', () => {
    expect(keyToString(ev({ key: 'Control', ctrlKey: true }))).toBe('Ctrl')
    expect(keyToString(ev({ key: 'Shift', shiftKey: true }))).toBe('Shift')
  })
})

describe('resolveKey', () => {
  it('設定が無ければ既定を使う', () => {
    expect(resolveKey('copy', {})).toBe('Ctrl+C')
  })

  it('設定があればそちらを優先する', () => {
    expect(resolveKey('copy', { copy: 'Ctrl+Insert' })).toBe('Ctrl+Insert')
  })

  it('空文字の設定は既定に落とす', () => {
    expect(resolveKey('copy', { copy: '' })).toBe('Ctrl+C')
  })
})

describe('matchAction', () => {
  it('既定のキーで当たる', () => {
    expect(matchAction(ev({ key: 'c', ctrlKey: true }), {})).toBe('copy')
    expect(matchAction(ev({ key: 'F5' }), {})).toBe('reload')
    expect(matchAction(ev({ key: 'q' }), {})).toBe('hoverParent')
    expect(matchAction(ev({ key: 'N', shiftKey: true }), {})).toBe('hoverSplitSearchPane')
    expect(matchAction(ev({ key: ' ' }), {})).toBe('hoverPreview')
    expect(matchAction(ev({ key: 'd', altKey: true }), {})).toBe('addressAlt')
  })

  // 素の Q はペイン、Alt+Q は窓。同じ指で層だけが変わるのが狙いなので、
  // 片方がもう片方を飲み込んでいないことを押さえておく。
  it('Alt の有無でペインと窓を撃ち分ける', () => {
    expect(matchAction(ev({ key: 'q' }), {})).toBe('hoverParent')
    expect(matchAction(ev({ key: 'q', altKey: true }), {})).toBe('swapWindow')
    expect(matchAction(ev({ key: 'w' }), {})).toBe('hoverClosePane')
    expect(matchAction(ev({ key: 'w', altKey: true }), {})).toBe('cycleWindow')
    expect(matchAction(ev({ key: 'W', altKey: true, shiftKey: true }), {})).toBe('cycleWindowBack')
  })

  it('設定で割り当てを変えたら新しい方で当たる', () => {
    const custom = { undo: 'Ctrl+U' }
    expect(matchAction(ev({ key: 'u', ctrlKey: true }), custom)).toBe('undo')
    expect(matchAction(ev({ key: 'g' }), { hoverParent: 'G' })).toBe('hoverParent')
  })

  it('該当が無ければ null', () => {
    expect(matchAction(ev({ key: 'q', ctrlKey: true, altKey: true }), {})).toBeNull()
  })

  // 修飾キーを押し始めた瞬間にアクションが暴発しないこと。
  it('修飾キー単体では発火しない', () => {
    expect(matchAction(ev({ key: 'Control', ctrlKey: true }), {})).toBeNull()
    expect(matchAction(ev({ key: 'Shift', ctrlKey: true, shiftKey: true }), {})).toBeNull()
  })

  it('Ctrl+Shift 系が Ctrl 系に誤って当たらない', () => {
    expect(matchAction(ev({ key: 'z', ctrlKey: true, shiftKey: true }), {})).toBe('zip')
    expect(matchAction(ev({ key: 'z', ctrlKey: true }), {})).toBe('undo')
  })
})

describe('ACTIONS', () => {
  it('id が重複していない', () => {
    const ids = ACTIONS.map((a) => a.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  // 既定同士がぶつかっていると、片方が永久に発火しない。
  it('既定キーが重複していない', () => {
    const keys = ACTIONS.map((a) => a.fallback)
    expect(new Set(keys).size).toBe(keys.length)
  })
})
