<script lang="ts">
  import type { NvApiControl } from '@features/game-details/lib/graphics-configurator';
  import Select from '@shared/ui/Select.svelte';

  export let controls: NvApiControl[] = [];
  export let ownerId = '';
  export let selections: Record<string, string> = {};
  export let busy = false;
  export let selectionKey: (componentId: string, controlId: string) => string;
  export let onNvapiSelection: (componentId: string, controlId: string, value: string) => void;
</script>

<div class="nvapi-section">
  <div class="subsection-head">
    <div>
      <p class="eyebrow">NVAPI</p>
      <strong>Driver profile controls</strong>
    </div>
    <span>Capability-based</span>
  </div>

  <div class="nvapi-grid">
    {#each controls as control}
      <label class="config-field">
        <span class="field-label">{control.label}</span>
        <Select
          size="sm"
          disabled={busy}
          ariaLabel={control.label}
          options={control.options}
          value={selections[selectionKey(ownerId, control.id)] ?? control.defaultValue}
          onValueChange={(value) => onNvapiSelection(ownerId, control.id, value)}
        />
        <small>{control.description}</small>
      </label>
    {/each}
  </div>
</div>

<style>
  .eyebrow {
    margin: 0 0 0.2rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-subtle);
    font-size: 0.6875rem;
  }

  .subsection-head {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }

  .subsection-head strong {
    color: var(--text-strong);
  }

  .subsection-head span,
  small {
    color: var(--text-muted);
  }

  .nvapi-section {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--accent-soft) 28%, var(--bg-soft));
  }

  .nvapi-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-3);
  }

  .config-field {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
    padding: var(--space-3);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-soft);
  }

  .field-label {
    display: block;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  small {
    display: block;
    font-size: 0.78rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  @media (max-width: 820px) {
    .subsection-head {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
