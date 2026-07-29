<script lang="ts">
  import type { VoidHandler } from '@shared/callbacks';
  import type { HTMLAttributes } from 'svelte/elements';
  import { t } from '@shared/i18n';
  import {
    Button,
    Empty,
    EmptyContent,
    EmptyDescription,
    EmptyHeader,
    EmptyTitle,
    Spinner,
  } from '@shared/ui';

  type Props = HTMLAttributes<HTMLDivElement> & {
    busy?: boolean;
    addGameButtonLabel?: string;
    onAddGame?: VoidHandler;
  };

  const {
    busy = false,
    addGameButtonLabel = '',
    onAddGame = () => undefined,
    class: className = '',
    ...rest
  }: Props = $props();

  const resolvedAddGameButtonLabel = $derived(addGameButtonLabel.trim() || t('games.addGame'));
</script>

<Empty {...rest} class={className}>
  <EmptyHeader class="w-full">
    <EmptyTitle>{t('games.empty.title')}</EmptyTitle>
    <EmptyDescription>
      {t('games.empty.description')}
    </EmptyDescription>
  </EmptyHeader>

  <EmptyContent>
    <Button variant="default" size="sm" disabled={busy} onclick={onAddGame}>
      {#if busy}
        <Spinner />
      {/if}
      {resolvedAddGameButtonLabel}
    </Button>
  </EmptyContent>
</Empty>
