import { describe, expect, it } from 'vitest';

import {
  hasKnownLaunchArgsInstructions,
  includesDx11LaunchArg,
  launchArgsInstructionKey,
} from './launch-args';

describe('Luma launch-argument callout', () => {
  it.each([
    ['Steam', 'gameDetails.luma.launchArgs.instructions.steam'],
    ['Gog', 'gameDetails.luma.launchArgs.instructions.gog'],
    ['Epic', 'gameDetails.luma.launchArgs.instructions.epic'],
    ['Ea', 'gameDetails.luma.launchArgs.instructions.ea'],
    ['Ubisoft', 'gameDetails.luma.launchArgs.instructions.ubisoft'],
  ])('uses the %s instruction', (launcher, expected) => {
    expect(launchArgsInstructionKey(launcher)).toBe(expected);
  });

  it('uses the generic instruction for an unknown launcher', () => {
    expect(launchArgsInstructionKey('Manual')).toBe(
      'gameDetails.luma.launchArgs.instructions.other',
    );
  });

  it('identifies when the launcher has a precise route', () => {
    expect(hasKnownLaunchArgsInstructions('Steam')).toBe(true);
    expect(hasKnownLaunchArgsInstructions('Manual')).toBe(false);
  });

  it('recognises the DX11 argument without changing any external configuration', () => {
    expect(includesDx11LaunchArg(['-NoD3D9Ex', ' -DX11 '])).toBe(true);
    expect(includesDx11LaunchArg(['-NoD3D9Ex'])).toBe(false);
  });
});
