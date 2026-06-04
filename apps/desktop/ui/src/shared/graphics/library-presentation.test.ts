import { describe, expect, it } from 'vitest';

import {
  createPresentedLibraries,
  displayLibraryFilePath,
  formatCompactLibraryLabel,
} from './library-presentation';

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

  it('prefers the dx12 entry point when presenting cohesive AMD FSR files', () => {
    expect(
      displayLibraryFilePath('amd_fsr', [
        { path: 'C:/Game/amd_fidelityfx_upscaler_dx12.dll' },
        { path: 'C:/Game/amd_fidelityfx_dx12.dll' },
      ]),
    ).toBe('C:/Game/amd_fidelityfx_dx12.dll');

    expect(
      displayLibraryFilePath('amd_fsr_upscaler', [
        { path: 'C:/Game/amd_fidelityfx_upscaler_dx12.dll' },
      ]),
    ).toBe('C:/Game/amd_fidelityfx_upscaler_dx12.dll');
  });
});
