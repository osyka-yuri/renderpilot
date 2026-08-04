import { describe, expect, it } from 'vitest';

import { createGameDetails, type GameLibraryComponent } from '@entities/game';

import { createVendorTabs } from './game-details-tabs';

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
