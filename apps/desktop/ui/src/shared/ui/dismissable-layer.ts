export type DismissableLayerReason = 'outside-pointer' | 'escape';

type DismissableLayerOriginalEventByReason = {
  readonly 'outside-pointer': PointerEvent;
  readonly escape: KeyboardEvent;
};

export type DismissableLayerEvent = {
  readonly [Reason in DismissableLayerReason]: {
    readonly reason: Reason;
    readonly originalEvent: DismissableLayerOriginalEventByReason[Reason];
  };
}[DismissableLayerReason];

export type DismissableLayerOptions = Readonly<{
  isEnabled: () => boolean;
  isEventInside: (event: Event) => boolean;
  onDismiss: (event: DismissableLayerEvent) => void;
}>;

export type DismissableLayerDispose = () => void;

type DismissableLayerEntry = Readonly<{
  id: number;
  options: DismissableLayerOptions;
}>;

type DismissableLayerState = {
  nextLayerId: number;
  attachedDocument: Document | null;
  layerStack: DismissableLayerEntry[];
};

const noopDispose: DismissableLayerDispose = () => undefined;

const pointerDownListenerOptions: AddEventListenerOptions = {
  capture: true,
  passive: true,
};

const keyDownListenerOptions: AddEventListenerOptions = {
  capture: true,
};

const state: DismissableLayerState = {
  nextLayerId: 1,
  attachedDocument: null,
  layerStack: [],
};

function getCurrentDocument(): Document | null {
  return typeof document === 'undefined' ? null : document;
}

function createLayerEntry(options: DismissableLayerOptions): DismissableLayerEntry {
  const entry: DismissableLayerEntry = {
    id: state.nextLayerId,
    options,
  };

  state.nextLayerId += 1;

  return entry;
}

function getTopEnabledLayer(): DismissableLayerEntry | null {
  for (let index = state.layerStack.length - 1; index >= 0; index -= 1) {
    const layer = state.layerStack[index];

    if (layer.options.isEnabled()) {
      return layer;
    }
  }

  return null;
}

function removeLayer(entry: DismissableLayerEntry): void {
  const index = state.layerStack.findIndex((layer) => layer.id === entry.id);

  if (index === -1) {
    return;
  }

  state.layerStack.splice(index, 1);
}

function dismissLayer(entry: DismissableLayerEntry, event: DismissableLayerEvent): void {
  entry.options.onDismiss(event);
}

function handlePointerDown(event: PointerEvent): void {
  const topLayer = getTopEnabledLayer();

  if (topLayer === null) {
    return;
  }

  if (topLayer.options.isEventInside(event)) {
    return;
  }

  dismissLayer(topLayer, {
    reason: 'outside-pointer',
    originalEvent: event,
  });
}

function handleKeyDown(event: KeyboardEvent): void {
  if (event.key !== 'Escape') {
    return;
  }

  const topLayer = getTopEnabledLayer();

  if (topLayer === null) {
    return;
  }

  event.preventDefault();

  dismissLayer(topLayer, {
    reason: 'escape',
    originalEvent: event,
  });
}

function addDocumentListeners(targetDocument: Document): void {
  targetDocument.addEventListener('pointerdown', handlePointerDown, pointerDownListenerOptions);
  targetDocument.addEventListener('keydown', handleKeyDown, keyDownListenerOptions);
}

function removeDocumentListeners(targetDocument: Document): void {
  targetDocument.removeEventListener('pointerdown', handlePointerDown, pointerDownListenerOptions);

  targetDocument.removeEventListener('keydown', handleKeyDown, keyDownListenerOptions);
}

function attachListeners(targetDocument: Document): void {
  if (state.attachedDocument === targetDocument) {
    return;
  }

  if (state.attachedDocument !== null) {
    removeDocumentListeners(state.attachedDocument);
  }

  addDocumentListeners(targetDocument);
  state.attachedDocument = targetDocument;
}

function detachListenersIfIdle(): void {
  if (state.layerStack.length > 0) {
    return;
  }

  if (state.attachedDocument === null) {
    return;
  }

  removeDocumentListeners(state.attachedDocument);
  state.attachedDocument = null;
}

export function registerDismissableLayer(
  options: DismissableLayerOptions,
): DismissableLayerDispose {
  const targetDocument = getCurrentDocument();

  if (targetDocument === null) {
    return noopDispose;
  }

  const entry = createLayerEntry(options);
  let isDisposed = false;

  state.layerStack.push(entry);
  attachListeners(targetDocument);

  return () => {
    if (isDisposed) {
      return;
    }

    isDisposed = true;

    removeLayer(entry);
    detachListenersIfIdle();
  };
}
