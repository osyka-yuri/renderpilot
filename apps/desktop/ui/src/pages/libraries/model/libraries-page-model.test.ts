import { describe, expect, it } from 'vitest';

import type { LibraryManifestEntry } from '@entities/library';
import {
  formatSignedDate,
  formatVersionLabel,
  getDefaultTypeForVendor,
  groupKeyForType,
  isVendor,
  libraryIdToGroupKey,
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
    it('returns "Unsigned" for unsigned signature', () => {
      const entry = sampleEntry({ signedAt: null });
      expect(formatSignedDate(entry.signature)).toBe('Unsigned');
    });

    it('returns formatted date for signed signature', () => {
      const entry = sampleEntry({ signedAt: '2024-03-15T10:30:00Z' });
      expect(formatSignedDate(entry.signature)).toBe('03/15/2024');
    });

    it('returns "Invalid date" for malformed date', () => {
      const entry = sampleEntry({ signedAt: 'not-a-date' });
      expect(formatSignedDate(entry.signature)).toBe('Invalid date');
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

  describe('getDefaultTypeForVendor', () => {
    it('returns first type for each vendor', () => {
      expect(getDefaultTypeForVendor('nvidia')).toBe('dlss');
      expect(getDefaultTypeForVendor('amd')).toBe('fsr');
      expect(getDefaultTypeForVendor('intel')).toBe('xess');
    });
  });
});

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
