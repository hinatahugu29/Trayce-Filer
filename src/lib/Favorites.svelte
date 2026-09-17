<script lang="ts">
  import PathRow from './PathRow.svelte'
  import * as api from './api'

  export let currentPath: string
  export let onNavigate: (path: string) => void
  /** 登録内容が変わった時に、他のペインの★表示も直すため親へ知らせる。 */
  export let onChanged: () => void = () => {}

  export let items: string[] = []
  export let missing: boolean[] = []

  /** この距離動いたら並べ替えとみなす。単なるクリックと区別する。 */
  const DRAG_THRESHOLD_PX = 4

  let grab: { index: number; y: number } | null = null
  let dragIndex: number | null = null
  let dropIndex: number | null = null

  function onPointerDown(index: number, ev: PointerEvent) {
    if (ev.button !== 0) return
    grab = { index, y: ev.clientY }
  }

  function onPointerMove(ev: PointerEvent) {
    if (!grab) return
    if (dragIndex === null) {
      if (Math.abs(ev.clientY - grab.y) < DRAG_THRESHOLD_PX) return
      dragIndex = grab.index
    }
    // 行の高さから逆算すると、パディングやスクロール量とずれるうえ
    // 高さを CSS と JS の二箇所で持つことになる。実要素に当てて決める。
    const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest('[data-fav-index]')
    if (el) dropIndex = Number((el as HTMLElement).dataset.favIndex)
  }

  async function onPointerUp(index: number) {
    const from = dragIndex
    const to = dropIndex
    grab = null
    dragIndex = null
    dropIndex = null

    if (from === null || to === null) {
      // 動かしていないなら、ただのクリックとして扱う。
      onNavigate(items[index])
      return
    }
    if (from === to) return

    await api.reorderFavorite(from, to)
    onChanged()
  }

  async function remove(path: string) {
    await api.removeFavorite(path)
    onChanged()
  }
</script>

<!--
  data-drop-favorites は、窓へ落とされたファイルの当たり判定に使う印。
  一覧の行からのドラッグは OS へ制御を渡すので DOM の drop は飛んでこない。
  Filer 側が座標から要素を引いて、この印を見て行き先を決める。
-->
<div
  class="list"
  data-drop-favorites
  on:pointermove={onPointerMove}
  on:pointerleave={() => {
    grab = null
    dragIndex = null
    dropIndex = null
  }}
>
  {#each items as path, i (path)}
    <div
      class="item"
      data-fav-index={i}
      class:dragging={dragIndex === i}
      class:dropTarget={dragIndex !== null && dropIndex === i && dragIndex !== i}
      role="button"
      tabindex="-1"
      on:pointerdown={(e) => onPointerDown(i, e)}
      on:pointerup={() => onPointerUp(i)}
      on:keydown={(e) => e.key === 'Enter' && onNavigate(path)}
    >
      <PathRow {path} current={path === currentPath} missing={missing[i] === false}>
        <button
          class="remove"
          type="button"
          title="お気に入りから外す"
          on:pointerdown|stopPropagation
          on:click|stopPropagation={() => remove(path)}
        >
          ✕
        </button>
      </PathRow>
    </div>
  {/each}

  {#if items.length === 0}
    <p class="empty">
      パス欄の ☆ を押すか<br />フォルダーをここへ<br />ドラッグすると登録されます
    </p>
  {/if}
</div>

<style>
  .list {
    height: 100%;
    overflow-y: auto;
    padding: 6px;
    box-sizing: border-box;
    background: #191919;
  }

  .item {
    height: 38px;
    box-sizing: border-box;
  }
  .dragging {
    opacity: 0.35;
  }
  /* 落ちる位置を線で示す。掴んだものがどこへ入るか分からないと並べ替えは怖い。 */
  .dropTarget {
    box-shadow: inset 0 2px 0 0 #4c9aff;
  }

  .remove {
    flex: none;
    align-self: center;
    width: 18px;
    height: 18px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 3px;
    color: #6a6a6a;
    font-size: 10px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
  }
  .item:hover .remove {
    opacity: 1;
  }
  .remove:hover {
    background: #4a2020;
    color: #ff9b9b;
  }

  .empty {
    margin: 24px 10px;
    text-align: center;
    font-size: 10.5px;
    line-height: 1.8;
    color: #5f5f5f;
  }
</style>
