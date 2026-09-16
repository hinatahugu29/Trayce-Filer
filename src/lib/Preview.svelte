<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core'
  import * as api from './api'
  import type { Preview } from './api'

  /** 選択中の1件のパス。null なら何も選ばれていない。複数選択時は先頭のみ扱う。 */
  export let path: string | null
  /**
   * 選択中の項目がフォルダかどうか。
   *
   * フォルダにも中身を出すのが目的。「入ってみたが違ったので戻る」という
   * 一番多い往復を、移動せずに済ませるため。
   */
  export let isDir = false
  /** 覗いた先へ実際に移動する。フォルダを見て「ここだ」と分かった時の行き先。 */
  export let onNavigate: (path: string) => void = () => {}

  let loaded: { path: string; preview: Preview } | null = null
  /** フォルダを覗いた結果。中身の一覧と、数え終わった概要。 */
  let folder: { path: string; entries: api.Entry[]; total: number } | null = null
  let measured: { files: number; bytes: number } | null = null
  let loading = false

  /** 覗き見は一覧の代わりではないので、先頭だけ見せて件数で補う。 */
  const PEEK_LIMIT = 200

  // 選択が変わるたびに読み直す。連打で古い結果が後から出て上書きする
  // ("late arrival") を防ぐため、リクエストごとに世代を振って最新だけ採用する。
  let generation = 0

  async function load(target: string | null, dir: boolean) {
    const gen = ++generation
    loaded = null
    folder = null
    measured = null
    if (!target) return

    loading = true
    try {
      if (dir) {
        const listing = await api.listDir(target, {
          key: 'name',
          descending: false,
          dirsFirst: true,
          showHidden: false,
        })
        if (gen !== generation) return
        folder = {
          path: target,
          entries: listing.entries.slice(0, PEEK_LIMIT),
          total: listing.entries.length,
        }
        // 総容量は木を歩くので時間がかかる。一覧を先に出し、数え終わったら足す。
        api
          .measureFolder(target)
          .then((m) => {
            if (gen === generation) measured = m
          })
          .catch(() => {})
      } else {
        const preview = await api.previewEntry(target)
        if (gen !== generation) return // 別の選択に進んでいたら捨てる
        loaded = { path: target, preview }
      }
    } catch {
      // プレビューが取れないだけなら何も表示しない。一覧の操作は邪魔しない。
    } finally {
      if (gen === generation) loading = false
    }
  }

  $: load(path, isDir)
</script>

<div class="preview">
  {#if !path}
    <p class="empty">プレビューする項目を選んでください</p>
  {:else if loading && !loaded}
    <p class="empty">読み込み中…</p>
  {:else if folder}
    {@const peeked = folder}
    <div class="folder-head">
      <span class="count">{folder.total} 項目</span>
      {#if measured}
        <span class="count">{api.formatSize(measured.bytes)}</span>
      {/if}
      <button type="button" class="go" on:click={() => onNavigate(peeked.path)}>ここへ移動</button>
    </div>
    {#if folder.total === 0}
      <p class="empty">空のフォルダーです</p>
    {:else}
      <ul class="peek">
        {#each folder.entries as entry (entry.name)}
          <li class:dir={entry.is_dir}>
            <span class="icon">{api.fileIcon(entry.name, entry.is_dir)}</span>
            <span class="name">{entry.name}</span>
          </li>
        {/each}
      </ul>
      {#if folder.total > folder.entries.length}
        <p class="note">先頭 {folder.entries.length} 件のみ表示しています</p>
      {/if}
    {/if}
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

  .folder-head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid #2a2a2a;
  }

  .folder-head .count {
    font-size: 10px;
    color: #8a8a8a;
  }

  .folder-head .go {
    margin-left: auto;
    padding: 2px 7px;
    border: 1px solid #3a3a3a;
    border-radius: 3px;
    font-size: 10px;
    color: #c8c8c8;
    background: #242424;
    cursor: pointer;
  }

  .folder-head .go:hover {
    border-color: #4f4f4f;
    background: #2e2e2e;
  }

  .peek {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .peek li {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 1px 0;
    font-size: 11px;
    color: #b4b4b4;
    white-space: nowrap;
    overflow: hidden;
  }

  .peek li.dir {
    color: #d6d6d6;
  }

  .peek .icon {
    flex: none;
  }

  .peek .name {
    overflow: hidden;
    text-overflow: ellipsis;
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
