import { describe, expect, it } from 'vitest'
import {
  applyLayout,
  captureLayout,
  describeLayout,
  describePath,
  joinRelative,
  parentOf,
  relativeTo,
  resolvePath,
  type PaneSnapshot,
} from './layouts'
import type { Layout } from './api'

describe('parentOf', () => {
  it('goes up one level', () => {
    expect(parentOf('C:\\work\\project\\src')).toBe('C:\\work\\project')
  })

  it('keeps the trailing separator on a drive root', () => {
    expect(parentOf('C:\\work')).toBe('C:\\')
  })

  it('stops at the drive root', () => {
    expect(parentOf('C:\\')).toBeNull()
    expect(parentOf('C:')).toBeNull()
  })

  it('ignores a trailing separator', () => {
    expect(parentOf('C:\\work\\project\\')).toBe('C:\\work')
  })

  it('accepts forward slashes', () => {
    expect(parentOf('C:/work/project')).toBe('C:\\work')
  })

  // 共有名より上は開ける場所ではない。`\\server` を返すと移動先が壊れる。
  it('stops at a UNC share', () => {
    expect(parentOf('\\\\server\\share')).toBeNull()
    expect(parentOf('\\\\server\\share\\docs')).toBe('\\\\server\\share')
  })
})

describe('relativeTo', () => {
  it('returns the remainder below the base', () => {
    expect(relativeTo('C:\\work', 'C:\\work\\project\\src')).toBe('project\\src')
  })

  it('is null for the base itself', () => {
    expect(relativeTo('C:\\work', 'C:\\work')).toBeNull()
  })

  it('is null for somewhere outside', () => {
    expect(relativeTo('C:\\work', 'D:\\other')).toBeNull()
    expect(relativeTo('C:\\work', 'C:\\workbench')).toBeNull()
  })

  // 大文字小文字は Windows では同じ場所を指す。
  it('ignores case', () => {
    expect(relativeTo('C:\\Work', 'c:\\work\\docs')).toBe('docs')
  })

  it('works from a drive root', () => {
    expect(relativeTo('C:\\', 'C:\\work')).toBe('work')
  })
})

describe('joinRelative', () => {
  it('joins without doubling separators', () => {
    expect(joinRelative('C:\\work', 'docs')).toBe('C:\\work\\docs')
    expect(joinRelative('C:\\work\\', 'docs')).toBe('C:\\work\\docs')
    expect(joinRelative('C:\\work', '\\docs')).toBe('C:\\work\\docs')
  })

  it('works from a drive root', () => {
    expect(joinRelative('C:\\', 'work')).toBe('C:\\work')
  })
})

describe('describePath', () => {
  const anchor = 'C:\\work\\project'

  it('recognises the anchor itself', () => {
    expect(describePath(anchor, anchor)).toEqual({ pathMode: 'current', path: '' })
  })

  it('recognises the parent', () => {
    expect(describePath('C:\\work', anchor)).toEqual({ pathMode: 'parent', path: '' })
  })

  it('recognises a descendant and keeps the relative name', () => {
    expect(describePath('C:\\work\\project\\src\\lib', anchor)).toEqual({
      pathMode: 'child',
      path: 'src\\lib',
    })
  })

  it('falls back to an absolute path for somewhere unrelated', () => {
    expect(describePath('D:\\archive', anchor)).toEqual({
      pathMode: 'absolute',
      path: 'D:\\archive',
    })
  })
})

describe('resolvePath', () => {
  const pane = (over: Partial<Layout['panes'][number]>): Layout['panes'][number] => ({
    kind: 'directory',
    pathMode: 'current',
    path: '',
    pinned: false,
    query: '',
    ...over,
  })

  it('round-trips through describePath', () => {
    const anchor = 'C:\\work\\project'
    for (const target of ['C:\\work\\project', 'C:\\work', 'C:\\work\\project\\src', 'D:\\other']) {
      const described = describePath(target, anchor)
      expect(resolvePath(pane(described), anchor)).toBe(target)
    }
  })

  it('applies the same shape at a different place', () => {
    expect(resolvePath(pane({ pathMode: 'parent' }), 'D:\\jobs\\2026')).toBe('D:\\jobs')
    expect(resolvePath(pane({ pathMode: 'child', path: 'src' }), 'D:\\jobs\\2026')).toBe(
      'D:\\jobs\\2026\\src'
    )
  })

  // 親が無い場所で「親」を求められても、無関係な場所を開くよりは基準に留まるほうが無難。
  it('stays on the anchor when there is no parent', () => {
    expect(resolvePath(pane({ pathMode: 'parent' }), 'C:\\')).toBe('C:\\')
  })
})

describe('captureLayout / applyLayout', () => {
  const snapshots: PaneSnapshot[] = [
    { kind: 'directory', path: 'C:\\work' },
    { kind: 'directory', path: 'C:\\work\\project', pinned: true },
    { kind: 'directory', path: 'C:\\work\\project\\src' },
    { kind: 'search', path: 'D:\\archive', query: 'ext:pdf' },
  ]

  it('stores relative shapes, not the paths they were captured at', () => {
    const layout = captureLayout('作業一式', snapshots, 'C:\\work\\project')
    expect(layout.panes.map((p) => p.pathMode)).toEqual([
      'parent',
      'current',
      'child',
      'absolute',
    ])
    expect(layout.panes[2].path).toBe('src')
  })

  it('reproduces the original layout at the original anchor', () => {
    const layout = captureLayout('作業一式', snapshots, 'C:\\work\\project')
    expect(applyLayout(layout, 'C:\\work\\project').map((p) => p.path)).toEqual([
      'C:\\work',
      'C:\\work\\project',
      'C:\\work\\project\\src',
      'D:\\archive',
    ])
  })

  // これが目的。同じ形を別の場所へ当て直せること。
  it('reproduces the shape at a different anchor', () => {
    const layout = captureLayout('作業一式', snapshots, 'C:\\work\\project')
    expect(applyLayout(layout, 'D:\\jobs\\2026').map((p) => p.path)).toEqual([
      'D:\\jobs',
      'D:\\jobs\\2026',
      'D:\\jobs\\2026\\src',
      // 無関係な場所は絶対パスのまま。基準を変えても付いてこない。
      'D:\\archive',
    ])
  })

  it('keeps pane roles, pinning and search terms', () => {
    const layout = captureLayout('作業一式', snapshots, 'C:\\work\\project')
    const applied = applyLayout(layout, 'D:\\jobs\\2026')
    expect(applied[1].pinned).toBe(true)
    expect(applied[3].kind).toBe('search')
    expect(applied[3].query).toBe('ext:pdf')
  })
})

describe('describeLayout', () => {
  it('summarises the shape in one line', () => {
    const layout = captureLayout(
      '作業一式',
      [
        { kind: 'directory', path: 'C:\\work' },
        { kind: 'directory', path: 'C:\\work\\project' },
        { kind: 'search', path: 'C:\\work', query: 'ext:pdf' },
      ],
      'C:\\work\\project'
    )
    expect(describeLayout(layout)).toBe('親 │ ここ │ 🔍ext:pdf')
  })
})
