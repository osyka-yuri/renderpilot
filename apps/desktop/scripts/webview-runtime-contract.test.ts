import { describe, expect, it } from 'vitest';

import tauriConfig from '../src-tauri/tauri.conf.json' with { type: 'json' };
import { edgeBuildTarget, parseWebViewRuntimeContract } from './webview-runtime-contract';

describe('WebView2 runtime contract', () => {
  it('derives the Edge build target from the committed Tauri config', () => {
    const contract = parseWebViewRuntimeContract(tauriConfig.bundle.windows.minimumWebview2Version);

    expect(contract).toEqual({ minimumVersion: '136.0.3240.44', major: 136 });
    expect(edgeBuildTarget(contract.minimumVersion)).toBe('edge136');
  });

  it.each([
    undefined,
    null,
    136,
    '',
    '136.0.3240',
    '136.0.3240.44.1',
    '136.x.3240.44',
    '0.0.0.1',
    '136.4294967296.0.0',
  ])('rejects an invalid minimum version: %s', (value) => {
    expect(() => edgeBuildTarget(value)).toThrow(TypeError);
  });
});
