<script lang="ts">
  import * as api from './api'
  import type { Settings, SortKey } from './api'
  import { ACTIONS, keyToString, resolveKey } from './shortcuts'
  import type { ActionId } from './shortcuts'

  export let open = false
  export let onClose: () => void = () => {}
  /** 保存後に呼ばれる。呼び出し側で表示に反映させる。 */
  export let onSaved: (settings: Settings) => void = () => {}

  let draft: Settings | null = null
  let error: string | null = null
  /** キー入力の記録中。対象のアクション id。 */
  let capturing: ActionId | null = null

  // 開くたびに現在値を読み直す。別の窓で変更された可能性がある。
  $: if (open && !draft) load()

  async function load() {
    try {
      draft = await api.getSettings()
      error = null
    } catch (e) {
      error = String(e)
    }
  }

  function close() {
    draft = null
    capturing = null
    onClose()
  }

  async function save() {
    if (!draft) return
    try {
      await api.saveSettings(draft)
      onSaved(draft)
      close()
    } catch (e) {
      error = String(e)
    }
  }

  async function reset() {
    try {
      draft = await api.resetSettings()
      onSaved(draft)
    } catch (e) {
      error = String(e)
    }
  }

  /**
   * ショートカットの記録。
   *
   * 押されたキーをそのまま設定値にする。テキストで打たせると
   * 表記ゆれ（`ctrl+c` / `Control+C`）で効かなくなるため、実キーから作る。
   */
  function onCaptureKey(ev: KeyboardEvent) {
    if (!capturing || !draft) return
    ev.preventDefault()
    ev.stopPropagation()

    if (ev.key === 'Escape') {
      capturing = null
      return
    }
    // 修飾キーだけの状態では確定しない。押している途中なので。
    if (['Control', 'Alt', 'Shift', 'Meta'].includes(ev.key)) return

    const key = keyToString(ev)
    draft.shortcuts = { ...draft.shortcuts, [capturing]: key }
    capturing = null
  }

  function clearShortcut(id: ActionId) {
    if (!draft) return
    const next = { ...draft.shortcuts }
    delete next[id]
    draft.shortcuts = next
  }

  /** 同じキーが2つ以上のアクションに割り当てられていないか。 */
  $: conflicts = (() => {
    if (!draft) return new Set<string>()
    const seen = new Map<string, number>()
    for (const a of ACTIONS) {
      const k = resolveKey(a.id, draft.shortcuts)
      seen.set(k, (seen.get(k) ?? 0) + 1)
    }
    return new Set([...seen.entries()].filter(([, n]) => n > 1).map(([k]) => k))
  })()

  const groups = ['移動', '編集', 'タブ', 'その他'] as const
  const sortKeys: { value: SortKey; label: string }[] = [
    { value: 'name', label: '名前' },
    { value: 'ext', label: '種類' },
    { value: 'size', label: 'サイズ' },
    { value: 'modified', label: '更新日時' },
  ]
</script>

<svelte:window on:keydown={onCaptureKey} />

