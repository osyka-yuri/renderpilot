/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import { legalDocumentLink, packageSummary } from '../model/library-package-test-fixtures';
import LibraryLegalDocumentsSheetTestHost from './LibraryLegalDocumentsSheet.test-host.svelte';

const mocks = vi.hoisted(() => ({
  openExternal: vi.fn<(url: string) => Promise<void>>(),
  toastError: vi.fn(),
}));

vi.mock('@shared/api', () => ({
  openExternal: mocks.openExternal,
}));

vi.mock('svelte-sonner', () => ({
  toast: {
    error: mocks.toastError,
  },
}));

describe('LibraryLegalDocumentsSheet', () => {
  let target: HTMLDivElement;
  let component: { close: () => void } | undefined;

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
    target = document.createElement('div');
    document.body.append(target);
    mocks.openExternal.mockResolvedValue();
  });

  afterEach(async () => {
    if (component) {
      component.close();
      flushSync();
      await settleOverlays();
      await unmount(component);
      component = undefined;
    }
    vi.clearAllMocks();
    vi.unstubAllGlobals();
    document.body.replaceChildren();
  });

  it('shows the package context and opens the exact validated document URL', async () => {
    const row = rowWithDocument();
    component = mount(LibraryLegalDocumentsSheetTestHost, {
      target,
      props: { initialRow: row, onClose: vi.fn() },
    });
    flushSync();

    await vi.waitFor(() => {
      expect(document.body.textContent).toContain('SDK License');
      expect(document.body.textContent).toContain('Example SDK 2.0.0');
    });

    openButton().click();

    await vi.waitFor(() => {
      expect(mocks.openExternal).toHaveBeenCalledExactlyOnceWith(
        row.legal_documents[0].content_url,
      );
    });
    expect(mocks.toastError).not.toHaveBeenCalled();
  });

  it('reports an external-open failure and re-enables the action', async () => {
    mocks.openExternal.mockRejectedValueOnce(new Error('shell unavailable'));
    component = mount(LibraryLegalDocumentsSheetTestHost, {
      target,
      props: { initialRow: rowWithDocument(), onClose: vi.fn() },
    });
    flushSync();

    const button = await vi.waitFor(openButton);
    button.click();

    await vi.waitFor(() => {
      expect(mocks.toastError).toHaveBeenCalledWith('Could not open the document');
      expect(button.disabled).toBe(false);
    });
  });

  it('delegates closing the sheet to its owner', async () => {
    const onClose = vi.fn();
    component = mount(LibraryLegalDocumentsSheetTestHost, {
      target,
      props: { initialRow: rowWithDocument(), onClose },
    });
    flushSync();

    const closeButton = await vi.waitFor(() => {
      const button = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
        (candidate) => candidate.textContent.trim() === 'Close',
      );
      if (!button) {
        throw new Error('legal documents close button is missing');
      }
      return button;
    });
    closeButton.click();

    await vi.waitFor(() => {
      expect(onClose).toHaveBeenCalledOnce();
    });
  });
});

function rowWithDocument() {
  return packageSummary({
    id: 'example-sdk',
    version: '2.0.0',
    displayName: 'Example SDK',
    legalDocuments: [legalDocumentLink()],
  });
}

function openButton(): HTMLButtonElement {
  const button = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
    (candidate) => candidate.textContent.trim() === 'Open',
  );
  if (!button) {
    throw new Error('legal document open button is missing');
  }
  return button;
}

async function settleOverlays(): Promise<void> {
  await tick();
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        resolve();
      });
    });
  });
  await tick();
}
