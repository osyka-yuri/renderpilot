<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import type { SettingFamily, SettingStateResponse } from '@features/nvapi-settings';
  import CpuIcon from '@lucide/svelte/icons/cpu';
  import HardDriveIcon from '@lucide/svelte/icons/hard-drive';
  import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    ItemGroup,
    ItemSeparator,
  } from '@shared/ui';
  import ComponentVersionRow from './ComponentVersionRow.svelte';
  import NvapiSettingRow from './NvapiSettingRow.svelte';

  type Props = {
    gameId: string;
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    family: SettingFamily;
    title: string;
    nvidia: NvidiaDriverContext;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, entryId: string | null) => void;
    onRollback: (componentId: string) => void;
  };

  const { gameId, component, group, family, title, nvidia, busy, onSwap, onRollback }: Props =
    $props();

  const settings = $derived(nvidia.settingsForFamily(family));
  const warnings = $derived(nvidia.familyWarnings(family));
  const dllInfo = $derived(nvidia.dllInfoForFamily(family));

  function rowDisabled(state: SettingStateResponse): boolean {
    return (
      busy ||
      nvidia.busy ||
      !nvidia.canWrite ||
      nvidia.isPending(state.setting_key) ||
      !state.has_profile_for_exe
    );
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <div class="flex items-start justify-between gap-3">
      <div class="grid min-w-0 gap-1">
        <CardTitle>{title}</CardTitle>
        <CardDescription>
          Swap the on-disk DLL, or override DLSS settings through the NVIDIA driver profile.
        </CardDescription>
      </div>
      {#if dllInfo}
        <div class="shrink-0 text-right text-xs text-muted-foreground">
          <div class="font-medium text-foreground">
            {dllInfo.manifest_label ?? `DLSS ${dllInfo.version}`}
          </div>
          <div>v{dllInfo.version}</div>
        </div>
      {/if}
    </div>
  </CardHeader>

  <CardContent class="grid gap-4">
    <!-- ── Physical DLL: swapped on disk in the game folder ── -->
    <div class="grid gap-1.5">
      <div class="flex items-center gap-1.5 px-1 text-xs font-medium text-muted-foreground">
        <HardDriveIcon class="size-3.5" aria-hidden="true" />
        <span>Library file — on disk in the game folder</span>
      </div>
      <ItemGroup class="rounded-md border bg-muted/30">
        <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />
      </ItemGroup>
    </div>

    <!-- ── Driver overrides: NVIDIA profile via NVAPI, no game files touched ── -->
    {#if settings.length > 0}
      <div class="grid gap-1.5">
        <div class="flex items-center gap-1.5 px-1 text-xs font-medium text-muted-foreground">
          <CpuIcon class="size-3.5" aria-hidden="true" />
          <span>Driver overrides — NVIDIA profile (NVAPI)</span>
        </div>

        {#if !nvidia.canWrite}
          <p
            class="rounded-md border border-yellow-500/40 bg-yellow-500/10 px-3 py-2 text-xs text-yellow-700"
          >
            Changing these requires administrator privileges — use the banner at the top of the
            window to relaunch.
          </p>
        {/if}

        {#each warnings as warning (warning)}
          <div
            class="rounded-md border border-yellow-500/40 bg-yellow-500/10 p-2 text-xs text-yellow-700"
          >
            {warning}
          </div>
        {/each}

        <ItemGroup class="rounded-md border bg-muted/30">
          {#each settings as state, index (state.setting_key)}
            {#if index > 0}
              <ItemSeparator />
            {/if}
            <NvapiSettingRow
              {state}
              disabled={rowDisabled(state)}
              onChange={(wire: string) => nvidia.setValue(gameId, state.setting_key, wire)}
              onRevertPredefined={() => nvidia.revert(gameId, state.setting_key, 'predefined')}
              onRevertBaseline={() => nvidia.revert(gameId, state.setting_key, 'baseline')}
            />
          {/each}
        </ItemGroup>
      </div>
    {/if}
  </CardContent>
</Card>
