export async function loadOperationsPage() {
  return (await import('./ui/OperationsPage.svelte')).default;
}
