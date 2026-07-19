import { describe, expect, it } from 'vitest';

import {
  canCheckVulkanLayerUpdates,
  describeReshadeHost,
  getAddonDescriptionKey,
  isManagedVulkanLayer,
  riskMessage,
  vulkanLayerHostDescription,
} from './reshade-presenters';
import type { HostFacts } from '@entities/addon';

import type { VulkanLayerDetection } from './types';

const PRESENT_HOST_FACTS = {
  path: 'C:\\Games\\Game\\dxgi.dll',
  slot: 'dxgi.dll',
  version: '6.5.1',
  addon_support: 'full',
  active: true,
  channel: {
    selected: 'stable',
    detected: 'stable',
  },
  update_status: 'current',
  is_custom_build: false,
} satisfies HostFacts;

describe('reshade presenters', () => {
  it('surfaces host conflicts as the blocking description', () => {
    expect(
      describeReshadeHost({
        detection: 'conflict',
        facts: PRESENT_HOST_FACTS,
      }),
    ).toBe('Multiple ReShade hosts found — active slot needs review');
  });

  it('builds a host description from version, support, and update status', () => {
    expect(
      describeReshadeHost({
        detection: 'present',
        facts: {
          ...PRESENT_HOST_FACTS,
          version: '6.4.0',
          addon_support: 'limited',
          update_status: 'repair_available',
        },
      }),
    ).toBe('6.4.0 · add-ons not supported · Repair ReShade for RenoDX add-on support');
  });

  it('labels a recognized custom build plainly, ignoring conflict/update wording', () => {
    expect(
      describeReshadeHost({
        // The backend reports a recognized custom build as a conflict (the
        // slot is never safe to write to); the frontend must still show the
        // plain custom-build label, not the generic conflict message.
        detection: 'conflict',
        facts: {
          ...PRESENT_HOST_FACTS,
          is_custom_build: true,
        },
      }),
    ).toBe('Custom build (e.g. GShade) — you manage updates yourself');
  });

  it('labels a host channel mismatch as a channel change, not a generic update', () => {
    expect(
      describeReshadeHost({
        detection: 'present',
        facts: {
          ...PRESENT_HOST_FACTS,
          update_status: 'channel_mismatch',
        },
      }),
    ).toBe('6.5.1 · Channel change available');
  });

  it('derives the add-on description key from config and install state', () => {
    expect(
      getAddonDescriptionKey(
        {
          present_on_disk: true,
          expected_path: 'C:\\Games\\Game\\renodx.addon64',
          discovered_path: 'C:\\Games\\Game\\renodx.addon64',
          enabled_by_config: false,
          load_mode: 'auto_search',
        },
        true,
      ),
    ).toBe('gameDetails.renodx.component.addonDisabled');
    expect(getAddonDescriptionKey(null, false)).toBe(
      'gameDetails.renodx.component.addonFileInstall',
    );
    expect(getAddonDescriptionKey(null, true)).toBe('gameDetails.renodx.component.addonDesc');
  });

  it('renders a catalogued risk message interpolated with the RenoDX display name', () => {
    expect(riskMessage({ severity: 'info', message_key: 'addon.risk.sp_safe' })).toBe(
      'No known anti-cheat signatures were found — installing RenoDX is likely safe, but not guaranteed.',
    );
  });

  it('falls back to the severity default for an uncatalogued message key', () => {
    expect(riskMessage({ severity: 'warn', message_key: 'does.not.exist' })).toBe(
      'Anti-cheat detected — installing may risk a ban.',
    );
  });

  it('treats installed and installed_disabled as managed Vulkan layers', () => {
    expect(isManagedVulkanLayer('installed')).toBe(true);
    expect(isManagedVulkanLayer('installed_disabled')).toBe(true);
    expect(isManagedVulkanLayer('not_installed')).toBe(false);
    expect(isManagedVulkanLayer('conflict')).toBe(false);
    expect(isManagedVulkanLayer(null)).toBe(false);
  });

  it('formats a host version only for managed layer detections', () => {
    expect(vulkanLayerHostDescription('installed', '6.5.1')).toBe('6.5.1');
    expect(vulkanLayerHostDescription('installed_disabled', null)).toBe('Version unknown');
    expect(vulkanLayerHostDescription('not_installed', '6.5.1')).toBeNull();
    expect(vulkanLayerHostDescription(null, '6.5.1')).toBeNull();
  });

  it('offers check-for-updates only when detection is present and supported', () => {
    const cases: [VulkanLayerDetection | null, boolean][] = [
      [null, false],
      ['not_installed', false],
      ['unsupported', false],
      ['installed', true],
      ['installed_disabled', true],
      ['conflict', true],
      ['external_read_only', true],
    ];
    for (const [detection, expected] of cases) {
      expect(canCheckVulkanLayerUpdates(detection)).toBe(expected);
    }
  });
});
