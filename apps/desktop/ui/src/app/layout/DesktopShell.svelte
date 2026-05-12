<script lang="ts">
  import type { Snippet } from 'svelte';
  import { type VoidHandler } from '@shared/utils';
  import type { ScreenHandler, Screen } from '@app/navigation/screen';

  import { Badge, Button } from '@shared/ui';

  type WorkspaceCopy = {
    label: string;
    heading: string;
  };

  const noop: VoidHandler = (): void => {
    return;
  };
  const noopNavigate: ScreenHandler = (): void => {
    return;
  };

  type Props = {
    screen: Screen;
    busy?: boolean;
    selectedGameTitle?: string | null;
    errorMessage?: string;
    onNavigate?: ScreenHandler;
    onBack?: VoidHandler;
    children?: Snippet;
  };

  const {
    screen,
    busy = false,
    selectedGameTitle = null,
    errorMessage = '',
    onNavigate = noopNavigate,
    onBack = noop,
    children,
  }: Props = $props();

  const workspaceCopy = $derived(resolveWorkspaceCopy(screen, selectedGameTitle));
  const normalizedErrorMessage = $derived(errorMessage.trim());
  const canGoBack = $derived(screen !== 'games');
  const showSettingsButton = $derived(screen !== 'settings');
  const showError = $derived(normalizedErrorMessage.length > 0);

  function handleOpenSettings(): void {
    onNavigate('settings');
  }

  function resolveWorkspaceCopy(currentScreen: Screen, gameTitle: string | null): WorkspaceCopy {
    switch (currentScreen) {
      case 'details':
        return {
          label: 'Game',
          heading: resolveHeading(gameTitle, 'Selected Game'),
        };

      case 'settings':
        return {
          label: 'Settings',
          heading: 'Settings',
        };

      case 'operations':
        return {
          label: 'Journal',
          heading: resolveHeading(gameTitle, 'Operation Journal'),
        };

      case 'games':
      default:
        return {
          label: 'Library',
          heading: 'Games',
        };
    }
  }

  function resolveHeading(value: string | null, fallback: string): string {
    return value?.trim() ?? fallback;
  }
</script>

<div
  class="
    mx-auto grid min-h-screen max-w-[min(var(--app-shell-width),100%)]
    grid-rows-[auto_minmax(0,1fr)] items-start gap-3 p-4
    max-md:p-3
  "
  aria-busy={busy}
>
  <header
    class="
      sticky top-4 z-20 grid gap-2 self-start
      max-md:top-3
    "
  >
    <div
      class="
        flex min-h-16 items-center justify-between gap-3 rounded-2xl border
        border-border-subtle bg-bg-layer/90 p-3 px-4 shadow-sm backdrop-blur-xl
        backdrop-saturate-120
        max-md:flex-wrap max-md:items-start max-md:p-3
      "
    >
      <div
        class="
          flex min-h-0 min-w-0 flex-1 items-center gap-3
          max-md:w-full max-md:items-start
        "
      >
        {#if canGoBack}
          <Button aria-label="Go back" iconOnly onclick={onBack}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M14.5 5.5L8 12l6.5 6.5"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </Button>
        {/if}

        <div class="grid min-w-0 gap-0.5">
          <p class="text-xs tracking-widest text-text-subtle uppercase">
            RenderPilot / {workspaceCopy.label}
          </p>
          <h1
            class="
              truncate text-2xl/tight font-semibold text-text-strong
              max-md:text-xl
            "
            title={workspaceCopy.heading}
          >
            {workspaceCopy.heading}
          </h1>
        </div>
      </div>

      <div
        class="
          ml-auto flex shrink-0 items-center gap-2
          max-md:ml-0 max-md:w-full max-md:justify-end
        "
      >
        {#if busy}
          <span class="inline-flex" role="status" aria-live="polite">
            <Badge pill dot tone="warning">Working</Badge>
          </span>
        {/if}

        {#if showSettingsButton}
          <Button aria-label="Open settings" iconOnly onclick={handleOpenSettings}>
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M9.6 3.4h4.8l.6 2.3c.5.1 1 .3 1.5.6l2.1-1.2 2.4 4.2-1.8 1.5c.1.5.1 1 .1 1.5s0 1-.1 1.5l1.8 1.5-2.4 4.2-2.1-1.2c-.5.3-1 .5-1.5.6l-.6 2.3H9.6L9 18.3c-.5-.1-1-.3-1.5-.6l-2.1 1.2L3 14.7l1.8-1.5c-.1-.5-.1-1-.1-1.5s0-1 .1-1.5L3 8.7l2.4-4.2 2.1 1.2c.5-.3 1-.5 1.5-.6l.6-2.3z"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linejoin="round"
              />
              <circle
                cx="12"
                cy="12"
                r="2.7"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
              />
            </svg>
          </Button>
        {/if}
      </div>
    </div>

    {#if showError}
      <div
        class="
          flex min-h-10 items-center gap-3 rounded-2xl border
          border-border-subtle bg-bg-layer/90 p-2 px-3 shadow-sm
          backdrop-blur-xl backdrop-saturate-120
          max-md:flex-col max-md:items-start max-md:gap-2
        "
        data-tone="danger"
        role="alert"
        aria-live="assertive"
      >
        <Badge pill dot tone="danger">Needs attention</Badge>
        <p class="min-w-0 text-xs/snug text-text-muted">
          {normalizedErrorMessage}
        </p>
      </div>
    {/if}
  </header>

  <main class="grid min-w-0 gap-4">
    {@render children?.()}
  </main>
</div>
