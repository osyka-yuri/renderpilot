import { describe, expect, it } from 'vitest';

import { createPresentedLibraries, formatCompactLibraryLabel } from './library-presentation';

describe('library-presentation', () => {
  it('returns compact labels for canonical slug values', () => {
    expect(formatCompactLibraryLabel('intel_xell')).toBe('XeLL');
    expect(formatCompactLibraryLabel('nvidia_streamline')).toBe('Streamline');
  });

  it('trims, deduplicates, filters unknown values, and sorts by vendor', () => {
    expect(
      createPresentedLibraries([
        ' steam ',
        'intel_xell',
        'unknown',
        'UNKNOWN',
        'dlss_super_resolution',
        'intel_xell',
      ]),
    ).toEqual([
      {
        tag: 'dlss_super_resolution',
        label: 'DLSS SR',
        vendorKey: 'nvidia',
      },
      {
        tag: 'intel_xell',
        label: 'XeLL',
        vendorKey: 'intel',
      },
      {
        tag: 'steam',
        label: 'Steam',
        vendorKey: 'other',
      },
    ]);
  });

  it('keeps vendor ordering shared across consumers', () => {
    expect(
      createPresentedLibraries(['steam', 'amd_fsr', 'intel_xell', 'dlss_super_resolution']).map(
        (library) => library.vendorKey,
      ),
    ).toEqual(['nvidia', 'amd', 'intel', 'other']);
  });
});
