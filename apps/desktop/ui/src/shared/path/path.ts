export function fileNameFromPath(path: string): string {
  const normalizedPath = path.replace(/\\/g, '/');
  const fileName = normalizedPath.split('/').filter(Boolean).pop();

  return fileName ?? path;
}

export function parentPath(path: string): string | null {
  const separatorIndex = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));

  if (separatorIndex < 0) {
    return null;
  }

  if (separatorIndex === 0) {
    return path.slice(0, 1);
  }

  if (separatorIndex === 2 && path[1] === ':') {
    return path.slice(0, 3);
  }

  return path.slice(0, separatorIndex);
}

export function sharedParentPath(paths: readonly string[]): string | null {
  const parents = paths.map(parentPath);
  const first = parents[0];

  if (first == null) {
    return null;
  }

  const comparisonKey = pathComparisonKey(first);
  return parents.every((path) => path !== null && pathComparisonKey(path) === comparisonKey)
    ? first
    : null;
}

function pathComparisonKey(path: string): string {
  const normalized = path.replace(/\\/g, '/');
  const isWindowsPath = /^[a-z]:\//i.test(normalized) || normalized.startsWith('//');

  return isWindowsPath ? normalized.toLowerCase() : normalized;
}
