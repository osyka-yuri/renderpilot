<script lang="ts">
  import type { VendorBlock, VendorKey } from '@features/game-details/lib/graphics-configurator';
  import Accordion, { type AccordionItem } from '@shared/ui/Accordion.svelte';
  import Badge from '@shared/ui/Badge.svelte';
  import TechnologyLibraryCard from './TechnologyLibraryCard.svelte';

  type UnknownRecord = Record<PropertyKey, unknown>;

  export let vendorBlocks: VendorBlock[] = [];
  export let accordionItems: AccordionItem[] = [];
  export let activeVendorKey: VendorKey | null = null;
  export let selectedArtifacts: Record<string, string> = {};
  export let selectedNvapiSelections: Record<string, string> = {};
  export let riskLevel: string | null | undefined = null;
  export let busy = false;

  export let selectionKey: (componentId: string, controlId: string) => string = (
    componentId,
    controlId,
  ) => `${componentId}:${controlId}`;

  export let onVendorChange: (vendorKey: VendorKey | null) => void = () => {
    return;
  };
  export let onArtifactSelection: (componentId: string, value: string) => void = () => {
    return;
  };
  export let onNvapiSelection: (
    componentId: string,
    controlId: string,
    value: string,
  ) => void = () => {
    return;
  };
  export let onBuildPlan: (componentId: string, artifactId: string) => void = () => {
    return;
  };

  $: vendorGroupsCount = vendorBlocks.length;
  $: vendorGroupsLabel =
    vendorGroupsCount === 1 ? '1 vendor group' : `${vendorGroupsCount} vendor groups`;

  $: hasDetectedSections = vendorBlocks.some((block) => block.sections.length > 0);
  $: isEmpty = !hasDetectedSections;

  $: vendorBlocksByKey = groupVendorBlocksByKey(vendorBlocks);

  function isRecord(value: unknown): value is UnknownRecord {
    return typeof value === 'object' && value !== null;
  }

  function getAccordionItemValue(item: unknown): VendorKey | null {
    if (!isRecord(item)) {
      return null;
    }

    const value = item.value;

    return typeof value === 'string' ? (value as VendorKey) : null;
  }

  function groupVendorBlocksByKey(blocks: VendorBlock[]): Map<VendorKey, VendorBlock[]> {
    const grouped = new Map<VendorKey, VendorBlock[]>();

    for (const block of blocks) {
      const existingBlocks = grouped.get(block.key) ?? [];
      grouped.set(block.key, [...existingBlocks, block]);
    }

    return grouped;
  }

  function getVendorBlocksForAccordionItem(item: unknown): VendorBlock[] {
    const vendorKey = getAccordionItemValue(item);

    if (!vendorKey) {
      return [];
    }

    return vendorBlocksByKey.get(vendorKey) ?? [];
  }

  function handleVendorChange(nextValue: string | null) {
    onVendorChange(nextValue as VendorKey | null);
  }
</script>

<section class="content-section" aria-labelledby="graphics-libraries-title">
  <header class="section-head">
    <div>
      <p class="eyebrow">Libraries</p>
      <h3 id="graphics-libraries-title">Graphics libraries</h3>
      <p class="section-copy">Detected graphics stacks and compatible local replacements.</p>
    </div>

    <Badge surface="outline" tone="muted">{vendorGroupsLabel}</Badge>
  </header>

  {#if isEmpty}
    <div class="empty-inline" role="status">
      No graphics-related components were detected for this installation.
    </div>
  {:else}
    <Accordion
      items={accordionItems}
      value={activeVendorKey}
      ariaLabel="Graphics vendors"
      onValueChange={handleVendorChange}
      let:item
    >
      {#each getVendorBlocksForAccordionItem(item) as vendorBlock (vendorBlock.key)}
        {#if vendorBlock.sections.length === 0}
          <div class="vendor-empty" role="status">
            No {vendorBlock.label} technologies detected for this installation yet.
          </div>
        {:else}
          <div class="library-configurator">
            {#each vendorBlock.sections as section}
              <TechnologyLibraryCard
                {section}
                {selectedArtifacts}
                {selectedNvapiSelections}
                {riskLevel}
                {busy}
                {selectionKey}
                {onArtifactSelection}
                {onNvapiSelection}
                {onBuildPlan}
              />
            {/each}
          </div>
        {/if}
      {/each}
    </Accordion>
  {/if}
</section>

<style>
  .content-section,
  .library-configurator {
    display: grid;
    gap: var(--space-3);
  }

  .section-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: var(--space-4);
    padding: 0 var(--space-1);
  }

  .eyebrow,
  h3,
  .section-copy {
    margin: 0;
  }

  .eyebrow {
    margin-bottom: 0.2rem;
    color: var(--text-subtle);
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  h3 {
    color: var(--text-strong);
    font-size: 1.05rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .section-copy {
    max-width: 48rem;
    margin-top: 0.25rem;
    color: var(--text-muted);
    font-size: 0.84rem;
    line-height: 1.45;
  }

  .empty-inline,
  .vendor-empty {
    padding: var(--space-4);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-xl);
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
    color: var(--text-muted);
    box-shadow: none;
  }

  .vendor-empty {
    border-radius: 0.75rem;
  }

  @media (max-width: 820px) {
    .section-head {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
