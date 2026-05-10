// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { registerDismissableLayer } from '@shared/ui/dismissable-layer';

type DismissableLayerOptions = Parameters<typeof registerDismissableLayer>[0];
type DismissableLayerDispose = ReturnType<typeof registerDismissableLayer>;
type DismissableLayerEvent = Parameters<DismissableLayerOptions['onDismiss']>[0];
type OnDismiss = DismissableLayerOptions['onDismiss'];

type TestLayerConfig = Readonly<{
  element?: HTMLElement;
  enabled?: boolean | (() => boolean);
  isEventInside?: DismissableLayerOptions['isEventInside'];
  onDismiss?: OnDismiss;
}>;

type TestLayer = Readonly<{
  element: HTMLElement;
  onDismiss: OnDismiss;
  dispose: DismissableLayerDispose;
}>;

type ExpectedDismiss = Pick<DismissableLayerEvent, 'reason' | 'originalEvent'>;

const registeredDisposers: DismissableLayerDispose[] = [];

function cleanupRegisteredLayers(): void {
  let dispose = registeredDisposers.pop();

  while (dispose !== undefined) {
    dispose();
    dispose = registeredDisposers.pop();
  }
}

function trackDispose(dispose: DismissableLayerDispose): DismissableLayerDispose {
  registeredDisposers.push(dispose);
  return dispose;
}

function appendElement(tagName = 'div'): HTMLElement {
  const element = document.createElement(tagName);

  document.body.appendChild(element);

  return element;
}

function appendElementIfNeeded(element: HTMLElement): HTMLElement {
  if (!element.isConnected) {
    document.body.appendChild(element);
  }

  return element;
}

function ensurePointerEvent(): void {
  if (typeof PointerEvent === 'function') {
    return;
  }

  globalThis.PointerEvent = MouseEvent as unknown as typeof PointerEvent;
}

function createPointerDownEvent(): PointerEvent {
  ensurePointerEvent();

  return new PointerEvent('pointerdown', {
    bubbles: true,
    cancelable: true,
  });
}

function dispatchPointerDown(target: EventTarget = document.body): PointerEvent {
  const event = createPointerDownEvent();

  target.dispatchEvent(event);

  return event;
}

function dispatchKeyDown(key: string): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key,
    bubbles: true,
    cancelable: true,
  });

  document.dispatchEvent(event);

  return event;
}

function createOnDismissMock(): OnDismiss {
  return vi.fn<(event: DismissableLayerEvent) => void>();
}

function createIsEnabled(enabled: boolean | (() => boolean)): () => boolean {
  return typeof enabled === 'function' ? enabled : () => enabled;
}

function createContainsEventTargetPredicate(
  element: HTMLElement,
): DismissableLayerOptions['isEventInside'] {
  return (event) => event.target instanceof Node && element.contains(event.target);
}

function createLayer(config: TestLayerConfig = {}): TestLayer {
  const element = appendElementIfNeeded(config.element ?? appendElement());
  const enabled = config.enabled ?? true;
  const onDismiss = config.onDismiss ?? createOnDismissMock();

  const dispose = trackDispose(
    registerDismissableLayer({
      isEnabled: createIsEnabled(enabled),
      isEventInside: config.isEventInside ?? createContainsEventTargetPredicate(element),
      onDismiss,
    }),
  );

  return {
    element,
    onDismiss,
    dispose,
  };
}

function expectLayerDismissedOnceWith(layer: TestLayer, expectedDismiss: ExpectedDismiss): void {
  expect(layer.onDismiss).toHaveBeenCalledTimes(1);
  expect(layer.onDismiss).toHaveBeenCalledWith(expect.objectContaining(expectedDismiss));
}

function expectLayerNotDismissed(layer: TestLayer): void {
  expect(layer.onDismiss).not.toHaveBeenCalled();
}

