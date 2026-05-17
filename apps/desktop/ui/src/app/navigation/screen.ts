export type Screen = 'games' | 'details' | 'operations' | 'settings' | 'libraries';

export type ScreenHandler = (screen: Screen) => void;
