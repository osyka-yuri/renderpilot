<script lang="ts">
  import { AddonToolStatusBadge, type AddonBadgeStatus } from '@entities/addon';

  const { status }: { status: AddonBadgeStatus } = $props();
</script>

<AddonToolStatusBadge {status} i18nPrefix="gameDetails.renodx" />
