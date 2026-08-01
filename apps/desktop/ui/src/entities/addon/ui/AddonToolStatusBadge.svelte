<script lang="ts">
  import { t, type MessageKeyWithoutParams } from '@shared/i18n';

  import AddonStatusBadge from './AddonStatusBadge.svelte';
  import type { AddonBadgeStatus } from '../model/badge-status';
  import type { AddonToolI18nPrefix } from './types';

  // Use the single source of truth so prop passing is unambiguously typed
  // (prevents svelte + eslint "unsafe assignment" reports in thin wrappers).
  type Props = {
    status: AddonBadgeStatus;
    i18nPrefix: AddonToolI18nPrefix;
  };

  function labelKey(
    prefix: AddonToolI18nPrefix,
    status: AddonBadgeStatus,
  ): MessageKeyWithoutParams {
    switch (status) {
      case 'untracked':
        return `${prefix}.updatesNotTracked`;
      case 'channel_mismatch':
        return `${prefix}.fresh.channelMismatch`;
      case 'unknown_needs_validation':
        return `${prefix}.fresh.validationRequired`;
      default:
        return `${prefix}.fresh.${status}`;
    }
  }

  const { status, i18nPrefix }: Props = $props();

  const label = $derived(t(labelKey(i18nPrefix, status)));
</script>

<AddonStatusBadge {status} {label} />
