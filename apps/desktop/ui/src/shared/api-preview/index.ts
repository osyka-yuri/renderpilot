export type DesktopCommandPayload = Record<string, unknown>;

export type DesktopInvoker<C extends string = string> = <T>(
  command: C,
  payload?: DesktopCommandPayload,
) => Promise<T>;

let previewInvoker: DesktopInvoker | null = null;

export function registerPreviewInvoker(invoker: DesktopInvoker): () => void {
  previewInvoker = invoker;

  return () => {
    if (previewInvoker === invoker) {
      previewInvoker = null;
    }
  };
}

export function clearPreviewInvoker(): void {
  previewInvoker = null;
}

export function isDesktopPreviewMode(): boolean {
  return typeof window !== 'undefined' && !hasTauriInternals(window);
}

export async function invokePreviewCommand<T>(
  command: string,
  payload?: DesktopCommandPayload,
): Promise<T> {
  const invoker = requirePreviewInvoker();

  return invoker<T>(command, payload);
}

function requirePreviewInvoker(): DesktopInvoker {
  if (!previewInvoker) {
    throw new Error('Desktop preview mode is active but no preview invoker was registered.');
  }

  return previewInvoker;
}

function hasTauriInternals(target: Window): boolean {
  return '__TAURI_INTERNALS__' in target;
}
