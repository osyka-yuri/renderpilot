<script lang="ts">
  import {
    formatArchitectureLabel,
    formatVersionLabel,
    type LibraryPackageRow,
  } from '../model/libraries-page-model';

  let {
    row,
    showPackageDisplayName,
  }: {
    row: LibraryPackageRow;
    showPackageDisplayName: () => boolean;
  } = $props();

  const versionLabel = $derived(formatVersionLabel(row));
  const architectureLabel = $derived(formatArchitectureLabel(row));
  const displayNameVisible = $derived(showPackageDisplayName());
</script>

<div class="flex flex-col gap-0.5">
  <span class="flex items-center gap-2">
    <span>{versionLabel}</span>
    {#if architectureLabel}
      <span class="rounded-sm border px-1.5 py-0.5 text-[10px] text-muted-foreground uppercase">
        {architectureLabel}
      </span>
    {/if}
  </span>
  {#if displayNameVisible}
    <span class="truncate text-xs text-muted-foreground" title={row.display_name}>
      {row.display_name}
    </span>
  {/if}
</div>
