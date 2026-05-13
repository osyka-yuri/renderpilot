import { invoke } from '@tauri-apps/api/core';
import { isPlainObject, requireNonBlankString } from '@shared/validation';
import {
  invokePreviewCommand,
  isDesktopPreviewMode,
  type DesktopCommandPayload,
} from '@shared/api-preview';
import { normalizeCommandError } from './errors';

function normalizeCommandPayload(payload: unknown): DesktopCommandPayload | undefined {
  if (payload === undefined) {
    return undefined;
  }

  if (!isPlainObject(payload)) {
    throw new TypeError('Command payload must be a plain object when provided.');
  }

  return payload;
}

async function invokeTauriCommand<T>(command: string, payload?: DesktopCommandPayload): Promise<T> {
  if (payload === undefined) {
    return invoke<T>(command);
  }

  return invoke<T>(command, payload);
}

export async function invokeDesktop<T>(command: string, payload?: unknown): Promise<T> {
  try {
    const safeCommand = requireNonBlankString(command, 'command');
    const safePayload = normalizeCommandPayload(payload);

    if (isDesktopPreviewMode()) {
      return await invokePreviewCommand<T>(safeCommand, safePayload);
    }

    return await invokeTauriCommand<T>(safeCommand, safePayload);
  } catch (error) {
    throw normalizeCommandError(error);
  }
}
