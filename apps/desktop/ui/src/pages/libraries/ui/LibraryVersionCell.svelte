<script lang="ts">
  import { Badge } from '@shared/ui';

  import {
    formatArchitectureLabel,
    formatVersionLabel,
    libraryPackageStateLabel,
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
  const stateLabel = $derived(libraryPackageStateLabel(row));
  const displayNameVisible = $derived(showPackageDisplayName());
</script>

<div class="flex flex-col gap-0.5">
  <span class="flex items-center gap-2">
    <span>{versionLabel}</span>
    {#if architectureLabel}
      <Badge variant="outline">{architectureLabel}</Badge>
    {/if}
    {#if stateLabel}
      <Badge variant="outline">{stateLabel}</Badge>
    {/if}
  </span>
  {#if displayNameVisible}
    <span class="truncate text-xs text-muted-foreground" title={row.display_name}>
      {row.display_name}
    </span>
  {/if}
</div>
