import { describe, expect, it } from 'vitest';

import { buildOptiScalerAvailability } from './optiscaler-test-fixtures';
import {
  optiscalerActionAvailability,
  optiscalerCardState,
  optiscalerBlockedAlertPresentation,
  optiscalerCompatibilityBadgeTone,
  optiscalerInstallVisible,
  optiscalerManagedTargetAvailable,
  toggleSelectedModule,
} from './presentation';
import type { OptiScalerModuleAvailability } from './types';

function module(
  id: string,
  overrides: Partial<OptiScalerModuleAvailability> = {},
): OptiScalerModuleAvailability {
  return {
    id,
    selected: false,
    optional: true,
    available: true,
    requires: [],
    conflicts: [],
    description: '',
    ...overrides,
  };
}

const baseReport = buildOptiScalerAvailability();

describe('OptiScaler presentation transitions', () => {
  it('selects requirements and removes dependent modules together', () => {
    const modules = [
      module('core', { optional: false }),
      module('bridge', { requires: ['core'] }),
      module('feature', { requires: ['bridge'] }),
    ];

    expect(toggleSelectedModule(modules, ['core'], 'feature', true)).toEqual([
      'core',
      'bridge',
      'feature',
    ]);
    expect(toggleSelectedModule(modules, ['core', 'bridge', 'feature'], 'bridge', false)).toEqual([
      'core',
    ]);
  });

  it('keeps required modules selected', () => {
    const modules = [module('core', { optional: false })];
    expect(toggleSelectedModule(modules, ['core'], 'core', false)).toEqual(['core']);
  });

  it('keeps a valid install visible while its mutation is busy', () => {
    expect(optiscalerActionAvailability(baseReport, false, true).install).toBe(false);
    expect(optiscalerInstallVisible(baseReport, false)).toBe(true);
  });

  it('keeps an unknown configuration without detected input hard blocked', () => {
    const base = buildOptiScalerAvailability();
    const report = buildOptiScalerAvailability({
      eligibility: {
        ...base.eligibility,
        available: false,
        block_code: 'input_not_detected',
      },
    });

    expect(optiscalerCardState(report)).toBe('blocked');
    expect(optiscalerActionAvailability(report, false, false).install).toBe(false);
  });

  it('excludes unavailable or unmanaged installs from managed target actions', () => {
    expect(optiscalerManagedTargetAvailable(baseReport)).toBe(true);
    expect(
      optiscalerManagedTargetAvailable({
        ...baseReport,
        lifecycle: { ...baseReport.lifecycle, unmanaged: true },
      }),
    ).toBe(false);
  });

  it('blocks managed mutations when the target is unavailable', () => {
    const base = buildOptiScalerAvailability();
    const blocked = buildOptiScalerAvailability({
      lifecycle: {
        ...base.lifecycle,
        maintenance_available: false,
        maintenance_block_code: 'catalog_unsupported',
      },
    });

    expect(optiscalerActionAvailability(blocked, true, false)).toMatchObject({
      update: false,
      modules: false,
      repair: false,
    });
    expect(optiscalerManagedTargetAvailable(blocked)).toBe(false);
    expect(optiscalerManagedTargetAvailable(null)).toBe(false);
  });

  it('keeps persisted maintenance available when current compatibility is no longer positive', () => {
    const report = buildOptiScalerAvailability({
      eligibility: { available: false, block_code: 'catalog_unsupported' },
      lifecycle: {
        ...baseReport.lifecycle,
        maintenance_available: true,
        maintenance_block_code: null,
      },
    });

    expect(optiscalerActionAvailability(report, true, false)).toMatchObject({
      update: true,
      modules: true,
      repair: true,
    });
    expect(optiscalerInstallVisible(report, true)).toBe(false);
    expect(optiscalerInstallVisible(report, false)).toBe(true);
    expect(optiscalerActionAvailability(report, false, false).install).toBe(false);
  });

  it('does not offer a fresh install while a Luma prerequisite remains unresolved', () => {
    const report = buildOptiScalerAvailability({
      prerequisite: { state: 'install_luma' },
    });

    expect(optiscalerCardState(report)).toBe('blocked');
    expect(optiscalerInstallVisible(report, false)).toBe(true);
    expect(optiscalerActionAvailability(report, false, false).install).toBe(false);
  });

  it('keeps a disabled install action visible across eligibility and proxy conflict blocks', () => {
    const proxyConflict = buildOptiScalerAvailability({ proxy_conflict: 'dxgi.dll' });
    expect(optiscalerInstallVisible(proxyConflict, false)).toBe(true);
    expect(optiscalerActionAvailability(proxyConflict, false, false).install).toBe(false);

    const archBlock = buildOptiScalerAvailability({
      eligibility: { available: false, block_code: 'requires_x64' },
    });
    expect(optiscalerInstallVisible(archBlock, false)).toBe(true);
    expect(optiscalerActionAvailability(archBlock, false, false).install).toBe(false);
  });

  it('projects catalog compatibility status onto addon badge tone', () => {
    expect(optiscalerCompatibilityBadgeTone('working')).toBe('verified');
    expect(optiscalerCompatibilityBadgeTone('conditional')).toBe('experimental');
    expect(optiscalerCompatibilityBadgeTone('untested')).toBe('untested');
    expect(optiscalerCompatibilityBadgeTone('unsupported')).toBe('unsupported');
  });

  it('classifies install_luma as an informational blocked alert and other blocks as warnings', () => {
    const lumaStep = buildOptiScalerAvailability({
      prerequisite: { state: 'install_luma' },
    });
    expect(optiscalerBlockedAlertPresentation(lumaStep)).toEqual({
      tone: 'default',
      icon: 'info',
    });

    const conflictingPrerequisite = buildOptiScalerAvailability({
      prerequisite: { state: 'remove_reno_dx' },
    });
    expect(optiscalerBlockedAlertPresentation(conflictingPrerequisite)).toEqual({
      tone: 'warning',
      icon: 'warning',
    });

    const eligibilityBlock = buildOptiScalerAvailability({
      prerequisite: { state: 'none' },
      eligibility: { available: false, block_code: 'requires_x64' },
    });
    expect(optiscalerBlockedAlertPresentation(eligibilityBlock)).toEqual({
      tone: 'warning',
      icon: 'warning',
    });
  });
});
