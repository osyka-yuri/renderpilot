export type { CommandErrorCode, SuggestedActionCode } from './generated/desktop-command-errors';
export {
  COMMAND_ERROR_CONTRACT,
  SUGGESTED_ACTION_CONTRACT,
} from './generated/desktop-command-errors';
export {
  ClientError,
  DesktopCommandError,
  getErrorCode,
  getErrorContractStatus,
  getErrorSeverity,
  isFileSafetyContextError,
  isCommandErrorCode,
  normalizeDesktopCommandError,
  type CodedError,
  type CommandErrorDto,
} from './model';
export {
  isLocalErrorCode,
  LOCAL_ERROR_CONTRACT,
  type LocalErrorCode,
  type LocalErrorSpec,
} from './local-contract';
export { reportClientError, reportDesktopCommandError } from './report';
