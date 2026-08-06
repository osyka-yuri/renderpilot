import { describe, expect, it } from 'vitest';

import { createGameDetails, type GameLibraryComponent } from '@entities/game';

import {
  ADDONS_TAB_VALUE,
  createGameDetailsTabs,
  createVendorTabs,
  reconcileGameDetailsTabValue,
} from './game-details-tabs';

function component(id: string, technology: string): GameLibraryComponent {
  return {
    id,
    game_id: 'game:test',
    kind: 'library',
    technology,
    swappability: 'swappable',
    files: [],
    rollback_available: false,
    d3d12_executable_status: null,
  };
}

describe('createVendorTabs', () => {
  it('groups and sorts without mutating the component source order', () => {
    const components = [
      component('fg', 'dlss_frame_generation'),
      component('amd', 'amd_fsr'),
      component('sr', 'dlss_super_resolution'),
      component('streamline', 'nvidia_streamline'),
    ];
    const tabs = createVendorTabs(createGameDetails({ components }));

    expect(tabs.map(({ key }) => key)).toEqual(['nvidia', 'amd']);
    expect(tabs[0]?.components.map(({ id }) => id)).toEqual(['sr', 'fg', 'streamline']);
    expect(tabs[1]?.components.map(({ id }) => id)).toEqual(['amd']);
    expect(components.map(({ id }) => id)).toEqual(['fg', 'amd', 'sr', 'streamline']);
  });
});

describe('createGameDetailsTabs', () => {
  it.each([
    { capabilities: [], expected: null },
    { capabilities: ['renodx'] as const, expected: ['renodx'] },
    { capabilities: ['luma'] as const, expected: ['luma'] },
    { capabilities: ['luma', 'renodx', 'luma'] as const, expected: ['renodx', 'luma'] },
  ])('derives a canonical add-on tab for $capabilities', ({ capabilities, expected }) => {
    const tabs = createGameDetailsTabs(
      createGameDetails({ addon_capabilities: [...capabilities] }),
    );

    expect(tabs.addonsTab?.capabilities ?? null).toEqual(expected);
    expect(tabs.values).toEqual(expected ? [ADDONS_TAB_VALUE] : []);
  });

  it('keeps the additional-library vendor distinct from the add-on tab', () => {
    const tabs = createGameDetailsTabs(
      createGameDetails({
        components: [component('unknown-vendor', 'custom_upscaler')],
        addon_capabilities: ['renodx'],
      }),
    );

    expect(tabs.values).toEqual(['other', ADDONS_TAB_VALUE]);
    expect(new Set(tabs.values).size).toBe(tabs.values.length);
  });
});

describe('reconcileGameDetailsTabValue', () => {
  it('keeps an available selection and otherwise falls back safely', () => {
    expect(reconcileGameDetailsTabValue('amd', ['nvidia', 'amd', ADDONS_TAB_VALUE])).toBe('amd');
    expect(reconcileGameDetailsTabValue(ADDONS_TAB_VALUE, ['nvidia'])).toBe('nvidia');
    expect(reconcileGameDetailsTabValue('missing', [])).toBe('');
  });
});
