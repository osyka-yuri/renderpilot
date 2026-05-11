export type Screen = 'games' | 'details' | 'operations' | 'settings';

export type ScreenHandler = (screen: Screen) => void;
