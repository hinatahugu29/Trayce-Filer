<script lang="ts">
  import { formatSize, formatModified, relativeTime } from './api'
  import type { Entry } from './api'

  export let entries: Entry[] = []

  /**
   * 一覧の下に空きができる時、そこを情報で埋める。
   *
   * ファイルが少ないフォルダほど余白が大きくなるが、そこは本来
   * 「このフォルダが何なのか」を掴むのに使える面積。既に読み込んだ
   * `entries` から計算するだけなので、追加の I/O は発生しない。
   */
  $: dirs = entries.filter((e) => e.is_dir).length
  $: files = entries.length - dirs
  $: totalSize = entries.reduce((sum, e) => sum + (e.is_dir ? 0 : e.size), 0)

  /** 最後に更新されたもの。「さっきいじったやつ」の入口になる。 */
  $: newest = entries.reduce<Entry | null>(
    (best, e) => (!best || e.modified > best.modified ? e : best),
    null
  )

  /** 拡張子の内訳。多い順に上位だけ。何が置いてある場所かが一目で分かる。 */
  $: byExt = (() => {
    const counts = new Map<string, number>()
    for (const e of entries) {
      if (e.is_dir) continue
      const key = e.ext || '(拡張子なし)'
      counts.set(key, (counts.get(key) ?? 0) + 1)
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6)
  })()

  /** 一番大きいファイル。容量を食っている犯人が分かる。 */
  $: largest = entries.reduce<Entry | null>(
    (best, e) => (e.is_dir ? best : !best || e.size > best.size ? e : best),
    null
  )
</script>

<div class="summary">
  {#if entries.length === 0}
    <p class="empty-title">このフォルダは空です</p>
    <p class="empty-hint">ファイルをここへドラッグすると取り込めます</p>
  {:else}
    <dl>
      <dt>内訳</dt>
      <dd>
        フォルダー {dirs} · ファイル {files}
        {#if totalSize > 0}<span class="dim">／ 合計 {formatSize(totalSize)}</span>{/if}
      </dd>

      {#if newest}
        <dt>最終更新</dt>
        <dd>
          <span class="name">{newest.name}</span>
          <span class="dim">{formatModified(newest.modified)}（{relativeTime(newest.modified)}）</span>
        </dd>
      {/if}

      {#if largest && largest.size > 0}
        <dt>最大</dt>
        <dd>
          <span class="name">{largest.name}</span>
          <span class="dim">{formatSize(largest.size)}</span>
        </dd>
      {/if}

      {#if byExt.length}
        <dt>種類</dt>
        <dd class="exts">
          {#each byExt as [ext, n]}
            <span class="chip">{ext}<span class="n">{n}</span></span>
          {/each}
        </dd>
      {/if}
    </dl>
  {/if}
</div>

<style>
  .summary {
    padding: 18px 20px;
    font-size: 11.5px;
    color: #7d7d7d;
    /* 一覧の続きに見えないよう、上に少し間を置く。 */
    border-top: 1px solid #262626;
    margin-top: 6px;
  }

  .empty-title {
    margin: 8px 0 4px;
    font-size: 13px;
    color: #8a8a8a;
  }
  .empty-hint {
    margin: 0;
    font-size: 11px;
    color: #5f5f5f;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
    margin: 0;
    align-items: baseline;
  }
  dt {
    color: #626262;
    font-size: 10.5px;
  }
  dd {
    margin: 0;
    min-width: 0;
    color: #9a9a9a;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name {
    color: #c2c2c2;
  }
  .dim {
    color: #6a6a6a;
    margin-left: 8px;
  }

  .exts {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    white-space: normal;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 1px 7px;
    background: #232323;
    border: 1px solid #333;
    border-radius: 10px;
    font-size: 10px;
    color: #9a9a9a;
  }
  .chip .n {
    color: #6a6a6a;
    font-variant-numeric: tabular-nums;
  }
</style>
