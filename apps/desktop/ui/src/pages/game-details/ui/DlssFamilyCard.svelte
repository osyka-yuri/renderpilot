<script lang="ts">
  import type { GameCandidateGroup, GameLibraryComponent } from '@entities/game';
  import {
    NvapiSettingGroup,
    type SettingFamily,
    type SettingStateResponse,
  } from '@features/nvapi-settings';
  import { t } from '@shared/i18n';
  import { displayPathRelativeTo } from '@shared/path';
  import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
    Card,
    CardContent,
    CardHeader,
    CardTitle,
    ItemGroup,
  } from '@shared/ui';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';
  import type { RollbackHandler, SwapHandler } from '../model/create-game-details-page-model';
  import ComponentVersionRow from './ComponentVersionRow.svelte';

  type Props = {
    gameId: string;
    installPath: string;
    files: readonly { component: GameLibraryComponent; group: GameCandidateGroup | null }[];
    family: SettingFamily;
    title: string;
    nvidia: NvidiaDriverContext;
    driverMutationBlocked: boolean;
    busy: boolean;
    onSwap: SwapHandler;
    onRollback: RollbackHandler;
  };

  const {
    gameId,
    installPath,
    files,
    family,
    title,
    nvidia,
    driverMutationBlocked,
    busy,
    onSwap,
    onRollback,
  }: Props = $props();
  const titleId = $props.id();

  const settings = $derived(nvidia.settingsForFamily(family));
  const warnings = $derived(nvidia.familyWarnings(family));
  const compatibilityReference = $derived.by(() => {
    const reference = nvidia.dllInfoForFamily(family);
    return files.length > 1 && nvidia.loaded && reference?.manifest_label
      ? displayPathRelativeTo(reference.path, installPath)
      : null;
  });

  function rowDisabled(state: SettingStateResponse): boolean {
    return (
      busy ||
      nvidia.busy ||
      driverMutationBlocked ||
      nvidia.isPending(state.setting_key) ||
      !state.has_profile_for_exe ||
      (state.catalog_readiness === 'notReady' && state.dll_kind !== null)
    );
  }
</script>

<Card>
  <CardHeader class="pb-2">
    <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
      <CardTitle id={titleId} level={2}>{title}</CardTitle>
      {#if files.length > 1}
        <span class="text-xs text-muted-foreground">
          {t('gameDetails.version.fileCount', { count: files.length })}
        </span>
      {/if}
    </div>
  </CardHeader>
  <CardContent class="@container grid min-w-0 gap-2">
    <ItemGroup class="min-w-0 divide-y rounded-md border bg-muted/30">
      {#each files as { component, group } (component.id)}
        <ComponentVersionRow
          {component}
          {group}
          {installPath}
          {busy}
          layout="file"
          {onSwap}
          {onRollback}
        />
      {/each}
    </ItemGroup>

    {#if nvidia.nvapiAvailable && settings.length > 0}
      <Accordion type="single" class="min-w-0">
        <AccordionItem value="driver">
          <AccordionTrigger aria-describedby={titleId} class="min-w-0 items-center py-2">
            {t('gameDetails.dlss.driverOverridesLabel')}
          </AccordionTrigger>
          <AccordionContent class="grid min-w-0 gap-2 pb-1">
            {#if compatibilityReference}
              <p class="text-xs break-all text-muted-foreground">
                {t('gameDetails.dlss.presetCompatibilityReference', {
                  path: compatibilityReference,
                })}
              </p>
            {/if}
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
              onRevertOriginal={(key: string) => {
                void nvidia.revert(gameId, key, 'original');
              }}
            />
          </AccordionContent>
        </AccordionItem>
      </Accordion>
    {/if}
  </CardContent>
</Card>
