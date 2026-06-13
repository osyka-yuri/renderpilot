import { describe, expect, it } from 'vitest';

import { t } from '@shared/i18n';
import type { BuildType, LibraryManifest, LibraryManifestEntry } from '@entities/library';
import {
  formatSignedDate,
  formatVersionLabel,
  getDefaultTypeForVendor,
  groupKeyForType,
  isMultiLibraryGroup,
  isVendor,
  libraryIdToGroupKey,
  selectLatestStableEntries,
} from './libraries-page-model';

describe('libraries-page-model', () => {
  describe('formatVersionLabel', () => {
    it('returns version only when label is absent', () => {
      const entry = sampleEntry({ versionValue: '3.5.0', buildLabel: null });
      expect(formatVersionLabel(entry)).toBe('3.5.0');
    });

    it('returns version with label when label present', () => {
      const entry = sampleEntry({ versionValue: '3.7.0', buildLabel: 'hotfix' });
      expect(formatVersionLabel(entry)).toBe('3.7.0 (hotfix)');
    });

    it('returns em dash for empty version', () => {
      const entry = sampleEntry({ versionValue: '', buildLabel: null });
      expect(formatVersionLabel(entry)).toBe('—');
    });
  });

  describe('formatSignedDate', () => {
    it('returns the unsigned label for unsigned signature', () => {
      const entry = sampleEntry({ signedAt: null });
      expect(formatSignedDate(entry.signature)).toBe(t('libraries.unsigned'));
    });

    it('returns a formatted date for a signed signature', () => {
      const entry = sampleEntry({ signedAt: '2024-03-15T10:30:00Z' });
      // Format follows the active locale (en: 03/15/2024, ru: 15.03.2024), so
      // assert on the locale-agnostic day/year parts rather than exact layout.
      const formatted = formatSignedDate(entry.signature);
      expect(formatted).toContain('2024');
      expect(formatted).toContain('15');
    });

    it('returns the invalid-date label for malformed date', () => {
      const entry = sampleEntry({ signedAt: 'not-a-date' });
      expect(formatSignedDate(entry.signature)).toBe(t('libraries.invalidDate'));
    });
  });

  describe('groupKeyForType', () => {
    it('maps known types to group keys', () => {
      expect(groupKeyForType('dlss')).toBe('dlss');
      expect(groupKeyForType('fsr')).toBe('fsr_31_dx12');
      expect(groupKeyForType('xess')).toBe('xess');
    });

    it('maps unknown type to "other"', () => {
      expect(groupKeyForType('unknown_type' as 'dlss')).toBe('other');
    });
  });

  describe('isVendor', () => {
    it('returns true for valid vendors', () => {
      expect(isVendor('nvidia')).toBe(true);
      expect(isVendor('amd')).toBe(true);
      expect(isVendor('intel')).toBe(true);
    });

    it('returns false for invalid vendors', () => {
      expect(isVendor('qualcomm')).toBe(false);
      expect(isVendor(42)).toBe(false);
      expect(isVendor(null)).toBe(false);
    });
  });

  describe('libraryIdToGroupKey', () => {
    it('maps known library ids', () => {
      expect(libraryIdToGroupKey('nvngx_dlss')).toBe('dlss');
      expect(libraryIdToGroupKey('amd_fidelityfx_dx12')).toBe('fsr_31_dx12');
      expect(libraryIdToGroupKey('libxess')).toBe('xess');
    });

    it('maps unknown ids to "other"', () => {
      expect(libraryIdToGroupKey('unknown.dll')).toBe('other');
    });
  });

  describe('isMultiLibraryGroup', () => {
    it('returns true for the streamline group', () => {
      expect(isMultiLibraryGroup('streamline')).toBe(true);
    });

    it('returns false for single-library groups', () => {
      expect(isMultiLibraryGroup('dlss')).toBe(false);
      expect(isMultiLibraryGroup('fsr_31_dx12')).toBe(false);
      expect(isMultiLibraryGroup('xess')).toBe(false);
      expect(isMultiLibraryGroup('dstorage')).toBe(false);
      expect(isMultiLibraryGroup('other')).toBe(false);
    });
  });

  describe('getDefaultTypeForVendor', () => {
    it('returns first type for each vendor', () => {
      expect(getDefaultTypeForVendor('nvidia')).toBe('dlss');
      expect(getDefaultTypeForVendor('amd')).toBe('fsr');
      expect(getDefaultTypeForVendor('intel')).toBe('xess');
    });
  });

  describe('selectLatestStableEntries', () => {
    it('returns an empty list for a null manifest', () => {
      expect(selectLatestStableEntries(null)).toEqual([]);
    });

    it('keeps only the newest stable build per library id', () => {
      const manifest = manifestOf([
        libraryEntry({ id: 'dlss-1', lib: 'nvngx_dlss', sort: '001' }),
        libraryEntry({ id: 'dlss-3', lib: 'nvngx_dlss', sort: '003' }),
        libraryEntry({ id: 'xess-1', lib: 'libxess', sort: '005' }),
      ]);

      const result = selectLatestStableEntries(manifest);

      expect(result).toHaveLength(2);
      expect(result.map((entry) => entry.entry_id).sort()).toEqual(['dlss-3', 'xess-1']);
    });

    it('ignores beta and debug builds when picking the latest version', () => {
      const manifest = manifestOf([
        libraryEntry({ id: 'xess-beta', lib: 'libxess', sort: '009', build: 'beta' }),
        libraryEntry({ id: 'xess-debug', lib: 'libxess', sort: '008', build: 'debug' }),
        libraryEntry({ id: 'xess-stable', lib: 'libxess', sort: '005', build: 'stable' }),
      ]);

      expect(selectLatestStableEntries(manifest).map((entry) => entry.entry_id)).toEqual([
        'xess-stable',
      ]);
    });

    it('treats Streamline as a bundle: all plugins of the newest release, not the max per plugin', () => {
      const manifest = manifestOf([
        // Newest release (002) — should be taken in full.
        libraryEntry({ id: 'sl-a-2', lib: 'sl_dlss', sort: '002' }),
        libraryEntry({ id: 'sl-b-2', lib: 'sl_common', sort: '002' }),
        // Older release (001) — dropped even though sl_extra only exists here.
        libraryEntry({ id: 'sl-a-1', lib: 'sl_dlss', sort: '001' }),
        libraryEntry({ id: 'sl-b-1', lib: 'sl_common', sort: '001' }),
        libraryEntry({ id: 'sl-extra-1', lib: 'sl_extra', sort: '001' }),
      ]);

      const result = selectLatestStableEntries(manifest);

      expect(result.map((entry) => entry.entry_id).sort()).toEqual(['sl-a-2', 'sl-b-2']);
    });
  });
});

