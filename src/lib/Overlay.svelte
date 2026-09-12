<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import type { UnlistenFn } from '@tauri-apps/api/event'
  import * as api from './api'
  import { splitPath, pathHue, elideLeft } from './api'
  import type { WindowInfo } from './api'

  const win = getCurrentWindow()

  let windows: WindowInfo[] = []
  let query = ''
  let cursor = 0
  let input: HTMLInputElement | null = null

  // 文字列で絞る。人はフォルダ名を文字で覚えているので、
  // 末尾のフォルダ名にも全体パスにも当たるようにする。
  $: filtered = windows.filter((w) => w.path.toLowerCase().includes(query.toLowerCase()))
  $: if (cursor >= filtered.length) cursor = Math.max(0, filtered.length - 1)

  async function refresh() {
    windows = await api.listWindows()
  }

  async function pick(w: WindowInfo | undefined) {
    if (!w) return
    await api.focusWindow(w.label)
    await api.hideOverlay()
  }

  function onKey(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
      api.hideOverlay()
    } else if (ev.key === 'ArrowDown') {
      ev.preventDefault()
      cursor = Math.min(cursor + 1, filtered.length - 1)
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault()
      cursor = Math.max(cursor - 1, 0)
    } else if (ev.key === 'Enter') {
      ev.preventDefault()
      pick(filtered[cursor])
    }
  }

  let unlistenFocus: UnlistenFn | null = null

  onMount(async () => {
    await refresh()
    input?.focus()

    // ホットキーで再表示されるたびに開き直すのではなく show されるだけなので、
    // 前面に来た時に中身を取り直す。そうしないと古い一覧が出る。
    unlistenFocus = await win.onFocusChanged(async ({ payload }) => {
      if (payload) {
        await refresh()
        query = ''
        cursor = 0
        input?.focus()
      }
    })
  })

  onDestroy(() => unlistenFocus?.())
</script>

<svelte:window on:keydown={onKey} />

<div class="overlay">
  <div class="head">
    <span class="title">開いている窓</span>
    <span class="count">{filtered.length}</span>
  </div>

  <input
    bind:this={input}
    bind:value={query}
    placeholder="フォルダ名で絞り込み…"
    spellcheck="false"
  />

  <ul role="listbox" aria-label="開いている窓">
    {#each filtered as w, i (w.label)}
      {@const parts = splitPath(w.path)}
      <li
        role="option"
        aria-selected={i === cursor}
        class:active={i === cursor}
        style="--hue: {pathHue(w.path)}"
        on:click={() => pick(w)}
        on:keydown={(e) => e.key === 'Enter' && pick(w)}
        on:mouseenter={() => (cursor = i)}
      >
        <span class="band" />
        <span class="text">
          <span class="tail">{parts.tail}</span>
          <span class="lead">{elideLeft(parts.lead)}</span>
        </span>
      </li>
    {/each}

    {#if filtered.length === 0}
      <li class="empty" role="presentation">
        {windows.length === 0 ? '開いている窓がありません' : '一致する窓がありません'}
      </li>
    {/if}
  </ul>

  <div class="foot">↑↓ 選択 · Enter 前面へ · Esc 閉じる</div>
</div>

<style>
  .overlay {
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
    background: #1c1c1cf2;
    border: 1px solid #3d3d3d;
    border-radius: 10px;
    overflow: hidden;
    backdrop-filter: blur(12px);
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 12px 14px 8px;
  }
  .title {
    font-size: 11px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: #888;
  }
  .count {
    font-size: 11px;
    color: #5b5b5b;
  }

  input {
    margin: 0 12px 10px;
    padding: 7px 10px;
    background: #262626;
    border: 1px solid #3f3f3f;
    border-radius: 6px;
    color: #eee;
    font: inherit;
    font-size: 13px;
    outline: none;
  }
  input:focus {
    border-color: #4c9aff;
  }

  ul {
    flex: 1;
    margin: 0;
    padding: 0 8px;
    list-style: none;
    overflow-y: auto;
    min-height: 0;
  }

  li {
    display: flex;
    align-items: stretch;
    gap: 9px;
    padding: 6px 8px 6px 0;
    border-radius: 6px;
    cursor: pointer;
  }
  li.active {
    background: #33517a;
  }

  /* ファイラ窓の左端と同じ色を出す。色で窓とリストが結び付く。 */
  .band {
    width: 4px;
    flex: none;
    border-radius: 2px;
    background: hsl(var(--hue) 70% 55%);
  }

  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  /* 末尾のフォルダ名を主役に。上位階層は文脈として下に小さく。 */
  .tail {
    font-size: 14px;
    font-weight: 600;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lead {
    font-size: 10px;
    color: #7e7e7e;
    overflow: hidden;
    white-space: nowrap;
  }

  .empty {
    padding: 18px 10px;
    color: #666;
    font-size: 12px;
    justify-content: center;
    cursor: default;
  }

  .foot {
    flex: none;
    padding: 8px 14px;
    border-top: 1px solid #303030;
    font-size: 10px;
    color: #666;
  }
</style>
