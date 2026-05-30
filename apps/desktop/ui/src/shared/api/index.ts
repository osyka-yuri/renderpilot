export type { CommandErrorDto, CommandErrorSeverity } from './types';
export {
  DesktopCommandError,
  describeCommandError,
  describeCommandErrorBrief,
  describeCommandErrorTechnical,
  normalizeCommandError,
} from './errors';
export { invokeDesktop } from './desktop-transport';
export {
  openFilePicker,
  openFolderPicker,
  type DialogFilter,
  type FilePickerOptions,
  type FolderPickerOptions,
} from './desktop-dialog';
