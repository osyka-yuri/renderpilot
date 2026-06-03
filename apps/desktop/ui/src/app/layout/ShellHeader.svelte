<script lang="ts">
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import type { ScreenHandler, Screen } from '@app/navigation/screen';
  import { t } from '@shared/i18n';
  import {
    Breadcrumb,
    BreadcrumbItem,
    BreadcrumbLink,
    BreadcrumbList,
    BreadcrumbPage,
    BreadcrumbSeparator,
    Button,
    SidebarTrigger,
  } from '@shared/ui';
  import DonateButton from './DonateButton.svelte';

  type Props = {
    screen: Screen;
    resolvedGameTitle: string;
    busy: boolean;
    refreshing: boolean;
    onNavigate: ScreenHandler;
    onRefresh: () => void;
  };

  const {
    screen,
    resolvedGameTitle,
    busy,
    refreshing,
    onNavigate,
    onRefresh,
  }: Props = $props();

  type BreadcrumbEntry =
    | {
        id: string;
        kind: 'link';
        label: string;
        target: Screen;
      }
    | {
        id: string;
        kind: 'page';
        label: string;
      };

  const breadcrumbs = $derived(createBreadcrumbs(screen, resolvedGameTitle));

  function createBreadcrumbs(currentScreen: Screen, gameTitle: string): BreadcrumbEntry[] {
    switch (currentScreen) {
      case 'games':
        return [{ id: 'games-page', kind: 'page', label: t('nav.games') }];

      case 'settings':
        return [{ id: 'settings-page', kind: 'page', label: t('nav.settings') }];

      case 'libraries':
        return [{ id: 'libraries-page', kind: 'page', label: t('nav.libraries') }];

      case 'details':
        return [
          { id: 'games-link', kind: 'link', label: t('nav.games'), target: 'games' },
          { id: 'game-page', kind: 'page', label: gameTitle },
        ];

      case 'operations':
        return [
          { id: 'games-link', kind: 'link', label: t('nav.games'), target: 'games' },
          { id: 'game-link', kind: 'link', label: gameTitle, target: 'details' },
          { id: 'operations-page', kind: 'page', label: t('nav.operations') },
        ];

      default: {
        return [{ id: 'fallback-games-page', kind: 'page', label: t('nav.games') }];
      }
    }
  }
</script>

<header class="flex shrink-0 items-center gap-2 border-b px-4 py-2">
  <SidebarTrigger />

  <Breadcrumb>
    <BreadcrumbList>
      {#each breadcrumbs as item, index (item.id)}
        {#if index > 0}
          <BreadcrumbSeparator />
        {/if}

        <BreadcrumbItem>
          {#if item.kind === 'link'}
            <BreadcrumbLink
              href={`#${item.target}`}
              onclick={(event: MouseEvent) => {
                event.preventDefault();
                onNavigate(item.target);
              }}
            >
              {item.label}
            </BreadcrumbLink>
          {:else}
            <BreadcrumbPage>{item.label}</BreadcrumbPage>
          {/if}
        </BreadcrumbItem>
      {/each}
    </BreadcrumbList>
  </Breadcrumb>

  <div class="ml-auto flex items-center gap-2">
    <DonateButton />
    
    <Button
      variant="outline"
      size="icon"
      disabled={busy}
      onclick={onRefresh}
      aria-label={t('shell.refresh')}
    >
      <RefreshCwIcon class={refreshing ? 'animate-spin' : ''} aria-hidden="true" />
    </Button>
  </div>
</header>
