<script lang="ts">
  import { t, type MessageKey } from '@shared/i18n';

  type Props = {
    textKey: MessageKey;
    linkKey: MessageKey;
    href: string;
    class?: string;
  };

  const { textKey, linkKey, href, class: className = '' }: Props = $props();
</script>

<p class={['text-xs text-muted-foreground', className].filter(Boolean).join(' ')}>
  {t(textKey)}
  <a {href} target="_blank" rel="noreferrer" class="underline underline-offset-2">
    {t(linkKey)}
  </a>
</p>
