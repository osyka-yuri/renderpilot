import type { GameDetails, GameGraphicsComponent } from '@entities/game';
import {
  libraryVendorOrder,
  libraryVendorKey,
  vendorLabelForLibraryVendorKey,
  type LibraryVendorKey,
} from '@shared/graphics';
import type { SettingFamily } from '@features/nvapi-settings';

export const NVIDIA_STREAMLINE_TECHNOLOGY = 'nvidia_streamline';

// Each DLSS DLL family is its own card (physical DLL swap + NVAPI driver
// overrides), exactly like Super Resolution — keyed off the component's
// technology. Streamline (sl.*.dll) is unrelated and keeps its own card.
export const DLSS_FAMILY_CARDS: Record<string, { family: SettingFamily; title: string }> = {
  dlss_super_resolution: { family: 'sr', title: 'NVIDIA DLSS Super Resolution' },
  dlss_ray_reconstruction: { family: 'rr', title: 'NVIDIA DLSS Ray Reconstruction' },
  dlss_frame_generation: { family: 'fg', title: 'NVIDIA DLSS Frame Generation' },
};

// Defines the display order of component technologies within their vendor tab.
// Technologies not in this list fall back to alphabetical sorting by ID.
const COMPONENT_TECHNOLOGY_ORDER: Record<string, number> = {
  dlss_super_resolution: 1,
  dlss_ray_reconstruction: 2,
  dlss_frame_generation: 3,
};

function compareComponents(a: GameGraphicsComponent, b: GameGraphicsComponent): number {
  const orderA = COMPONENT_TECHNOLOGY_ORDER[a.technology] ?? 999;
  const orderB = COMPONENT_TECHNOLOGY_ORDER[b.technology] ?? 999;

  if (orderA !== orderB) return orderA - orderB;
  return a.id.localeCompare(b.id);
}

export type VendorTab = {
  key: LibraryVendorKey;
  label: string;
  components: GameGraphicsComponent[];
};

/**
 * Derives the active graphics components for a game and groups them into
 * vendor-specific tabs. Sorts tabs according to a predefined vendor order,
 * and components within tabs by technology importance.
 */
export function createVendorTabs(details: GameDetails | null): VendorTab[] {
  if (!details) return [];

  const byVendor = libraryVendorOrder.reduce(
    (acc, key) => {
      acc[key] = [];
      return acc;
    },
    {} as Record<LibraryVendorKey, GameGraphicsComponent[]>,
  );

  for (const component of details.components) {
    const key = libraryVendorKey(component.technology);
    byVendor[key].push(component);
  }

  return libraryVendorOrder
    .map((key) => {
      const components = byVendor[key];
      components.sort(compareComponents);

      return {
        key,
        label: vendorLabelForLibraryVendorKey(key),
        components,
      };
    })
    .filter((tab) => tab.components.length > 0);
}
