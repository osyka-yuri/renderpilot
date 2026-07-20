<script lang="ts">
  import { t } from '@shared/i18n';
  import { AddonBlockedMessage } from '@entities/addon';

  import type { LumaStore } from '../model/create-luma-store.svelte';

  type Props = {
    store: LumaStore;
  };

  const { store }: Props = $props();

  /**
   * Luma and RenoDX are mutually exclusive per game. `isBlockedByOtherAddon`
   * covers both a tracked RenoDX record and unmanaged RenoDX files found on
   * disk; `isUnmanagedPresent` is the (rare) fallback when Luma's own stray
   * files could not be auto-adopted.
   */
  const selfUnmanagedMessage = $derived(
    store.isBlockedByOtherAddon ? null : t('gameDetails.luma.unmanagedPresent'),
  );
</script>

<AddonBlockedMessage
  blockedAddon="luma"
  installedAddon={store.otherAddonKind}
  fallbackInstalledAddon="renodx"
  unmanaged={store.otherAddonUnmanaged}
  {selfUnmanagedMessage}
/>
