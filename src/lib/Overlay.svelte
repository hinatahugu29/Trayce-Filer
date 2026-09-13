<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import type { UnlistenFn } from '@tauri-apps/api/event'
  import * as api from './api'
  import { splitPath, pathHue, elideLeft } from './api'
  import type { WindowInfo } from './api'
  import Workbench from './Workbench.svelte'

  const win = getCurrentWindow()

  let windows: WindowInfo[] = []
  let query = ''
  let cursor = 0
  let input: HTMLInputElement | null = null

  // 表示モード：'workbench'（俯瞰ワークベンチ）または 'list'（コンパクトリスト）
  let viewMode: 'workbench' | 'list' = 'workbench'

  // 📌 ピン留め（固定）状態。ONのときはカード選択時もオーバーレイが閉じない
  let pinned = false

  // 一時通知メッセージ
  let noteText: string | null = null
  let noteTimer: number | null = null

  function showNote(msg: string) {
    noteText = msg
    if (noteTimer !== null) clearTimeout(noteTimer)
    noteTimer = window.setTimeout(() => {
      noteText = null
      noteTimer = null
    }, 2800)
  }

  // 文字列で絞る（リスト表示時用）
  // 大文字小文字・全角半角は区別しない（ペインの絞り込みや検索と同じ規則）。
  $: filtered = windows.filter((w) => {
    const needle = api.foldForSearch(query)
    return [w.path, w.active_tab_label, ...w.panes.flatMap((pane) => [pane.path, pane.query])]
      .some((value) => api.foldForSearch(value).includes(needle))
  })
  $: if (cursor >= filtered.length) cursor = Math.max(0, filtered.length - 1)

  async function refresh() {
    windows = await api.listWindows()
  }

  // モード切り替え（サイズ・位置連動）
  async function switchMode(mode: 'workbench' | 'list') {
    if (viewMode === mode) return
    viewMode = mode
    try {
      await api.setOverlayMode(mode === 'workbench' ? 'workbench' : 'compact')
    } catch (e) {
      console.error('Failed to set overlay mode:', e)
    }
  }

  // ウィンドウ選択時
  async function pick(w: WindowInfo | undefined) {
    if (!w) return
    await api.focusWindow(w.label)
    if (!pinned) {
      await api.hideOverlay()
    } else {
      showNote(`「${splitPath(w.path).tail || w.path}」を前面に表示`)
    }
  }

  async function pickPane(w: WindowInfo, pane: api.WindowPaneInfo) {
    await api.focusPane(w.label, pane.id)
    if (!pinned) {
      await api.hideOverlay()
    } else {
      const name = splitPath(pane.path).tail || pane.path
      showNote(`「${pane.kind === 'search' && pane.query ? pane.query : name}」ペインを選択`)
      await refresh()
    }
  }

  async function pickTab(w: WindowInfo, tab: api.WindowTabInfo) {
    await api.focusTab(w.label, tab.id)
    if (!pinned) {
      await api.hideOverlay()
    } else {
      showNote('「' + tab.label + '」タブを選択')
      // 対象WebViewがタブを切り替え、ペイン構成を再送する短い猶予を置く。
      window.setTimeout(refresh, 40)
    }
  }

  // ウィンドウを閉じる
  async function handleCloseWindow(label: string) {
    try {
      await api.closeWindow(label)
      await refresh()
      showNote('ウィンドウを閉じました')
    } catch (e) {
      showNote(`閉じるのに失敗: ${e}`)
    }
  }

  function onKey(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
      api.hideOverlay()
      return
    }

    // Ctrl+P でピン留めトグル
    if (ev.ctrlKey && (ev.key === 'p' || ev.key === 'P')) {
      ev.preventDefault()
      pinned = !pinned
      showNote(pinned ? '📌 ピン留め: ON（画面を固定）' : 'ピン留め: OFF')
      return
    }

    if (viewMode === 'list') {
      if (ev.key === 'ArrowDown') {
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
  }

  let unlistenFocus: UnlistenFn | null = null

  onMount(async () => {
    await refresh()
    // 初期表示モードに応じてサイズ適用
    try {
      await api.setOverlayMode(viewMode === 'workbench' ? 'workbench' : 'compact')
    } catch {}

    if (viewMode === 'list') {
      input?.focus()
    }

    // ホットキーで再表示されるたびに中身を取り直す
    unlistenFocus = await win.onFocusChanged(async ({ payload }) => {
      if (payload) {
        await refresh()
        query = ''
        cursor = 0
        if (viewMode === 'list') input?.focus()
      }
    })
  })

  onDestroy(() => {
    unlistenFocus?.()
    if (noteTimer !== null) clearTimeout(noteTimer)
  })
</script>

<svelte:window on:keydown={onKey} />

<div class="overlay" class:is-workbench={viewMode === 'workbench'}>
  <!-- グローバルヘッダーバー -->
  <header class="top-nav">
    <div class="nav-left">
      <div class="brand">
        <span class="brand-icon">🗂️</span>
        <span class="brand-title">窓ナビゲーター</span>
        <span class="win-count-badge">{windows.length}</span>
      </div>

      <!-- モード切り替えタブ -->
      <div class="mode-tabs" role="tablist">
        <button
          type="button"
          role="tab"
          aria-selected={viewMode === 'workbench'}
          class="tab-btn"
          class:active={viewMode === 'workbench'}
          on:click={() => switchMode('workbench')}
          title="複数ウィンドウを全画面でタイル俯瞰＆擬似ペイン構築"
        >
          ⛶ 俯瞰ワークベンチ
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={viewMode === 'list'}
          class="tab-btn"
          class:active={viewMode === 'list'}
          on:click={() => switchMode('list')}
          title="コンパクトなクイックスイッチャー（右端リスト）"
        >
          📋 リスト
        </button>
      </div>
    </div>

    <div class="nav-right">
      <!-- 📌 ピン留めトグルボタン -->
      <button
        type="button"
        class="pin-toggle"
        class:pinned
        title={pinned ? 'ピン留め解除 (Ctrl+P)' : '画面を固定してファイル整理 (Ctrl+P)'}
        on:click={() => {
          pinned = !pinned
          showNote(pinned ? '📌 ピン留め: ON（画面を固定）' : 'ピン留め: OFF')
        }}
      >
        <span class="pin-icon">{pinned ? '📌' : '📍'}</span>
        <span class="pin-label">{pinned ? '固定中' : '固定'}</span>
      </button>

      <!-- 閉じるボタン -->
      <button
        type="button"
        class="close-window-btn"
        title="オーバーレイを閉じる (Esc)"
        on:click={() => api.hideOverlay()}
      >
        ✕
      </button>
    </div>
  </header>

  <!-- トースト通知 -->
  {#if noteText}
    <div class="toast-note">
      {noteText}
    </div>
  {/if}

  <!-- メインコンテンツ -->
  <main class="content-body">
    {#if viewMode === 'workbench'}
      <Workbench
        {windows}
        {pinned}
        onSelectWindow={pick}
        onSelectPane={pickPane}
        onSelectTab={pickTab}
        onCloseWindow={handleCloseWindow}
        onNote={showNote}
      />
    {:else}
      <!-- 従来のコンパクトリスト表示 -->
      <div class="list-container">
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
              <div class="window-row">
                <span class="band" />
                <span class="text">
                  <span class="tail">{parts.tail || w.path}</span>
                  <span class="lead">{w.active_tab_label}{w.tab_count > 1 ? ` · 他${w.tab_count - 1}タブ` : ''}</span>
                </span>
                <span class="pane-count">{w.panes.length}ペイン</span>
              </div>
              <div class="pane-children">
                {#each w.panes as pane, paneIndex (`${w.label}-${pane.id}`)}
                  {@const paneParts = splitPath(pane.path)}
                  <button
                    type="button"
                    class:current={pane.is_active}
                    title={pane.path}
                    on:click|stopPropagation={() => pickPane(w, pane)}
                  >
                    <span class="pane-kind">{pane.kind === 'search' ? '⌕' : '▣'}</span>
                    <span class="pane-number">{paneIndex + 1}</span>
                    <span class="pane-text">
                      <strong>{pane.kind === 'search' && pane.query ? pane.query : paneParts.tail || pane.path}</strong>
                      <small>{pane.kind === 'search' ? `検索 · ${elideLeft(pane.path, 28)}` : elideLeft(pane.path, 32)}</small>
                    </span>
                  </button>
                {/each}
              </div>
            </li>
          {/each}

          {#if filtered.length === 0}
            <li class="empty" role="presentation">
              {windows.length === 0 ? '開いている窓がありません' : '一致する窓がありません'}
            </li>
          {/if}
        </ul>
      </div>
    {/if}
  </main>

  <!-- フッターステータスバー -->
  <footer class="bottom-bar">
    <div class="shortcuts-guide">
      {#if viewMode === 'workbench'}
        <span>カードクリック: 前面へ</span>
        <span class="sep">·</span>
        <span>👁️: 一時除外で疑似ペイン化</span>
        <span class="sep">·</span>
        <span>通常ペインへD&D: ファイル移動 (Ctrl+D&Dでコピー)</span>
        <span class="sep">·</span>
        <span>Ctrl+P: ピン留め</span>
        <span class="sep">·</span>
        <span>Esc: 閉じる</span>
      {:else}
        <span>↑↓: 選択</span>
        <span class="sep">·</span>
        <span>Enter: 前面へ</span>
        <span class="sep">·</span>
        <span>Ctrl+P: ピン留め</span>
        <span class="sep">·</span>
        <span>Esc: 閉じる</span>
      {/if}
    </div>
  </footer>
</div>

<style>
  .overlay {
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
    background: #18181af2;
    border: 1px solid #3d3d44;
    border-radius: 11px;
    overflow: hidden;
    backdrop-filter: blur(16px);
    color: #e5e5e5;
    position: relative;
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
  }

  /* ナビゲーションヘッダー */
  .top-nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    background: #141416;
    border-bottom: 1px solid #2d2d33;
    flex: none;
    gap: 12px;
  }

  .nav-left {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .brand-icon {
    font-size: 16px;
  }
  .brand-title {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #f3f3f3;
  }
  .win-count-badge {
    padding: 1px 6px;
    background: #2a2a32;
    border: 1px solid #3d3d48;
    border-radius: 10px;
    font-size: 10px;
    color: #8bb2ff;
    font-weight: 600;
  }

  /* モード切り替えタブ */
  .mode-tabs {
    display: flex;
    background: #202026;
    border: 1px solid #33333d;
    border-radius: 6px;
    padding: 2px;
    gap: 2px;
  }
  .tab-btn {
    padding: 4px 10px;
    background: none;
    border: none;
    border-radius: 4px;
    color: #888;
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .tab-btn:hover {
    color: #eee;
  }
  .tab-btn.active {
    background: #3b82f6;
    color: #fff;
    font-weight: 600;
    box-shadow: 0 2px 6px rgba(59, 130, 246, 0.35);
  }

  .nav-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* ピン留めボタン */
  .pin-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: #24242a;
    border: 1px solid #383844;
    border-radius: 6px;
    color: #aaa;
    font-size: 12px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .pin-toggle:hover {
    background: #2d2d36;
    color: #fff;
  }
  .pin-toggle.pinned {
    background: #1e3a8a;
    border-color: #3b82f6;
    color: #93c5fd;
    font-weight: 600;
  }

  .close-window-btn {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    border-radius: 6px;
    color: #999;
    font-size: 13px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }
  .close-window-btn:hover {
    background: #383842;
    color: #fff;
  }

  /* トースト通知 */
  .toast-note {
    position: absolute;
    top: 56px;
    left: 50%;
    transform: translateX(-50%);
    background: #2563eb;
    color: #fff;
    padding: 6px 14px;
    border-radius: 20px;
    font-size: 12px;
    font-weight: 500;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.45);
    z-index: 100;
    pointer-events: none;
    animation: fadeIn 0.2s ease;
  }
  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translate(-50%, -6px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }

  /* メインコンテンツ領域 */
  .content-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* リストモード時のコンテナ */
  .list-container {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    padding-top: 10px;
  }

  input {
    margin: 0 12px 10px;
    padding: 8px 11px;
    background: #24242a;
    border: 1px solid #3c3c46;
    border-radius: 6px;
    color: #eee;
    font: inherit;
    font-size: 13px;
    outline: none;
  }
  input:focus {
    border-color: #3b82f6;
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
    flex-direction: column;
    padding: 5px;
    border-radius: 6px;
    cursor: pointer;
  }
  .window-row { display: flex; align-items: stretch; min-width: 0; gap: 10px; padding: 3px 5px 6px 0; }
  li.active {
    background: #274068;
  }

  .band {
    width: 4px;
    flex: none;
    border-radius: 2px;
    background: hsl(var(--hue) 75% 55%);
  }

  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .pane-count { margin-left: auto; align-self: center; color: #78818c; font-size: 9px; white-space: nowrap; }
  .pane-children { display: flex; flex-direction: column; gap: 2px; margin-left: 13px; padding-left: 8px; border-left: 1px solid #3b414a; }
  .pane-children button { display: flex; min-width: 0; align-items: center; gap: 6px; border: 1px solid transparent; border-radius: 4px; background: transparent; padding: 5px 6px; color: #aeb5bd; text-align: left; cursor: pointer; }
  .pane-children button:hover { background: #303743; color: #fff; }
  .pane-children button.current { border-color: #486887; background: #27384c; }
  .pane-kind { width: 14px; flex: none; color: #76a8db; text-align: center; }
  .pane-number { width: 13px; height: 13px; flex: none; border-radius: 7px; background: #383e47; color: #9ba5af; font-size: 8px; line-height: 13px; text-align: center; }
  .pane-text { display: flex; min-width: 0; flex: 1; flex-direction: column; }
  .pane-text strong, .pane-text small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pane-text strong { color: inherit; font-size: 11px; font-weight: 500; }
  .pane-text small { color: #737d86; font-size: 9px; }
  .tail {
    font-size: 14px;
    font-weight: 600;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lead {
    font-size: 11px;
    color: #8a8a92;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty {
    padding: 24px 10px;
    color: #777;
    font-size: 12px;
    justify-content: center;
    cursor: default;
  }

  /* フッター */
  .bottom-bar {
    flex: none;
    padding: 8px 14px;
    background: #141416;
    border-top: 1px solid #29292f;
    font-size: 11px;
    color: #777;
  }
  .shortcuts-guide {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .sep {
    color: #444;
  }
</style>
