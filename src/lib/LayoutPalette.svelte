<script lang="ts">
  import { tick } from 'svelte'
  import type { Layout } from './api'
  import { describeLayout } from './layouts'
  import { splitPath } from './api'

  export let open = false
  export let layouts: Layout[] = []
  /** 配置を当てる基準フォルダ。相対で保存された場所はここから組み立てられる。 */
  export let anchor = ''
  export let onClose: () => void = () => {}
  export let onApply: (layout: Layout) => void = () => {}
  export let onSave: (name: string) => void = () => {}
  export let onDelete: (index: number) => void = () => {}
  /** 並べ替え。Ctrl+1..9 の割り当ては並び順そのものなので、ここで動かせる必要がある。 */
  export let onMove: (index: number, delta: number) => void = () => {}

  let name = ''
  let nameInput: HTMLInputElement | null = null

  // 開くたびに入力を空へ戻す。前回の名前が残っていると上書き保存と勘違いする。
  let wasOpen = false
  $: if (open !== wasOpen) {
    wasOpen = open
    if (open) {
      name = ''
      tick().then(() => nameInput?.focus())
    }
  }

  $: here = anchor ? splitPath(anchor).tail || anchor : ''

  function save() {
    const trimmed = name.trim()
    if (!trimmed) return
    onSave(trimmed)
    name = ''
  }

  function onKey(ev: KeyboardEvent) {
    if (!open) return
    if (ev.key === 'Escape') {
      ev.preventDefault()
      onClose()
      return
    }
    // 入力中の数字は名前の一部。パレットからの番号選択と食い合わせない。
    const el = ev.target as HTMLElement | null
    if (el && (el.tagName === 'INPUT' || el.isContentEditable)) return

    const digit = Number(ev.key)
    if (Number.isInteger(digit) && digit >= 1 && digit <= 9 && layouts[digit - 1]) {
      ev.preventDefault()
      onApply(layouts[digit - 1])
    }
  }
</script>

<svelte:window on:keydown={onKey} />

{#if open}
  <div class="veil" role="presentation" on:pointerdown={onClose} />
  <div class="dialog" role="dialog" aria-modal="true" aria-label="配置">
    <header>
      <h2>配置</h2>
      <button type="button" class="x" title="閉じる" on:click={onClose}>✕</button>
    </header>

    <p class="anchor">
      基準: <strong>{here || '（場所なし）'}</strong>
      <span class="hint">「親」「ここ」で覚えた場所は、この基準から組み立て直されます</span>
    </p>

    <div class="body">
      {#if layouts.length === 0}
        <p class="empty">
          保存された配置はありません。<br />
          いまのペインの並びに名前を付けて保存すると、別の場所でも同じ形を呼び出せます。
        </p>
      {:else}
        <ul class="list">
          {#each layouts as layout, i (layout.name + i)}
            <li>
              <button
                type="button"
                class="apply"
                title="この配置を今の場所に当てる"
                on:click={() => onApply(layout)}
              >
                <span class="num">{i < 9 ? i + 1 : '·'}</span>
                <span class="text">
                  <span class="name">{layout.name}</span>
                  <span class="shape">{describeLayout(layout)}</span>
                </span>
              </button>
              <span class="tools">
                <button
                  type="button"
                  title="上へ（番号が若くなる）"
                  disabled={i === 0}
                  on:click={() => onMove(i, -1)}>▲</button
                >
                <button
                  type="button"
                  title="下へ"
                  disabled={i === layouts.length - 1}
                  on:click={() => onMove(i, 1)}>▼</button
                >
                <button
                  type="button"
                  class="danger"
                  title="この配置を削除（フォルダーには触れません）"
                  on:click={() => onDelete(i)}>✕</button
                >
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <footer>
      <input
        bind:this={nameInput}
        bind:value={name}
        placeholder="いまの並びに名前を付けて保存"
        spellcheck="false"
        aria-label="配置の名前"
        on:keydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            save()
          }
        }}
      />
      <button type="button" class="primary" disabled={!name.trim()} on:click={save}>保存</button>
    </footer>
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
    width: min(520px, 92vw);
    max-height: 80vh;
    background: #202020;
    border: 1px solid #444;
    border-radius: 8px;
    box-shadow: 0 12px 32px #000a;
  }

  header {
    display: flex;
    align-items: center;
    padding: 10px 12px;
    border-bottom: 1px solid #333;
  }

  h2 {
    margin: 0;
    font-size: 12.5px;
    font-weight: 600;
    color: #ddd;
  }

  .x {
    margin-left: auto;
    border: 0;
    font-size: 12px;
    color: #999;
    background: none;
    cursor: pointer;
  }

  .x:hover {
    color: #ddd;
  }

  .anchor {
    margin: 0;
    padding: 8px 12px;
    font-size: 10.5px;
    color: #9a9a9a;
    border-bottom: 1px solid #2c2c2c;
  }

  .anchor strong {
    color: #d6d6d6;
    font-weight: 600;
  }

  .anchor .hint {
    display: block;
    margin-top: 2px;
    font-size: 10px;
    color: #6f6f6f;
  }

  .body {
    flex: 1;
    overflow: auto;
    padding: 8px;
  }

  .empty {
    margin: 18px 10px;
    text-align: center;
    font-size: 10.5px;
    line-height: 1.7;
    color: #6a6a6a;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .list li {
    display: flex;
    align-items: stretch;
    gap: 2px;
  }

  .apply {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 6px 8px;
    text-align: left;
    border: 1px solid #333;
    border-radius: 4px;
    background: #262626;
    cursor: pointer;
  }

  .apply:hover {
    border-color: #4f4f4f;
    background: #2e2e2e;
  }

  .num {
    flex: none;
    width: 16px;
    text-align: center;
    font-size: 10px;
    color: #8a8a8a;
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    font-size: 11.5px;
    color: #ddd;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* 名前だけでは何の形か思い出せないので、必ず形を併記する。 */
  .shape {
    font-size: 10px;
    color: #7f7f7f;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tools {
    display: flex;
    flex: none;
    align-items: stretch;
  }

  .tools button {
    width: 22px;
    border: 0;
    font-size: 9px;
    color: #7f7f7f;
    background: none;
    cursor: pointer;
  }

  .tools button:hover:not(:disabled) {
    color: #ddd;
  }

  .tools button:disabled {
    color: #3c3c3c;
    cursor: default;
  }

  .tools .danger:hover {
    color: #e08080;
  }

  footer {
    display: flex;
    gap: 6px;
    padding: 9px 12px;
    border-top: 1px solid #333;
  }

  footer input {
    flex: 1;
    min-width: 0;
    padding: 4px 7px;
    border: 1px solid #3a3a3a;
    border-radius: 3px;
    font-size: 11px;
    color: #ddd;
    background: #1a1a1a;
  }

  footer input:focus {
    outline: none;
    border-color: #5a7fa8;
  }

  .primary {
    padding: 4px 12px;
    border: 1px solid #4a6b8a;
    border-radius: 3px;
    font-size: 11px;
    color: #dde8f2;
    background: #2f4257;
    cursor: pointer;
  }

  .primary:hover:not(:disabled) {
    background: #3a5169;
  }

  .primary:disabled {
    border-color: #383838;
    color: #666;
    background: #262626;
    cursor: default;
  }
</style>
