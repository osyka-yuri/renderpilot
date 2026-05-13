import { describe, expect, it } from 'vitest';

import {
  buildLibraryFilterOptions,
  groupLibraryFilterOptions,
  mergeVendorDraftLibraries,
  selectedLibrariesForVendor,
} from './library-filter-options';

describe('library-filter-options', () => {
  describe('buildLibraryFilterOptions', () => {
    it('assigns stable vendor groups and sort order', () => {
      expect(
        buildLibraryFilterOptions([
          'steam',
          'intel_xell',
          'amd_fsr_frame_generation',
          'dlss_super_resolution',
        ]),
      ).toEqual([
        {
          value: 'dlss_super_resolution',
          label: 'DLSS SR',
          vendorKey: 'nvidia',
          vendorLabel: 'NVIDIA',
        },
        {
          value: 'amd_fsr_frame_generation',
          label: 'FSR FG',
          vendorKey: 'amd',
          vendorLabel: 'AMD',
        },
        {
          value: 'intel_xell',
          label: 'XeLL',
          vendorKey: 'intel',
          vendorLabel: 'Intel',
        },
        {
          value: 'steam',
          label: 'Steam',
          vendorKey: 'other',
          vendorLabel: 'Additional',
        },
      ]);
    });

    it('trims, deduplicates, and excludes unknown values', () => {
      expect(
        buildLibraryFilterOptions([
          ' unknown ',
          'UNKNOWN',
          ' dlss_super_resolution ',
          'dlss_super_resolution',
        ]),
      ).toEqual([
        {
          value: 'dlss_super_resolution',
          label: 'DLSS SR',
          vendorKey: 'nvidia',
          vendorLabel: 'NVIDIA',
        },
      ]);
    });
  });

  describe('groupLibraryFilterOptions', () => {
    it('groups by vendor in a stable order and omits empty groups', () => {
      const options = buildLibraryFilterOptions([
        'intel_xell',
        'dlss_super_resolution',
        'steam',
        'nvidia_reflex',
      ]);

      expect(groupLibraryFilterOptions(options)).toEqual([
        {
          vendorKey: 'nvidia',
          vendorLabel: 'NVIDIA',
          options: [
            {
              value: 'dlss_super_resolution',
              label: 'DLSS SR',
              vendorKey: 'nvidia',
              vendorLabel: 'NVIDIA',
            },
            {
              value: 'nvidia_reflex',
              label: 'Reflex',
              vendorKey: 'nvidia',
              vendorLabel: 'NVIDIA',
            },
          ],
        },
        {
          vendorKey: 'intel',
          vendorLabel: 'Intel',
          options: [
            {
              value: 'intel_xell',
              label: 'XeLL',
              vendorKey: 'intel',
              vendorLabel: 'Intel',
            },
          ],
        },
        {
          vendorKey: 'other',
          vendorLabel: 'Additional',
          options: [
            {
              value: 'steam',
              label: 'Steam',
              vendorKey: 'other',
              vendorLabel: 'Additional',
            },
          ],
        },
      ]);
    });
  });

  describe('vendor draft selection helpers', () => {
    it('returns selected libraries for a vendor only', () => {
      const options = buildLibraryFilterOptions([
        'dlss_super_resolution',
        'nvidia_reflex',
        'amd_fsr',
      ]);
      const nvidiaOptions = options.filter((option) => option.vendorKey === 'nvidia');

      expect(selectedLibrariesForVendor(['amd_fsr', 'nvidia_reflex'], nvidiaOptions)).toEqual([
        'nvidia_reflex',
      ]);
    });

    it('replaces only the active vendor slice while preserving other selections', () => {
      const options = buildLibraryFilterOptions([
        'dlss_super_resolution',
        'nvidia_reflex',
        'amd_fsr',
      ]);
      const nvidiaOptions = options.filter((option) => option.vendorKey === 'nvidia');

      expect(
        mergeVendorDraftLibraries(
          ['amd_fsr', 'nvidia_reflex'],
          nvidiaOptions,
          ['dlss_super_resolution'],
        ),
      ).toEqual(['amd_fsr', 'dlss_super_resolution']);
    });
  });
});