import { describe, expect, it } from 'vitest';

import {
  createPresentedLibraries,
  displayLibraryFilePath,
  formatCompactLibraryLabel,
  presentLibraryFiles,
} from './library-presentation';

describe('library-presentation', () => {
  it('returns compact labels for canonical slug values', () => {
    expect(formatCompactLibraryLabel('intel_xell')).toBe('XeLL');
    expect(formatCompactLibraryLabel('nvidia_streamline')).toBe('Streamline');
    expect(formatCompactLibraryLabel('openvr')).toBe('OpenVR');
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
      createPresentedLibraries([
        'steam',
        'openvr',
        'amd_fsr',
        'intel_xell',
        'direct_storage',
        'dlss_super_resolution',
      ]).map((library) => library.vendorKey),
    ).toEqual(['nvidia', 'amd', 'intel', 'microsoft', 'valve', 'other']);
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

  it('presents the complete DXC package as one ordered install unit', () => {
    expect(
      presentLibraryFiles('microsoft_dxc', [
        { path: 'C:/Game/dxil.dll' },
        { path: 'C:/Game/dxcompiler.dll' },
      ]),
    ).toEqual({
      label: 'dxcompiler.dll + dxil.dll',
      fileCount: 2,
      locations: ['C:/Game'],
    });
  });

  it('keeps a standalone installed DXC compiler literal', () => {
    expect(presentLibraryFiles('microsoft_dxc', [{ path: 'C:/Game/dxcompiler.dll' }])).toEqual({
      label: 'dxcompiler.dll',
      fileCount: 1,
      locations: ['C:/Game/dxcompiler.dll'],
    });
  });

  it('keeps a complete DXC package readable when Windows separators are used', () => {
    expect(
      presentLibraryFiles('microsoft_dxc', [
        { path: 'D:\\Game\\dxcompiler.dll' },
        { path: 'D:\\Game\\dxil.dll' },
      ]),
    ).toEqual({
      label: 'dxcompiler.dll + dxil.dll',
      fileCount: 2,
      locations: ['D:\\Game'],
    });
  });

  it('shows both exact paths when DXC package members are in different directories', () => {
    expect(
      presentLibraryFiles('microsoft_dxc', [
        { path: 'C:/Compiler/dxcompiler.dll' },
        { path: 'D:/Validator/dxil.dll' },
      ]),
    ).toEqual({
      label: 'dxcompiler.dll + dxil.dll',
      fileCount: 2,
      locations: ['C:/Compiler/dxcompiler.dll', 'D:/Validator/dxil.dll'],
    });
  });

  it('returns null when no component files are available', () => {
    expect(presentLibraryFiles('microsoft_dxc', [])).toBeNull();
  });
});
