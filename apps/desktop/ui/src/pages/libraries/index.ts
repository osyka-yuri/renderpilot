export async function loadLibrariesPage() {
  return (await import('./ui/LibrariesPage.svelte')).default;
}
