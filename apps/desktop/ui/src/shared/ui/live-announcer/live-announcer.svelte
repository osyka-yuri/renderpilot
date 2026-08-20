<script lang="ts">
  type Props = {
    message: string;
    politeness?: 'polite' | 'assertive';
  };

  let { message, politeness = 'polite' }: Props = $props();
  let announcedMessage = $state('');

  $effect(() => {
    const nextMessage = message;
    const timer = window.setTimeout(() => {
      announcedMessage = nextMessage;
    });

    return () => {
      window.clearTimeout(timer);
    };
  });
</script>

<p class="sr-only" role="status" aria-live={politeness} aria-atomic="true">{announcedMessage}</p>
