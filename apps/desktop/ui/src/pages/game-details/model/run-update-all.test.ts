import { describe, expect, it, vi } from 'vitest';

vi.mock('@shared/notifications', () => ({
  publishPresentedErrorNotification: vi.fn(),
}));

import { defaultHostFacts } from '@entities/addon';
import { createRenoDxStore } from '@features/renodx';
import { publishPresentedErrorNotification } from '@shared/notifications';

import { runUpdateAll, UpdateAllError, UpdateAllOwnerChangedError } from './run-update-all';

const ITEM = {
  componentId: 'dlss',
  artifactId: 'artifact:new',
  isDownloaded: false,
};

function ownerOptions() {
  const owner = Object.freeze({ gameId: 'game:1', installPath: '/game' });
  return { owner, isOwnerCurrent: () => true };
}

describe('runUpdateAll', () => {
  it('supports an add-on-only batch and preserves store order', async () => {
    const events: string[] = [];
    const first = {
      update: vi.fn(() => {
        events.push('renodx');
        return Promise.resolve('ok' as const);
      }),
    };
    const second = {
      update: vi.fn(() => {
        events.push('luma');
        return Promise.resolve('ok' as const);
      }),
    };
    const onBulkSwap = vi.fn();

    await runUpdateAll({
      ...ownerOptions(),
      items: [],
      addonUpdates: [
        { step: 'renodx', store: first },
        { step: 'luma', store: second },
      ],
      onBulkSwap,
    });

    expect(events).toEqual(['renodx', 'luma']);
    expect(onBulkSwap).not.toHaveBeenCalled();
  });

  it('runs library updates before the captured add-on batch', async () => {
    const events: string[] = [];
    const store = {
      update: vi.fn(() => {
        events.push('addon');
        return Promise.resolve('ok' as const);
      }),
    };

    await runUpdateAll({
      ...ownerOptions(),
      items: [ITEM],
      addonUpdates: [{ step: 'renodx', store }],
      onBulkSwap: () => {
        events.push('libraries');
        return Promise.resolve();
      },
    });

    expect(events).toEqual(['libraries', 'addon']);
  });

  it('attempts later add-ons before reporting unexpected failures', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };

    const result = runUpdateAll({
      ...ownerOptions(),
      items: [ITEM],
      addonUpdates: [
        {
          step: 'renodx',
          store: { update: vi.fn(() => Promise.reject(new Error('addon failed'))) },
        },
        { step: 'luma', store: later },
      ],
      onBulkSwap: () => Promise.reject(new Error('libraries failed')),
    });

    await expect(result).rejects.toThrow('One or more update-all steps failed');
    expect(later.update).toHaveBeenCalledWith('game:1');
  });

  it('treats store.update resolving failed as a failure and still runs later stores', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };

    const result = runUpdateAll({
      ...ownerOptions(),
      items: [],
      addonUpdates: [
        { step: 'renodx', store: { update: vi.fn(() => Promise.resolve('failed' as const)) } },
        { step: 'luma', store: later },
      ],
      onBulkSwap: vi.fn(),
    });

    await expect(result).rejects.toThrow('One or more update-all steps failed');
    expect(later.update).toHaveBeenCalledWith('game:1');
  });

  it('does not treat store.update skipped as a failure', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };

    await runUpdateAll({
      ...ownerOptions(),
      items: [],
      addonUpdates: [
        { step: 'renodx', store: { update: vi.fn(() => Promise.resolve('skipped' as const)) } },
        { step: 'luma', store: later },
      ],
      onBulkSwap: vi.fn(),
    });

    expect(later.update).toHaveBeenCalledWith('game:1');
  });

  it('stops after a delayed add-on when the same game switches install path', async () => {
    const mutation = Promise.withResolvers<'skipped'>();
    const started = Promise.withResolvers<undefined>();
    let installPath = '/game/first';
    const owner = Object.freeze({ gameId: 'game:1', installPath });
    const first = {
      update: vi.fn(() => {
        started.resolve(undefined);
        return mutation.promise;
      }),
    };
    const second = {
      update: vi.fn(() => Promise.resolve('ok' as const)),
    };

    const run = runUpdateAll({
      owner,
      isOwnerCurrent: () => installPath === owner.installPath,
      items: [],
      addonUpdates: [
        { step: 'luma', store: first },
        { step: 'optiscaler', store: second },
      ],
      onBulkSwap: vi.fn(),
    });

    await started.promise;
    installPath = '/game/second';
    mutation.resolve('skipped');

    await expect(run).rejects.toBeInstanceOf(UpdateAllOwnerChangedError);
    expect(second.update).not.toHaveBeenCalled();
  });

  it('stops remaining add-ons after a stale safety context', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };
    const stale = { code: 'safety_context_stale' };

    const result = runUpdateAll({
      ...ownerOptions(),
      items: [],
      addonUpdates: [
        {
          step: 'renodx',
          store: {
            update: vi.fn(() => Promise.resolve('failed' as const)),
            safetyContextError: stale,
          },
        },
        { step: 'luma', store: later },
      ],
      onBulkSwap: vi.fn(),
    });

    const error = await result.catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(UpdateAllError);
    expect((error as UpdateAllError).failures).toEqual([
      { step: 'renodx', error: stale, reportedByStore: true },
    ]);
    expect(later.update).not.toHaveBeenCalled();
  });

  it('does not attribute a retained safety error to a skipped add-on update', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };

    await runUpdateAll({
      ...ownerOptions(),
      items: [],
      addonUpdates: [
        {
          step: 'renodx',
          store: {
            update: vi.fn(() => Promise.resolve('skipped' as const)),
            safetyContextError: { code: 'safety_context_stale' },
          },
        },
        { step: 'luma', store: later },
      ],
      onBulkSwap: vi.fn(),
    });

    expect(later.update).toHaveBeenCalledOnce();
  });

  it('stops on RenoDX token-capture failure and retries without stale store error', async () => {
    const staleCapture = { code: 'safety_context_stale' };
    const staleBackend = Object.assign(new Error('backend safety token is stale'), {
      code: 'safety_context_stale',
    });
    const requireSafetyTokens = vi
      .fn()
      .mockRejectedValueOnce(staleCapture)
      .mockResolvedValueOnce({ gameContextToken: 'backend-error-token' })
      .mockResolvedValueOnce(null)
      .mockResolvedValue({ gameContextToken: 'fresh-game-token' });
    const installedState = {
      status: 'installed' as const,
      host_kind: 'proxy' as const,
      version: '1.0.0',
      addon_dated: null,
      installed_at: 1,
      updated_at: 1,
      dlss_fix_evidence_present: false,
      addon_tracked: true,
    };
    const availability = {
      state: installedState,
      host_detection: 'present',
      host_facts: defaultHostFacts('stable'),
      actions: {},
      reshade_stable_supported: true,
      renodx_addon: null,
      install_torn: false,
      outcome: { kind: 'unsupported' },
      manual_install: null,
      vulkan_layer: {
        layer_detection: 'not_installed',
        layer_facts: {
          manifest_path: null,
          dll_path: null,
          version: null,
          architecture: 'unknown',
          loader_visibility: 'normal',
        },
        diagnostic_reasons: [],
        actions: {},
      },
    };
    const api = {
      getAvailability: vi.fn(() => Promise.resolve(availability)),
      checkUpdate: vi.fn(() =>
        Promise.resolve({
          addon: 'available',
          host: 'current',
          dlssFix: null,
          overall: 'available',
        }),
      ),
      update: vi.fn().mockRejectedValueOnce(staleBackend).mockResolvedValue(installedState),
      dlssFixAvailability: vi.fn(() =>
        Promise.resolve({ kind: 'binding', state: 'none', actions: [] }),
      ),
      vulkanLayerStatus: vi.fn(() => Promise.resolve(availability.vulkan_layer)),
    } as unknown as NonNullable<NonNullable<Parameters<typeof createRenoDxStore>[0]>['api']>;
    const renodx = createRenoDxStore({ api, requireSafetyTokens });
    await renodx.load('game:1');
    vi.mocked(publishPresentedErrorNotification).mockClear();

    const luma = { update: vi.fn(() => Promise.resolve('ok' as const)) };
    const optiscaler = { update: vi.fn(() => Promise.resolve('ok' as const)) };
    const options = {
      ...ownerOptions(),
      items: [],
      addonUpdates: [
        { step: 'renodx' as const, store: renodx },
        { step: 'luma' as const, store: luma },
        { step: 'optiscaler' as const, store: optiscaler },
      ],
      onBulkSwap: vi.fn(),
    };

    const firstAttempt = runUpdateAll(options);
    await expect(firstAttempt).rejects.toMatchObject({
      failures: [{ step: 'renodx', error: staleCapture, reportedByStore: true }],
    });
    expect(renodx.safetyContextError).toBe(staleCapture);
    expect(luma.update).not.toHaveBeenCalled();
    expect(optiscaler.update).not.toHaveBeenCalled();
    expect(api.update).not.toHaveBeenCalled();
    expect(publishPresentedErrorNotification).toHaveBeenCalledOnce();

    await expect(runUpdateAll(options)).rejects.toMatchObject({
      failures: [{ step: 'renodx', error: staleBackend, reportedByStore: true }],
    });
    expect(renodx.safetyContextError).toBe(staleBackend);
    expect(luma.update).not.toHaveBeenCalled();
    expect(optiscaler.update).not.toHaveBeenCalled();
    expect(api.update).toHaveBeenCalledOnce();
    expect(publishPresentedErrorNotification).toHaveBeenCalledTimes(2);

    // A later cancellation has no current safety error and must mask the
    // backend error retained by the core from the preceding attempt.
    await runUpdateAll(options);

    expect(renodx.safetyContextError).toBeNull();
    expect(api.update).toHaveBeenCalledOnce();
    expect(luma.update).toHaveBeenCalledOnce();
    expect(optiscaler.update).toHaveBeenCalledOnce();
    expect(publishPresentedErrorNotification).toHaveBeenCalledTimes(2);

    await runUpdateAll(options);

    expect(renodx.safetyContextError).toBeNull();
    expect(api.update).toHaveBeenCalledWith('game:1', 'fresh-game-token', undefined);
    expect(api.update).toHaveBeenCalledTimes(2);
    expect(luma.update).toHaveBeenCalledTimes(2);
    expect(optiscaler.update).toHaveBeenCalledTimes(2);
    expect(publishPresentedErrorNotification).toHaveBeenCalledTimes(2);
  });

  it('aborts add-on updates when the library batch reports a stale context', async () => {
    const later = { update: vi.fn(() => Promise.resolve('ok' as const)) };
    const stale = Object.assign(new Error('stale safety context'), {
      code: 'safety_context_scope_mismatch',
    });

    const result = runUpdateAll({
      ...ownerOptions(),
      items: [ITEM],
      addonUpdates: [{ step: 'renodx', store: later }],
      onBulkSwap: () => Promise.reject(stale),
    });

    const error = await result.catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(UpdateAllError);
    expect((error as UpdateAllError).failures).toEqual([{ step: 'libraries', error: stale }]);
    expect(later.update).not.toHaveBeenCalled();
  });

  it('attributes isolated failures to their workflow step', async () => {
    const result = runUpdateAll({
      ...ownerOptions(),
      items: [ITEM],
      addonUpdates: [
        { step: 'renodx', store: { update: vi.fn(() => Promise.resolve('failed' as const)) } },
      ],
      onBulkSwap: () => Promise.reject(new Error('library failed')),
    });

    const error = await result.catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(UpdateAllError);
    expect((error as UpdateAllError).failures.map((failure) => failure.step)).toEqual([
      'libraries',
      'renodx',
    ]);
    expect((error as UpdateAllError).failures[1]?.error).toMatchObject({
      code: 'update_all_step_failed',
    });
  });
});
