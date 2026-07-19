import type { ActionDescriptor, HostFacts, ReshadeChannel } from './types';
import { defaultHostFacts } from './store-helpers';

/** Shared ActionDescriptor builder for Luma/RenoDX store tests. */
export function action(overrides: Partial<ActionDescriptor> = {}): ActionDescriptor {
  return {
    enabled: true,
    requires_confirmation: false,
    confirmation_scope: null,
    disabled_reason: null,
    target_channel: null,
    ...overrides,
  };
}

/** Default host facts for store tests, keyed by the tool's default channel. */
export function defaultTestHostFacts(defaultChannel: ReshadeChannel): HostFacts {
  return defaultHostFacts(defaultChannel);
}
