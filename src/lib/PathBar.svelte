<script lang="ts">
  import { tick } from 'svelte'
  import * as api from './api'
  import { splitPath, pathHue } from './api'

  export let path: string
  /** 上位階層やアドレスバーからの移動先。 */
  export let onNavigate: (path: string) => void = () => {}
  export let showHidden = false

  $: ({ lead, tail } = splitPath(path))
  $: hue = pathHue(path)

  /** 上位階層をパンくずに割る。各要素はそこまでの絶対パスを持つ。 */
  $: crumbs = (() => {
    if (!lead) return []
    const parts = lead.replace(/[\\/]+$/, '').split(/[\\/]/)
    let acc = ''
    return parts.map((part, i) => {
      acc = i === 0 ? part + '\\' : acc + part + '\\'
      return { label: part || '\\', path: acc }
    })
  })()

  // ---- アドレスバー ----
  //
  // 常時テキスト欄にすると、このアプリの肝である「末尾フォルダ名が主役」が
  // 崩れる。普段はパンくずを見せ、必要な時だけ編集に切り替える。
  let editing = false
  let draft = ''
  let input: HTMLInputElement | null = null
  let suggestions: string[] = []
  let picked = -1

  export async function beginEdit() {
    draft = path
    editing = true
    suggestions = []
    picked = -1
    await tick()
    input?.select()
  }

  function cancel() {
    editing = false
    suggestions = []
  }

  async function refreshSuggestions() {
    picked = -1
    suggestions = draft.trim() ? await api.completePath(draft, showHidden) : []
  }

  function commit(target = draft) {
    const value = target.trim()
    editing = false
    suggestions = []
    if (value && value !== path) onNavigate(value)
  }

  function onKey(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
      ev.preventDefault()
      cancel()
    } else if (ev.key === 'ArrowDown') {
      ev.preventDefault()
      picked = Math.min(picked + 1, suggestions.length - 1)
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault()
      picked = Math.max(picked - 1, -1)
    } else if (ev.key === 'Enter') {
      ev.preventDefault()
      // 候補を選んでいればそれ、していなければ打った内容をそのまま使う。
      commit(picked >= 0 ? suggestions[picked] : draft)
    } else if (ev.key === 'Tab' && suggestions.length) {
      // Tab は先頭候補で補完。移動はせず、続けて打てるようにする。
      ev.preventDefault()
      draft = suggestions[picked >= 0 ? picked : 0] + '\\'
      refreshSuggestions()
    }
  }
</script>

<!--
  このアプリの中核。窓が何枚も重なった状態から目的の窓を見つける手掛かりは
  「末尾のフォルダ名」と「色」なので、その2つを最優先で読ませる。
  色帯を左端に置いているのは、窓が右方向に隠れても左端は残りやすいため。
-->
<header style="--hue: {hue}">
  <div class="band" />
  <div class="text">
    {#if editing}
      <div class="address">
        <!-- svelte-ignore a11y-autofocus -->
        <input
          bind:this={input}
          bind:value={draft}
          autofocus
          spellcheck="false"
          aria-label="パスを入力"
          on:input={refreshSuggestions}
          on:keydown={onKey}
          on:blur={cancel}
        />
        {#if suggestions.length}
          <ul class="suggest">
            {#each suggestions as s, i}
              <li>
                <button
                  type="button"
                  class:on={i === picked}
                  on:mousedown|preventDefault={() => commit(s)}
                  on:mouseenter={() => (picked = i)}
                >
                  {s}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {:else}
      <nav class="lead">
        {#each crumbs as crumb, i}
          <button type="button" on:click={() => onNavigate(crumb.path)}>{crumb.label}</button>
          {#if i < crumbs.length - 1}<span class="sep">›</span>{/if}
        {/each}
        <button
          type="button"
          class="edit"
          title="パスを編集 (Ctrl+L)"
          on:click={beginEdit}
        >
          ✎
        </button>
      </nav>
      <button type="button" class="tail" title="クリックしてパスを編集" on:click={beginEdit}>
        {tail}
      </button>
    {/if}
  </div>
  <slot />
</header>

<style>
  /* 狭いペイン（分割時など）では、操作ボタンの列を下の段へ回す。
     同じ段に詰めると、ボタンの幅だけでパスの欄が潰れ、パンくずが1段1要素に折り返していた。 */
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: stretch;
    column-gap: 12px;
    background: #202020;
    border-bottom: 1px solid #383838;
    flex: none;
  }

  /* 窓が重なっても左端は残りやすい。色だけで「どのプロジェクトか」を当てる手掛かり。 */
  .band {
    width: 6px;
    flex: none;
    background: hsl(var(--hue) 70% 55%);
  }

  /* パスの欄はこの幅を確保できない時、ボタンの列を折り返させる。 */
  .text {
    flex: 1 1 240px;
    min-width: 0;
    padding: 8px 4px 10px 6px;
    position: relative;
  }

  .lead {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    font-size: 11px;
    line-height: 1.4;
    color: #7a7a7a;
  }
  .lead button {
    background: none;
    border: 0;
    padding: 1px 3px;
    font: inherit;
    color: inherit;
    cursor: pointer;
    border-radius: 3px;
  }
  .lead button:hover {
    background: #333;
    color: #ccc;
  }
  .lead .edit {
    margin-left: 4px;
    color: #5f5f5f;
  }
  .sep {
    color: #555;
  }

  /* 末尾のフォルダ名。ここが一番大きい。 */
  .tail {
    display: block;
    width: 100%;
    margin-top: 1px;
    padding: 0;
    background: none;
    border: 0;
    text-align: left;
    font: inherit;
    font-size: 21px;
    font-weight: 650;
    line-height: 1.15;
    color: #fff;
    letter-spacing: -0.01em;
    cursor: text;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .address input {
    width: 100%;
    box-sizing: border-box;
    padding: 6px 9px;
    background: #171717;
    border: 1px solid #4c9aff;
    border-radius: 5px;
    color: #eee;
    font-family: Consolas, monospace;
    font-size: 14px;
    outline: none;
  }

  .suggest {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    z-index: 5;
    margin: 4px 0 0;
    padding: 4px;
    list-style: none;
    max-height: 260px;
    overflow-y: auto;
    background: #232323;
    border: 1px solid #444;
    border-radius: 6px;
    box-shadow: 0 8px 24px #000a;
  }
  .suggest button {
    display: block;
    width: 100%;
    padding: 4px 8px;
    background: none;
    border: 0;
    border-radius: 4px;
    text-align: left;
    font-family: Consolas, monospace;
    font-size: 11.5px;
    color: #c8c8c8;
    cursor: default;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .suggest button.on {
    background: #33517a;
    color: #fff;
  }
</style>
