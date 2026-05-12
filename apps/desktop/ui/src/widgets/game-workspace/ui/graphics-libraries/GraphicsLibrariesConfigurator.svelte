<script lang="ts">
  import type { VendorBlock, VendorKey } from '@features/graphics-configurator';
  import { Accordion, Badge, EmptyStatePanel, SectionHeader, type AccordionItem } from '@shared/ui';
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

  const {
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

  function groupVendorBlocksByKey(
    blocks: readonly VendorBlock[],
  ): Partial<Record<VendorKey, VendorBlock[]>> {
    const groupedBlocks: Partial<Record<VendorKey, VendorBlock[]>> = {};

    for (const block of blocks) {
      const group = groupedBlocks[block.key];

      if (group) {
        group.push(block);
      } else {
        groupedBlocks[block.key] = [block];
      }
    }

    return groupedBlocks;
  }

  function getVendorBlocksForAccordionItem(item: AccordionPanelItem): VendorBlock[] {
    if (!isVendorKey(item.value)) {
      return [];
    }

    return vendorBlocksByKey[item.value] ?? [];
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

<section class="grid gap-3" aria-labelledby="graphics-libraries-title">
  {#snippet itemContent(item: AccordionPanelItem)}
    {@const itemVendorBlocks = getVendorBlocksForAccordionItem(item)}

    {#each itemVendorBlocks as vendorBlock, vendorBlockIndex (getVendorBlockRenderKey(vendorBlock, vendorBlockIndex))}
      {#if vendorBlock.sections.length === 0}
        <EmptyStatePanel role="status">
          No {vendorBlock.label} libraries detected for this installation yet.
        </EmptyStatePanel>
      {:else}
        <div class="grid gap-3">
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

  <SectionHeader
    eyebrow="Libraries"
    title="Graphics libraries"
    titleId="graphics-libraries-title"
    description="Detected graphics stacks and compatible local replacements."
  >
    <Badge surface="outline" tone="muted">{vendorGroupsLabel}</Badge>
  </SectionHeader>

  {#if isEmpty}
    <EmptyStatePanel role="status">
      No graphics-related components were detected for this installation.
    </EmptyStatePanel>
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
