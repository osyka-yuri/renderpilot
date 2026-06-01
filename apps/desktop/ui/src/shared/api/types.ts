export type CommandErrorSeverity = 'warning' | 'error';

/**
 * A backend-suggested remediation. `key` is a stable localization key
 * (`suggested_action.*`) used to translate via the UI catalog; `text` is the
 * English fallback emitted by Rust when the key is absent from the catalog.
 */
export type SuggestedActionDto = {
  key: string;
  text: string;
};

export type CommandErrorDto = {
  code: string;
  severity: CommandErrorSeverity;
  messageKey: string;
  details: string;
  suggestedActions: SuggestedActionDto[];
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
