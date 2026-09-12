<script lang="ts">
  import { onMount } from 'svelte'
  import { splitPath } from './api'
  import * as api from './api'

  export let items: string[] = []
  export let onNavigate: (path: string) => void
  export let onRemove: (path: string) => void = () => {}
  export let onRemoveMany: (paths: string[]) => void = () => {}
  export let onClear: () => void = () => {}
  export let onTransfer: (paths: string[], moveFiles: boolean) => void = () => {}
  export let transferBusy = false
  export let destinationPath = ''

  let exist: boolean[] = []
  let refreshToken = 0
  let selectedKeys = new Set<string>()
  let anchor = 0
  let lastItemSignature = ''

  $: itemSignature = items.map(api.pathIdentity).join('\n')
  $: if (itemSignature !== lastItemSignature) {
    lastItemSignature = itemSignature
    const valid = new Set(items.map(api.pathIdentity))
    selectedKeys = new Set([...selectedKeys].filter((key) => valid.has(key)))
    anchor = Math.min(anchor, Math.max(0, items.length - 1))
  }

  $: selectedPaths = items.filter((path) => selectedKeys.has(api.pathIdentity(path)))
  $: transferPaths = selectedPaths.length ? selectedPaths : items

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

  function selectItem(path: string, index: number, ev: MouseEvent) {
    const key = api.pathIdentity(path)
    if (ev.shiftKey) {
      const [from, to] = anchor <= index ? [anchor, index] : [index, anchor]
      selectedKeys = new Set(items.slice(from, to + 1).map(api.pathIdentity))
    } else if (ev.ctrlKey) {
      selectedKeys.has(key) ? selectedKeys.delete(key) : selectedKeys.add(key)
      selectedKeys = new Set(selectedKeys)
      anchor = index
    } else {
      selectedKeys = new Set([key])
      anchor = index
    }
  }

  function onKeyDown(ev: KeyboardEvent) {
    if (ev.ctrlKey && ev.key.toLowerCase() === 'a') {
      ev.preventDefault()
      selectedKeys = new Set(items.map(api.pathIdentity))
    } else if (ev.key === 'Escape') {
      selectedKeys = new Set()
    }
  }
</script>

<div class="tray">
  <div class="summary">
    <span>{items.length ? `${items.length} 件を収集中` : 'Alt+Click で項目を集める'}</span>
    {#if items.length}
      <button type="button" on:click={onClear}>すべて外す</button>
    {/if}
  </div>

  {#if items.length}
    <div class="destination" title={destinationPath}>
      <span>送り先</span>
      <strong>{splitPath(destinationPath).tail || destinationPath}</strong>
    </div>
    <div class="actions">
      <button type="button" disabled={transferBusy} on:click={() => onTransfer(transferPaths, false)}>
        {selectedPaths.length ? `選択 ${selectedPaths.length}件をコピー` : 'すべてコピー'}
      </button>
      <button type="button" disabled={transferBusy} on:click={() => onTransfer(transferPaths, true)}>
        {selectedPaths.length ? `選択 ${selectedPaths.length}件を移動` : 'すべて移動'}
      </button>
    </div>
    {#if selectedPaths.length}
      <button class="remove-selected" type="button" on:click={() => onRemoveMany(selectedPaths)}>
        選択 {selectedPaths.length}件をトレイから外す
      </button>
    {/if}
  {/if}

  <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
  <div class="items" tabindex="0" role="listbox" aria-label="収集トレイ" on:keydown={onKeyDown}>
    {#each items as path, i (api.pathIdentity(path))}
      {@const parts = splitPath(path)}
      <div
        class="item"
        class:selected={selectedKeys.has(api.pathIdentity(path))}
        class:missing={exist[i] === false}
        title={path}
        role="option"
        aria-selected={selectedKeys.has(api.pathIdentity(path))}
      >
        <button class="open" type="button" on:click={(e) => selectItem(path, i, e)} on:dblclick={() => reveal(path)}>
          <span class="icon">{exist[i] === false ? '⚠' : '◈'}</span>
          <span class="text">
            <strong>{parts.tail || path}</strong>
            <small>{exist[i] === false ? '見つかりません' : parts.lead}</small>
          </span>
        </button>
        <button class="remove" type="button" title="トレイから外す" on:click|stopPropagation={() => onRemove(path)}>×</button>
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
  .actions { display: flex; gap: 5px; padding: 6px 7px; border-bottom: 1px solid #2c2c2c; }
  .actions button { flex: 1; padding: 5px 3px; border: 1px solid #345266; border-radius: 3px; color: #b9d9ee; background: #1d303c; font-size: 10px; }
  .actions button:hover:not(:disabled) { background: #25475a; }
  .actions button:disabled { opacity: .45; cursor: default; }
  .destination { display: flex; gap: 6px; align-items: center; padding: 5px 8px 0; color: #67747c; font-size: 9.5px; }
  .destination strong { color: #9bb9c8; font-size: 10.5px; font-weight: 500; }
  .remove-selected { margin: 0 7px 6px; padding: 3px; border: 1px solid #4c3b3b; border-radius: 3px; color: #bd8f8f; font-size: 9.5px; }
  .remove-selected:hover { background: #382727; color: #efb2b2; }
  .item { display: flex; align-items: center; border-radius: 4px; color: #ccc; }
  .item:hover { background: #252b31; }
  .item.selected { background: #294b55; box-shadow: inset 2px 0 #67c8da; }
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
