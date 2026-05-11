<script lang="ts">
  import type { NvApiControl } from '@entities/component';
  import { Select } from '@shared/ui';

  type NvApiControlViewModel = NvApiControl & {
    selectionId: string;
    value: string;
  };

  type Props = {
    controls?: NvApiControl[];
    ownerId?: string;
    selections?: Record<string, string>;
    busy?: boolean;
    selectionKey?: (componentId: string, controlId: string) => string;
    onNvapiSelection?: (componentId: string, controlId: string, value: string) => void;
  };

  let {
    controls = [],
    ownerId = '',
    selections = {},
    busy = false,
    selectionKey = (componentId, controlId) => `${componentId}:${controlId}`,
    onNvapiSelection = () => undefined,
  }: Props = $props();

  const hasControls = $derived(controls.length > 0);
  const controlsDisabled = $derived(busy || !ownerId);
  const controlItems = $derived(controls.map(toControlViewModel));

  function toControlViewModel(control: NvApiControl): NvApiControlViewModel {
    const selectionId = selectionKey(ownerId, control.id);

    return {
      ...control,
      selectionId,
      value: selections[selectionId] ?? control.defaultValue,
    };
  }

  function handleControlSelection(controlId: string, value: string) {
    if (controlsDisabled) {
      return;
    }

    onNvapiSelection(ownerId, controlId, value);
  }
</script>

<section class="nvapi-section" aria-label="NVAPI driver profile controls">
  <header class="subsection-head">
    <div>
      <p class="eyebrow">NVAPI</p>
      <strong>Driver profile controls</strong>
    </div>

    <span>Capability-based</span>
  </header>

  {#if hasControls}
    <div class="nvapi-grid">
      {#each controlItems as control (control.selectionId)}
        <label class="config-field">
          <span class="field-label">{control.label}</span>

          <Select
            size="sm"
            disabled={controlsDisabled}
            aria-label={`NVAPI ${control.label}`}
            options={control.options}
            value={control.value}
            onValueChange={(value: string) => {
              handleControlSelection(control.id, value);
            }}
          />

          {#if control.description}
            <small>{control.description}</small>
          {/if}
        </label>
      {/each}
    </div>
  {:else}
    <div class="empty-inline" role="status">
      No NVAPI controls are available for this component.
    </div>
  {/if}
</section>

<style>
  .nvapi-section {
    display: grid;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--accent-soft) 28%, var(--bg-soft));
  }

  .subsection-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-4);
  }

  .eyebrow {
    margin: 0 0 0.2rem;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .subsection-head strong {
    color: var(--text-strong);
  }

  .subsection-head span,
  small {
    color: var(--text-muted);
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

  .empty-inline {
    padding: var(--space-4);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
    color: var(--text-muted);
  }

  @media (max-width: 820px) {
    .subsection-head {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
