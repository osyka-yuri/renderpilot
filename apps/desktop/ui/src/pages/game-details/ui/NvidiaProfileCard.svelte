<script lang="ts">
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import {
    Alert,
    AlertDescription,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from '@shared/ui';
  import { fileNameFromPath } from '@shared/path';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';

  type Props = {
    gameId: string;
    nvapi: NvidiaDriverContext;
  };

  const { gameId, nvapi }: Props = $props();

  const AUTO_DETECT_VALUE = '__auto__';

  function handleCandidateChange(value: string | undefined) {
    if (!value) return;
    if (value === AUTO_DETECT_VALUE) {
      void nvapi.clearExecutableOverride(gameId);
    } else {
      void nvapi.setExecutableOverride(gameId, value);
    }
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <CardTitle>NVIDIA driver profile</CardTitle>
    <CardDescription>
      The driver overrides below are written to this executable's NVIDIA profile.
    </CardDescription>
  </CardHeader>

  <CardContent class="grid gap-2">
    <Item size="sm" variant="muted" class="rounded-md">
      <ItemContent>
        <ItemTitle>Profile target</ItemTitle>
        <ItemDescription>
          {#if !nvapi.hasStates && nvapi.busy}
            Loading driver state…
          {:else if nvapi.effectiveExeSource === 'override'}
            Manually pinned to this executable.
          {:else if nvapi.effectiveExeSource === 'auto'}
            Auto-detected (file size, path depth, NVAPI profile probe).
          {:else}
            No executable detected for this installation.
          {/if}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        <Select type="single" disabled={nvapi.busy} onValueChange={handleCandidateChange}>
          <SelectTrigger size="sm" class="w-72">
            {#if nvapi.effectiveExe}
              <span class="truncate">{fileNameFromPath(nvapi.effectiveExe)}</span>
            {:else}
              <span class="text-muted-foreground">No executable</span>
            {/if}
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={AUTO_DETECT_VALUE} label="Auto-detect (top candidate)">
              Auto-detect (top candidate)
            </SelectItem>
            {#each nvapi.supportedCandidates as candidate (candidate.absolute_path)}
              <SelectItem value={candidate.absolute_path} label={candidate.file_name}>
                <span class="flex flex-col">
                  <span>{candidate.file_name}</span>
                  <span class="text-xs text-muted-foreground">{candidate.relative_path}</span>
                </span>
              </SelectItem>
            {/each}
            {#each nvapi.filteredOutCandidates as candidate (candidate.absolute_path)}
              <SelectItem
                value={candidate.absolute_path}
                label={`${candidate.file_name} (filtered)`}
              >
                <span class="flex flex-col">
                  <span
                    >{candidate.file_name}
                    <span class="text-muted-foreground">(filtered)</span></span
                  >
                  <span class="text-xs text-muted-foreground">{candidate.relative_path}</span>
                </span>
              </SelectItem>
            {/each}
          </SelectContent>
        </Select>
      </ItemActions>
    </Item>

    {#if nvapi.hasStates && !nvapi.hasProfile && nvapi.effectiveExe}
      <p class="px-4 text-xs text-muted-foreground">
        NVIDIA has no profile for <code class="font-mono"
          >{fileNameFromPath(nvapi.effectiveExe)}</code
        > yet. Launch the game once, then come back.
      </p>
    {/if}

    {#if nvapi.loadError}
      <div
        class="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-sm text-destructive"
      >
        {nvapi.loadError}
      </div>
    {/if}

    {#each nvapi.profileWarnings as warning (warning)}
      <Alert variant="warning" size="sm" role="note">
        <TriangleAlertIcon aria-hidden="true" />
        <AlertDescription>{warning}</AlertDescription>
      </Alert>
    {/each}
  </CardContent>
</Card>
