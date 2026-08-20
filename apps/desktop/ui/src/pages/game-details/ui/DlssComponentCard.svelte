<script lang="ts">
  import type { GameCandidateGroup, GameLibraryComponent } from '@entities/game';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import type { RollbackHandler, SwapHandler } from '../model/create-game-details-page-model';
  import {
    NvapiSettingGroup,
    type SettingFamily,
    type SettingStateResponse,
  } from '@features/nvapi-settings';
  import CpuIcon from '@lucide/svelte/icons/cpu';
  import HardDriveIcon from '@lucide/svelte/icons/hard-drive';
  import { Card, CardContent, CardDescription, CardHeader, CardTitle, ItemGroup } from '@shared/ui';
  import { t } from '@shared/i18n';
  import ComponentVersionRow from './ComponentVersionRow.svelte';
  import { displayDllLabel, isDllDependentCatalogBlocked } from './DlssComponentCard.model';

  type Props = {
    gameId: string;
    component: GameLibraryComponent;
    group: GameCandidateGroup | null;
    family: SettingFamily;
    title: string;
    nvidia: NvidiaDriverContext;
    nvapiAvailable: boolean;
    busy: boolean;
    onSwap: SwapHandler;
    onRollback: RollbackHandler;
  };

  const {
    gameId,
    component,
    group,
    family,
    title,
    nvidia,
    nvapiAvailable,
    busy,
    onSwap,
    onRollback,
  }: Props = $props();

  const settings = $derived(nvidia.settingsForFamily(family));
  const warnings = $derived(nvidia.familyWarnings(family));
  const dllInfo = $derived(nvidia.dllInfoForFamily(family));
  const dllLabel = $derived(
    dllInfo === null ? null : displayDllLabel(dllInfo, t('gameDetails.nvapi.versionUnavailable')),
  );

  function rowDisabled(state: SettingStateResponse): boolean {
    return (
      busy ||
      nvidia.busy ||
      nvidia.isPending(state.setting_key) ||
      !state.has_profile_for_exe ||
      isDllDependentCatalogBlocked(state)
    );
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <div class="flex items-start justify-between gap-3">
      <div class="grid min-w-0 gap-1">
        <CardTitle level={2}>{title}</CardTitle>
        <CardDescription>
          {nvapiAvailable
            ? t('gameDetails.dlss.description')
            : t('gameDetails.dlss.descriptionSwapOnly')}
        </CardDescription>
      </div>
      {#if dllInfo}
        <div class="shrink-0 text-end text-xs text-muted-foreground">
          <div class="font-medium text-foreground">
            {dllLabel}
          </div>
          {#if dllInfo.version !== null}
            <div>v{dllInfo.version}</div>
          {/if}
        </div>
      {/if}
    </div>
  </CardHeader>

  <CardContent class="grid gap-4">
    <!-- ── Physical DLL: swapped on disk in the game folder ── -->
    <div class="grid gap-1.5">
      <div class="flex items-center gap-1.5 px-1 text-xs font-medium text-muted-foreground">
        <HardDriveIcon class="size-3.5" aria-hidden="true" />
        <span>{t('gameDetails.dlss.libraryFileLabel')}</span>
      </div>
      <ItemGroup class="rounded-md border bg-muted/30">
        <ComponentVersionRow {component} {group} {busy} {onSwap} {onRollback} />
      </ItemGroup>
    </div>

    <!-- ── Driver overrides: NVIDIA profile via NVAPI, no game files touched ── -->
    {#if nvapiAvailable && settings.length > 0}
      <div class="grid gap-1.5">
        <div class="flex items-center gap-1.5 px-1 text-xs font-medium text-muted-foreground">
          <CpuIcon class="size-3.5" aria-hidden="true" />
          <span>{t('gameDetails.dlss.driverOverridesLabel')}</span>
        </div>

        <NvapiSettingGroup
          {settings}
          {warnings}
          {rowDisabled}
          onChange={(key: string, wire: string) => {
            void nvidia.setValue(gameId, key, wire);
          }}
          onRevertPredefined={(key: string) => {
            void nvidia.revert(gameId, key, 'predefined');
          }}
          onRevertBaseline={(key: string) => {
            void nvidia.revert(gameId, key, 'baseline');
          }}
        />
      </div>
    {/if}
  </CardContent>
</Card>