{#if open}
  <div class="veil" role="presentation" on:pointerdown={close} />
  <div class="dialog" role="dialog" aria-modal="true" aria-label="設定">
    <header>
      <h2>設定</h2>
      <button type="button" class="x" title="閉じる" on:click={close}>✕</button>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {/if}

    {#if draft}
      <div class="body">
        <section>
          <h3>表示</h3>
          <label><input type="checkbox" bind:checked={draft.showHidden} /> 隠しファイルを表示する</label>
          <label><input type="checkbox" bind:checked={draft.showSidebar} /> 起動時にサイドバーを開く</label>
          <label><input type="checkbox" bind:checked={draft.showPreview} /> 起動時にプレビューを開く</label>
          <label><input type="checkbox" bind:checked={draft.dirsFirst} /> フォルダーを先頭にまとめる</label>
          <label><input type="checkbox" bind:checked={draft.restoreSession} /> 起動時に前回のセッション（タブ・ペイン）を復元する</label>

          <div class="row">
            <span class="row-label">既定の並び順</span>
            <select bind:value={draft.sortKey}>
              {#each sortKeys as k}<option value={k.value}>{k.label}</option>{/each}
            </select>
            <label class="inline">
              <input type="checkbox" bind:checked={draft.sortDescending} /> 降順
            </label>
          </div>
        </section>

        <section>
          <h3>操作</h3>
          <!-- 既定で有効。誤操作で消えるのがファイラで一番怖い。 -->
          <label>
            <input type="checkbox" bind:checked={draft.confirmTrash} /> ゴミ箱へ送る前に確認する
          </label>

          <div class="row">
            <span class="row-label">窓一覧のホットキー</span>
            <input class="hotkey" bind:value={draft.overlayHotkey} spellcheck="false" />
          </div>
          <p class="note">
            OS 全体に登録されます。他のアプリと衝突すると効きません（変更後は再起動が必要です）。
          </p>
        </section>

        <section>
          <h3>ショートカット</h3>
          {#if conflicts.size}
            <p class="warn">同じキーが複数の操作に割り当てられています。片方が効きません。</p>
          {/if}

          {#each groups as group}
            <h4>{group}</h4>
            <ul class="keys">
              {#each ACTIONS.filter((a) => a.group === group) as action}
                {@const key = resolveKey(action.id, draft.shortcuts)}
                <li>
                  <span class="action">{action.label}</span>
                  <button
                    type="button"
                    class="key"
                    class:capturing={capturing === action.id}
                    class:conflict={conflicts.has(key)}
                    on:click={() => (capturing = action.id)}
                  >
                    {capturing === action.id ? 'キーを押してください…' : key}
                  </button>
                  {#if draft.shortcuts[action.id]}
                    <button
                      type="button"
                      class="revert"
                      title="既定に戻す"
                      on:click={() => clearShortcut(action.id)}>↺</button
                    >
                  {:else}
                    <span class="revert-space" />
                  {/if}
                </li>
              {/each}
            </ul>
          {/each}
        </section>
      </div>

      <footer>
        <button type="button" class="reset" on:click={reset}>初期値に戻す</button>
        <span class="spacer" />
        <button type="button" on:click={close}>やめる</button>
        <button type="button" class="primary" on:click={save}>保存</button>
      </footer>
    {/if}
  </div>
{/if}

<style>
  .veil {
    position: fixed;
    inset: 0;
    background: #0009;
    z-index: 50;
  }

  .dialog {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    z-index: 51;
    display: flex;
    flex-direction: column;
    width: min(560px, 92vw);
    max-height: 86vh;
    background: #202020;
    border: 1px solid #444;
    border-radius: 8px;
    box-shadow: 0 20px 60px #000c;
  }

  header {
    display: flex;
    align-items: center;
    flex: none;
    padding: 12px 14px;
    border-bottom: 1px solid #333;
  }
  h2 {
    flex: 1;
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: #eee;
  }
  .x {
    width: 24px;
    height: 24px;
    background: none;
    border: 0;
    border-radius: 4px;
    color: #888;
    cursor: pointer;
  }
  .x:hover {
    background: #333;
    color: #fff;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 16px 16px;
  }

  section {
    margin-top: 16px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #7d7d7d;
  }
  h4 {
    margin: 14px 0 6px;
    font-size: 10.5px;
    color: #6a6a6a;
  }

  label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: 12px;
    color: #ccc;
    cursor: pointer;
  }
  label.inline {
    padding: 0;
  }
  input[type='checkbox'] {
    accent-color: #4c9aff;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
  }
  .row-label {
    font-size: 12px;
    color: #ccc;
  }
  select,
  .hotkey {
    padding: 4px 8px;
    background: #191919;
    border: 1px solid #3f3f3f;
    border-radius: 4px;
    color: #ddd;
    font: inherit;
    font-size: 12px;
    outline: none;
  }
  .hotkey {
    flex: 1;
    font-family: Consolas, monospace;
  }
  select:focus,
  .hotkey:focus {
    border-color: #4c9aff;
  }

  .note {
    margin: 4px 0 0;
    font-size: 10.5px;
    line-height: 1.6;
    color: #6a6a6a;
  }
  .warn {
    margin: 0 0 8px;
    padding: 6px 9px;
    background: #3a3320;
    border-left: 3px solid #c9a227;
    border-radius: 3px;
    font-size: 11px;
    color: #e8d9a0;
  }
  .error {
    margin: 10px 16px 0;
    padding: 8px 10px;
    background: #3a1d1d;
    border-left: 3px solid #e05252;
    font-size: 11.5px;
    color: #ff9b9b;
  }

  .keys {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .keys li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
  }
  .action {
    flex: 1;
    font-size: 12px;
    color: #ccc;
  }
  .key {
    flex: none;
    min-width: 130px;
    padding: 3px 8px;
    background: #191919;
    border: 1px solid #3f3f3f;
    border-radius: 4px;
    color: #cfcfcf;
    font-family: Consolas, monospace;
    font-size: 11px;
    cursor: pointer;
  }
  .key:hover {
    border-color: #5a5a5a;
    color: #fff;
  }
  .key.capturing {
    border-color: #4c9aff;
    background: #1d2a3d;
    color: #9cd0ff;
  }
  .key.conflict {
    border-color: #c9a227;
    color: #e8d9a0;
  }
  .revert,
  .revert-space {
    flex: none;
    width: 22px;
    height: 22px;
  }
  .revert {
    padding: 0;
    background: none;
    border: 0;
    border-radius: 4px;
    color: #777;
    cursor: pointer;
  }
  .revert:hover {
    background: #333;
    color: #ccc;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
    padding: 12px 14px;
    border-top: 1px solid #333;
  }
  .spacer {
    flex: 1;
  }
  footer button {
    padding: 6px 14px;
    background: #2f2f2f;
    border: 1px solid #454545;
    border-radius: 4px;
    color: #ccc;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  footer button:hover {
    background: #3a3a3a;
    color: #fff;
  }
  footer .primary {
    background: #33517a;
    border-color: #4c9aff;
    color: #fff;
  }
  footer .primary:hover {
    background: #3f6396;
  }
  footer .reset {
    background: none;
    border-color: transparent;
    color: #7d7d7d;
  }
  footer .reset:hover {
    background: #3a2020;
    color: #ff9b9b;
  }
</style>
