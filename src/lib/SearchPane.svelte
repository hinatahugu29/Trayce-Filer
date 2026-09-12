<script lang="ts">
  import type { PaneKind, SavedSearchState, Settings } from './api'
  import { splitPath } from './api'
  import { matchAction } from './shortcuts'

  export let directoryPath: string
  export let search: SavedSearchState
  export let settings: Settings
  export let closable = false
  export let active = false
  export let multi = false
  export let keyboardTarget = false
  export let onSearchChange: (search: SavedSearchState) => void = () => {}
  export let onKindChange: (kind: PaneKind) => void = () => {}
  export let onSplit: (path: string) => void = () => {}
  export let onClose: () => void = () => {}
  export let onActivate: () => void = () => {}
  export let onHoverChange: (hovered: boolean) => void = () => {}
  export let onNote: (message: string) => void = () => {}

  $: scope = search.scopePaths[0] || directoryPath
  $: scopeLabel = splitPath(scope).tail || scope

  export function currentPath(): string {
    return directoryPath
  }

  export async function reload() {
    // The search worker is introduced in Phase 11.
  }

  export async function acceptDrop(_paths: string[]) {
    onNote('検索ペインはコピー・移動先にはできません')
  }

  function updateQuery(query: string) {
    onSearchChange({ ...search, query })
  }

  function onPaneKey(ev: KeyboardEvent) {
    const el = ev.target as HTMLElement | null
    if (!keyboardTarget || (el && (el.tagName === 'INPUT' || el.isContentEditable))) return
    switch (matchAction(ev, settings.shortcuts)) {
      case 'hoverClosePane':
        if (!closable) return
        ev.preventDefault()
        onClose()
        break
      case 'hoverSplitPane':
        ev.preventDefault()
        onSplit(directoryPath)
        break
      default:
        break
    }
  }
</script>

<svelte:window on:keydown={onPaneKey} />

<section
  class="search-pane"
  class:active
  class:multi
  class:keyboard-target={keyboardTarget}
  on:pointerdown={onActivate}
  on:pointerenter={() => onHoverChange(true)}
  on:pointerleave={() => onHoverChange(false)}
>
  <header>
    <div class="identity">
      <span class="kind">検索</span>
      <span class="scope" title={scope}>対象: {scopeLabel}</span>
    </div>
    <div class="actions">
      <button type="button" title="通常のフォルダペインに戻す" on:click={() => onKindChange('directory')}>▣</button>
      <button type="button" title="このペインを左右に分割" on:click={() => onSplit(directoryPath)}>⫿</button>
      {#if closable}
        <button type="button" title="このペインを閉じる" on:click={onClose}>✕</button>
      {/if}
    </div>
  </header>

  <div class="query-row">
    <input
      value={search.query}
      placeholder="検索する名前やパス"
      spellcheck="false"
      aria-label="検索語"
      on:input={(event) => updateQuery(event.currentTarget.value)}
    />
    <button type="button" disabled title="検索処理は次の実装段階で有効になります">検索</button>
  </div>

  <div class="empty">
    <span class="mark">⌕</span>
    <strong>検索ペイン</strong>
    <p>検索条件を保持する外枠ができました。</p>
    <small>検索の実行と結果表示は次の段階で接続します。</small>
  </div>
</section>

<style>
  .search-pane {
    display: flex;
    flex: 1;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    background: #191b1d;
    color: #ddd;
  }
  .search-pane.multi:not(.active) { background: #151719; }
  .search-pane.multi.active { box-shadow: inset 0 2px 0 #4c9aff; }
  .search-pane.multi.keyboard-target {
    box-shadow: inset 0 2px 0 #63cfad, inset 0 0 0 1px rgba(99, 207, 173, 0.22);
  }
  header {
    display: flex;
    min-height: 64px;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid #303438;
    background: #202326;
    padding: 0 9px 0 14px;
  }
  .identity { display: flex; min-width: 0; flex-direction: column; gap: 4px; }
  .kind { color: #f0f0f0; font-size: 18px; font-weight: 700; }
  .scope { overflow: hidden; color: #8f9ba5; font-size: 11px; text-overflow: ellipsis; white-space: nowrap; }
  .actions { display: flex; gap: 4px; }
  button {
    height: 24px;
    border: 1px solid #454b50;
    border-radius: 4px;
    background: #303438;
    color: #b8c0c7;
    cursor: pointer;
  }
  button:disabled { cursor: default; opacity: 0.45; }
  .actions button { width: 26px; padding: 0; }
  .query-row { display: flex; gap: 6px; border-bottom: 1px solid #2b2f32; padding: 7px 10px; }
  input {
    min-width: 0;
    flex: 1;
    border: 1px solid #3c444a;
    border-radius: 4px;
    outline: none;
    background: #22262a;
    color: #eee;
    padding: 6px 9px;
  }
  input:focus { border-color: #63cfad; box-shadow: 0 0 0 1px rgba(99, 207, 173, 0.2); }
  .empty {
    display: grid;
    flex: 1;
    place-content: center;
    justify-items: center;
    color: #7e8992;
    text-align: center;
  }
  .empty .mark { margin-bottom: 8px; color: #63cfad; font-size: 36px; }
  .empty strong { color: #c8ced3; }
  .empty p { margin: 8px 0 3px; }
  .empty small { color: #68727a; }
</style>
