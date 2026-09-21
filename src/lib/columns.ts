/**
 * 一覧に列を何本出すかの決め方。
 *
 * 以前はペインの幅だけを見て、520px以下なら更新日時、380px以下なら種類を畳んでいた。
 * この規則はそこに並んでいる名前が実際どれだけの幅を要るかを見ないので、
 * 名前の短いフォルダでも「幅が足りない」ことにして日時を捨てていた。
 * 3ペインを並べた1920pxの画面では、名前の欄に70px以上を余らせたまま
 * 更新日時が消える、ということが普通に起きる。
 *
 * ここでは逆に、名前が要る幅を先に決め、余ったぶんに何本入るかを引き算で出す。
 */

/** 名前以外の列。名前の欄は常に出るので、ここには含めない。 */
export type OptionalColumn = 'size' | 'modified' | 'ext'

/**
 * 残す順。前にあるものほど最後まで残る。
 *
 * 種類が最初に消えるのは、拡張子が名前そのものに出ていて一番重複が大きいため。
 * サイズが最後まで残るのは、転送先を選ぶ時の「入るかどうか」がここでしか分からないため。
 */
export const COLUMN_PRIORITY: OptionalColumn[] = ['size', 'modified', 'ext']

/** 各列の幅(px)。FileList の CSS と一致させること。 */
export const COLUMN_WIDTH: Record<OptionalColumn, number> = {
  size: 78,
  modified: 108,
  ext: 62,
}

/** 名前の欄をこれ以上は削らない。ここを割ると、どの項目なのか分からなくなる。 */
export const NAME_FLOOR = 150

/**
 * 名前の欄に回す上限。
 *
 * 長い名前に合わせて無制限に譲ると、名前が長いフォルダでは列が1本も出なくなる。
 * ある幅から先は、名前を省略してでも列を残すほうが読める。
 */
export const NAME_CAP = 320

/** 名前の文字そのもの以外に要る幅（左の余白・アイコン・間隔・右の逃げ）。 */
export const NAME_CHROME = 44

/**
 * 出す列を決める。戻り値は残す順に並ぶ。
 *
 * `wantedName` は名前の欄に欲しい幅（文字の幅 + `NAME_CHROME`）。
 */
export function pickColumns(available: number, wantedName: number): OptionalColumn[] {
  const reserve = Math.min(Math.max(wantedName, NAME_FLOOR), NAME_CAP)

  const kept: OptionalColumn[] = []
  let used = 0
  for (const column of COLUMN_PRIORITY) {
    const width = COLUMN_WIDTH[column]
    // サイズだけは、名前が最低幅を保てる限り残す。ここを希望幅で判定すると、
    // 狭いペインで名前しか出ない一覧になり、転送先を選べなくなる。
    const need = column === 'size' ? NAME_FLOOR : reserve
    if (available - used - width < need) break
    used += width
    kept.push(column)
  }
  return kept
}

/**
 * 版面を決める代表の名前を選ぶ。
 *
 * 一番長い1件に合わせてはいけない。桁外れに長い名前が1つ混ざっているだけで、
 * そのフォルダからは列が消える。ほとんどの名前が収まれば足りるので、
 * 長さの上位から少し譲った位置にあるものを使い、残りは省略に任せる。
 *
 * 並べ替えずに数え上げで求める。Windowsのファイル名は255文字までなので、
 * 長さを数えるだけで分位が出る（3万件の一覧でも一巡で済む）。
 */
export function representativeName(names: string[], percentile = 0.9): string {
  if (names.length === 0) return ''

  const MAX_LENGTH = 255
  const histogram = new Uint32Array(MAX_LENGTH + 1)
  for (const name of names) {
    histogram[Math.min(name.length, MAX_LENGTH)] += 1
  }

  const target = Math.floor(names.length * percentile)
  let seen = 0
  let pick = 0
  for (let length = 0; length <= MAX_LENGTH; length++) {
    seen += histogram[length]
    if (seen > target) {
      pick = length
      break
    }
  }

  return names.find((name) => Math.min(name.length, MAX_LENGTH) === pick) ?? names[0]
}
