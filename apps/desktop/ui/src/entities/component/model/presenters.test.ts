import { describe, expect, it } from 'vitest';

import { displayComponentFilePath, formatCompactLibraryLabel, formatLibrary } from './presenters';

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

  describe('displayComponentFilePath', () => {
    it('prefers the dx12 entry point for cohesive AMD FSR components', () => {
      expect(
        displayComponentFilePath({
          technology: 'amd_fsr',
          files: [
            { path: 'C:/Game/amd_fidelityfx_upscaler_dx12.dll' },
            { path: 'C:/Game/amd_fidelityfx_dx12.dll' },
          ],
        }),
      ).toBe('C:/Game/amd_fidelityfx_dx12.dll');
    });

    it('falls back to the first file for native or non-FSR components', () => {
      expect(
        displayComponentFilePath({
          technology: 'amd_fsr_upscaler',
          files: [{ path: 'C:/Game/amd_fidelityfx_upscaler_dx12.dll' }],
        }),
      ).toBe('C:/Game/amd_fidelityfx_upscaler_dx12.dll');
      expect(
        displayComponentFilePath({
          technology: 'dlss_super_resolution',
          files: [{ path: 'C:/Game/nvngx_dlss.dll' }],
        }),
      ).toBe('C:/Game/nvngx_dlss.dll');
    });
  });
});
