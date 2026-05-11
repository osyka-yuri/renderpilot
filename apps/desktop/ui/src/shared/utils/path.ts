export function fileNameFromPath(path: string): string {
  const normalizedPath = path.replace(/\\/g, '/');
  const fileName = normalizedPath.split('/').filter(Boolean).pop();

  return fileName ?? path;
}
