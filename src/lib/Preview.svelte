<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core'
  import * as api from './api'
  import type { Preview } from './api'

  /** 選択中の1件のパス。null なら何も選ばれていない。複数選択時は先頭のみ扱う。 */
  export let path: string | null

  let loaded: { path: string; preview: Preview } | null = null
  let loading = false

  // 選択が変わるたびに読み直す。連打で古い結果が後から出て上書きする
  // ("late arrival") を防ぐため、リクエストごとに世代を振って最新だけ採用する。
  let generation = 0

  async function load(target: string | null) {
    const gen = ++generation
    loaded = null
    if (!target) return

    loading = true
    try {
      const preview = await api.previewEntry(target)
      if (gen !== generation) return // 別の選択に進んでいたら捨てる
      loaded = { path: target, preview }
    } catch {
      // プレビューが取れないだけなら何も表示しない。一覧の操作は邪魔しない。
    } finally {
      if (gen === generation) loading = false
    }
  }

  $: load(path)
</script>

<div class="preview">
  {#if !path}
    <p class="empty">プレビューするファイルを選んでください</p>
  {:else if loading && !loaded}
    <p class="empty">読み込み中…</p>
  {:else if loaded}
    {#if loaded.preview.kind === 'image'}
      <!-- 実データは Rust から受け取らず、asset プロトコル経由でブラウザに直接読ませる。
           大きい画像でも IPC を経由しないので、一覧のやり取りを重くしない。 -->
      <img src={convertFileSrc(loaded.path)} alt="" />
    {:else if loaded.preview.kind === 'text'}
      <pre>{loaded.preview.text}</pre>
      {#if loaded.preview.truncated}
        <p class="note">先頭部分のみ表示しています</p>
      {/if}
    {:else}
      <p class="empty">{loaded.preview.reason}</p>
    {/if}
  {/if}
</div>

<style>
  .preview {
    height: 100%;
    overflow: auto;
    box-sizing: border-box;
    padding: 10px;
    background: #171717;
  }

  .empty {
    margin: 20px 8px;
    text-align: center;
    font-size: 10.5px;
    color: #5f5f5f;
  }

  img {
    display: block;
    max-width: 100%;
    height: auto;
    margin: 0 auto;
    border-radius: 3px;
  }

  pre {
    margin: 0;
    font-family: Consolas, monospace;
    font-size: 11px;
    line-height: 1.5;
    color: #ccc;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .note {
    margin: 8px 0 0;
    font-size: 10px;
    color: #6a6a6a;
  }
</style>
