<script lang="ts">
  import { t } from '@shared/i18n';

  import { AddonStateMessage } from '@entities/addon';

  type Props = {
    /** Official installer URL for the architecture this game needs
     * (`AvailabilityReport.vcredist_installer_url`). */
    installerUrl: string;
  };

  const { installerUrl }: Props = $props();
</script>

<AddonStateMessage tone="warning" icon="warning" message={t('gameDetails.luma.vcredistWarning')}>
  {#snippet actions()}
    <a
      href={installerUrl}
      target="_blank"
      rel="noreferrer"
      class="text-sm font-medium underline underline-offset-2"
    >
      {t('gameDetails.luma.vcredistLink')}
    </a>
  {/snippet}
</AddonStateMessage>
