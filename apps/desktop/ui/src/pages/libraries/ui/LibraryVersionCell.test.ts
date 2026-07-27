/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { setLanguageMode } from '@shared/i18n';

import { packageSummary } from '../model/library-package-test-fixtures';
import LibraryVersionCell from './LibraryVersionCell.svelte';

describe('LibraryVersionCell', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  beforeEach(() => {
    setLanguageMode('en');
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (component) {
      await unmount(component);
      component = undefined;
    }
    target.remove();
  });

  it('renders the exact package version once alongside its architecture', () => {
    component = mount(LibraryVersionCell, {
      target,
      props: {
        row: packageSummary({
          id: 'd3d12-preview',
          version: '1.721.2-preview',
          channel: 'preview',
        }),
        showPackageDisplayName: () => false,
      },
    });
    flushSync();

    expect(target.textContent.match(/1\.721\.2-preview/gu)).toHaveLength(1);
    expect(target.textContent.match(/preview/giu)).toHaveLength(1);
    expect(target.textContent.match(/x64/gu)).toHaveLength(1);
  });

  it('renders the package state as semantic text', () => {
    component = mount(LibraryVersionCell, {
      target,
      props: {
        row: packageSummary({
          id: 'corrupt-local-package',
          availability: 'local_only',
          localState: 'corrupt',
        }),
        showPackageDisplayName: () => false,
      },
    });
    flushSync();

    expect(target.textContent).toContain('Corrupt files');
  });
});
