import { describe, it, expect, vi } from 'vitest'
import { armMiddleClick, onMiddleClick } from './middleclick'

/** MouseEvent の必要な部分だけ用意する。 */
function ev(button: number) {
  return {
    button,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  } as unknown as MouseEvent & { preventDefault: ReturnType<typeof vi.fn>; stopPropagation: ReturnType<typeof vi.fn> }
}

describe('armMiddleClick', () => {
  // 潰さないと、隣に開くのと同時にオートスクロールが始まる。
  it('中ボタンの既定動作だけ止める', () => {
    const middle = ev(1)
    armMiddleClick(middle)
    expect(middle.preventDefault).toHaveBeenCalled()
  })

  it('左と右には触らない', () => {
    for (const button of [0, 2]) {
      const other = ev(button)
      armMiddleClick(other)
      expect(other.preventDefault).not.toHaveBeenCalled()
    }
  })
})

describe('onMiddleClick', () => {
  it('中ボタンで呼ぶ', () => {
    const run = vi.fn()
    onMiddleClick(run)(ev(1))
    expect(run).toHaveBeenCalledTimes(1)
  })

  // auxclick は左以外で飛ぶので、右クリックを巻き込むと
  // コンテキストメニューと同時に隣へ開いてしまう。
  it('右ボタンでは呼ばない', () => {
    const run = vi.fn()
    onMiddleClick(run)(ev(2))
    expect(run).not.toHaveBeenCalled()
  })

  it('左ボタンでは呼ばない', () => {
    const run = vi.fn()
    onMiddleClick(run)(ev(0))
    expect(run).not.toHaveBeenCalled()
  })

  // 場所の行は入れ子になる（ツリー、その場展開した子）。止めないと
  // 祖先の行が同じ中クリックで自分の場所を開く。
  it('中ボタンの時だけ伝播を止める', () => {
    const middle = ev(1)
    onMiddleClick(() => {})(middle)
    expect(middle.stopPropagation).toHaveBeenCalled()

    const right = ev(2)
    onMiddleClick(() => {})(right)
    expect(right.stopPropagation).not.toHaveBeenCalled()
  })
})
