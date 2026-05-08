import type { ActionReturn } from 'svelte/action';

const DEFAULT_PORTAL_TARGET = 'body';

export type PortalTarget = Element | string | null | undefined;

/**
 * Svelte action to portal an element to another DOM location.
 *
 * Usage:
 *   <div use:portal />
 *   <div use:portal="#modal-root" />
 *   <div use:portal={someElement} />
 */
export function portal(
  node: HTMLElement,
  target: PortalTarget = DEFAULT_PORTAL_TARGET,
): ActionReturn<PortalTarget> {
  let currentTarget: Element | null = null;
  let destroyed = false;

  function resolveTarget(nextTarget: PortalTarget): Element {
    const resolvedTarget = nextTarget ?? DEFAULT_PORTAL_TARGET;
    const doc = node.ownerDocument;

    if (typeof resolvedTarget !== 'string') {
      return resolvedTarget;
    }

    const selector = resolvedTarget.trim();

    if (!selector) {
      throw new Error('Portal target selector must not be empty');
    }

    if (selector.toLowerCase() === 'body') {
      return doc.body;
    }

    const targetNode = doc.querySelector(selector);

    if (!targetNode) {
      throw new Error(`Portal target '${selector}' was not found`);
    }

    return targetNode;
  }

  function move(nextTarget: PortalTarget): void {
    if (destroyed) {
      return;
    }

    const targetNode = resolveTarget(nextTarget);

    if (targetNode === node || node.contains(targetNode)) {
      throw new Error('Portal target must not be the node itself or its descendant');
    }

    if (currentTarget === targetNode && node.parentNode === targetNode) {
      return;
    }

    targetNode.appendChild(node);
    currentTarget = targetNode;
  }

  move(target);

  return {
    update(nextTarget: PortalTarget): void {
      move(nextTarget);
    },

    destroy(): void {
      destroyed = true;

      /**
       * Remove only if the node is still inside the portal target we control.
       * This avoids accidentally removing the node if external code moved it.
       */
      if (currentTarget && node.parentNode === currentTarget) {
        currentTarget.removeChild(node);
      }

      currentTarget = null;
    },
  };
}
