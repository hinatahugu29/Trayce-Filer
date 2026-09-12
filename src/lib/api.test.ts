import { describe, it, expect } from 'vitest'
import {
  splitPath,
  pathHue,
  elideLeft,
  ancestorsOf,
  relativeTime,
  formatSize,
  formatModified,
  joinPath,
} from './api'

describe('splitPath', () => {
  it('末尾のフォルダ名と上位階層を分ける', () => {
    expect(splitPath('C:\\Users\\hinat')).toEqual({ lead: 'C:\\Users', tail: 'hinat' })
  })

  // rtl 表示で `\C:\Users` と出てしまった不具合の再発防止。
  it('lead に末尾の区切り文字を残さない', () => {
    expect(splitPath('C:\\Users\\hinat').lead).not.toMatch(/[\\/]$/)
  })

  it('末尾の区切り文字が付いていても同じ結果になる', () => {
    expect(splitPath('C:\\Users\\hinat\\')).toEqual({ lead: 'C:\\Users', tail: 'hinat' })
  })

  it('区切りが無ければ全体を tail にする', () => {
    expect(splitPath('hinat')).toEqual({ lead: '', tail: 'hinat' })
  })

  it('ドライブ直下を扱える', () => {
    expect(splitPath('C:\\')).toEqual({ lead: '', tail: 'C:' })
  })

  it('スラッシュ区切りも扱える', () => {
    expect(splitPath('/home/user/docs')).toEqual({ lead: '/home/user', tail: 'docs' })
  })
})

describe('pathHue', () => {
  it('同じパスは常に同じ色になる', () => {
    expect(pathHue('C:\\a\\b')).toBe(pathHue('C:\\a\\b'))
  })

  it('違うパスは基本的に違う色になる', () => {
    expect(pathHue('C:\\projectA')).not.toBe(pathHue('C:\\projectB'))
  })

  it('必ず 0〜359 に収まる', () => {
    for (const p of ['', 'C:\\', 'C:\\very\\deep\\nested\\path\\here', '日本語のフォルダ']) {
      const h = pathHue(p)
      expect(h).toBeGreaterThanOrEqual(0)
      expect(h).toBeLessThan(360)
    }
  })
})

describe('joinPath', () => {
  it('親と名前をつなぐ', () => {
    expect(joinPath('C:\\Users', 'hinat')).toBe('C:\\Users\\hinat')
  })

  it('親の末尾に区切りがあっても重複させない', () => {
    expect(joinPath('C:\\', 'Users')).toBe('C:\\Users')
    expect(joinPath('C:\\Users\\', 'hinat')).toBe('C:\\Users\\hinat')
  })

  it('スラッシュ区切りの親はスラッシュを使う', () => {
    expect(joinPath('/home/user', 'docs')).toBe('/home/user/docs')
  })

  // ドライブ直下は `C:\` から `C:` に潰れてはいけない。
  it('ドライブ直下でも壊れない', () => {
    expect(joinPath('C:\\', 'Windows')).toBe('C:\\Windows')
  })
})

describe('ancestorsOf', () => {
  it('ルートから自分自身まで並べる', () => {
    expect(ancestorsOf('C:\\Users\\hinat')).toEqual(['C:\\', 'C:\\Users', 'C:\\Users\\hinat'])
  })

  // drives() は 'C:\' 形式を返すので、先頭がそれと一致しないとツリーが繋がらない。
  it('先頭はドライブ根の形になる', () => {
    expect(ancestorsOf('C:\\Users\\hinat')[0]).toBe('C:\\')
  })

  it('ドライブ根そのものを渡しても壊れない', () => {
    expect(ancestorsOf('C:\\')).toEqual(['C:\\'])
  })

  it('空文字なら空配列', () => {
    expect(ancestorsOf('')).toEqual([])
  })

  it('末尾の区切りが付いていても同じ結果になる', () => {
    expect(ancestorsOf('C:\\Users\\hinat\\')).toEqual(ancestorsOf('C:\\Users\\hinat'))
  })
})

describe('formatSize', () => {
  it('1KB 未満はバイトのまま', () => {
    expect(formatSize(0)).toBe('0 B')
    expect(formatSize(1023)).toBe('1023 B')
  })

  it('単位を繰り上げる', () => {
    expect(formatSize(1024)).toBe('1.0 KB')
    expect(formatSize(1024 * 1024)).toBe('1.0 MB')
    expect(formatSize(1024 ** 3)).toBe('1.0 GB')
  })

  // 桁が揃っていないと一覧で大小を目で比べられない。
  it('10以上は小数を落として桁を揃える', () => {
    expect(formatSize(1024 * 15)).toBe('15 KB')
    expect(formatSize(1024 * 1.5)).toBe('1.5 KB')
  })

  it('ディレクトリは空にする', () => {
    expect(formatSize(0, true)).toBe('')
    expect(formatSize(9999, true)).toBe('')
  })
})

describe('formatModified', () => {
  const now = new Date(2026, 6, 25, 15, 30).getTime()

  it('取得できていない場合は空', () => {
    expect(formatModified(0, now)).toBe('')
  })

  // 今日のものが日付で埋まると「さっきのやつ」が探しにくい。
  it('今日のものは時刻だけ', () => {
    expect(formatModified(new Date(2026, 6, 25, 9, 5).getTime(), now)).toBe('09:05')
  })

  it('同じ年なら月日と時刻', () => {
    expect(formatModified(new Date(2026, 2, 3, 8, 7).getTime(), now)).toBe('3/03 08:07')
  })

  it('別の年なら年月日', () => {
    expect(formatModified(new Date(2024, 11, 31, 23, 59).getTime(), now)).toBe('2024/12/31')
  })
})

describe('relativeTime', () => {
  const now = 1_000_000_000_000
  const ago = (ms: number) => relativeTime(now - ms, now)

  const SEC = 1000
  const MIN = 60 * SEC
  const HOUR = 60 * MIN
  const DAY = 24 * HOUR

  it('直近は「たった今」', () => {
    expect(ago(0)).toBe('たった今')
    expect(ago(59 * SEC)).toBe('たった今')
  })

  it('分・時間・日で切り替わる', () => {
    expect(ago(5 * MIN)).toBe('5分前')
    expect(ago(3 * HOUR)).toBe('3時間前')
    expect(ago(1 * DAY)).toBe('昨日')
    expect(ago(5 * DAY)).toBe('5日前')
  })

  it('月・年まで丸める', () => {
    expect(ago(60 * DAY)).toBe('2か月前')
    expect(ago(400 * DAY)).toBe('1年前')
  })

  // 時計のずれで未来の時刻が入っても壊さない。
  it('未来の時刻でも負の表示にならない', () => {
    expect(relativeTime(now + 10_000, now)).toBe('たった今')
  })
})

describe('elideLeft', () => {
  it('収まる文字列はそのまま返す', () => {
    expect(elideLeft('C:\\Users', 42)).toBe('C:\\Users')
  })

  it('あふれたら先頭を削り、末尾側を残す', () => {
    const long = 'C:\\a\\b\\c\\d\\e\\f\\g\\h\\i\\j\\k\\l\\m\\n\\o\\p\\q\\r\\s\\t'
    const out = elideLeft(long, 20)
    expect(out).toHaveLength(20)
    expect(out.startsWith('…')).toBe(true)
    expect(long.endsWith(out.slice(1))).toBe(true)
  })
})
