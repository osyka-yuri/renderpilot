type LauncherId = string;

/**
 * Builds a safe launcher order from persisted user state and current availability.
 *
 * Guarantees:
 * - result contains only currently available launchers
 * - result contains no duplicates
 * - persisted order is preserved where still valid
 * - newly available launchers are appended in `availableLaunchers` order
 */
export function buildInitialLauncherOrder(
  persistedOrder: readonly LauncherId[] | null | undefined,
  availableLaunchers: readonly LauncherId[],
): LauncherId[] {
  return canonicalizeLauncherOrder(persistedOrder ?? [], availableLaunchers);
}

export function canonicalizeLauncherOrder(
  order: readonly LauncherId[],
  availableLaunchers: readonly LauncherId[],
): LauncherId[] {
  const available = [...new Set(availableLaunchers)];
  const availableSet = new Set(available);

  const result: LauncherId[] = [];
  const used = new Set<LauncherId>();

  for (const launcher of order) {
    if (availableSet.has(launcher) && !used.has(launcher)) {
      result.push(launcher);
      used.add(launcher);
    }
  }

  for (const launcher of available) {
    if (!used.has(launcher)) {
      result.push(launcher);
      used.add(launcher);
    }
  }

  return result;
}
