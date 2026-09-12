import { describe, expect, it } from 'vitest'
import { matchesSearchQuery, parseSearchQuery } from './searchQuery'

describe('search query', () => {
  const entry = { name: 'Blue Icon.png', path: 'C:\\Assets\\Current\\Blue Icon.png' }

  it('matches AND terms as they are typed', () => {
    expect(matchesSearchQuery(entry, 'blue png', true)).toBe(true)
    expect(matchesSearchQuery(entry, 'blue jpg', true)).toBe(false)
    expect(matchesSearchQuery(entry, '', true)).toBe(true)
  })

  it('supports OR, exclusion, full-width spaces, and optional paths', () => {
    expect(matchesSearchQuery(entry, 'jpg|png　!draft', true)).toBe(true)
    expect(matchesSearchQuery(entry, 'png !current', true)).toBe(false)
    expect(matchesSearchQuery(entry, 'current', false)).toBe(false)
  })

  it('parses exclusion markers without keeping empty tokens', () => {
    expect(parseSearchQuery('blue|green -draft !')).toEqual([
      [
        { exclude: false, value: 'blue' },
        { exclude: false, value: 'green' },
      ],
      [{ exclude: true, value: 'draft' }],
    ])
  })
})
