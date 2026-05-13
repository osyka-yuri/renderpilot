import { describe, expect, it } from 'vitest';

import { formatCompactLibraryLabel, formatLibrary } from './presenters';

describe('presenters', () => {
  describe('formatLibrary', () => {
    it('keeps canonical long-form labels for known graphics technologies', () => {
      expect(formatLibrary('dlss_super_resolution')).toBe('DLSS Super Resolution');
      expect(formatLibrary('nvidia_reflex')).toBe('NVIDIA Reflex');
      expect(formatLibrary('intel_xell')).toBe('Intel Xe Low Latency');
      expect(formatLibrary('amd_fsr_ray_regeneration')).toBe('AMD FSR Ray Regeneration');
    });

    it('returns Unknown for unrecognised values', () => {
      expect(formatLibrary('unknown')).toBe('Unknown');
      expect(formatLibrary('does_not_exist')).toBe('Does Not Exist');
    });
  });

  describe('formatCompactLibraryLabel', () => {
    it('returns compact labels for known technologies and falls back for others', () => {
      expect(formatCompactLibraryLabel('dlss_super_resolution')).toBe('DLSS SR');
      expect(formatCompactLibraryLabel('intel_xell')).toBe('XeLL');
      expect(formatCompactLibraryLabel('does_not_exist')).toBe('Does Not Exist');
    });
  });
});
