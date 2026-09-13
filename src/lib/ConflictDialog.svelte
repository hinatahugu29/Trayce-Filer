<script lang="ts" context="module">
  export type ConflictRequest = { names: string[]; moveFiles: boolean }
</script>

<script lang="ts">
  import { tick } from 'svelte'
  import type { ConflictPolicy } from './api'

  /** null なら閉じている。 */
  export let request: ConflictRequest | null = null
  /** null はキャンセル（何も転送しない）。 */
  export let onChoose: (policy: ConflictPolicy | null) => void

  /** 名前を全部並べると画面を埋めるので、先頭だけ見せて残りは件数にする。 */
  const SHOWN = 8

  let firstButton: HTMLButtonElement | null = null

  // 開いたら最初の選択肢へ焦点を移す。Enter で最も安全な「両方残す」になる。
  $: if (request) tick().then(() => firstButton?.focus())

  // 焦点がダイアログ外にあっても Esc で閉じられるよう窓全体で受ける。開いている時だけ反応する。
  function onKey(ev: KeyboardEvent) {
    if (request && ev.key === 'Escape') {
      ev.preventDefault()
      onChoose(null)
    }
  }
</script>

<svelte:window on:keydown={onKey} />

{#if request}
  <div class="veil" role="presentation" on:pointerdown|self={() => onChoose(null)}>
    <div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="conflict-title" tabindex="-1">
      <h2 id="conflict-title">同じ名前が {request.names.length}件 あります</h2>
      <p class="lead">{request.moveFiles ? '移動' : 'コピー'}先に既にある項目をどうしますか？</p>
      <ul>
        {#each request.names.slice(0, SHOWN) as name}
          <li title={name}>{name}</li>
        {/each}
        {#if request.names.length > SHOWN}
          <li class="more">ほか {request.names.length - SHOWN}件</li>
        {/if}
      </ul>
      <div class="choices">
        <button type="button" bind:this={firstButton} on:click={() => onChoose('rename')}>
          <strong>両方残す</strong>
          <small>新しい方を「名前 (2)」にする</small>
        </button>
        <button type="button" on:click={() => onChoose('overwrite')}>
          <strong>上書き</strong>
          <small>既存はゴミ箱へ送る（戻せます）</small>
        </button>
        <button type="button" on:click={() => onChoose('skip')}>
          <strong>スキップ</strong>
          <small>同名の項目は{request.moveFiles ? '移動' : 'コピー'}しない</small>
        </button>
      </div>
      <div class="footer">
        <button type="button" class="cancel" on:click={() => onChoose(null)}>キャンセル <kbd>Esc</kbd></button>
      </div>
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
    width: min(440px, calc(100vw - 32px));
    padding: 16px 18px 14px;
    background: #252525;
    border: 1px solid #454545;
    border-radius: 8px;
    box-shadow: 0 16px 40px #000c;
    color: #ddd;
    font-size: 12px;
  }

  h2 {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: 600;
    color: #f0f0f0;
  }

  .lead {
    margin: 0 0 10px;
    color: #a8a8a8;
  }

  ul {
    margin: 0 0 12px;
    padding: 6px 10px;
    max-height: 150px;
    overflow: auto;
    list-style: none;
    background: #1c1c1c;
    border: 1px solid #333;
    border-radius: 4px;
  }

  li {
    padding: 2px 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  li.more {
    color: #8a8a8a;
  }

  .choices {
    display: grid;
    gap: 6px;
  }

  .choices button {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    background: #2e2e2e;
    border: 1px solid #444;
    border-radius: 5px;
    color: #e6e6e6;
    font: inherit;
    text-align: left;
    cursor: default;
  }

  .choices button:hover,
  .choices button:focus-visible {
    background: #33517a;
    border-color: #4d74a8;
    outline: none;
  }

  .choices small {
    color: #9a9a9a;
  }

  .choices button:hover small,
  .choices button:focus-visible small {
    color: #c7d6ea;
  }

  .footer {
    display: flex;
    justify-content: flex-end;
    margin-top: 10px;
  }

  .cancel {
    padding: 4px 10px;
    background: none;
    border: 1px solid #444;
    border-radius: 4px;
    color: #bbb;
    font: inherit;
  }

  .cancel:hover {
    background: #333;
  }

  kbd {
    margin-left: 4px;
    font-size: 10px;
    color: #888;
  }
</style>
