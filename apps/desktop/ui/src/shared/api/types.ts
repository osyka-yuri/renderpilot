export type CommandErrorSeverity = 'warning' | 'error';

export type CommandErrorDto = {
  code: string;
  severity: CommandErrorSeverity;
  messageKey: string;
  details: string;
  suggestedActions: string[];
};
