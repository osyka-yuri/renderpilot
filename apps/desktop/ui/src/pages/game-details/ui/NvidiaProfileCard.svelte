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
  } from '@shared/ui';
  import { t } from '@shared/i18n';
  import type { NvidiaDriverContext } from '../model/create-nvidia-driver-context.svelte';

  type Props = {
    nvapi: NvidiaDriverContext;
  };

  const { nvapi }: Props = $props();

  // The executable selector now lives in the shared game-level GameExecutableCard;
  // this card is purely NVIDIA profile status. Render only when there is something
  // to report (a missing profile, a warning, or a load error) so it never shows as
  // an empty card for a game whose profile is fine.
  const showMissingProfile = $derived(nvapi.hasStates && !nvapi.hasProfile && !!nvapi.effectiveExe);
  const hasContent = $derived(
    showMissingProfile || !!nvapi.loadError || nvapi.profileWarnings.length > 0,
  );
</script>

{#if hasContent}
  <Card>
    <CardHeader class="pb-2">
      <CardTitle>{t('gameDetails.profile.title')}</CardTitle>
      <CardDescription>
        {t('gameDetails.profile.description')}
      </CardDescription>
    </CardHeader>

    <CardContent class="grid gap-2">
      {#if showMissingProfile}
        <Alert variant="warning" size="sm" role="note">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertDescription>
            {t('gameDetails.profile.noProfile')}
          </AlertDescription>
        </Alert>
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
{/if}
