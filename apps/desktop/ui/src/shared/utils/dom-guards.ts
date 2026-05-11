export function isHTMLElement(value: unknown): value is HTMLElement {
  return typeof HTMLElement !== 'undefined' && value instanceof HTMLElement;
}

export function isNode(value: unknown): value is Node {
  return typeof Node !== 'undefined' && value instanceof Node;
}
