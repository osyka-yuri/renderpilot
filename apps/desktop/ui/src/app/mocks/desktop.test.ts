import { clearPreviewInvoker, invokePreviewCommand } from '@shared/api-preview';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { registerMockInvoker, resetMockDesktopState } from './desktop';

describe('desktop preview startup contract', () => {
  beforeEach(() => {
    clearPreviewInvoker();
    resetMockDesktopState();
    registerMockInvoker();
  });

  afterEach(() => {
    clearPreviewInvoker();
  });

  it('accepts the portable trial readiness handshake', async () => {
    await expect(invokePreviewCommand('portable_trial_ready')).resolves.toBeUndefined();
  });
});
