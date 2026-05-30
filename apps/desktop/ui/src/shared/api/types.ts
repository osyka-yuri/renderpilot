export type CommandErrorSeverity = 'warning' | 'error';

export type CommandErrorDto = {
  code: string;
  severity: CommandErrorSeverity;
  messageKey: string;
  details: string;
  suggestedActions: string[];
  /**
   * Backend-provided technical message, populated only in dev builds
   * (Rust strips this field from JSON in release). When present, it's a
   * much better source for a user-facing error toast than `details`,
   * which is a generic localized fallback ("The operation could not be
   * completed."). Use {@link describeCommandErrorTechnical} to prefer
   * this when available.
   */
  debugDetails?: string;
};
