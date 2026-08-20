<script lang="ts">
  import CircleArrowUpIcon from '@lucide/svelte/icons/circle-arrow-up';
  import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
  import type { ScreenHandler } from '@app/navigation/screen';
  import type { ShellNavigation } from '@app/navigation/shell-navigation';
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
    Spinner,
  } from '@shared/ui';
  import DonateButton from './DonateButton.svelte';

  type Props = {
    navigation: ShellNavigation;
    busy: boolean;
    refreshing: boolean;
    onNavigate: ScreenHandler;
    onPreload: ScreenHandler;
    onRefresh: () => void;
    updateAvailable?: boolean;
    updateOpening?: boolean;
    onOpenUpdate?: () => void;
  };

  const {
    navigation,
    busy,
    refreshing,
    onNavigate,
    onPreload,
    onRefresh,
    updateAvailable = false,
    updateOpening = false,
    onOpenUpdate = () => undefined,
  }: Props = $props();
</script>

<header class="flex shrink-0 items-center gap-2 border-b px-4 py-2">
  <SidebarTrigger label={t('shell.sidebar.toggle')} />

  <Breadcrumb label={navigation.breadcrumbLabel}>
    <BreadcrumbList>
      {#each navigation.breadcrumbs as item, index (item.id)}
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
              onpointerenter={() => {
                onPreload(item.target);
              }}
              onfocus={() => {
                onPreload(item.target);
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

  <div class="ms-auto flex items-center gap-2">
    {#if updateAvailable}
      <Button variant="outline" size="sm" disabled={updateOpening} onclick={onOpenUpdate}>
        {#if updateOpening}
          <Spinner />
        {:else}
          <CircleArrowUpIcon aria-hidden="true" />
        {/if}
        {t('shell.updateAvailable')}
      </Button>
    {/if}

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
