<script lang="ts">
  import { tick } from 'svelte'
  import * as api from './api'

  /** 名前を変える対象（一覧の表示順）。null なら閉じている。 */
  export let paths: string[] | null = null
  export let onClose: () => void
  /** 適用後に呼ばれる。件数を渡す。 */
  export let onApplied: (count: number) => void

  let rule: api.RenameRule = api.defaultRenameRule()
  let previews: api.RenamePreview[] = []
  let error: string | null = null
  let applying = false
  let firstInput: HTMLInputElement | null = null
  let requestSeq = 0

  // 開くたびに規則を初期化する。前回の置換が残ったまま別の選択に当たる事故を避ける。
  let openedFor: string[] | null = null
  $: if (paths !== openedFor) {
    openedFor = paths
    if (paths) {
      rule = api.defaultRenameRule()
      error = null
      tick().then(() => firstInput?.focus())
    }
  }

  // 入力のたびに結果を作り直す。連続入力では最後の依頼の結果だけを使う。
  $: if (paths) refresh(paths, rule)

  async function refresh(targets: string[], current: api.RenameRule) {
    const seq = ++requestSeq
    try {
      const result = await api.planBulkRename(targets, current)
      if (seq === requestSeq) previews = result
    } catch (e) {
      if (seq === requestSeq) error = String(e)
    }
  }

  $: changed = previews.filter((p) => p.status === 'ok').length
  $: blocked = previews.filter((p) => p.status === 'invalid' || p.status === 'conflict').length

  async function apply() {
    if (!paths || blocked || !changed || applying) return
    applying = true
    try {
      const count = await api.applyBulkRename(paths, rule)
      onApplied(count)
    } catch (e) {
      error = String(e)
    } finally {
      applying = false
    }
  }

  function onKey(ev: KeyboardEvent) {
    if (!paths) return
    if (ev.key === 'Escape') {
      ev.preventDefault()
      onClose()
    } else if (ev.key === 'Enter' && (ev.ctrlKey || ev.metaKey)) {
      ev.preventDefault()
      apply()
    }
  }

  const STATUS_LABEL: Record<api.RenamePreview['status'], string> = {
    ok: '変更',
    unchanged: 'そのまま',
    invalid: '不正',
    conflict: '衝突',
  }
</script>

<svelte:window on:keydown={onKey} />

{#if paths}
  <div class="veil" role="presentation" on:pointerdown|self={onClose}>
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="bulk-rename-title">
      <header>
        <h2 id="bulk-rename-title">{paths.length}件の名前をまとめて変更</h2>
      </header>

      <div class="form">
        <label>
          <span>探す</span>
          <input bind:this={firstInput} bind:value={rule.find} spellcheck="false" placeholder="置き換える文字（空なら置換しない）" />
        </label>
        <label>
          <span>置換</span>
          <input bind:value={rule.replace} spellcheck="false" placeholder="置き換え後の文字" />
        </label>
        <label class="inline">
          <input type="checkbox" bind:checked={rule.matchCase} /> 大文字小文字を区別する
        </label>

        <label>
          <span>名前の形</span>
          <input bind:value={rule.template} spellcheck="false" placeholder={'{name}'} />
        </label>
        <p class="hint"><code>{'{name}'}</code> 元の名前（置換後） ・ <code>{'{n}'}</code> 連番 ・ 拡張子は変わりません</p>

        <div class="row">
          <label>
            <span>連番の開始</span>
            <input type="number" min="0" bind:value={rule.start} />
          </label>
          <label>
            <span>桁数</span>
            <input type="number" min="1" max="10" bind:value={rule.digits} />
          </label>
          <label>
            <span>大文字小文字</span>
            <select bind:value={rule.case}>
              <option value="">変えない</option>
              <option value="lower">小文字にする</option>
              <option value="upper">大文字にする</option>
            </select>
          </label>
        </div>
      </div>

      <div class="preview" aria-label="変更後の名前">
        <table>
          <thead>
            <tr><th>今の名前</th><th>新しい名前</th><th /></tr>
          </thead>
          <tbody>
            {#each previews as preview (preview.path)}
              <tr class={preview.status} title={preview.message}>
                <td>{preview.name}</td>
                <td>{preview.newName}</td>
                <td class="status">{preview.message || STATUS_LABEL[preview.status]}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      {#if error}<p class="error">{error}</p>{/if}

      <footer>
        <span class="summary">
          変更 {changed}件{#if blocked}・<strong>問題 {blocked}件（解消するまで適用できません）</strong>{/if}
        </span>
        <button type="button" on:click={onClose}>やめる <kbd>Esc</kbd></button>
        <button type="button" class="primary" disabled={!changed || blocked > 0 || applying} on:click={apply}>
          変更する <kbd>Ctrl+Enter</kbd>
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    background: #0008;
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(680px, calc(100vw - 32px));
    max-height: 86vh;
    background: #232323;
    border: 1px solid #454545;
    border-radius: 8px;
    box-shadow: 0 16px 40px #000c;
    color: #ddd;
    font-size: 12px;
  }

  header {
    padding: 12px 16px 6px;
  }

  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: #f0f0f0;
  }

  .form {
    display: grid;
    gap: 6px;
    padding: 6px 16px 10px;
  }

  label {
    display: grid;
    grid-template-columns: 84px 1fr;
    align-items: center;
    gap: 8px;
  }

  label.inline {
    display: flex;
    margin-left: 92px;
  }

  .row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }

  .row label {
    grid-template-columns: auto 1fr;
  }

  input:not([type='checkbox']),
  select {
    min-width: 0;
    padding: 5px 8px;
    background: #181818;
    border: 1px solid #444;
    border-radius: 4px;
    color: #eee;
    font: inherit;
  }

  input:focus,
  select:focus {
    outline: none;
    border-color: #4c9aff;
  }

  .hint {
    margin: 0 0 0 92px;
    color: #8a8a8a;
    font-size: 11px;
  }

  code {
    color: #b9cbe4;
  }

  .preview {
    flex: 1;
    min-height: 120px;
    overflow: auto;
    margin: 0 16px;
    border: 1px solid #333;
    border-radius: 4px;
    background: #1b1b1b;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    position: sticky;
    top: 0;
    padding: 4px 8px;
    background: #262626;
    color: #9a9a9a;
    font-weight: normal;
    text-align: left;
  }

  td {
    padding: 3px 8px;
    border-top: 1px solid #2a2a2a;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 280px;
  }

  tr.unchanged td {
    color: #777;
  }

  tr.ok td:nth-child(2) {
    color: #9fd8b5;
  }

  tr.invalid td,
  tr.conflict td {
    color: #f0aaa4;
  }

  td.status {
    width: 1%;
    color: #8a8a8a;
    font-size: 11px;
  }

  .error {
    margin: 8px 16px 0;
    color: #f0aaa4;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px 12px;
  }

  .summary {
    flex: 1;
    color: #9a9a9a;
  }

  .summary strong {
    color: #f0aaa4;
    font-weight: normal;
  }

  footer button {
    padding: 5px 12px;
    background: #2e2e2e;
    border: 1px solid #444;
    border-radius: 4px;
    color: #ddd;
    font: inherit;
  }

  footer button.primary:not(:disabled) {
    background: #33517a;
    border-color: #4d74a8;
    color: #fff;
  }

  footer button:disabled {
    opacity: 0.5;
  }

  kbd {
    margin-left: 4px;
    font-size: 10px;
    color: #999;
  }
</style>
