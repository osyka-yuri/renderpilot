import { describe, expect, it } from 'vitest';

import {
  canSwitchReshadeChannel,
  getAddonDescriptionKey,
  getReshadeDescription,
  getReshadeSwitchTarget,
  humanizeMessageKey,
  isReshadeSwitchDisabled,
  riskFallbackKey,
} from './reshade-presenters';
import type { ReshadeHost } from './types';

const PRESENT_HOST = {
  status: 'present',
  path: 'C:\\Games\\Game\\dxgi.dll',
  slot: 'dxgi.dll',
  version: '6.5.1',
  addon_support: 'full',
  identity: 'confirmed',
  active: {
    state: 'active',
    reason: 'detected_by_matcher',
  },
} satisfies ReshadeHost;

describe('reshade presenters', () => {
  it('surfaces host conflicts as the blocking description', () => {
    expect(
      getReshadeDescription({
        host: PRESENT_HOST,
        action: 'up_to_date',
        conflict: true,
        ownership: { kind: 'managed', health: 'healthy' },
      }),
    ).toEqual({
      kind: 'conflict',
      key: 'gameDetails.renodx.host.conflictMultiple',
    });
  });

  it('builds a stable host description from version, health, support, and action', () => {
    expect(
      getReshadeDescription({
        host: {
          ...PRESENT_HOST,
          version: '6.4.0',
          addon_support: 'none',
        },
        action: 'reinstall_with_addon_support',
        conflict: false,
        ownership: { kind: 'managed', health: 'missing' },
      }),
    ).toEqual({
      kind: 'parts',
      fallbackKey: 'gameDetails.renodx.host.versionUnknown',
      parts: [
        {
          kind: 'version',
          key: 'gameDetails.renodx.host.version',
          version: '6.4.0',
        },
        {
          kind: 'message',
          key: 'gameDetails.renodx.host.health.missing',
        },
        {
          kind: 'message',
          key: 'gameDetails.renodx.host.addons.none',
        },
        {
          kind: 'message',
          key: 'gameDetails.renodx.host.action.reinstall_with_addon_support',
        },
      ],
    });
  });

  it('derives channel switch affordances', () => {
    expect(getReshadeSwitchTarget('stable')).toBe('nightly');
    expect(getReshadeSwitchTarget('nightly')).toBe('stable');
    expect(getReshadeSwitchTarget(null)).toBeNull();
    expect(canSwitchReshadeChannel({ kind: 'managed', health: 'healthy' }, 'nightly')).toBe(true);
    expect(canSwitchReshadeChannel({ kind: 'unmanaged_compatible' }, 'nightly')).toBe(false);
    expect(isReshadeSwitchDisabled({ busy: false, target: 'stable', stableSupported: false })).toBe(
      true,
    );
    expect(
      isReshadeSwitchDisabled({ busy: false, target: 'nightly', stableSupported: false }),
    ).toBe(false);
  });

  it('derives the add-on description key from config and provenance', () => {
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

  it('maps risk severity to its fallback message key', () => {
    expect(riskFallbackKey('block')).toBe('gameDetails.renodx.riskBlocked');
    expect(riskFallbackKey('warn')).toBe('gameDetails.renodx.riskWarn');
    expect(riskFallbackKey('info')).toBe('gameDetails.renodx.riskSafe');
  });

  it('humanizes namespaced note keys for the catalog-miss fallback', () => {
    expect(humanizeMessageKey('gameDetails.renodx.note.run_in_dx12')).toBe('run in dx12');
    expect(humanizeMessageKey('plain')).toBe('plain');
  });
});
