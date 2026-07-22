/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

import { legalDocumentLink, packageSummary } from '../model/library-package-test-fixtures';
import LibraryLegalCellTestHost from './LibraryLegalCell.test-host.svelte';

describe('LibraryLegalCell', () => {
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

  it('keeps packages without verified documents visually quiet', () => {
    component = mount(LibraryLegalCellTestHost, {
      target,
      props: {
        row: packageSummary({ id: 'without-legal' }),
        onOpen: vi.fn(),
      },
    });
    flushSync();

    expect(target.textContent.trim()).toBe('—');
    expect(target.querySelector('button')).toBeNull();
  });

  it('opens the exact row from an icon-only details action', () => {
    const row = packageSummary({
      id: 'with-license',
      version: '2.0.0',
      legalDocuments: [legalDocumentLink()],
    });
    const onOpen = vi.fn();
    component = mount(LibraryLegalCellTestHost, {
      target,
      props: { row, onOpen },
    });
    flushSync();

    const button = target.querySelector<HTMLButtonElement>('button');
    expect(button?.textContent.trim()).toBe('');
    expect(button?.querySelector('svg')).not.toBeNull();
    expect(button?.getAttribute('aria-label')).toContain(row.display_name);
    expect(button?.getAttribute('aria-label')).toContain('2.0.0');
    button?.click();
    expect(onOpen).toHaveBeenCalledWith(row);
  });

  it('uses the same icon-only action for multiple documents', () => {
    const row = packageSummary({
      id: 'with-multiple-documents',
      legalDocuments: [
        legalDocumentLink(),
        legalDocumentLink({
          legal_document_id: `notice.${'b'.repeat(64)}`,
          kind: 'notice',
          title: 'Third-Party Notices',
          file_name: 'ThirdPartyNotices.txt',
          content_url: `https://cdn.example.test/libraries/legal/sha256/${'b'.repeat(64)}.txt`,
        }),
      ],
    });
    component = mount(LibraryLegalCellTestHost, {
      target,
      props: { row, onOpen: vi.fn() },
    });
    flushSync();

    const button = target.querySelector<HTMLButtonElement>('button');
    expect(button?.textContent.trim()).toBe('');
    expect(button?.querySelector('svg')).not.toBeNull();
    expect(button?.getAttribute('aria-label')).toContain(row.display_name);
  });
});
