<script lang="ts">
  import type { GameCandidateGroup, GameGraphicsComponent } from '@entities/game';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import type { SettingFamily, SettingStateResponse } from '@features/nvapi-settings';
  import CpuIcon from '@lucide/svelte/icons/cpu';
  import HardDriveIcon from '@lucide/svelte/icons/hard-drive';
  import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
  import {
    Alert,
    AlertDescription,
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
    ItemGroup,
    ItemSeparator,
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import ComponentVersionRow from './ComponentVersionRow.svelte';
  import NvapiSettingRow from './NvapiSettingRow.svelte';

  type Props = {
    gameId: string;
    component: GameGraphicsComponent;
    group: GameCandidateGroup | null;
    family: SettingFamily;
    title: string;
    nvidia: NvidiaDriverContext;
    nvapiAvailable: boolean;
    busy: boolean;
    onSwap: (componentId: string, artifactId: string, isDownloaded: boolean) => void;
    onRollback: (componentId: string) => void;
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
          {nvapiAvailable
            ? t('gameDetails.dlss.description')
            : t('gameDetails.dlss.descriptionSwapOnly')}
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

        {#if !nvidia.canWrite}
          <Alert variant="warning" size="sm" role="note">
            <TriangleAlertIcon aria-hidden="true" />
            <AlertDescription>
              {t('gameDetails.dlss.adminRequired')}
            </AlertDescription>
          </Alert>
        {/if}

        {#each warnings as warning (warning)}
          <Alert variant="warning" size="sm" role="note">
            <TriangleAlertIcon aria-hidden="true" />
            <AlertDescription>{warning}</AlertDescription>
          </Alert>
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
