/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import {
  catalogCandidate,
  component as componentFixture,
  group,
} from '../model/candidate-group-fixtures';
import ComponentVersionRowTestHost from './ComponentVersionRow.test-host.svelte';

describe('ComponentVersionRow catalog releases', () => {
  let target: HTMLDivElement;
  let mounted: object | undefined;

  beforeEach(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = vi.fn();
        unobserve = vi.fn();
        disconnect = vi.fn();
      },
    );
    Object.defineProperties(HTMLElement.prototype, {
      hasPointerCapture: {
        configurable: true,
        value: vi.fn(() => false),
      },
      releasePointerCapture: {
        configurable: true,
        value: vi.fn(),
      },
    });
    target = document.createElement('div');
    document.body.append(target);
  });

  afterEach(async () => {
    if (mounted) {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      flushSync();
      await settleOverlays();
      await unmount(mounted);
      mounted = undefined;
    }
    vi.unstubAllGlobals();
    delete (HTMLElement.prototype as Partial<HTMLElement>).hasPointerCapture;
    delete (HTMLElement.prototype as Partial<HTMLElement>).releasePointerCapture;
    document.body.replaceChildren();
  });

  it('shows each stable and preview release exactly once', async () => {
    mounted = mountRow([
      catalogCandidate('1.721.3', { artifact_id: 'stable' }),
      catalogCandidate('1.721.2-preview', {
        artifact_id: 'preview',
        catalog_package: {
          package_id: 'preview',
          release: { version: '1.721.2-preview', channel: 'preview', label: null },
          availability: 'available',
          automatic_selection_allowed: false,
          presentation: null,
        },
      }),
    ]);

    await openVersionSelect();
    expect(selectItemLabels()).toContain('v1.721.3');
    expect(selectItemLabels()).toContain('v1.721.2-preview');
    expect(versionOccurrenceCount('v1.721.3')).toBe(1);
    expect(versionOccurrenceCount('v1.721.2-preview')).toBe(1);
    expect(document.querySelector('[data-slot="select-content"] [data-slot="badge"]')).toBeNull();
  });

  it('keeps a verified local-only preview available to manual selection', async () => {
    mounted = mountRow([
      catalogCandidate('1.721.2-preview', {
        artifact_id: 'local-preview',
        catalog_package: {
          package_id: 'local-preview',
          release: { version: '1.721.2-preview', channel: 'preview', label: null },
          availability: 'local_only',
          automatic_selection_allowed: false,
          presentation: null,
        },
        is_downloaded: true,
      }),
    ]);

    await openVersionSelect();
    expect(selectItemLabels()).toContain('v1.721.2-preview');
    expect(versionOccurrenceCount('v1.721.2-preview')).toBe(1);
  });

  it('shows an installed preview as current alongside other preview releases', async () => {
    const candidateGroup = group('component:preview', 'microsoft_dxc', '10.0.0', [
      catalogCandidate('1.721.1-preview', {
        artifact_id: 'older-preview',
        catalog_package: {
          package_id: 'older-preview',
          release: { version: '1.721.1-preview', channel: 'preview', label: null },
          availability: 'available',
          automatic_selection_allowed: false,
          presentation: null,
        },
      }),
    ]);
    candidateGroup.version_report = {
      kind: 'known',
      technical_version: null,
      release_label: null,
      catalog_release: {
        version: '1.721.2-preview',
        channel: 'preview',
        label: null,
      },
    };
    mounted = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: componentFixture('component:preview', 'microsoft_dxc'),
        group: candidateGroup,
        busy: false,
        onSwap: vi.fn(),
        onRollback: vi.fn(),
      },
    });
    flushSync();

    expect(versionSelectTrigger().textContent).toContain('1.721.2-preview');
    await openVersionSelect();
    expect(selectItemLabels()).toContain('v1.721.2-preview');
    expect(selectItemLabels()).toContain('v1.721.1-preview');
  });

  it('keeps Xiph options concise while showing the complete release composition', async () => {
    mounted = mountRow([
      catalogCandidate('1.3.7', {
        artifact_id: 'xiph',
        catalog_package: {
          package_id: 'xiph_vorbis.vorbis-1.3.7.ogg-1.3.6.r1.x64.plain',
          release: {
            version: '1.3.7',
            channel: 'stable',
            label: null,
            components: { ogg: '1.3.6', vorbis: '1.3.7' },
          },
          availability: 'available',
          automatic_selection_allowed: true,
          presentation: {
            variant: 'shared.plain',
            architecture: 'X64',
            unsigned: true,
            provenance: {
              kind: 'source_build',
              sources: {
                ogg: {
                  repository: 'xiph/ogg',
                  version: '1.3.6',
                  tag: 'v1.3.6',
                  tag_object_sha: 'db03f3f4f8dd37a9f0c7f0b2cfd8a0d2d3c6f5c4',
                  commit_sha: 'be05b13e98b048f0b5a0f5fa8ce514d56db5f822',
                  archive_url: 'https://downloads.xiph.org/releases/ogg/libogg-1.3.6.tar.xz',
                  archive_sha256:
                    '5c8253428e181840cd20d41f3ca16557a9cc04bad4a3d04cce84808677fa1061',
                },
                vorbis: {
                  repository: 'xiph/vorbis',
                  version: '1.3.7',
                  tag: 'v1.3.7',
                  tag_object_sha: '0c55b9f34f7f14a84ecfe1140ffecf95e8c9a596',
                  commit_sha: '0657aee69dec8508a0011f47f3b69d7538e9d262',
                  archive_url: 'https://downloads.xiph.org/releases/vorbis/libvorbis-1.3.7.tar.xz',
                  archive_sha256:
                    'b33cc4934322bcbf6efcbacf49e3ca01aadbea4114ec9589d1b1e9d20f72954b',
                },
              },
              build_revision: 1,
              recipe_sha256: 'a'.repeat(64),
              verification_policy_sha256: 'b'.repeat(64),
              patches: {},
              toolchain: {
                runner_image: 'windows-2025-vs2026',
                compiler: 'MSVC',
                linker: 'link.exe',
                windows_sdk: '10.0',
                cmake: '4.1',
              },
            },
            legal_documents: [
              {
                legal_document_id: 'xiph-license',
                kind: 'license',
                title: 'Xiph BSD licenses',
                format: 'text',
                file_name: 'LICENSE.txt',
                content_url: 'https://cdn.example.test/xiph/LICENSE.txt',
              },
            ],
          },
        },
      }),
    ]);

    await openVersionSelect();
    const text =
      document.body.querySelector('[role="option"][data-value="xiph"]')?.textContent ?? '';
    expect(text).toContain('Ogg 1.3.6');
    expect(text).toContain('Vorbis 1.3.7');
    expect(text).not.toContain('Unsigned');
    expect(text).not.toContain('source-build');
    expect(text).not.toContain('shared.plain');
    expect(text).not.toContain('be05b13e98b0');
    expect(text).not.toContain('windows-2025-vs2026');
    expect(text).not.toContain('Xiph BSD licenses');
  });

  it('shows only differing member versions for generic composite packages', async () => {
    mounted = mountRow([
      catalogCandidate('2.0.0', {
        artifact_id: 'nuget-composite',
        catalog_package: {
          package_id: 'example.nuget.composite',
          release: {
            version: '2.0.0',
            channel: 'stable',
            label: null,
            components: { core: '2.0.0', support: '1.4.0' },
          },
          availability: 'available',
          automatic_selection_allowed: true,
          presentation: {
            variant: 'runtime',
            architecture: 'X64',
            unsigned: false,
            provenance: {
              kind: 'nuget',
              package_id: 'Example.Composite',
              version: '2.0.0',
              package_sha512: 'fixture',
            },
            legal_documents: [],
          },
        },
      }),
      catalogCandidate('1.0.0', {
        artifact_id: 'github-composite',
        catalog_package: {
          package_id: 'example.github.composite',
          release: {
            version: '1.0.0',
            channel: 'stable',
            label: null,
            components: { core: '1.0.0', support: '0.9.0' },
          },
          availability: 'available',
          automatic_selection_allowed: true,
          presentation: {
            variant: 'runtime',
            architecture: 'X64',
            unsigned: false,
            provenance: {
              kind: 'github_release',
              repository: 'example/composite',
              tag: 'v1.0.0',
              commit_sha: 'a'.repeat(40),
            },
            legal_documents: [],
          },
        },
      }),
    ]);

    await openVersionSelect();
    const nuget = document.body.querySelector('[role="option"][data-value="nuget-composite"]');
    const github = document.body.querySelector('[role="option"][data-value="github-composite"]');
    expect(nuget?.textContent).toContain('Core 2.0.0');
    expect(nuget?.textContent).toContain('Support 1.4.0');
    expect(github?.textContent).toContain('Core 1.0.0');
    expect(github?.textContent).toContain('Support 0.9.0');
    expect(nuget?.textContent).not.toContain('NuGet Example.Composite');
    expect(github?.textContent).not.toContain('example/composite');
  });

  function mountRow(candidates: Parameters<typeof group>[3]): object {
    const result = mount(ComponentVersionRowTestHost, {
      target,
      props: {
        component: componentFixture('component:preview', 'microsoft_dxc'),
        group: group('component:preview', 'microsoft_dxc', '10.0.0', candidates),
        busy: false,
        onSwap: vi.fn(),
        onRollback: vi.fn(),
      },
    });
    flushSync();
    return result;
  }

  async function openVersionSelect(): Promise<void> {
    const trigger = versionSelectTrigger();
    trigger.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0 }));
    trigger.click();
    flushSync();
    await vi.waitFor(() => {
      expect(document.body.querySelector('[role="listbox"]')).not.toBeNull();
    });
  }

  function versionSelectTrigger(): HTMLButtonElement {
    const trigger = target.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]');
    if (!trigger) {
      throw new Error('Version selector was not rendered');
    }
    return trigger;
  }
});

function selectItemLabels(): string[] {
  return [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].map((option) =>
    option.textContent.replace(/\s+/gu, ''),
  );
}

function versionOccurrenceCount(versionLabel: string): number {
  return [...document.body.querySelectorAll<HTMLElement>('[role="option"]')].filter((option) =>
    option.textContent.includes(versionLabel),
  ).length;
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
