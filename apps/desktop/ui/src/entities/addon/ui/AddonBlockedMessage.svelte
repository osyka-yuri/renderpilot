<script lang="ts">
  import { t } from '@shared/i18n';
  import { ADDON_DISPLAY_NAME, type AddonKind } from '@shared/model';

  import AddonStateMessage from './AddonStateMessage.svelte';

  type Props = {
    /** Addon currently shown on the card (the blocked tool). */
    blockedAddon: AddonKind;
    /** Peer that occupies the game, when known. */
    installedAddon?: AddonKind | null;
    /** Fallback peer when `installedAddon` is unset. */
    fallbackInstalledAddon: AddonKind;
    /** Peer block is unmanaged debris rather than a tracked install. */
    unmanaged?: boolean;
    /** Tool-local copy when this tool's own unmanaged debris is present. */
    selfUnmanagedMessage?: string | null;
  };

  const {
    blockedAddon,
    installedAddon = null,
    fallbackInstalledAddon,
    unmanaged = false,
    selfUnmanagedMessage = null,
  }: Props = $props();

  const message = $derived(
    selfUnmanagedMessage ??
      t(
        unmanaged
          ? 'gameDetails.addon.blockedByOtherAddon.unmanaged'
          : 'gameDetails.addon.blockedByOtherAddon.tracked',
        {
          installedAddon: ADDON_DISPLAY_NAME[installedAddon ?? fallbackInstalledAddon],
          blockedAddon: ADDON_DISPLAY_NAME[blockedAddon],
        },
      ),
  );
</script>

<AddonStateMessage tone="default" icon="info" {message} />
