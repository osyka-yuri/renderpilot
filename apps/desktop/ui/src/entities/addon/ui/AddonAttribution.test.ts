/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import AddonAttribution from './AddonAttribution.svelte';

describe('AddonAttribution', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

  function render(): void {
    component = mount(AddonAttribution, {
      target,
      props: {
        textKey: 'gameDetails.luma.attribution',
        linkKey: 'gameDetails.luma.attributionLink',
        href: 'https://example.test/luma',
      },
    });
    flushSync();
  }

  beforeEach(() => {
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

  it('renders attribution text and an external project link', () => {
    render();

    expect(target.textContent).toContain('Luma Framework by Filoppi.');

    const link = target.querySelector<HTMLAnchorElement>('a');
    expect(link?.getAttribute('href')).toBe('https://example.test/luma');
    expect(link?.getAttribute('target')).toBe('_blank');
    expect(link?.getAttribute('rel')).toBe('noreferrer');
    expect(link?.textContent).toContain('View project');
  });
});
