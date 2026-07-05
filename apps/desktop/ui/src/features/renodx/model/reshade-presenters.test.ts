import { describe, expect, it } from 'vitest';

import {
  getAddonDescriptionKey,
  getReshadeDescription,
  humanizeMessageKey,
  riskFallbackKey,
  riskMessage,
} from './reshade-presenters';
import type { HostFacts } from './types';

const PRESENT_HOST_FACTS = {
  path: 'C:\\Games\\Game\\dxgi.dll',
  slot: 'dxgi.dll',
  version: '6.5.1',
  addon_support: 'full',
  active: true,
  channel: {
    selected: 'stable',
    effective: 'stable',
    detected: 'stable',
  },
  update_status: 'current',
  is_custom_build: false,
} satisfies HostFacts;

describe('reshade presenters', () => {
  it('surfaces host conflicts as the blocking description', () => {
    expect(
      getReshadeDescription({
        detection: 'conflict',
        facts: PRESENT_HOST_FACTS,
      }),
    ).toEqual({
      kind: 'conflict',
      key: 'gameDetails.renodx.host.conflictMultiple',
    });
  });

  it('builds a host description from version, support, and update status', () => {
    expect(
      getReshadeDescription({
        detection: 'present',
        facts: {
          ...PRESENT_HOST_FACTS,
          version: '6.4.0',
          addon_support: 'limited',
          update_status: 'repair_available',
        },
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
          key: 'gameDetails.renodx.host.addons.none',
        },
        {
          kind: 'message',
          key: 'gameDetails.renodx.host.action.repair_host',
        },
      ],
    });
  });

  it('labels a recognized custom build plainly, ignoring conflict/update wording', () => {
    expect(
      getReshadeDescription({
        // The backend reports a recognized custom build as a conflict (the
        // slot is never safe to write to); the frontend must still show the
        // plain custom-build label, not the generic conflict message.
        detection: 'conflict',
        facts: {
          ...PRESENT_HOST_FACTS,
          is_custom_build: true,
        },
      }),
    ).toEqual({
      kind: 'parts',
      fallbackKey: 'gameDetails.renodx.host.versionUnknown',
      parts: [{ kind: 'message', key: 'gameDetails.renodx.host.customBuild' }],
    });
  });

  it('labels a host channel mismatch as a channel change, not a generic update', () => {
    expect(
      getReshadeDescription({
        detection: 'present',
        facts: {
          ...PRESENT_HOST_FACTS,
          update_status: 'channel_mismatch',
        },
      }),
    ).toEqual({
      kind: 'parts',
      fallbackKey: 'gameDetails.renodx.host.versionUnknown',
      parts: [
        {
          kind: 'version',
          key: 'gameDetails.renodx.host.version',
          version: '6.5.1',
        },
        {
          kind: 'message',
          key: 'gameDetails.renodx.fresh.channelMismatch',
        },
      ],
    });
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

  it('maps risk severity to its fallback message key', () => {
    expect(riskFallbackKey('warn')).toBe('gameDetails.addon.riskWarn');
    expect(riskFallbackKey('info')).toBe('gameDetails.addon.riskSafe');
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

  it('humanizes namespaced note keys for the catalog-miss fallback', () => {
    expect(humanizeMessageKey('gameDetails.renodx.note.run_in_dx12')).toBe('run in dx12');
    expect(humanizeMessageKey('plain')).toBe('plain');
  });
});
