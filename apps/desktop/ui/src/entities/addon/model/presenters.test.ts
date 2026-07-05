import { describe, expect, it } from 'vitest';

import { formatHostDescription, type HostDescription } from './presenters';

describe('formatHostDescription', () => {
  it('renders a conflict as its blocking translation', () => {
    const description = {
      kind: 'conflict',
      key: 'gameDetails.renodx.host.conflictMultiple',
    } satisfies HostDescription;

    expect(formatHostDescription(description)).toBe(
      'Multiple ReShade hosts found — active slot needs review',
    );
  });

  it('renders version and message parts in order', () => {
    const description = {
      kind: 'parts',
      fallbackKey: 'gameDetails.renodx.host.versionUnknown',
      parts: [
        { kind: 'version', key: 'gameDetails.renodx.host.version', version: '6.5.1' },
        { kind: 'message', key: 'gameDetails.renodx.host.addons.none' },
      ],
    } satisfies HostDescription;

    expect(formatHostDescription(description)).toBe('6.5.1 · add-ons not supported');
  });

  it('uses the fallback translation when no parts are available', () => {
    const description = {
      kind: 'parts',
      fallbackKey: 'gameDetails.renodx.host.versionUnknown',
      parts: [],
    } satisfies HostDescription;

    expect(formatHostDescription(description)).toBe('Version unknown');
  });
});
