<script lang="ts">
  import type { VendorBlock, VendorKey } from '@features/graphics-configurator';
  import type { VendorAccordionItem } from '../../model/vendor-accordion';
  import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
    Badge,
    Card,
    CardContent,
    CardDescription,
    CardTitle,
  } from '@shared/ui';
  import LibrarySectionCard from './LibrarySectionCard.svelte';

  type LibrarySection = VendorBlock['sections'][number];

  type SelectionMap = Record<string, string>;

  type SelectionKeyFactory = (componentId: string, controlId: string) => string;
  type VendorChangeHandler = (vendorKey: VendorKey | null) => void;
  type ArtifactSelectionHandler = (componentId: string, value: string) => void;
  type NvapiSelectionHandler = (componentId: string, controlId: string, value: string) => void;
  type BuildPlanHandler = (componentId: string, artifactId: string) => void;

  const VENDOR_KEYS = ['nvidia', 'amd', 'intel', 'other'] as const satisfies readonly VendorKey[];
  const VALID_VENDOR_KEYS = new Set<string>(VENDOR_KEYS);

  type Props = {
    vendorBlocks?: VendorBlock[];
    accordionItems?: VendorAccordionItem[];
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
    onVendorChange = () => undefined,
    onArtifactSelection = () => undefined,
    onNvapiSelection = () => undefined,
    onBuildPlan = () => undefined,
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

  function getVendorBlocksForAccordionItem(item: VendorAccordionItem): VendorBlock[] {
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

  function handleVendorChange(nextValue: string): void {
    onVendorChange(isVendorKey(nextValue) ? nextValue : null);
  }
</script>

<section class="grid gap-3" aria-labelledby="graphics-libraries-title">
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="grid gap-1">
      <p class="text-xs font-medium tracking-wider text-muted-foreground uppercase">Libraries</p>
      <h3 id="graphics-libraries-title" class="text-base/5 font-semibold text-foreground">
        Graphics libraries
      </h3>
      <p class="text-sm text-muted-foreground">
        Detected graphics stacks and compatible local replacements.
      </p>
    </div>
    <Badge variant="outline">{vendorGroupsLabel}</Badge>
  </div>

  {#if isEmpty}
    <Card>
      <CardContent role="status">
        <CardTitle>No graphics components detected</CardTitle>
        <CardDescription>
          No graphics-related components were detected for this installation.
        </CardDescription>
      </CardContent>
    </Card>
  {:else}
    <Accordion
      type="single"
      value={activeVendorKey ?? undefined}
      aria-label="Graphics vendors"
      onValueChange={handleVendorChange}
      class="w-full"
    >
      {#each accordionItems as item (item.value)}
        <AccordionItem value={item.value}>
          <AccordionTrigger>
            <div class="grid w-full gap-2 text-left">
              <div class="flex flex-wrap items-start justify-between gap-3">
                <div class="grid gap-1">
                  <span class="text-sm/5 font-semibold text-foreground">{item.title}</span>
                  <span class="text-xs/snug text-muted-foreground">{item.summary}</span>
                </div>

                <div class="flex flex-wrap items-center justify-end gap-2">
                  {#if item.meta}
                    <span class="text-xs text-muted-foreground">{item.meta}</span>
                  {/if}

                  {#each item.badges as badge (`${badge.label}-${badge.variant ?? 'default'}`)}
                    <Badge variant={badge.variant}>
                      {badge.label}
                    </Badge>
                  {/each}
                </div>
              </div>
            </div>
          </AccordionTrigger>

          <AccordionContent>
            {@const itemVendorBlocks = getVendorBlocksForAccordionItem(item)}

            <div class="grid gap-3">
              {#each itemVendorBlocks as vendorBlock, vendorBlockIndex (getVendorBlockRenderKey(vendorBlock, vendorBlockIndex))}
                {#if vendorBlock.sections.length === 0}
                  <Card>
                    <CardContent role="status">
                      <CardTitle>No {vendorBlock.label} libraries detected</CardTitle>
                      <CardDescription>
                        This vendor group does not expose any compatible libraries for the current
                        installation yet.
                      </CardDescription>
                    </CardContent>
                  </Card>
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
            </div>
          </AccordionContent>
        </AccordionItem>
      {/each}
    </Accordion>
  {/if}
</section>
