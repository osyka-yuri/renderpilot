import { describe, expect, it } from 'vitest';

import { t } from '@shared/i18n';
import {
  compareReleaseVersions,
  filterPackageRows,
  formatArchitectureLabel,
  formatSignedDate,
  formatVersionLabel,
  selectLatestStablePackages,
  shouldShowPackageDisplayName,
  typeOptionsByVendor,
} from './libraries-page-model';
import { packagesOf } from './library-package-test-fixtures';

describe('library package presentation', () => {
  it('exposes OpenVR as Valve catalog data without UI-side inference', () => {
    expect(typeOptionsByVendor.valve).toEqual([
      { value: 'openvr', label: 'OpenVR', technology: 'openvr' },
    ]);
    const rows = packagesOf([
      { id: 'openvr.x64', vendor: 'valve', technology: 'openvr', variant: 'runtime' },
      { id: 'other', vendor: 'nvidia', technology: 'nvidia_dlss_sr', variant: 'runtime' },
    ]);
    expect(filterPackageRows(rows, 'valve', 'openvr').map((row) => row.package_id)).toEqual([
      'openvr.x64',
    ]);
  });

  it('filters backend summaries by vendor, technology, and explicit variant', () => {
    const rows = packagesOf([
      { id: 'fsr.dx12', vendor: 'amd', technology: 'amd_fsr', variant: 'dx12_runtime' },
      { id: 'fsr.sdk', vendor: 'amd', technology: 'amd_fsr', variant: 'sdk_bundle' },
      { id: 'fsr.vulkan', vendor: 'amd', technology: 'amd_fsr', variant: 'vulkan_runtime' },
    ]);

    expect(filterPackageRows(rows, 'amd', 'fsr').map((row) => row.package_id)).toEqual([
      'fsr.dx12',
      'fsr.sdk',
    ]);
    expect(filterPackageRows(rows, 'amd', 'fsr_vk').map((row) => row.package_id)).toEqual([
      'fsr.vulkan',
    ]);
  });

  it('uses numeric version ordering for both latest selection and table sorting', () => {
    const rows = packagesOf([
      {
        id: 'dx12.old',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'dx12_runtime',
        version: '4.0.9',
      },
      {
        id: 'dx12.new',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'dx12_runtime',
        version: '4.0.10',
      },
      {
        id: 'debug.newer',
        vendor: 'amd',
        technology: 'amd_fsr',
        variant: 'dx12_runtime',
        version: '9.0.0',
        channel: 'debug',
      },
    ]);

    expect(selectLatestStablePackages(rows).map((row) => row.package_id)).toEqual(['dx12.new']);
    expect(compareReleaseVersions('4.0.10', '4.0.9')).toBeGreaterThan(0);
  });

  it('keeps distinct runtime compatibility targets in the latest selection', () => {
    const selected = selectLatestStablePackages(
      packagesOf([
        {
          id: 'agility.617.old',
          vendor: 'microsoft',
          technology: 'd3d12_agility',
          version: '1.617.0',
          compatibilityVersion: 617,
        },
        {
          id: 'agility.617.new',
          vendor: 'microsoft',
          technology: 'd3d12_agility',
          version: '1.617.1',
          compatibilityVersion: 617,
        },
        {
          id: 'agility.618',
          vendor: 'microsoft',
          technology: 'd3d12_agility',
          version: '1.618.0',
          compatibilityVersion: 618,
        },
      ]),
    );

    expect(selected.map((row) => row.package_id).sort()).toEqual([
      'agility.617.new',
      'agility.618',
    ]);
  });

  it('orders every version segment with u64 precision', () => {
    expect(
      compareReleaseVersions('1.18446744073709551615', '1.18446744073709551614'),
    ).toBeGreaterThan(0);
  });

  it('formats release, target, and primary signature from the compact summary', () => {
    const [row] = packagesOf([
      {
        id: 'dxc.1.9',
        version: '1.9.0',
        label: 'SDK release',
        architecture: 'X86',
        signature: { status: 'signed', signed_at: null },
      },
    ]);

    expect(formatVersionLabel(row)).toBe('1.9.0 (SDK release)');
    expect(formatArchitectureLabel(row)).toBe('x86');
    expect(formatSignedDate(row.primary_signature)).toBe('—');
    expect(formatSignedDate({ status: 'unsigned' })).toBe(t('libraries.unsigned'));
    expect(formatSignedDate({ status: 'signed', signed_at: 'invalid' })).toBe(
      t('libraries.invalidDate'),
    );
  });

  it('appends the supplemental catalog annotation verbatim', () => {
    const [legacyRuntime, sdkBundle, preview, beta] = packagesOf([
      {
        id: 'fsr.legacy',
        version: '1.0.1.41314',
        label: 'FSR 3.1.4',
      },
      {
        id: 'fsr.sdk',
        version: '4.1.1.2740',
        label: null,
      },
      {
        id: 'radiance.preview',
        version: '0.9.0.2740',
        label: 'preview',
      },
      {
        id: 'sdk.beta',
        version: '4.1.1.2740',
        label: 'Beta White Collie',
        channel: 'beta',
      },
    ]);

    expect(formatVersionLabel(legacyRuntime)).toBe('1.0.1.41314 (FSR 3.1.4)');
    expect(formatVersionLabel(sdkBundle)).toBe('4.1.1.2740');
    expect(formatVersionLabel(preview)).toBe('0.9.0.2740 (preview)');
    expect(formatVersionLabel(beta)).toBe('4.1.1.2740 (Beta White Collie)');
  });

  it('shows package names only when the active list contains distinct names', () => {
    expect(
      shouldShowPackageDisplayName(
        packagesOf([
          { id: 'dlss.old', displayName: 'NVIDIA DLSS Super Resolution' },
          { id: 'dlss.new', displayName: '  nvidia dlss   super resolution ' },
        ]),
      ),
    ).toBe(false);
    expect(
      shouldShowPackageDisplayName(
        packagesOf([
          { id: 'fsr.legacy', displayName: 'AMD FidelityFX Super Resolution' },
          { id: 'fsr.sdk', displayName: 'AMD FidelityFX SDK DirectX 12' },
        ]),
      ),
    ).toBe(true);
  });
});
