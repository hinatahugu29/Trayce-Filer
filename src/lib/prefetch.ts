/**
 * ディレクトリ一覧の短命キャッシュと先読み。
 *
 * 「潜って、違ったので戻る」は最も多い往復で、その戻り先は直前まで見ていた場所である。
 * 読み直す必要はまずないので、少しの間だけ持っておいて即座に描けるようにする。
 *
 * 目的は往復の体感を消すことだけなので、寿命は短く取る。長く持つと、外で変更された
 * 内容を見せてしまう危険のほうが大きい。実際の変更通知（`FS_CHANGED`）が来たら
 * その場所は必ず捨てる。
 */
import * as api from './api'
import type { Listing, SortSpec } from './api'

/** これを過ぎたキャッシュは使わない。往復1回ぶんを賄えればよい。 */
const TTL_MS = 10_000
/** 保持する場所の数。先読みは隣接階層だけなので、この程度で足りる。 */
const CAP = 24

type Hit = { listing: Listing; sort: SortSpec; at: number }

const cache = new Map<string, Hit>()

/** 並べ替えが違えば中身の順序も違う。同じ場所でも別物として扱う。 */
function sameSort(a: SortSpec, b: SortSpec): boolean {
  return (
    a.key === b.key &&
    a.descending === b.descending &&
    a.dirsFirst === b.dirsFirst &&
    a.showHidden === b.showHidden
  )
}

function put(path: string, listing: Listing, sort: SortSpec) {
  const key = api.pathIdentity(path)
  cache.delete(key)
  cache.set(key, { listing, sort: { ...sort }, at: Date.now() })
  // Map は挿入順を保つので、これだけで古いものから落ちる。
  while (cache.size > CAP) {
    const oldest = cache.keys().next().value
    if (oldest === undefined) break
    cache.delete(oldest)
  }
}

/** その場所の控えを捨てる。変更通知を受けたら必ず呼ぶ。 */
export function invalidate(path: string) {
  cache.delete(api.pathIdentity(path))
}

/** すべて捨てる。窓を閉じる時など。 */
export function clear() {
  cache.clear()
}

/** 控えがあればそれを返す。無ければ null（呼び出し側が読みに行く）。 */
export function peek(path: string, sort: SortSpec): Listing | null {
  const hit = cache.get(api.pathIdentity(path))
  if (!hit) return null
  if (Date.now() - hit.at > TTL_MS || !sameSort(hit.sort, sort)) {
    cache.delete(api.pathIdentity(path))
    return null
  }
  return hit.listing
}

/** 控えがあればそれを、無ければ読んで控える。 */
export async function listDir(path: string, sort: SortSpec): Promise<Listing> {
  const hit = peek(path, sort)
  if (hit) return hit
  const listing = await api.listDir(path, sort)
  put(listing.path, listing, sort)
  return listing
}

/** 読み終わった一覧を控えに入れる。キャッシュを通さず読んだ場合に使う。 */
export function remember(listing: Listing, sort: SortSpec) {
  put(listing.path, listing, sort)
}

/**
 * ここを超える読み込み時間なら「遅い場所」とみなす。
 *
 * ネットワークドライブはパスからは見分けられない（T: のように割り当てられていると
 * ローカルと区別が付かない）。種別を当てにいくより、直前の読み込みが実際に
 * どれだけ掛かったかで決めるほうが確実で、遅い USB や巨大フォルダにも効く。
 */
const SLOW_SOURCE_MS = 150
/** 遅い場所で先読みする数。親だけに絞る。 */
const SLOW_CHILD_LIMIT = 0

/** 同時に投げる数。速い場所でも一度に撃つと本来の操作と帯域を取り合う。 */
const CONCURRENCY = 2

/** いま有効な先読みの世代。移動したら増やし、前の場所ぶんは捨てる。 */
let generation = 0
let inFlight = 0
let queue: { path: string; sort: SortSpec; gen: number }[] = []

function pump() {
  while (inFlight < CONCURRENCY) {
    const next = queue.shift()
    if (!next) return
    // 別の場所へ移った後の先読みは、いまの操作の邪魔にしかならない。
    if (next.gen !== generation) continue
    if (peek(next.path, next.sort)) continue
    inFlight += 1
    api
      .listDir(next.path, next.sort)
      .then((fetched) => {
        if (next.gen === generation) put(fetched.path, fetched, next.sort)
      })
      .catch(() => {})
      .finally(() => {
        inFlight -= 1
        pump()
      })
  }
}

/**
 * 隣接する階層を裏で読んでおく。
 *
 * 親と、いま見えている子フォルダが対象。失敗は握りつぶす（先読みは
 * あくまで前倒しで、できなくても本来の操作は成立する）。
 *
 * `lastLoadMs` はこの場所自体の読み込みに掛かった時間。遅い場所では
 * 先読みのほうが本来の操作より重くなるので、対象を絞る。
 */
export function warm(listing: Listing, sort: SortSpec, lastLoadMs = 0, childLimit = 8) {
  // 前の場所ぶんの待ち行列を無効にする。
  generation += 1
  queue = []

  const slow = lastLoadMs >= SLOW_SOURCE_MS
  const limit = slow ? SLOW_CHILD_LIMIT : childLimit

  const targets: string[] = []
  if (listing.parent) targets.push(listing.parent)
  // 子の数は親とは別に数える。まとめて数えると、親が無い場所（ドライブ直下）で
  // 枠が1つ余り、遅い場所でも子を1件読みに行ってしまう。
  let children = 0
  for (const entry of listing.entries) {
    if (children >= limit) break
    if (!entry.is_dir) continue
    targets.push(api.joinPath(listing.path, entry.name))
    children += 1
  }

  for (const target of targets) {
    if (peek(target, sort)) continue
    queue.push({ path: target, sort, gen: generation })
  }
  pump()
}
