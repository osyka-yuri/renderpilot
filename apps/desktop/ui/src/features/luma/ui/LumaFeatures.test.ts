/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import LumaFeatures from './LumaFeatures.svelte';

describe('LumaFeatures', () => {
  let target: HTMLDivElement;
  let component: object | undefined;

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

  it('shows the separately reported DLSS / FSR and HDR statuses', () => {
    component = mount(LumaFeatures, {
      target,
      props: { features: { dlss_fsr: 'supported', hdr: 'experimental' } },
    });
    flushSync();

    expect(target.textContent).toContain('Features');
    expect(target.textContent).toContain('DLSS / FSR: Supported');
    expect(target.textContent).toContain('HDR: Experimental');
  });

  it('renders unsupported and unknown labels without borrowing confidence copy', () => {
    component = mount(LumaFeatures, {
      target,
      props: { features: { dlss_fsr: 'unsupported', hdr: 'unknown' } },
    });
    flushSync();

    expect(target.textContent).toContain('DLSS / FSR: Not supported');
    expect(target.textContent).toContain('HDR: Unknown');
  });
});
