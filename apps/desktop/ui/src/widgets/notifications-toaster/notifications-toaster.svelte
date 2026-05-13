<script lang="ts">
  import { onMount } from 'svelte';

  import { getActiveNotifications, subscribeToNotificationEvents } from '@shared/notifications';

  import { Toaster } from '@shared/ui';
  import { dismissSonnerNotification, publishSonnerNotification } from './notification-adapter';

  onMount(() => {
    for (const notification of getActiveNotifications()) {
      publishSonnerNotification(notification);
    }

    return subscribeToNotificationEvents((event) => {
      if (event.type === 'published') {
        publishSonnerNotification(event.notification);
        return;
      }

      dismissSonnerNotification(event.id);
    });
  });
</script>

<Toaster />
