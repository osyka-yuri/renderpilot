export const LAUNCHER_DISPLAY_LABELS: Record<string, string> = {
  Steam: 'Steam',
  Epic: 'Epic Games Store',
  Gog: 'GOG Galaxy',
  Ubisoft: 'Ubisoft Connect',
  Ea: 'EA App',
  BattleNet: 'Battle.net',
  Xbox: 'Xbox App',
  Manual: 'Manual',
};

export function getLauncherDisplayLabel(value: string): string {
  return LAUNCHER_DISPLAY_LABELS[value] ?? value;
}
