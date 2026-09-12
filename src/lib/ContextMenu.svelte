<script lang="ts" context="module">
  /** 型は module context に置く。インスタンス側では `export type` が使えない。 */
  export type MenuItem =
    | {
        kind: 'item'
        label: string
        hint?: string
        disabled?: boolean
        danger?: boolean
        run: () => void
      }
    | { kind: 'sep' }
</script>

<script lang="ts">
  import { tick } from 'svelte'

  export let items: MenuItem[] = []
  /** 開く位置（クライアント座標）。null なら閉じている。 */
  export let at: { x: number; y: number } | null = null
  export let onClose: () => void = () => {}

  let el: HTMLElement | null = null
  let pos = { x: 0, y: 0 }

  // 画面外へはみ出さないよう、実サイズを測ってから位置を決める。
  // カーソル位置に素直に置くと、画面の右端・下端でメニューが切れる。
  $: if (at) place(at)

  async function place(p: { x: number; y: number }) {
    pos = p
    await tick()
    if (!el) return
    const r = el.getBoundingClientRect()
    const margin = 6
    pos = {
      x: Math.min(p.x, window.innerWidth - r.width - margin),
      y: Math.min(p.y, window.innerHeight - r.height - margin),
    }
  }

  function choose(item: MenuItem) {
    if (item.kind !== 'item' || item.disabled) return
    onClose()
    item.run()
  }

  function onKey(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
      ev.preventDefault()
      onClose()
    }
  }
</script>

{#if at}
  <!-- 背面の覆い。どこをクリックしても閉じる。
       右クリックでも閉じたいので contextmenu も拾う。 -->
  <div
    class="veil"
    role="presentation"
    on:pointerdown={onClose}
    on:contextmenu|preventDefault={onClose}
  />
  <div
    class="menu"
    bind:this={el}
    role="menu"
    tabindex="-1"
    style="left: {pos.x}px; top: {pos.y}px"
    on:keydown={onKey}
  >
    {#each items as item}
      {#if item.kind === 'sep'}
        <div class="sep" role="separator" />
      {:else}
        <button
          type="button"
          role="menuitem"
          class:danger={item.danger}
          disabled={item.disabled}
          on:click={() => choose(item)}
        >
          <span class="label">{item.label}</span>
          {#if item.hint}<span class="hint">{item.hint}</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 40;
  }

  .menu {
    position: fixed;
    z-index: 41;
    min-width: 200px;
    padding: 4px;
    background: #252525;
    border: 1px solid #454545;
    border-radius: 6px;
    box-shadow: 0 10px 30px #000b;
  }

  button {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    padding: 5px 10px;
    background: none;
    border: 0;
    border-radius: 4px;
    color: #ddd;
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: default;
    white-space: nowrap;
  }
  button:hover:not(:disabled) {
    background: #33517a;
    color: #fff;
  }
  button:disabled {
    color: #5a5a5a;
  }
  button.danger:hover:not(:disabled) {
    background: #5a2626;
    color: #ffb4b4;
  }

  .label {
    flex: 1;
  }
  /* ショートカット表記。操作を覚えてもらうための導線でもある。 */
  .hint {
    flex: none;
    font-size: 10.5px;
    color: #7d7d7d;
  }
  button:hover:not(:disabled) .hint {
    color: #b9cbe4;
  }

  .sep {
    height: 1px;
    margin: 4px 6px;
    background: #3a3a3a;
  }
</style>
