<script lang="ts">
  import PathRow from './PathRow.svelte'
  import { openPath } from '@tauri-apps/plugin-opener'
  import { splitPath } from './api'
  import * as api from './api'

  export let currentPath: string
  export let onNavigate: (path: string) => void
  /** ファイル行の → 用。この機能が生んだ跳躍で移動の集計を歪ませない。 */
  export let onNavigateUncounted: (path: string) => void = (path) => onNavigate(path)
  /** 登録内容が変わった時に、他のペインの★表示も直すため親へ知らせる。 */
  export let onChanged: () => void = () => {}

  export let items: string[] = []
  /** 各行の種別。★は場所もファイルも持つので、行の動詞はここから決める。 */
  export let kinds: api.PathKind[] = []

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
      activate(items[index], index)
      return
    }
    if (from === to) return

    await api.reorderFavorite(from, to)
    onChanged()
  }

  /**
   * 行の主たる動詞。種別から導く。
   *
   * フォルダは「行く」、ファイルは「開く」。トレイが取った形（登録時に宣言させず、
   * 種別から動詞を決める）と同じで、向きだけが逆になる。
   */
  async function activate(path: string, index: number) {
    if (kinds[index]?.isDir !== false) return onNavigate(path)
    if (!api.confirmLaunch(path)) return
    try {
      await openPath(path)
    } catch (e) {
      console.error(e)
    }
  }

  /** ファイル行だけが持つ動詞。含まれる場所を開く。 */
  function goToFolder(path: string) {
    onNavigateUncounted(splitPath(path).lead || path)
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
    {@const kind = kinds[i]}
    {@const isFile = kind?.isDir === false && kind?.exists !== false}
    <div
      class="item"
      data-fav-index={i}
      class:dragging={dragIndex === i}
      class:dropTarget={dragIndex !== null && dropIndex === i && dragIndex !== i}
      role="button"
      tabindex="-1"
      on:pointerdown={(e) => onPointerDown(i, e)}
      on:pointerup={() => onPointerUp(i)}
      on:keydown={(e) => e.key === 'Enter' && activate(path, i)}
    >
      <PathRow
        {path}
        current={path === currentPath}
        missing={kind?.exists === false}
        icon={isFile ? api.fileIcon(splitPath(path).tail, false) : ''}
      >
        {#if isFile}
          <!-- ファイル行だけが持つ動詞。クリックは開く方なので、場所へ行く側を別に出す。
               当たり判定を分けてあるので、掴んでの並べ替えはそのまま使える。 -->
          <button
            class="go"
            type="button"
            title="含まれる場所を開く"
            on:pointerdown|stopPropagation
            on:click|stopPropagation={() => goToFolder(path)}
          >
            →
          </button>
        {/if}
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
      パス欄の ☆ を押すか<br />フォルダー・ファイルを<br />ここへドラッグすると<br />登録されます
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

  /* 普段は薄く、行に触れると濃くする。一覧のインライン展開やトレイの → と同じ作法。 */
  .go {
    flex: none;
    align-self: center;
    width: 18px;
    height: 18px;
    padding: 0;
    background: none;
    border: 0;
    border-radius: 3px;
    color: #3f4650;
    font-size: 11px;
    line-height: 1;
    cursor: pointer;
  }
  .item:hover .go {
    color: #8fbce8;
  }
  .go:hover {
    background: #22303c;
    color: #cfe6fb;
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
