import { transferConflicts, type ConflictPolicy } from './api'

/** 衝突の確認ダイアログに渡す内容。 */
export type ConflictRequest = { names: string[]; moveFiles: boolean }

/**
 * 転送前の衝突確認。ペインとワークベンチで同じ手順を使う。
 *
 * `show` にダイアログの表示状態を渡す（null で閉じる）。
 * `ask` は衝突が無ければ確認なしで 'rename' を返し、あればダイアログの選択を待つ。null は取りやめ。
 * `choose` はダイアログの選択をそのまま渡す。
 */
export function createConflictPrompt(show: (request: ConflictRequest | null) => void) {
  let pending: ((policy: ConflictPolicy | null) => void) | null = null

  return {
    get open() {
      return pending !== null
    },

    async ask(paths: string[], dest: string, moveFiles: boolean): Promise<ConflictPolicy | null> {
      const names = await transferConflicts(paths, dest)
      if (!names.length) return 'rename'
      return new Promise((resolve) => {
        pending = resolve
        show({ names, moveFiles })
      })
    },

    choose(policy: ConflictPolicy | null) {
      const resolve = pending
      pending = null
      show(null)
      resolve?.(policy)
    },
  }
}
