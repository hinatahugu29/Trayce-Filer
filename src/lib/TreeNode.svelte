<script lang="ts">
  import { onMount } from 'svelte'
  import * as api from './api'
  import { joinPath } from './api'
  import type { Entry } from './api'
  import Self from './TreeNode.svelte'
  import { armMiddleClick, onMiddleClick } from './middleclick'

  export let path: string
  export let name: string
  export let depth = 0
  /** 現在ペインが開いている場所。ここまでの経路は自動で開く。 */
  export let currentPath: string
  /** 自動展開すべき祖先パスの集合。 */
  export let autoOpen: Set<string>
  export let onNavigate: (path: string) => void
  /** 中クリック用。いまのペインを動かさず、その場所を隣のペインで開く。 */
  export let onOpenBeside: (path: string) => void = () => {}
  /** 一覧と揃える。ツリーだけ隠しフォルダが見えると混乱する。 */
  export let showHidden = false

  let expanded = false
  let children: Entry[] | null = null
  let loading = false
  let error = false

  /**
   * ユーザーが手で開閉した後は、自動展開に上書きさせない。
   *
   * これが無いと、現在地の祖先（`C:\` など）は常に autoOpen に入り続けるため、
   * 手で畳んでも次の再評価で即座に開き直ってしまう
   * （`expanded` を書き換えるたび、その変更自体がこの下のリアクティブブロックを
   * 再トリガーする）。畳んだ意思は尊重する。
   */
  let userToggled = false

  async function load() {
    if (children || loading) return
    loading = true
    try {
      children = await api.listSubdirs(path, showHidden)
      error = false
    } catch {
      // 権限が無いフォルダなどはここに来る。潰さずに印だけ出す。
      children = []
      error = true
    } finally {
      loading = false
    }
  }

  async function toggle() {
    userToggled = true
    expanded = !expanded
    if (expanded) await load()
  }

  /**
   * 名前側も一方向の「開く」ではなく、同じ行をもう一度押せば畳めるようにする。
   * 移動と開閉を同時に行うが、userToggled を立てるため現在地の自動展開に
   * 直後から押し戻されることはない。
   */
  async function activate() {
    onNavigate(path)
    await toggle()
  }

  // 現在地までの経路にいるなら勝手に開く。
  // 「あの辺にあったはず」を辿れるようにするのがツリーの役目なので、
  // 開いている場所が閉じたままだと意味がない。
  // ただしユーザーが一度でも手で操作したノードは、以後この自動展開の対象から外す。
  $: if (autoOpen.has(path) && !expanded && !userToggled) {
    expanded = true
    load()
  }

  let rowEl: HTMLElement | null = null

  // 表示設定が変わったら読み直す。キャッシュを持っているので明示的に捨てる。
  let lastShowHidden = showHidden
  $: if (showHidden !== lastShowHidden) {
    lastShowHidden = showHidden
    children = null
    if (expanded) load()
  }

  $: isCurrent = currentPath === path

  // 現在地が画面外のままだと、ツリーを出している意味が無い。
  // 深い階層に移動した時ほど効く。
  $: if (isCurrent && rowEl) {
    rowEl.scrollIntoView({ block: 'nearest' })
  }

  onMount(() => {
    if (autoOpen.has(path)) load()
  })
</script>

<div class="node">
  <div
    class="row"
    class:current={isCurrent}
    bind:this={rowEl}
    style="padding-left: {depth * 12 + 6}px"
    role="treeitem"
    aria-selected={isCurrent}
    aria-expanded={expanded}
    tabindex="-1"
    on:click={activate}
    on:keydown={(e) => e.key === 'Enter' && activate()}
    on:mousedown={armMiddleClick}
    on:auxclick={onMiddleClick(() => onOpenBeside(path))}
  >
    <button
      class="twisty"
      type="button"
      tabindex="-1"
      aria-label={expanded ? '閉じる' : '開く'}
      on:click|stopPropagation={toggle}
    >
      {expanded ? '▾' : '▸'}
    </button>
    <span class="name" title={path}>{name}</span>
    {#if error}<span class="denied" title="読み取れません">!</span>{/if}
  </div>

  {#if expanded && children}
    {#each children as child (child.name)}
      <Self
        path={joinPath(path, child.name)}
        name={child.name}
        depth={depth + 1}
        {currentPath}
        {autoOpen}
        {onNavigate}
        {onOpenBeside}
        {showHidden}
      />
    {/each}
    {#if children.length === 0 && !error}
      <div class="empty" style="padding-left: {(depth + 1) * 12 + 20}px">（なし）</div>
    {/if}
  {/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 22px;
    padding-right: 6px;
    font-size: 12px;
    cursor: default;
    user-select: none;
    white-space: nowrap;
  }
  .row:hover {
    background: #2b2b2b;
  }
  /* 今いる場所。ツリー上で自分の位置が分かることが肝心。 */
  .row.current {
    background: #2d4a6b;
  }
  .row.current .name {
    color: #fff;
    font-weight: 600;
  }

  .twisty {
    width: 15px;
    flex: none;
    padding: 0;
    background: none;
    border: 0;
    color: #8c8c8c;
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }
  .twisty:hover {
    color: #ccc;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    color: #c8c8c8;
  }

  .denied {
    flex: none;
    color: #a35;
    font-size: 10px;
  }

  .empty {
    height: 20px;
    font-size: 10px;
    color: #5a5a5a;
    line-height: 20px;
  }
</style>
