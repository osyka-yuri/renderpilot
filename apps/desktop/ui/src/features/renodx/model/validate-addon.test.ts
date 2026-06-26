import { describe, expect, it } from 'vitest';

import { addonArch, validateAddonFile } from './validate-addon';

describe('validateAddonFile', () => {
  it('hard-errors on a non-add-on extension', () => {
    const v = validateAddonFile('C:/x/readme.txt', { gameArch: 'x64', expectedAddonName: null });
    expect(v.error?.key).toBe('gameDetails.renodx.fileInstall.errorExtension');
    expect(v.warning).toBeNull();
  });

  it('hard-errors on an architecture mismatch', () => {
    const v = validateAddonFile('C:/x/renodx-game.addon32', {
      gameArch: 'x64',
      expectedAddonName: null,
    });
    expect(v.error?.key).toBe('gameDetails.renodx.fileInstall.errorArch');
    expect(v.error?.params).toEqual({ addon: '32-bit', game: '64-bit' });
    expect(v.warning).toBeNull();
  });

  it('warns (not errors) on an unexpected file name', () => {
    // The renodx-cyberpunk → Alan Wake 2 mistake the confirm dialog must catch.
    const v = validateAddonFile('C:/x/renodx-cyberpunk.addon64', {
      gameArch: 'x64',
      expectedAddonName: 'renodx-alanwake2',
    });
    expect(v.error).toBeNull();
    expect(v.warning?.key).toBe('gameDetails.renodx.fileInstall.warnName');
  });

  it('does not warn when the game is unknown (no expected name)', () => {
    const v = validateAddonFile('C:/x/anything.addon64', {
      gameArch: null,
      expectedAddonName: null,
    });
    expect(v.error).toBeNull();
    expect(v.warning).toBeNull();
  });

  it('accepts a matching expected name and architecture', () => {
    const v = validateAddonFile('C:/x/renodx-cp2077.addon64', {
      gameArch: 'x64',
      expectedAddonName: 'renodx-cp2077',
    });
    expect(v.error).toBeNull();
    expect(v.warning).toBeNull();
    expect(v.fileName).toBe('renodx-cp2077.addon64');
  });

  it('reads the architecture from the extension', () => {
    expect(addonArch('x.addon64')).toBe('x64');
    expect(addonArch('x.addon32')).toBe('x86');
    expect(addonArch('x.dll')).toBeNull();
  });
});