function libraryEntry(options: {
  id: string;
  lib: string;
  sort: string;
  build?: BuildType;
}): LibraryManifestEntry {
  return {
    entry_id: options.id,
    library: { id: options.lib, file_name: `${options.lib}.dll` },
    version: { value: options.sort, sort_key: options.sort },
    build: { type: options.build ?? 'stable', label: null },
    files: {
      dll: { size_bytes: 1, hashes: { sha256: '0'.repeat(64) } },
      zip: { size_bytes: 1, download_url: 'https://example.com/file.zip' },
    },
    signature: { status: 'unsigned' },
  };
}

function manifestOf(entries: LibraryManifestEntry[]): LibraryManifest {
  return { schema_version: 1, generated_at: '2024-01-01T00:00:00Z', entries };
}

function sampleEntry(options: {
  versionValue?: string;
  buildLabel?: string | null;
  signedAt?: string | null;
}): LibraryManifestEntry {
  return {
    entry_id: 'test-entry',
    library: { id: 'nvngx_dlss', file_name: 'nvngx_dlss.dll' },
    version: { value: options.versionValue ?? '1.0.0', sort_key: options.versionValue ?? '1.0.0' },
    build: { type: 'stable', label: options.buildLabel ?? null },
    files: {
      dll: {
        size_bytes: 1024,
        hashes: { sha256: '0000000000000000000000000000000000000000000000000000000000000000' },
      },
      zip: { size_bytes: 2048, download_url: 'https://example.com/file.zip' },
    },
    signature:
      options.signedAt === null || options.signedAt === undefined
        ? { status: 'unsigned' }
        : { status: 'signed', signed_at: options.signedAt },
  };
}
