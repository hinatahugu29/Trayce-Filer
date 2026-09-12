<script lang="ts">
  import PathRow from './PathRow.svelte'
  import * as api from './api'
  import { relativeTime } from './api'
  import type { HistoryEntry } from './api'

  export let currentPath: string
  export let onNavigate: (path: string) => void
  export let onChanged: () => void = () => {}

  export let items: HistoryEntry[] = []
  export let missing: boolean[] = []

  let query = ''

  // 履歴は増えるので絞り込みが要る。パス全体に当てる。
  $: filtered = items
    .map((entry, i) => ({ entry, exists: missing[i] !== false }))
    .filter(({ entry }) => entry.path.toLowerCase().includes(query.toLowerCase()))

  async function clear() {
    await api.clearHistory()
    onChanged()
  }
</script>

<div class="wrap">
  <div class="head">
    <input bind:value={query} placeholder="絞り込み…" spellcheck="false" />
    <button type="button" title="履歴を消す" on:click={clear} disabled={items.length === 0}>
      消去
    </button>
  </div>

  <div class="list">
    {#each filtered as { entry, exists } (entry.path)}
      <div
        class="item"
        role="button"
        tabindex="-1"
        on:click={() => onNavigate(entry.path)}
        on:keydown={(e) => e.key === 'Enter' && onNavigate(entry.path)}
      >
        <PathRow
          path={entry.path}
          meta={relativeTime(entry.at)}
          current={entry.path === currentPath}
          missing={!exists}
        />
      </div>
    {/each}

    {#if filtered.length === 0}
      <p class="empty">
        {items.length === 0 ? 'まだ履歴がありません' : '一致する履歴がありません'}
      </p>
    {/if}
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: #191919;
  }

  .head {
    display: flex;
    gap: 5px;
    flex: none;
    padding: 6px 6px 4px;
  }
  input {
    flex: 1;
    min-width: 0;
    padding: 4px 7px;
    background: #242424;
    border: 1px solid #3a3a3a;
    border-radius: 4px;
    color: #ddd;
    font: inherit;
    font-size: 11px;
    outline: none;
  }
  input:focus {
    border-color: #4c9aff;
  }
  .head button {
    flex: none;
    padding: 0 7px;
    background: #2b2b2b;
    border: 1px solid #3a3a3a;
    border-radius: 4px;
    color: #999;
    font-size: 10px;
    cursor: pointer;
  }
  .head button:hover:not(:disabled) {
    background: #4a2020;
    border-color: #6a3030;
    color: #ff9b9b;
  }
  .head button:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 6px 6px;
  }
  .item {
    cursor: default;
  }

  .empty {
    margin: 24px 10px;
    text-align: center;
    font-size: 10.5px;
    color: #5f5f5f;
  }
</style>
