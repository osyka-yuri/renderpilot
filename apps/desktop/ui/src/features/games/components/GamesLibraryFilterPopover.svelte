<script lang="ts">
  import type { LibraryFilterOption } from '@features/games/games-screen-model';

  import Badge from '@shared/ui/Badge.svelte';
  import BadgeGroup from '@shared/ui/BadgeGroup.svelte';
  import Button from '@shared/ui/Button.svelte';
  import Surface from '@shared/ui/Surface.svelte';

  type LibraryValue = string;
  type LibraryToggleHandler = (library: LibraryValue) => void;
  type VoidHandler = () => void;

  type Props = {
    libraryFilterOptions?: readonly LibraryFilterOption[];
    draftLibraries?: readonly LibraryValue[];
    onToggleLibrary?: LibraryToggleHandler;
    onCancel?: VoidHandler;
    onApply?: VoidHandler;
  };

  const LIBRARIES_LABEL = 'Libraries';
  const EMPTY_LIBRARIES_LABEL = 'No libraries detected';

  let {
    libraryFilterOptions = [],
    draftLibraries = [],
    onToggleLibrary,
    onCancel,
    onApply,
  }: Props = $props();

  const hasLibraryOptions = $derived(libraryFilterOptions.length > 0);
  const selectedLibraries = $derived.by(() => new Set(draftLibraries));

  function isLibrarySelected(library: LibraryValue): boolean {
    return selectedLibraries.has(library);
  }

  function getLibraryFromEvent(event: MouseEvent): LibraryValue | null {
    const trigger = event.currentTarget;

    if (!(trigger instanceof HTMLButtonElement)) {
      return null;
    }

    return trigger.dataset.library ?? null;
  }

  function handleLibraryToggle(event: MouseEvent): void {
    const library = getLibraryFromEvent(event);

    if (library === null) {
      return;
    }

    onToggleLibrary?.(library);
  }

  function handleCancel(): void {
    onCancel?.();
  }

  function handleApply(): void {
    onApply?.();
  }
</script>

<Surface class="filters-popover-shell" radius="md">
  <p class="filter-label">{LIBRARIES_LABEL}</p>

  <BadgeGroup class="filters-libraries" role="group" aria-label={LIBRARIES_LABEL}>
    {#if hasLibraryOptions}
      {#each libraryFilterOptions as option (option.value)}
        {@const selected = isLibrarySelected(option.value)}

        <button
          type="button"
          class="tech-chip"
          class:is-active={selected}
          data-library={option.value}
          aria-pressed={selected}
          onclick={handleLibraryToggle}
        >
          {option.label}
        </button>
      {/each}
    {:else}
      <Badge tone="muted">{EMPTY_LIBRARIES_LABEL}</Badge>
    {/if}
  </BadgeGroup>

  <div class="filters-popover-separator" aria-hidden="true"></div>

  <div class="filters-popover-actions">
    <Button variant="secondary" size="sm" onclick={handleCancel}>Cancel</Button>
    <Button variant="primary" size="sm" onclick={handleApply}>Apply</Button>
  </div>
</Surface>

<style>
  :global(.filters-popover-shell) {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-3);
    border: 0;
    background: transparent;
    box-shadow: none;
  }

  .filter-label {
    margin: 0;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  :global(.filters-libraries) {
    gap: var(--space-2);
    align-items: flex-start;
  }

  .tech-chip {
    appearance: none;
    padding: 0.28rem 0.62rem;
    border: 1px solid var(--border-control);
    border-radius: 999px;
    background: var(--bg-control);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.8125rem;
    line-height: 1.2;
    cursor: pointer;
    user-select: none;
    transition:
      border-color 160ms ease,
      background 160ms ease,
      color 160ms ease,
      box-shadow 160ms ease;
  }

  .tech-chip:hover {
    border-color: var(--border-strong);
    background: var(--bg-control-hover);
    color: var(--text-strong);
  }

  .tech-chip:focus-visible {
    outline: none;
    border-color: var(--accent-outline);
    box-shadow: var(--shadow-focus);
  }

  .tech-chip.is-active {
    border-color: color-mix(in srgb, var(--accent) 70%, var(--border-strong));
    background: color-mix(in srgb, var(--accent-soft) 72%, var(--bg-control));
    color: var(--accent-strong);
  }

  .filters-popover-separator {
    height: 1px;
    background: var(--border-subtle);
  }

  .filters-popover-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  @media (prefers-reduced-motion: reduce) {
    .tech-chip {
      transition: none;
    }
  }
</style>
