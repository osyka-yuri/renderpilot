/**
 * @vitest-environment jsdom
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

import { t } from '@shared/i18n';
import type { AddGameConfirmation, AddGameInspection } from '../model/add-game';
import type { AddGameDialogState } from '../model/add-game-flow.svelte';
import AddGameDialog from './AddGameDialog.svelte';

type DialogTestProps = {
  state: AddGameDialogState;
  onClose?: () => void;
  onChooseFolder?: () => void | Promise<void>;
  onConfirm?: (confirmation: AddGameConfirmation) => void | Promise<void>;
  onRollbackAndConfirm?: (confirmation: AddGameConfirmation) => void | Promise<void>;
};

describe('AddGameDialog', () => {
  let target: HTMLDivElement;
  let component: Record<string, never> | undefined;
  let initialBodyStyle: string;

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
    initialBodyStyle = document.body.style.cssText;
  });

  afterEach(async () => {
    const mountedComponent = component;
    component = undefined;
    try {
      if (mountedComponent) {
        await closeAndUnmountDialog(mountedComponent, initialBodyStyle);
      }
    } finally {
      vi.unstubAllGlobals();
      document.body.replaceChildren();
    }
  });

  it('offers an explicit identity-preserving correction for an oversized manual root', async () => {
    const onConfirm = vi.fn();
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'D:/Games/The Last of Us Part I',
          recommendation: recommendation('D:/Games', 'existing_catalog'),
          rootCorrection: {
            gameId: 'game:oversized-root',
            status: 'ready',
            cleanupActions: [],
            blockers: [],
          },
          decision: correctionDecision(),
          relationship: {
            kind: 'inside_existing',
            gameIds: ['game:oversized-root'],
            provenInstallRoots: [],
          },
          warnings: [
            {
              contractStatus: 'known',
              code: 'inside_existing_install',
              parameters: {},
            },
          ],
        }),
        errorPresentation: null,
      },
      onConfirm,
    });
    await render();

    expect(document.body.textContent).not.toContain(t('addGame.warning.insideExistingInstall'));
    expect(document.body.textContent).not.toContain('Backend fallback must not be rendered.');
    expect(document.body.textContent).toContain(t('addGame.replaceRootDescription'));
    expect(
      document.body.querySelector('[data-add-game-warning="nested_root_selected"]'),
    ).toBeNull();
    const rootItems = document.body.querySelectorAll(
      '[data-slot="item"][data-variant="muted"][data-size="sm"]',
    );
    expect(rootItems).toHaveLength(2);
    expect(rootItems[0].querySelector('[data-slot="item-title"]')?.textContent).toBe(
      t('addGame.selectedFolder'),
    );
    expect(rootItems[1].querySelector('[data-slot="item-title"]')?.textContent).toBe(
      t('addGame.existingRoot'),
    );
    button(t('addGame.replaceExistingRoot')).click();
    expect(onConfirm).toHaveBeenCalledWith({
      rootChoice: 'selected',
      allowRootCorrection: true,
      chosenExecutable: null,
    });
  });

  it('replaces a covered warning with one actionable unavailable reason', async () => {
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'C:/Games/Black Flag/Binaries',
          recommendation: recommendation('C:/Games/Black Flag', 'existing_catalog'),
          relationship: {
            kind: 'inside_existing',
            gameIds: ['game:black-flag'],
            provenInstallRoots: [],
          },
          warnings: [
            {
              contractStatus: 'known',
              code: 'inside_existing_install',
              parameters: {},
            },
          ],
          decision: { kind: 'unavailable', reasons: ['inside_existing_install'] },
        }),
        errorPresentation: null,
      },
    });
    await render();

    const message = t('addGame.unavailable.insideExistingInstall');
    expect(document.body.textContent.split(message)).toHaveLength(2);
    expect(document.body.textContent).not.toContain(t('addGame.warning.insideExistingInstall'));
    const unavailable = document.body.querySelector(
      '[data-add-game-unavailable="inside_existing_install"]',
    );
    expect(unavailable?.querySelector('ul')).toBeNull();
    expect(unavailable?.textContent.trim()).toBe(message);
    expect(document.body.textContent).not.toContain(t('addGame.replaceRootDescription'));
    expect(() => button(t('addGame.replaceExistingRoot'))).toThrow();
    expect(() => button(t('addGame.rootCorrection.rollbackAndReplace'))).toThrow();
  });

  it('renders every independent unavailable reason as a separate alert', async () => {
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'D:/Games/Black Flag/Binaries',
          relationship: {
            kind: 'inside_existing',
            gameIds: ['game:black-flag'],
            provenInstallRoots: [],
          },
          decision: {
            kind: 'unavailable',
            reasons: ['inside_existing_install', 'no_readable_executable'],
          },
          warnings: [],
        }),
        errorPresentation: null,
      },
    });
    await render();

    const unavailable = Array.from(document.body.querySelectorAll('[data-add-game-unavailable]'));
    expect(unavailable).toHaveLength(2);
    expect(unavailable.map((alert) => alert.getAttribute('data-add-game-unavailable'))).toEqual([
      'inside_existing_install',
      'no_readable_executable',
    ]);
    expect(unavailable[0].textContent).toContain(t('addGame.unavailable.insideExistingInstall'));
    expect(unavailable[1].textContent).toContain(t('addGame.unavailable.noReadableExecutable'));
    for (const alert of unavailable) {
      expect(alert.getAttribute('data-slot')).toBe('alert');
      expect(alert.querySelector('ul')).toBeNull();
      expect(alert.textContent).not.toContain(t('addGame.cannotAddTitle'));
    }
    expect(() => button(t('addGame.add'))).toThrow();
  });

  it('shows one actionable reason for a shared library without legacy warning noise', async () => {
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'D:/Games',
          boundary: {
            kind: 'multiple_install_container',
            completeness: 'complete',
            candidateRoots: ['D:/Games/A', 'D:/Games/B'],
            evidence: ['executable_branch'],
          },
          relationship: {
            kind: 'contains_proven_install',
            gameIds: ['game:a'],
            provenInstallRoots: ['D:/Games/A'],
          },
          decision: {
            kind: 'unavailable',
            reasons: ['multiple_installs'],
          },
          warnings: [
            {
              contractStatus: 'unknown',
              code: 'multiple_installs_suspected',
              parameters: {},
            },
            {
              contractStatus: 'known',
              code: 'contains_proven_install',
              parameters: {},
            },
          ],
        }),
        errorPresentation: null,
      },
    });
    await render();

    const unavailable = document.body.querySelector(
      '[data-add-game-unavailable="multiple_installs"]',
    );
    expect(unavailable?.querySelector('ul')).toBeNull();
    expect(unavailable?.textContent.trim()).toBe(t('addGame.unavailable.multipleInstalls'));
    expect(document.body.textContent).not.toContain(t('addGame.warning.multipleInstallsSuspected'));
    expect(document.body.textContent).not.toContain(t('addGame.warning.containsProvenInstall'));
    expect(document.body.textContent).not.toContain('Backend multiple-install fallback');
    expect(document.body.textContent).not.toContain('Backend proven-install fallback');
  });

  it('requires an explicit rollback confirmation before replacing an incompatible root', async () => {
    const onConfirm = vi.fn();
    const onRollbackAndConfirm = vi.fn();
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'D:/Games/The Last of Us Part I',
          recommendation: recommendation('D:/Games', 'existing_catalog'),
          relationship: {
            kind: 'inside_existing',
            gameIds: ['game:oversized-root'],
            provenInstallRoots: [],
          },
          rootCorrection: {
            gameId: 'game:oversized-root',
            status: 'cleanup_required',
            cleanupActions: [
              { kind: 'rollback_component', componentId: 'component:a' },
              { kind: 'rollback_component', componentId: 'component:b' },
            ],
            blockers: [],
          },
          decision: correctionDecision(),
        }),
        errorPresentation: null,
      },
      onConfirm,
      onRollbackAndConfirm,
    });
    await render();

    expect(document.body.textContent).toContain(t('addGame.rootCorrection.rollbackTitle'));
    const action = button(t('addGame.rootCorrection.rollbackAndReplace'));
    action.click();
    expect(onConfirm).not.toHaveBeenCalled();
    expect(onRollbackAndConfirm).toHaveBeenCalledWith({
      rootChoice: 'selected',
      allowRootCorrection: true,
      chosenExecutable: null,
    });
  });

  it('offers an identity-preserving correction from an internal subtree to the game root', async () => {
    const onConfirm = vi.fn();
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'D:/Games/The Last of Us Part I',
          relationship: {
            kind: 'expands_existing',
            gameIds: ['game:too-narrow'],
            provenInstallRoots: [],
          },
          rootCorrection: {
            gameId: 'game:too-narrow',
            status: 'ready',
            cleanupActions: [],
            blockers: [],
          },
          decision: correctionDecision(),
        }),
        errorPresentation: null,
      },
      onConfirm,
    });
    await render();

    expect(document.body.textContent).toContain(t('addGame.replaceRootDescription'));
    button(t('addGame.correctRoot')).click();
    expect(onConfirm).toHaveBeenCalledWith({
      rootChoice: 'selected',
      allowRootCorrection: true,
      chosenExecutable: null,
    });
  });

  it('uses the catalog action attached to the currently selected backend option', async () => {
    const onConfirm = vi.fn();
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'D:/Games/Example/Subtree',
          recommendation: recommendation('D:/Games/Example', 'existing_catalog'),
          relationship: {
            kind: 'inside_existing',
            gameIds: ['game:example'],
            provenInstallRoots: [],
          },
          rootCorrection: {
            gameId: 'game:example',
            status: 'ready',
            cleanupActions: [],
            blockers: [],
          },
          decision: {
            kind: 'review',
            defaultOption: { rootChoice: 'recommended', catalogAction: 'rescan' },
            options: [
              { rootChoice: 'selected', catalogAction: 'correct_existing_root' },
              { rootChoice: 'recommended', catalogAction: 'rescan' },
            ],
          },
        }),
        errorPresentation: null,
      },
      onConfirm,
    });
    await render();

    expect(() => button(t('addGame.replaceExistingRoot'))).toThrow();
    button(t('addGame.rescan')).click();
    expect(onConfirm).toHaveBeenCalledWith({
      rootChoice: 'recommended',
      allowRootCorrection: false,
      chosenExecutable: null,
    });
  });

  it('shows exact non-component blockers without offering an unsafe correction action', async () => {
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'D:/Games/The Last of Us Part I',
          recommendation: recommendation('D:/Games', 'existing_catalog'),
          relationship: {
            kind: 'inside_existing',
            gameIds: ['game:oversized-root'],
            provenInstallRoots: [],
          },
          rootCorrection: {
            gameId: 'game:oversized-root',
            status: 'blocked',
            cleanupActions: [],
            blockers: ['pending_recovery', 'installed_addon', 'nvapi'],
          },
          decision: { kind: 'unavailable', reasons: ['root_correction_blocked'] },
        }),
        errorPresentation: null,
      },
    });
    await render();

    const blockers = Array.from(document.body.querySelectorAll('[data-root-correction-blocker]'));
    expect(blockers).toHaveLength(3);
    expect(blockers.map((alert) => alert.getAttribute('data-root-correction-blocker'))).toEqual([
      'pending_recovery',
      'installed_addon',
      'nvapi',
    ]);
    const expectedMessages = [
      t('addGame.rootCorrection.blocker.pendingRecovery'),
      t('addGame.rootCorrection.blocker.installedAddon'),
      t('addGame.rootCorrection.blocker.nvapi'),
    ];
    for (const [index, alert] of blockers.entries()) {
      expect(alert.getAttribute('data-slot')).toBe('alert');
      expect(alert.getAttribute('data-root-correction-status')).toBe('blocked');
      expect(alert.querySelector('ul')).toBeNull();
      expect(alert.querySelector('[data-slot="alert-title"]')).toBeNull();
      expect(alert.textContent.trim()).toBe(expectedMessages[index]);
    }
    expect(() => button(t('addGame.replaceExistingRoot'))).toThrow();
    expect(() => button(t('addGame.rootCorrection.rollbackAndReplace'))).toThrow();
  });

  it('lets the user override a recommendation when the selected subtree has a readable PE', async () => {
    const onConfirm = vi.fn();
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'C:/Games/Jedi Survivor/SwGame',
          boundary: {
            kind: 'engine_project_subtree',
            completeness: 'complete',
            candidateRoots: ['C:/Games/Jedi Survivor'],
            evidence: ['engine_structure'],
          },
          recommendation: recommendation('C:/Games/Jedi Survivor', 'engine_distribution_root'),
          executables: [
            {
              path: 'C:/Games/Jedi Survivor/SwGame/Binaries/Win64/JediSurvivor.exe',
              relativePath: 'Binaries/Win64/JediSurvivor.exe',
              sizeBytes: 1024,
              rankScore: 100,
              validWindowsPe: true,
              rejectionKind: null,
              rejectionToken: null,
            },
          ],
          decision: {
            kind: 'review',
            defaultOption: { rootChoice: 'selected', catalogAction: 'add' },
            options: [
              { rootChoice: 'selected', catalogAction: 'add' },
              { rootChoice: 'recommended', catalogAction: 'add' },
            ],
          },
        }),
        errorPresentation: null,
      },
      onConfirm,
    });
    await render();

    expect(button(t('addGame.add')).disabled).toBe(false);
    expect(document.body.textContent).toContain(t('addGame.recommendedFolder'));
    expect(document.body.textContent).toContain(t('addGame.selectedFolder'));
    const fieldset = document.body.querySelector('fieldset');
    expect(fieldset?.querySelector('legend')?.textContent.trim()).toBe(t('addGame.installRoot'));
    expect(
      fieldset?.querySelector('[data-slot="radio-group"]')?.getAttribute('aria-labelledby'),
    ).toBe('add-game-install-root-label');
    expect(fieldset?.querySelectorAll('[data-slot="item"]')).toHaveLength(2);
    expect(document.body.querySelector('label[for="add-game-selected-root"]')).not.toBeNull();
    button(t('addGame.add')).click();
    expect(onConfirm).toHaveBeenCalledWith({
      rootChoice: 'selected',
      allowRootCorrection: false,
      chosenExecutable: null,
    });
  });

  it('keeps the recommendation as the only valid choice when the selected folder has no PE', async () => {
    const onConfirm = vi.fn();
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          selectedRoot: 'C:/Games/Black Flag/D3D12',
          boundary: {
            kind: 'binary_subtree',
            completeness: 'complete',
            candidateRoots: ['C:/Games/Black Flag'],
            evidence: ['engine_structure'],
          },
          recommendation: recommendation('C:/Games/Black Flag', 'engine_distribution_root'),
          executables: [],
          warnings: [
            {
              contractStatus: 'known',
              code: 'no_readable_executable',
              parameters: {},
            },
          ],
          decision: {
            kind: 'review',
            defaultOption: { rootChoice: 'recommended', catalogAction: 'add' },
            options: [{ rootChoice: 'recommended', catalogAction: 'add' }],
          },
        }),
        errorPresentation: null,
      },
      onConfirm,
    });
    await render();

    expect(document.body.querySelector('label[for="add-game-selected-root"]')).toBeNull();
    expect(document.body.textContent).toContain(t('addGame.warning.noReadableExecutable'));
    button(t('addGame.add')).click();
    expect(onConfirm).toHaveBeenCalledWith({
      rootChoice: 'recommended',
      allowRootCorrection: false,
      chosenExecutable: null,
    });
  });

  it('renders repeated diagnostics through the shared alert component', async () => {
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection({
          warnings: [
            { contractStatus: 'known', code: 'filesystem_probe_error', parameters: {} },
            { contractStatus: 'known', code: 'filesystem_probe_error', parameters: {} },
          ],
        }),
        errorPresentation: null,
      },
    });
    await render();

    const warnings = document.body.querySelectorAll(
      '[data-add-game-warning="filesystem_probe_error"]',
    );
    expect(warnings).toHaveLength(2);
    for (const warning of warnings) {
      expect(warning.getAttribute('data-slot')).toBe('alert');
      expect(warning.textContent).toContain(t('addGame.warning.filesystemProbeError'));
    }
  });

  it('renders error actions and recovery paths as structured presentation data', async () => {
    const recoveryBundlePath = 'C:/Recovery/catalog-consolidation.bundle';
    component = mountDialog({
      state: {
        kind: 'review',
        inspection: inspection(),
        errorPresentation: {
          code: 'catalog_consolidation_blocked',
          severity: 'error',
          message: t('user_message.catalog_consolidation_blocked'),
          suggestedActions: [
            {
              code: 'refresh_or_scan_game_folder',
              label: t('suggested_action.refresh_or_scan_game_folder'),
            },
          ],
          recoveryBundlePath,
          contractStatus: 'known',
        },
      },
    });
    await render();

    const alert = document.body.querySelector('[data-slot="alert"]');
    const actions = alert?.querySelector('ul');
    expect(alert?.querySelector('[data-slot="alert-title"]')?.textContent).toBe(
      t('addGame.cannotAddTitle'),
    );
    expect(alert?.querySelector('svg')?.getAttribute('aria-hidden')).toBe('true');
    expect(actions?.querySelectorAll('li')).toHaveLength(1);
    expect(actions?.classList.contains('list-disc')).toBe(false);
    expect(document.body.textContent).toContain(t('user_message.catalog_consolidation_blocked'));
    expect(document.body.textContent).toContain(t('suggested_action.refresh_or_scan_game_folder'));
    expect(document.body.textContent).toContain(
      t('error.recoveryBundlePath', { path: recoveryBundlePath }),
    );
    expect(document.body.textContent).not.toContain('PRIVATE backend prose');
  });

  function mountDialog(overrides: DialogTestProps) {
    return mount(AddGameDialog, {
      target,
      props: {
        state: overrides.state,
        onClose: overrides.onClose ?? vi.fn(),
        onChooseFolder: overrides.onChooseFolder ?? vi.fn(),
        onConfirm: overrides.onConfirm ?? vi.fn(),
        onRollbackAndConfirm: overrides.onRollbackAndConfirm ?? vi.fn(),
      },
    });
  }
});

async function render(): Promise<void> {
  flushSync();
  await tick();
}

async function closeAndUnmountDialog(
  component: Record<string, never>,
  initialBodyStyle: string,
): Promise<void> {
  try {
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    flushSync();
    // bits-ui restores its body scroll lock asynchronously after the dialog closes.
    await waitForBodyStyle(initialBodyStyle);
  } finally {
    await unmount(component);
  }
}

async function waitForBodyStyle(expectedCssText: string): Promise<void> {
  await vi.waitFor(() => {
    expect(document.body.style.cssText).toBe(expectedCssText);
  });
}

function inspection(overrides: Partial<AddGameInspection> = {}): AddGameInspection {
  return {
    selectedRoot: 'C:/Games/Black Flag',
    inspectionFingerprint: 'inspection:test',
    catalogGeneration: 3,
    boundary: {
      kind: 'single_install',
      completeness: 'complete',
      candidateRoots: ['C:/Games/Black Flag'],
      evidence: ['root_executable'],
    },
    recommendation: null,
    relationship: {
      kind: 'new',
      gameIds: [],
      provenInstallRoots: [],
    },
    executables: [
      {
        path: 'C:/Games/Black Flag/AC4BFSP.exe',
        relativePath: 'AC4BFSP.exe',
        sizeBytes: 1024,
        rankScore: 100,
        validWindowsPe: true,
        rejectionKind: null,
        rejectionToken: null,
      },
    ],
    requiresExplicitExecutable: false,
    rootCorrection: null,
    decision: {
      kind: 'review',
      defaultOption: { rootChoice: 'selected', catalogAction: 'add' },
      options: [{ rootChoice: 'selected', catalogAction: 'add' }],
    },
    warnings: [],
    ...overrides,
  };
}

function correctionDecision(): AddGameInspection['decision'] {
  return {
    kind: 'review',
    defaultOption: { rootChoice: 'selected', catalogAction: 'correct_existing_root' },
    options: [{ rootChoice: 'selected', catalogAction: 'correct_existing_root' }],
  };
}

function recommendation(
  root: string,
  source: NonNullable<AddGameInspection['recommendation']>['source'] = 'root_executable',
): NonNullable<AddGameInspection['recommendation']> {
  return {
    root,
    source,
    confidence: source === 'launcher_manifest' ? 'authoritative' : 'suggested',
    completeness: 'complete',
    evidence:
      source === 'launcher_manifest'
        ? ['launcher_manifest']
        : source === 'engine_distribution_root'
          ? ['engine_distribution_root', 'engine_structure']
          : ['root_executable'],
  };
}

function button(label: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find(
    (candidate) => candidate.textContent.trim() === label,
  );
  if (!found) {
    throw new Error(`button not found: ${label}`);
  }
  return found;
}
