import { describe, expect, it } from 'vitest';

import { describeReshadeHost, getReshadeDescription } from './luma-presenters';
import type { HostFacts } from '@entities/addon';

const PRESENT_HOST_FACTS = {
  path: 'C:\\Games\\Game\\dxgi.dll',
  slot: 'dxgi.dll',
  version: '6.5.1',
  addon_support: 'full',
  active: true,
  channel: {
    selected: 'nightly',
    detected: 'nightly',
  },
  update_status: 'current',
  is_custom_build: false,
} satisfies HostFacts;

const ENABLED_ACTION = {
  enabled: true,
  requires_confirmation: false,
  confirmation_scope: null,
  disabled_reason: null,
  target_channel: null,
} as const;

describe('luma presenters', () => {
  it('surfaces host conflicts as the blocking description', () => {
    expect(
      getReshadeDescription({
        detection: 'conflict',
        facts: PRESENT_HOST_FACTS,
      }),
    ).toEqual({
      kind: 'conflict',
      key: 'gameDetails.luma.host.conflictMultiple',
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
      fallbackKey: 'gameDetails.luma.host.versionUnknown',
      parts: [
        {
          kind: 'version',
          key: 'gameDetails.luma.host.version',
          version: '6.4.0',
        },
        {
          kind: 'message',
          key: 'gameDetails.luma.host.addons.none',
        },
        {
          kind: 'message',
          key: 'gameDetails.luma.host.action.repair_host',
        },
      ],
    });
  });

  it('labels a recognized custom build plainly, ignoring conflict/update wording', () => {
    expect(
      getReshadeDescription({
        detection: 'conflict',
        facts: {
          ...PRESENT_HOST_FACTS,
          is_custom_build: true,
        },
      }),
    ).toEqual({
      kind: 'parts',
      fallbackKey: 'gameDetails.luma.host.versionUnknown',
      parts: [{ kind: 'message', key: 'gameDetails.luma.host.customBuild' }],
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
      fallbackKey: 'gameDetails.luma.host.versionUnknown',
      parts: [
        {
          kind: 'version',
          key: 'gameDetails.luma.host.version',
          version: '6.5.1',
        },
        {
          kind: 'message',
          key: 'gameDetails.luma.fresh.channelMismatch',
        },
      ],
    });
  });

  it('does not ask to validate a host when its lifecycle exposes no maintenance action', () => {
    const input = {
      detection: 'present' as const,
      facts: {
        ...PRESENT_HOST_FACTS,
        version: '6.7.3.1',
        channel: { selected: 'nightly' as const, detected: null },
        update_status: 'unknown_needs_validation' as const,
      },
      actions: { use_existing: ENABLED_ACTION },
    };

    expect(getReshadeDescription(input)).toEqual({
      kind: 'parts',
      fallbackKey: 'gameDetails.luma.host.versionUnknown',
      parts: [
        {
          kind: 'version',
          key: 'gameDetails.luma.host.version',
          version: '6.7.3.1',
        },
      ],
    });
    expect(describeReshadeHost(input)).toBe('6.7.3.1');
  });

  it('keeps validation wording when a host maintenance action exists', () => {
    expect(
      getReshadeDescription({
        detection: 'present',
        facts: {
          ...PRESENT_HOST_FACTS,
          update_status: 'unknown_needs_validation',
        },
        actions: { update: ENABLED_ACTION },
      }),
    ).toEqual({
      kind: 'parts',
      fallbackKey: 'gameDetails.luma.host.versionUnknown',
      parts: [
        {
          kind: 'version',
          key: 'gameDetails.luma.host.version',
          version: '6.5.1',
        },
        {
          kind: 'message',
          key: 'gameDetails.luma.fresh.validationRequired',
        },
      ],
    });
  });
});
