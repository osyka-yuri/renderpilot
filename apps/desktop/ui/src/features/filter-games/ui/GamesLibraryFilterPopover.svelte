<script lang="ts">
  import { cn } from '@shared/utils';
  import type { LibraryFilterOption } from '@entities/game';

  import { Badge, BadgeGroup, Button, Surface } from '@shared/ui';

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

  const {
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

  function chipClass(selected: boolean): string {
    return cn(
      'px-2.5 py-1',
      'rounded-full border border-border-control',
      'bg-bg-control text-text-muted',
      'cursor-pointer text-xs/tight select-none',
      'transition duration-160',
      'hover:border-border-strong hover:bg-bg-control-hover hover:text-text-strong',
      'focus-visible:border-accent-outline focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg-base focus-visible:outline-none',
      selected && 'border-accent bg-accent-soft text-accent-strong',
    );
  }
</script>

<div class="grid gap-3 p-3">
  <Surface radius="md">
    <p class="text-xs tracking-widest text-text-subtle uppercase">
      {LIBRARIES_LABEL}
    </p>

    <BadgeGroup role="group" aria-label={LIBRARIES_LABEL}>
      {#if hasLibraryOptions}
        {#each libraryFilterOptions as option (option.value)}
          {@const selected = isLibrarySelected(option.value)}

          <button
            type="button"
            class={chipClass(selected)}
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

    <div class="h-px bg-border-subtle" aria-hidden="true"></div>

    <div class="flex items-center justify-end gap-2">
      <Button variant="secondary" size="sm" onclick={handleCancel}>Cancel</Button>
      <Button variant="primary" size="sm" onclick={handleApply}>Apply</Button>
    </div>
  </Surface>
</div>
