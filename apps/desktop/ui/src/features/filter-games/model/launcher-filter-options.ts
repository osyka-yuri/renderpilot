import { getLauncherDisplayLabel } from '@entities/game';

export type LauncherFilterOption = {
  value: string;
  label: string;
};

export function buildLauncherFilterOptions(availableLaunchers: readonly string[]): LauncherFilterOption[] {
  return availableLaunchers.map((value) => ({ value, label: getLauncherDisplayLabel(value) }));
}
