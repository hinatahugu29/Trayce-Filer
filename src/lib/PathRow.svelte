<script lang="ts">
  import { splitPath, pathHue, elideLeft } from './api'

  export let path: string
  /** 右端に出す補足（履歴の「5分前」など）。 */
  export let meta = ''
  export let current = false
  /** 実在しない場所は沈ませる。消えたフォルダを掴んでも仕方がない。 */
  export let missing = false

  $: parts = splitPath(path)
</script>

<!--
  お気に入り・履歴・窓一覧で同じ見た目にする。
  末尾フォルダ名が主役、色帯でどの場所かを当てる、という規則をアプリ全体で揃える。
-->
<div class="row" class:current class:missing style="--hue: {pathHue(path)}" title={path}>
  <span class="band" />
  <span class="text">
    <span class="tail">{parts.tail}</span>
    <span class="lead">{elideLeft(parts.lead, 34)}</span>
  </span>
  {#if meta}<span class="meta">{meta}</span>{/if}
  <slot />
</div>

<style>
  .row {
    display: flex;
    align-items: stretch;
    gap: 7px;
    padding: 5px 6px 5px 0;
    border-radius: 5px;
    cursor: default;
    user-select: none;
  }
  .row:hover {
    background: #2b2b2b;
  }
  .row.current {
    background: #2d4a6b;
  }
  .row.missing {
    opacity: 0.42;
  }

  .band {
    width: 3px;
    flex: none;
    border-radius: 2px;
    background: hsl(var(--hue) 70% 55%);
  }
  .row.missing .band {
    filter: saturate(0);
  }

  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .tail {
    font-size: 12px;
    font-weight: 600;
    color: #e4e4e4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lead {
    font-size: 9.5px;
    color: #7c7c7c;
    overflow: hidden;
    white-space: nowrap;
  }

  .meta {
    flex: none;
    align-self: center;
    font-size: 9.5px;
    color: #6d6d6d;
    white-space: nowrap;
  }
</style>
