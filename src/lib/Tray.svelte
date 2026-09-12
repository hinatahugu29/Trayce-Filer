<script lang="ts">
  import { onMount } from 'svelte'
  import { splitPath } from './api'
  import * as api from './api'

  export let items: string[] = []
  export let onNavigate: (path: string) => void
  export let onRemove: (path: string) => void = () => {}
  export let onClear: () => void = () => {}

  let exist: boolean[] = []
  let refreshToken = 0

  async function refresh() {
    const token = ++refreshToken
    const result = await api.pathsExist(items)
    if (token === refreshToken) exist = result
  }

  $: if (items) refresh()
  onMount(refresh)

  function reveal(path: string) {
    const { lead } = splitPath(path)
    onNavigate(lead || path)
  }
</script>

<div class="tray">
  <div class="summary">
    <span>{items.length ? `${items.length} 件を収集中` : 'Alt+Click で項目を集める'}</span>
    {#if items.length}
      <button type="button" on:click={onClear}>すべて外す</button>
    {/if}
  </div>

  <div class="items">
    {#each items as path, i (api.pathIdentity(path))}
      {@const parts = splitPath(path)}
      <div class="item" class:missing={exist[i] === false} title={path}>
        <button class="open" type="button" on:dblclick={() => reveal(path)}>
          <span class="icon">{exist[i] === false ? '⚠' : '◈'}</span>
          <span class="text">
            <strong>{parts.tail || path}</strong>
            <small>{exist[i] === false ? '見つかりません' : parts.lead}</small>
          </span>
        </button>
        <button class="remove" type="button" title="トレイから外す" on:click={() => onRemove(path)}>×</button>
      </div>
    {/each}
  </div>
</div>

<style>
  .tray { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .summary { display: flex; align-items: center; gap: 6px; padding: 7px 8px; border-bottom: 1px solid #2c2c2c; color: #8f9aaa; font-size: 10.5px; }
  .summary span { flex: 1; }
  button { border: 0; background: none; color: inherit; font: inherit; cursor: pointer; }
  .summary button { color: #8fbce8; }
  .items { flex: 1; min-height: 0; overflow: auto; padding: 4px; }
  .item { display: flex; align-items: center; border-radius: 4px; color: #ccc; }
  .item:hover { background: #252b31; }
  .item.missing { opacity: .58; }
  .open { display: flex; align-items: center; gap: 7px; flex: 1; min-width: 0; padding: 6px; text-align: left; }
  .icon { flex: none; color: #65c5a5; }
  .missing .icon { color: #d7a65b; }
  .text { display: flex; flex-direction: column; min-width: 0; }
  strong, small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  strong { font-size: 11.5px; font-weight: 500; }
  small { color: #6f7882; font-size: 9.5px; direction: rtl; text-align: left; }
  .remove { flex: none; width: 24px; height: 28px; color: #68717a; font-size: 14px; }
  .remove:hover { color: #ff9b9b; }
</style>
