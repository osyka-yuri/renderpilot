<script lang="ts">
  import type { NvApiControl } from '@entities/component';
  import {
    Alert,
    AlertDescription,
    AlertTitle,
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
  } from '@shared/ui';

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

  type SelectOption = NvApiControl['options'][number];

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

  function optionLabel(options: readonly SelectOption[], value: string, fallback: string): string {
    return options.find((option) => option.value === value)?.label ?? fallback;
  }
</script>

<section class="grid gap-3" aria-label="NVAPI driver profile controls">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="grid gap-1">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">NVAPI</p>
      <h5 class="text-sm/5 font-semibold text-foreground">Driver profile controls</h5>
    </div>

    <span class="text-sm/5 text-muted-foreground">Capability-based</span>
  </div>

  {#if hasControls}
    <div class="grid grid-cols-[repeat(auto-fit,minmax(220px,1fr))] gap-3">
      {#each controlItems as control (control.selectionId)}
        <div class="grid min-w-0 gap-2">
          <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">
            {control.label}
          </p>

          <Select
            type="single"
            items={control.options}
            value={control.value}
            disabled={controlsDisabled}
            onValueChange={(value: string) => {
              handleControlSelection(control.id, value);
            }}
          >
            <SelectTrigger size="sm" class="w-full" aria-label={`NVAPI ${control.label}`}>
              {optionLabel(control.options, control.value, 'Choose an option')}
            </SelectTrigger>
            <SelectContent>
              {#each control.options as option (option.value)}
                <SelectItem value={option.value} label={option.label}>{option.label}</SelectItem>
              {/each}
            </SelectContent>
          </Select>

          {#if control.description}
            <small class="block text-xs/snug wrap-break-word text-muted-foreground"
              >{control.description}</small
            >
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <Alert>
      <AlertTitle>No NVAPI controls</AlertTitle>
      <AlertDescription>No NVAPI controls are available for this component.</AlertDescription>
    </Alert>
  {/if}
</section>
