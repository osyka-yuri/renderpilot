<script lang="ts">
  import type { ScreenHandler, VoidHandler } from '@shared/utils/callbacks';
  import Badge from '@shared/ui/Badge.svelte';
  import Button from '@shared/ui/Button.svelte';
  import type { Screen } from '@app/routes/screen';

  const noop: VoidHandler = (): void => {};
  const noopNavigate: ScreenHandler = (_screen: Screen): void => {};

  export let screen: Screen;
  export let busy = false;
  export let selectedGameTitle: string | null = null;
  export let errorMessage = '';
  export let onNavigate: ScreenHandler = noopNavigate;
  export let onBack: VoidHandler = noop;

  $: workspaceCopy = resolveWorkspaceCopy(screen, selectedGameTitle);
  $: canGoBack = screen !== 'games';
  $: showSettingsButton = screen !== 'settings';
  $: showError = !!errorMessage;

  function handleNavigate(nextScreen: Screen): void {
    onNavigate(nextScreen);
  }

  function resolveWorkspaceCopy(
    currentScreen: Screen,
    gameTitle: string | null,
  ): {
    label: string;
    heading: string;
  } {
    switch (currentScreen) {
      case 'details':
        return {
          label: 'Game',
          heading: gameTitle ?? 'Selected Game',
        };
      case 'settings':
        return {
          label: 'Settings',
          heading: 'Settings',
        };
      case 'operations':
        return {
          label: 'Journal',
          heading: gameTitle ?? 'Operation Journal',
        };
      default:
        return {
          label: 'Library',
          heading: 'Games',
        };
    }
  }
</script>

<div class="shell">
  <header class="app-header">
    <div class="command-row">
      <div class="nav-group">
        {#if canGoBack}
          <Button ariaLabel="Go back" iconOnly onclick={onBack}>
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

        <div class="title-stack">
          <p class="eyebrow">RenderPilot / {workspaceCopy.label}</p>
          <h1>{workspaceCopy.heading}</h1>
        </div>
      </div>

      <div class="action-group">
        {#if busy}
          <Badge pill dot tone="warning">Working</Badge>
        {/if}

        {#if showSettingsButton}
          <Button ariaLabel="Open settings" iconOnly onclick={() => handleNavigate('settings')}>
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
      <div class="info-bar" data-tone="danger" role="alert">
        <Badge pill dot tone="danger">Needs attention</Badge>
        <p class="feedback-copy error">{errorMessage}</p>
      </div>
    {/if}
  </header>

  <main class="workspace-body">
    <slot />
  </main>
</div>

<style>
  .shell {
    min-height: 100vh;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    align-items: start;
    gap: var(--space-3);
    padding: var(--space-4);
    max-width: min(var(--app-shell-width), 100%);
    margin: 0 auto;
  }

  .app-header {
    position: sticky;
    top: 0;
    z-index: 20;
    align-self: start;
    display: grid;
    gap: var(--space-2);
  }

  .command-row {
    display: flex;
    justify-content: space-between;
    gap: var(--space-3);
    align-items: center;
    min-height: 4rem;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--bg-layer) 86%, transparent);
    border: 1px solid var(--border-subtle);
    box-shadow: var(--shadow-card);
    backdrop-filter: blur(20px) saturate(120%);
  }

  .nav-group {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    flex: 1;
    min-height: 0;
  }

  .title-stack {
    display: grid;
    gap: 0.12rem;
    min-width: 0;
  }

  .action-group {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    flex-shrink: 0;
    margin-left: auto;
  }

  .eyebrow {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.6875rem;
    color: var(--text-subtle);
  }

  h1 {
    margin: 0;
    color: var(--text-strong);
    font-size: 1.45rem;
    line-height: 1.15;
    font-weight: 600;
  }

  .feedback-copy {
    margin: 0;
    color: var(--text-muted);
  }

  .info-bar {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    min-height: 2.4rem;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--bg-layer) 92%, transparent);
    box-shadow: var(--shadow-card);
    backdrop-filter: blur(18px) saturate(120%);
  }

  .info-bar[data-tone='danger'] {
    border-color: color-mix(in srgb, var(--danger) 30%, var(--border-subtle));
    background: color-mix(in srgb, var(--danger) 8%, var(--bg-layer));
  }

  .feedback-copy {
    min-width: 0;
    font-size: 0.8125rem;
    line-height: 1.3;
  }

  .feedback-copy.error {
    color: var(--danger);
  }

  .workspace-body {
    display: grid;
    gap: var(--space-4);
    min-width: 0;
  }

  @media (max-width: 1080px) {
    .command-row {
      align-items: center;
    }
  }

  @media (max-width: 720px) {
    .shell {
      padding: var(--space-3);
    }

    .command-row {
      padding: var(--space-3);
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .nav-group {
      width: 100%;
      align-items: flex-start;
    }

    .action-group {
      width: 100%;
      justify-content: flex-end;
      margin-left: 0;
    }

    .info-bar {
      align-items: flex-start;
      flex-direction: column;
      gap: var(--space-2);
    }

    h1 {
      font-size: 1.25rem;
    }
  }
</style>
