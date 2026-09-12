export type SearchToken = { exclude: boolean; value: string }

function normalize(value: string): string {
  return value.replace(/　/g, ' ').replace(/\//g, '\\').toLocaleLowerCase()
}

export function parseSearchQuery(query: string): SearchToken[][] {
  return normalize(query)
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((part) =>
      part
        .split('|')
        .filter(Boolean)
        .map((value) => {
          const exclude = value.startsWith('!') || value.startsWith('-')
          return { exclude, value: exclude ? value.slice(1) : value }
        })
        .filter((token) => token.value.length > 0)
    )
    .filter((clause) => clause.length > 0)
}

export function matchesSearchQuery(
  entry: { name: string; path: string },
  query: string,
  matchPath: boolean
): boolean {
  const name = normalize(entry.name)
  const path = matchPath ? normalize(entry.path) : ''
  const contains = (term: string) => name.includes(term) || (matchPath && path.includes(term))

  return parseSearchQuery(query).every((clause) => {
    const included = clause.filter((token) => !token.exclude)
    const exclusionsClear = clause.filter((token) => token.exclude).every((token) => !contains(token.value))
    return exclusionsClear && (included.length === 0 || included.some((token) => contains(token.value)))
  })
}
