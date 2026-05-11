export type NvApiControl = {
  id: string;
  label: string;
  description: string;
  defaultValue: string;
  options: {
    value: string;
    label: string;
  }[];
};

export const nvapiControlsByLibrary: Record<string, NvApiControl[]> = {
  DlssSuperResolution: [
    {
      id: 'preset',
      label: 'DLSS preset override',
      description:
        'Capability-based preset selection when the driver profile exposes a DLSS override.',
      defaultValue: 'safe_original',
      options: [
        { value: 'safe_original', label: 'Safe Original' },
        { value: 'quality', label: 'Quality' },
        { value: 'balanced', label: 'Balanced' },
        { value: 'performance', label: 'Performance' },
      ],
    },
    {
      id: 'indicator',
      label: 'DLSS indicator',
      description: 'Driver-side indicator/debug setting when exposed by the NVIDIA profile.',
      defaultValue: 'off',
      options: [
        { value: 'off', label: 'Off' },
        { value: 'on', label: 'On' },
      ],
    },
  ],

  DlssFrameGeneration: [
    {
      id: 'fg_profile',
      label: 'DLSS FG profile',
      description:
        'Frame Generation profile when the driver can override the game-controlled default.',
      defaultValue: 'game_controlled',
      options: [
        { value: 'game_controlled', label: 'Game Controlled' },
        { value: 'low_latency', label: 'Low Latency' },
        { value: 'stable_120hz', label: 'Stable 120 Hz' },
        { value: 'stable_144hz', label: 'Stable 144 Hz' },
      ],
    },
  ],

  DlssRayReconstruction: [
    {
      id: 'rr_profile',
      label: 'DLSS RR profile',
      description: 'Ray Reconstruction profile when the driver exposes a controllable path.',
      defaultValue: 'game_controlled',
      options: [
        { value: 'game_controlled', label: 'Game Controlled' },
        { value: 'quality', label: 'Quality' },
        { value: 'balanced', label: 'Balanced' },
        { value: 'safe_original', label: 'Safe Original' },
      ],
    },
  ],
};
