import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest'
import type { Entry, Listing, SortSpec } from './api'

// Tauri の invoke はブラウザ側に居ないので、api ごと差し替える。
// ここで見たいのは「何回・どの場所を読みに行くか」という先読みの判断だけ。
const listDir = vi.fn<(path: string, sort: SortSpec) => Promise<Listing>>()

vi.mock('./api', async () => {
  const actual = await vi.importActual<typeof import('./api')>('./api')
  return { ...actual, listDir: (path: string, sort: SortSpec) => listDir(path, sort) }
})

const prefetch = await import('./prefetch')

const SORT: SortSpec = { key: 'name', descending: false, dirsFirst: true, showHidden: false }

function dir(name: string): Entry {
  return { name, is_dir: true, size: 0, modified: 0, ext: '', hidden: false }
}

function listing(path: string, parent: string | null, names: string[]): Listing {
  return { path, parent, entries: names.map(dir) }
}

/** 待ち行列は Promise を挟むので、投げ終わるまでマイクロタスクを回す。 */
async function settle() {
  for (let i = 0; i < 10; i++) await Promise.resolve()
}

beforeEach(() => {
  prefetch.clear()
  listDir.mockReset()
  listDir.mockImplementation(async (path) => listing(path, null, []))
})

afterEach(() => {
  prefetch.clear()
})

describe('warm', () => {
  it('速い場所では親と見えている子を読みに行く', async () => {
    const here = listing('C:\\work', 'C:\\', ['a', 'b', 'c'])
    prefetch.warm(here, SORT, 10)
    await settle()

    const asked = listDir.mock.calls.map((call) => call[0])
    expect(asked).toContain('C:\\')
    expect(asked).toContain('C:\\work\\a')
    expect(asked).toContain('C:\\work\\c')
  })

  it('子の数には上限がある', async () => {
    const many = Array.from({ length: 40 }, (_, i) => `d${i}`)
    prefetch.warm(listing('C:\\work', 'C:\\', many), SORT, 10)
    await settle()
    // 親1 + 子8。一度の移動で40件を読みに行ってはいけない。
    expect(listDir.mock.calls.length).toBeLessThanOrEqual(9)
  })

  // ネットワークドライブはパスから見分けられないので、掛かった時間で判断する。
  // ここを間違えると、遅い場所ほど先読みが本来の操作の邪魔をする。
  it('遅かった場所では親しか読まない', async () => {
    prefetch.warm(listing('T:\\仕様書', 'T:\\', ['a', 'b', 'c']), SORT, 711)
    await settle()

    const asked = listDir.mock.calls.map((call) => call[0])
    expect(asked).toEqual(['T:\\'])
  })

  it('親が無い場所では遅ければ何も読まない', async () => {
    prefetch.warm(listing('T:\\', null, ['a', 'b']), SORT, 711)
    await settle()
    expect(listDir).not.toHaveBeenCalled()
  })

  it('移動したら前の場所ぶんの待ち行列は捨てる', async () => {
    const many = Array.from({ length: 40 }, (_, i) => `d${i}`)
    prefetch.warm(listing('C:\\old', 'C:\\', many), SORT, 10)
    // 投げ終わる前に別の場所へ移る。
    prefetch.warm(listing('C:\\new', 'C:\\', ['x']), SORT, 10)
    await settle()

    const asked = listDir.mock.calls.map((call) => call[0])
    // 既に投げたぶんは取り消せないが、待ち行列に積んだだけのものは捨てる。
    // 40件ぶん読みに行ってしまわないことが要点。
    expect(asked.filter((path) => path.startsWith('C:\\old')).length).toBeLessThanOrEqual(
      2 // CONCURRENCY
    )
    expect(asked).toContain('C:\\new\\x')
  })

  it('既に控えてある場所は読み直さない', async () => {
    const parent = listing('C:\\', null, ['work'])
    prefetch.remember(parent, SORT)
    prefetch.warm(listing('C:\\work', 'C:\\', []), SORT, 10)
    await settle()
    expect(listDir).not.toHaveBeenCalled()
  })
})

describe('peek', () => {
  it('並べ替えが違えば別物として扱う', () => {
    prefetch.remember(listing('C:\\work', 'C:\\', ['a']), SORT)
    expect(prefetch.peek('C:\\work', SORT)).not.toBeNull()
    expect(prefetch.peek('C:\\work', { ...SORT, descending: true })).toBeNull()
  })

  it('大文字小文字が違っても同じ場所として引ける', () => {
    prefetch.remember(listing('C:\\Work', 'C:\\', ['a']), SORT)
    expect(prefetch.peek('c:\\work', SORT)).not.toBeNull()
  })

  // 外で変更された内容を見せるのが一番まずい。通知を受けたら必ず捨てる。
  it('変更を受けた場所は捨てる', () => {
    prefetch.remember(listing('C:\\work', 'C:\\', ['a']), SORT)
    prefetch.invalidate('c:\\work\\')
    expect(prefetch.peek('C:\\work', SORT)).toBeNull()
  })
})
