<script lang="ts">
  import type { NvApiControl } from '@entities/component';
  import { EmptyStatePanel, InfoTile, Select, SectionHeader } from '@shared/ui';

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

  const {
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

<section
  class="grid gap-3 rounded-2xl border border-border-subtle bg-accent-soft p-4"
  aria-label="NVAPI driver profile controls"
>
  <SectionHeader eyebrow="NVAPI" title="Driver profile controls" class="px-0">
    <span class="text-text-muted">Capability-based</span>
  </SectionHeader>

  {#if hasControls}
    <div class="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
      {#each controlItems as control (control.selectionId)}
        <InfoTile as="label" label={control.label}>
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
            <small class="block text-xs/snug wrap-break-word text-text-muted"
              >{control.description}</small
            >
          {/if}
        </InfoTile>
      {/each}
    </div>
  {:else}
    <EmptyStatePanel role="status">
      No NVAPI controls are available for this component.
    </EmptyStatePanel>
  {/if}
</section>
