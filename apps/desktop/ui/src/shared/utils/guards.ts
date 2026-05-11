export function isString(value: unknown): value is string {
  return typeof value === 'string';
}

export function isNonEmptyString(value: unknown): value is string {
  return isString(value) && value.trim().length > 0;
}

export function isNumber(value: unknown): value is number {
  return typeof value === 'number';
}

export function isFiniteNumber(value: unknown): value is number {
  return isNumber(value) && Number.isFinite(value);
}

export function isBoolean(value: unknown): value is boolean {
  return typeof value === 'boolean';
}

export function isFunction(value: unknown): value is (...args: unknown[]) => unknown {
  return typeof value === 'function';
}

export function isArray(value: unknown): value is unknown[] {
  return Array.isArray(value);
}

export function isObject(value: unknown): value is object {
  return typeof value === 'object' && value !== null;
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return isObject(value);
}

export function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (!isObject(value) || Array.isArray(value)) {
    return false;
  }

  const prototype = Reflect.getPrototypeOf(value);

  return prototype === Object.prototype || prototype === null;
}

export function isDefined<T>(value: T | null | undefined): value is T {
  return value !== null && value !== undefined;
}

export type ErrorLike = {
  message?: unknown;
  name?: unknown;
};

export function isErrorLike(value: unknown): value is ErrorLike {
  return (
    isObject(value) &&
    (('message' in value && isString(value.message)) || ('name' in value && isString(value.name)))
  );
}
