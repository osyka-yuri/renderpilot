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
  import { t } from '@shared/i18n';
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
    <CardTitle>{t('gameDetails.profile.title')}</CardTitle>
    <CardDescription>
      {t('gameDetails.profile.description')}
    </CardDescription>
  </CardHeader>

  <CardContent class="grid gap-2">
    <Item size="sm" variant="outline" class="rounded-md bg-muted/30">
      <ItemContent>
        <ItemTitle>{t('gameDetails.profile.target')}</ItemTitle>
        <ItemDescription>
          {#if !nvapi.hasStates && nvapi.busy}
            {t('gameDetails.profile.loading')}
          {:else if nvapi.effectiveExeSource === 'override'}
            {t('gameDetails.profile.pinnedManual')}
          {:else if nvapi.effectiveExeSource === 'auto'}
            {t('gameDetails.profile.autoDetected')}
          {:else}
            {t('gameDetails.profile.noExeDetected')}
          {/if}
        </ItemDescription>
      </ItemContent>
      <ItemActions>
        <Select type="single" disabled={nvapi.busy} onValueChange={handleCandidateChange}>
          <SelectTrigger size="sm" class="w-72">
            {#if nvapi.effectiveExe}
              <span class="truncate">{fileNameFromPath(nvapi.effectiveExe)}</span>
            {:else}
              <span class="text-muted-foreground">{t('gameDetails.profile.noExe')}</span>
            {/if}
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={AUTO_DETECT_VALUE} label={t('gameDetails.profile.autoDetect')}>
              {t('gameDetails.profile.autoDetect')}
            </SelectItem>
            {#each nvapi.supportedCandidates as candidate (candidate.absolute_path)}
              <SelectItem value={candidate.absolute_path} label={candidate.file_name}>
                <div class="flex flex-col">
                  <span>{candidate.file_name}</span>
                  <span class="text-xs text-muted-foreground">{candidate.relative_path}</span>
                </div>
              </SelectItem>
            {/each}
            {#each nvapi.filteredOutCandidates as candidate (candidate.absolute_path)}
              <SelectItem
                value={candidate.absolute_path}
                label={t('gameDetails.profile.filteredLabel', { fileName: candidate.file_name })}
              >
                <div class="flex flex-col">
                  <span
                    >{candidate.file_name}
                    <span class="text-muted-foreground">{t('gameDetails.profile.filteredTag')}</span
                    ></span
                  >
                  <span class="text-xs text-muted-foreground">{candidate.relative_path}</span>
                </div>
              </SelectItem>
            {/each}
          </SelectContent>
        </Select>
      </ItemActions>
    </Item>

    {#if nvapi.hasStates && !nvapi.hasProfile && nvapi.effectiveExe}
      <p class="px-4 text-xs text-muted-foreground">
        {t('gameDetails.profile.noProfile', { exe: fileNameFromPath(nvapi.effectiveExe) })}
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