describe('dismissable-layer', () => {
  afterEach(() => {
    cleanupRegisteredLayers();
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  describe('outside pointer dismissal', () => {
    it('dismisses only the top-most enabled layer', () => {
      const bottomLayer = createLayer();
      const topLayer = createLayer();

      const originalEvent = dispatchPointerDown();

      expectLayerDismissedOnceWith(topLayer, {
        reason: 'outside-pointer',
        originalEvent,
      });

      expectLayerNotDismissed(bottomLayer);
    });

    it('does not dismiss when pointerdown happens inside the top layer boundary', () => {
      const layer = createLayer({
        element: appendElement('button'),
      });

      dispatchPointerDown(layer.element);

      expectLayerNotDismissed(layer);
    });

    it('uses the top layer boundary instead of lower layer boundaries', () => {
      const bottomLayer = createLayer();
      const topLayer = createLayer();

      const originalEvent = dispatchPointerDown(bottomLayer.element);

      expectLayerDismissedOnceWith(topLayer, {
        reason: 'outside-pointer',
        originalEvent,
      });

      expectLayerNotDismissed(bottomLayer);
    });

    it('dismisses the next layer after the top layer is disposed', () => {
      const bottomLayer = createLayer();
      const topLayer = createLayer();

      topLayer.dispose();

      const originalEvent = dispatchPointerDown();

      expectLayerNotDismissed(topLayer);

      expectLayerDismissedOnceWith(bottomLayer, {
        reason: 'outside-pointer',
        originalEvent,
      });
    });
  });

  describe('escape key dismissal', () => {
    it('dismisses only the top-most enabled layer and prevents default', () => {
      const bottomLayer = createLayer();
      const topLayer = createLayer();

      const originalEvent = dispatchKeyDown('Escape');

      expect(originalEvent.defaultPrevented).toBe(true);

      expectLayerDismissedOnceWith(topLayer, {
        reason: 'escape',
        originalEvent,
      });

      expectLayerNotDismissed(bottomLayer);
    });

    it('does not handle non-escape keydown events', () => {
      const layer = createLayer();

      const originalEvent = dispatchKeyDown('Enter');

      expect(originalEvent.defaultPrevented).toBe(false);
      expectLayerNotDismissed(layer);
    });

    it('does not prevent escape keydown when no enabled layer exists', () => {
      const layer = createLayer({
        enabled: false,
      });

      const originalEvent = dispatchKeyDown('Escape');

      expect(originalEvent.defaultPrevented).toBe(false);
      expectLayerNotDismissed(layer);
    });
  });

  describe('enabled state', () => {
    it('skips disabled layers and dismisses the next enabled layer', () => {
      const bottomLayer = createLayer({
        enabled: true,
      });

      const topLayer = createLayer({
        enabled: false,
      });

      const originalEvent = dispatchPointerDown();

      expectLayerNotDismissed(topLayer);

      expectLayerDismissedOnceWith(bottomLayer, {
        reason: 'outside-pointer',
        originalEvent,
      });
    });

    it('does nothing when all layers are disabled', () => {
      const bottomLayer = createLayer({
        enabled: false,
      });

      const topLayer = createLayer({
        enabled: false,
      });

      dispatchPointerDown();

      expectLayerNotDismissed(topLayer);
      expectLayerNotDismissed(bottomLayer);
    });

    it('reads enabled state lazily on every event', () => {
      let isEnabled = false;

      const layer = createLayer({
        enabled: () => isEnabled,
      });

      dispatchPointerDown();

      expectLayerNotDismissed(layer);

      isEnabled = true;

      const originalEvent = dispatchPointerDown();

      expectLayerDismissedOnceWith(layer, {
        reason: 'outside-pointer',
        originalEvent,
      });
    });
  });

  describe('disposal', () => {
    it('does not dismiss disposed layers', () => {
      const layer = createLayer();

      layer.dispose();

      dispatchPointerDown();
      dispatchKeyDown('Escape');

      expectLayerNotDismissed(layer);
    });

    it('allows dispose to be called multiple times', () => {
      const layer = createLayer();

      expect(() => {
        layer.dispose();
        layer.dispose();
      }).not.toThrow();

      dispatchPointerDown();

      expectLayerNotDismissed(layer);
    });

    it('allows a layer to dispose itself while handling dismiss', () => {
      let dispose: DismissableLayerDispose = () => undefined;

      const onDismiss: OnDismiss = vi.fn(() => {
        dispose();
      });

      dispose = trackDispose(
        registerDismissableLayer({
          isEnabled: () => true,
          isEventInside: () => false,
          onDismiss,
        }),
      );

      dispatchKeyDown('Escape');
      dispatchKeyDown('Escape');

      expect(onDismiss).toHaveBeenCalledTimes(1);
    });

    it('removes document listeners after the last layer is disposed', () => {
      const removeEventListenerSpy = vi.spyOn(document, 'removeEventListener');

      const layer = createLayer();

      layer.dispose();

      expect(removeEventListenerSpy).toHaveBeenCalledWith(
        'pointerdown',
        expect.any(Function),
        expect.objectContaining({
          capture: true,
          passive: true,
        }),
      );

      expect(removeEventListenerSpy).toHaveBeenCalledWith(
        'keydown',
        expect.any(Function),
        expect.objectContaining({
          capture: true,
        }),
      );
    });
  });
});
