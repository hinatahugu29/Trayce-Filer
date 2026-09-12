<script lang="ts">
  import { formatSize, formatSpeed, formatEta, splitPath } from './api'
  import * as api from './api'
  import type { ProgressEvent } from './api'

  export let progress: ProgressEvent | null = null

  $: ratio =
    progress && progress.bytesTotal > 0
      ? Math.min(1, progress.bytesDone / progress.bytesTotal)
      : 0
  $: name = progress?.current ? splitPath(progress.current).tail : ''
  $: speedStr = progress?.bytesPerSec !== undefined ? formatSpeed(progress.bytesPerSec) : ''
  $: etaStr = progress?.etaSecs !== undefined && progress?.etaSecs !== null ? formatEta(progress.etaSecs) : ''
</script>

{#if progress}
  <div class="bar" role="status" aria-live="polite">
    {#if progress.scanning}
      <span class="label">集計中…</span>
      <div class="track indeterminate"><div class="fill" /></div>
    {:else}
      <span class="label">
        {progress.filesDone} / {progress.filesTotal} 件
      </span>
      <div class="track">
        <div class="fill" style="width: {ratio * 100}%" />
      </div>
      <span class="bytes">
        {formatSize(progress.bytesDone)} / {formatSize(progress.bytesTotal)}
      </span>
      {#if speedStr}
        <span class="speed">{speedStr}</span>
      {/if}
      {#if etaStr}
        <span class="eta">{etaStr}</span>
      {/if}
      <span class="current" title={progress.current}>{name}</span>
    {/if}

    <button type="button" on:click={() => api.cancelTransfer(progress.id)}>中断</button>
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: none;
    padding: 5px 12px;
    background: #1f2c3d;
    border-top: 1px solid #33517a;
    font-size: 11px;
    color: #cfe0f5;
  }

  .label {
    flex: none;
    font-variant-numeric: tabular-nums;
  }

  .track {
    flex: 1;
    min-width: 60px;
    height: 6px;
    background: #16202c;
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: #4c9aff;
    transition: width 120ms linear;
  }

  /* 総量が分かるまでは割合を出せないので、動いていることだけ示す。 */
  .indeterminate .fill {
    width: 35%;
    animation: sweep 1.1s ease-in-out infinite;
    transition: none;
  }
  @keyframes sweep {
    0% {
      margin-left: -35%;
    }
    100% {
      margin-left: 100%;
    }
  }

  .bytes {
    flex: none;
    color: #8fb4de;
    font-variant-numeric: tabular-nums;
  }
  .speed {
    flex: none;
    color: #51c4d3;
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }
  .eta {
    flex: none;
    color: #ffb703;
    font-variant-numeric: tabular-nums;
  }
  .current {
    flex: 1;
    min-width: 0;
    color: #7f9ec2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button {
    flex: none;
    padding: 3px 10px;
    background: #33517a;
    border: 1px solid #4c9aff;
    border-radius: 4px;
    color: #fff;
    font: inherit;
    font-size: 10.5px;
    cursor: pointer;
  }
  button:hover {
    background: #3f6396;
  }
</style>
