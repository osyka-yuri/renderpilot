<script lang="ts">
  import type { VendorKey, VendorBlock } from '@features/game-details/lib/graphics-configurator';
  import Accordion, { type AccordionItem } from '@shared/ui/Accordion.svelte';
  import Badge from '@shared/ui/Badge.svelte';
  import TechnologyLibraryCard from './TechnologyLibraryCard.svelte';

  export let vendorBlocks: VendorBlock[] = [];
  export let accordionItems: AccordionItem[] = [];
  export let activeVendorKey: VendorKey | null = null;
  export let selectedArtifacts: Record<string, string> = {};
  export let selectedNvapiSelections: Record<string, string> = {};
  export let riskLevel: string | null | undefined = null;
  export let busy = false;
  export let selectionKey: (componentId: string, controlId: string) => string;
  export let onVendorChange: (vendorKey: VendorKey | null) => void;
  export let onArtifactSelection: (componentId: string, value: string) => void;
  export let onNvapiSelection: (componentId: string, controlId: string, value: string) => void;
  export let onBuildPlan: (componentId: string, artifactId: string) => void;

  $: isEmpty = vendorBlocks.every((block) => block.sections.length === 0);
</script>

<section class="content-section">
  <div class="section-head">
    <div>
      <p class="eyebrow">Libraries</p>
      <h3>Graphics libraries</h3>
      <p class="section-copy">Detected graphics stacks and compatible local replacements.</p>
    </div>
    <Badge surface="outline" tone="muted">{vendorBlocks.length} vendor groups</Badge>
  </div>

  {#if isEmpty}
    <div class="empty-inline">No graphics-related components were detected for this installation.</div>
  {:else}
    <Accordion
      items={accordionItems}
      value={activeVendorKey}
      ariaLabel="Graphics vendors"
      onValueChange={(nextValue) => onVendorChange(nextValue as VendorKey)}
      let:item
    >
      {#each vendorBlocks.filter((block) => block.key === item.value) as vendorBlock}
        {#if vendorBlock.sections.length === 0}
          <div class="vendor-empty">No {vendorBlock.label} technologies detected for this installation yet.</div>
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
    gap: 1rem;
    align-items: end;
    padding: 0 var(--space-1);
  }

  .eyebrow {
    margin: 0 0 0.2rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-subtle);
    font-size: 0.6875rem;
  }

  h3 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .section-copy,
  .empty-inline {
    margin: 0.25rem 0 0;
    color: var(--text-muted);
  }

  .section-copy {
    max-width: 48rem;
    font-size: 0.84rem;
    line-height: 1.45;
  }

  .empty-inline,
  .vendor-empty {
    padding: var(--space-4);
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-xl);
    background: color-mix(in srgb, var(--bg-card) 62%, transparent);
    box-shadow: none;
    color: var(--text-muted);
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
