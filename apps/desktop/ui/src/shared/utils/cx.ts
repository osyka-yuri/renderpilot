export type ClassDictionary = Record<string, unknown>;

export type ClassArray = ClassValue[];

export type ClassValue =
  | string
  | number
  | boolean
  | null
  | undefined
  | ClassArray
  | ClassDictionary;

/**
 * Normalizes class values into a single space-separated string.
 *
 * Supports:
 * - strings and numbers as class names;
 * - arrays with nested class values;
 * - objects where keys are class names and values control inclusion;
 * - falsey values are ignored.
 *
 * Similar to `clsx` / `classnames`.
 */
export function cx(...values: ClassValue[]): string {
  const classes: string[] = [];

  for (const value of values) {
    appendClassValue(classes, value);
  }

  return classes.join(' ');
}

function appendClassValue(classes: string[], value: ClassValue): void {
  if (!value) {
    return;
  }

  if (typeof value === 'string' || typeof value === 'number') {
    const text = String(value).trim();

    if (text.length > 0) {
      classes.push(text);
    }

    return;
  }

  if (isClassArray(value)) {
    appendClassArray(classes, value);
    return;
  }

  if (isClassDictionary(value)) {
    appendClassDictionary(classes, value);
  }
}

function appendClassArray(classes: string[], values: ClassArray): void {
  for (const value of values) {
    appendClassValue(classes, value);
  }
}

function appendClassDictionary(classes: string[], dictionary: ClassDictionary): void {
  for (const [className, enabled] of Object.entries(dictionary)) {
    if (enabled) {
      classes.push(className);
    }
  }
}

function isClassArray(value: ClassValue): value is ClassArray {
  return Array.isArray(value);
}

function isClassDictionary(value: ClassValue): value is ClassDictionary {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
