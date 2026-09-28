const ENGINE_LABELS: Record<string, string> = {
  EasyAntiCheat: 'Easy Anti-Cheat',
  easy_anti_cheat: 'Easy Anti-Cheat',
  'easyanti cheat': 'Easy Anti-Cheat',
  'Easy Anti-Cheat': 'Easy Anti-Cheat',
  BattlEye: 'BattlEye',
  battleye: 'BattlEye',
  battl_eye: 'BattlEye',
};

function presentDetectedEngine(engine: string): string {
  const trimmed = engine.trim();
  return ENGINE_LABELS[trimmed] ?? trimmed;
}

/** Normalizes and deduplicates anti-cheat names for a mutation confirmation. */
export function presentDetectedEngines(engines: readonly string[]): string[] {
  return [...new Set(engines.map(presentDetectedEngine).filter((engine) => engine.length > 0))];
}
