<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import Filer from './lib/Filer.svelte'
  import Overlay from './lib/Overlay.svelte'

  // 全ての窓が同じ index.html を読む。どの役割を描くかは窓のラベルで決める。
  const isOverlay = getCurrentWindow().label === 'overlay'

  // オーバーレイだけ body を透過させる（角丸を活かすため）。
  if (isOverlay) document.body.classList.add('overlay')
</script>

{#if isOverlay}
  <Overlay />
{:else}
  <Filer />
{/if}
