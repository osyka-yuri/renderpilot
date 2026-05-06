export type VoidHandler = () => void;
export type GameSelectionHandler = (gameId: string) => void;
export type OperationHandler = (operationId: string) => void;
export type BuildPlanHandler = (componentId: string, artifactId: string) => void;
export type ScreenHandler = (screen: import('../../app/routes/screen').Screen) => void;
export type ThemeModeHandler = (
	mode: import('../theme/theme-mode').ThemeMode,
) => void;
export type LanguageModeHandler = (mode: 'system' | 'en' | 'ru') => void;