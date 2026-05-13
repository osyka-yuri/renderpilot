import type { VendorBlock, VendorKey } from '@features/graphics-configurator';
import type { BadgeVariant } from '@shared/ui';

export type VendorAccordionBadge = {
  label: string;
  variant?: BadgeVariant;
};

export type VendorAccordionItem = {
  value: VendorKey;
  title: string;
  meta?: string;
  summary: string;
  badges: VendorAccordionBadge[];
};

export type VendorAccordionState = {
  gameId: string | null;
  activeVendorKey: VendorKey | null;
  hasSelectedVendorManually: boolean;
};

export function createVendorAccordionState(gameId: string | null): VendorAccordionState {
  return {
    gameId,
    activeVendorKey: null,
    hasSelectedVendorManually: false,
  };
}

export function handleVendorChange(
  state: VendorAccordionState,
  vendorKey: VendorKey | null,
): VendorAccordionState {
  return {
    ...state,
    activeVendorKey: vendorKey,
    hasSelectedVendorManually: true,
  };
}

export function resolvePreferredVendorKey(blocks: VendorBlock[]): VendorKey {
  const populatedBlock = blocks.find((block) => block.sections.length > 0);

  if (populatedBlock !== undefined) {
    return populatedBlock.key;
  }

  if (blocks.length > 0) {
    return blocks[0].key;
  }

  return 'nvidia';
}

export function hasVendorKey(blocks: VendorBlock[], vendorKey: VendorKey): boolean {
  return blocks.some((block) => block.key === vendorKey);
}

export function isActiveVendorKeyUsable(
  blocks: VendorBlock[],
  vendorKey: VendorKey | null,
): boolean {
  return vendorKey === null || hasVendorKey(blocks, vendorKey);
}

export function hasVisibleVendorContent(block: VendorBlock): boolean {
  return block.key !== 'other' || block.sections.length > 0;
}

export function resolveEffectiveVendorKey(
  visibleVendorBlocks: VendorBlock[],
  vendorAccordionState: VendorAccordionState,
): VendorKey {
  const preferredVendorKey = resolvePreferredVendorKey(visibleVendorBlocks);

  const activeVendorKeyIsUsable = isActiveVendorKeyUsable(
    visibleVendorBlocks,
    vendorAccordionState.activeVendorKey,
  );

  if (vendorAccordionState.hasSelectedVendorManually && activeVendorKeyIsUsable) {
    return vendorAccordionState.activeVendorKey ?? preferredVendorKey;
  }

  return preferredVendorKey;
}

export function buildVendorAccordionItems(
  visibleVendorBlocks: VendorBlock[],
): VendorAccordionItem[] {
  return visibleVendorBlocks.map(buildVendorAccordionItem);
}

function buildVendorAccordionItem(vendorBlock: VendorBlock): VendorAccordionItem {
  const hasSections = vendorBlock.sections.length > 0;

  return {
    value: vendorBlock.key,
    title: vendorBlock.label,
    meta: hasSections ? formatFileCount(vendorBlock.totalFiles) : undefined,
    summary: vendorLibrarySummary(vendorBlock),
    badges: hasSections
      ? [
          {
            label: formatLibraryCount(vendorBlock.sections.length),
          },
          {
            label: formatReplacementCount(vendorBlock.totalCandidates),
            variant: vendorBlock.totalCandidates > 0 ? 'secondary' : 'outline',
          },
        ]
      : [{ label: 'Empty', variant: 'outline' }],
  };
}

function vendorLibrarySummary(vendorBlock: VendorBlock): string {
  const labels = vendorBlock.sections.map((section) => section.label);

  if (labels.length === 0) {
    return 'No detected libraries yet.';
  }

  if (labels.length <= 2) {
    return labels.join(' · ');
  }

  return `${labels.slice(0, 2).join(' · ')} +${labels.length - 2} more`;
}

function formatFileCount(count: number): string {
  return `${count} ${count === 1 ? 'file' : 'files'}`;
}

function formatLibraryCount(count: number): string {
  return `${count} ${count === 1 ? 'library' : 'libraries'}`;
}

function formatReplacementCount(count: number): string {
  return `${count} replacement ${count === 1 ? 'version' : 'versions'}`;
}
