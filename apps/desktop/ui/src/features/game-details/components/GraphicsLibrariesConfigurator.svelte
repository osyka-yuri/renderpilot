<script lang="ts">
  import type { VendorBlock, VendorKey } from '@features/game-details/lib/graphics-configurator';
  import Accordion, { type AccordionItem } from '@shared/ui/Accordion.svelte';
  import Badge from '@shared/ui/Badge.svelte';
  import LibrarySectionCard from './LibrarySectionCard.svelte';

  type AccordionPanelItem = {
    value: string;
  };

  type LibrarySection = VendorBlock['sections'][number];

  type SelectionMap = Record<string, string>;

  type SelectionKeyFactory = (componentId: string, controlId: string) => string;
  type VendorChangeHandler = (vendorKey: VendorKey | null) => void;
  type ArtifactSelectionHandler = (componentId: string, value: string) => void;
  type NvapiSelectionHandler = (componentId: string, controlId: string, value: string) => void;
  type BuildPlanHandler = (componentId: string, artifactId: string) => void;

  const VENDOR_KEYS = ['nvidia', 'amd', 'intel', 'other'] as const satisfies readonly VendorKey[];
  const VALID_VENDOR_KEYS = new Set<string>(VENDOR_KEYS);

  const noop = () => undefined;

  type Props = {
    vendorBlocks?: VendorBlock[];
    accordionItems?: AccordionItem[];
    activeVendorKey?: VendorKey | null;
    selectedArtifacts?: SelectionMap;
    selectedNvapiSelections?: SelectionMap;
    riskLevel?: string | null | undefined;
    busy?: boolean;
    selectionKey?: SelectionKeyFactory;
    onVendorChange?: VendorChangeHandler;
    onArtifactSelection?: ArtifactSelectionHandler;
    onNvapiSelection?: NvapiSelectionHandler;
    onBuildPlan?: BuildPlanHandler;
  };

  let {
    vendorBlocks = [],
    accordionItems = [],
    activeVendorKey = null,
    selectedArtifacts = {},
    selectedNvapiSelections = {},
    riskLevel = null,
    busy = false,
    selectionKey = (componentId, controlId) => `${componentId}:${controlId}`,
    onVendorChange = noop,
    onArtifactSelection = noop,
    onNvapiSelection = noop,
    onBuildPlan = noop,
  }: Props = $props();

  const vendorGroupsLabel = $derived(formatVendorGroupsLabel(vendorBlocks.length));
  const isEmpty = $derived(!vendorBlocks.some(hasSections));
  const vendorBlocksByKey = $derived(groupVendorBlocksByKey(vendorBlocks));

  function formatVendorGroupsLabel(count: number): string {
    return `${count} vendor group${count === 1 ? '' : 's'}`;
  }

  function hasSections(block: VendorBlock): boolean {
    return block.sections.length > 0;
  }

  function isVendorKey(value: unknown): value is VendorKey {
    return typeof value === 'string' && VALID_VENDOR_KEYS.has(value);
  }

  function groupVendorBlocksByKey(blocks: readonly VendorBlock[]): Map<VendorKey, VendorBlock[]> {
    const groupedBlocks = new Map<VendorKey, VendorBlock[]>();

    for (const block of blocks) {
      const group = groupedBlocks.get(block.key);

      if (group) {
        group.push(block);
      } else {
        groupedBlocks.set(block.key, [block]);
      }
    }

    return groupedBlocks;
  }

  function getVendorBlocksForAccordionItem(item: AccordionPanelItem): VendorBlock[] {
    if (!isVendorKey(item.value)) {
      return [];
    }

    return vendorBlocksByKey.get(item.value) ?? [];
  }

  function getVendorBlockRenderKey(block: VendorBlock, index: number): string {
    return `${block.key}:${index}`;
  }

  function getSectionRenderKey(section: LibrarySection): string {
    return `${section.libraryKey}:${section.nvapiOwnerId}`;
  }

  function handleVendorChange(nextValue: string | null): void {
    onVendorChange(isVendorKey(nextValue) ? nextValue : null);
  }
</script>

<section class="content-section" aria-labelledby="graphics-libraries-title">
  {#snippet itemContent(item: AccordionPanelItem)}
    {@const itemVendorBlocks = getVendorBlocksForAccordionItem(item)}

    {#each itemVendorBlocks as vendorBlock, vendorBlockIndex (getVendorBlockRenderKey(vendorBlock, vendorBlockIndex))}
      {#if vendorBlock.sections.length === 0}
        <div class="empty-state vendor-empty" role="status">
          No {vendorBlock.label} libraries detected for this installation yet.
        </div>
      {:else}
        <div class="library-configurator">
          {#each vendorBlock.sections as section (getSectionRenderKey(section))}
            <LibrarySectionCard
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
  {/snippet}

  <header class="section-head">
    <div>
      <p class="eyebrow">Libraries</p>
      <h3 id="graphics-libraries-title">Graphics libraries</h3>
      <p class="section-copy">Detected graphics stacks and compatible local replacements.</p>
    </div>

    <Badge surface="outline" tone="muted">{vendorGroupsLabel}</Badge>
  </header>

  {#if isEmpty}
    <div class="empty-state" role="status">
      No graphics-related components were detected for this installation.
    </div>
  {:else}
    <Accordion
      items={accordionItems}
      value={activeVendorKey}
      aria-label="Graphics vendors"
      onValueChange={handleVendorChange}
      {itemContent}
    />
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
    align-items: flex-end;
    justify-content: space-between;
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
    letter-spacing: 0.08em;
    text-transform: uppercase;
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

  .empty-state {
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
      align-items: flex-start;
      flex-direction: column;
    }
  }
</style>
