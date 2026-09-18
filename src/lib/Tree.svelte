<script lang="ts">
  import { onMount } from 'svelte'
  import TreeNode from './TreeNode.svelte'
  import * as api from './api'
  import { ancestorsOf } from './api'

  /** ペインが今開いている場所。ツリーはここまで自動で開き、位置を示す。 */
  export let currentPath: string
  export let onNavigate: (path: string) => void
  export let onOpenBeside: (path: string) => void = () => {}
  export let showHidden = false

  let roots: string[] = []

  // 現在地までの経路。TreeNode がこれを見て自分を開くか決める。
  $: autoOpen = new Set(ancestorsOf(currentPath))

  onMount(async () => {
    roots = await api.drives()
  })
</script>

<div class="tree" role="tree" aria-label="フォルダツリー">
  {#each roots as root (root)}
    <TreeNode
      path={root}
      name={root.replace(/\\$/, '')}
      {currentPath}
      {autoOpen}
      {onNavigate}
      {onOpenBeside}
      {showHidden}
    />
  {/each}
</div>

<style>
  .tree {
    width: 100%;
    height: 100%;
    overflow: auto;
    padding: 6px 0;
    box-sizing: border-box;
    background: #191919;
  }
</style>
