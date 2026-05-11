import { humanizeToken } from '@shared/utils';
import { COMPONENT_UPPERCASE_WORDS } from './vocabulary';

const COMPONENT_LABELS: Record<string, string> = {
  dlss_super_resolution: 'DLSS Super Resolution',
  dlss_frame_generation: 'DLSS Frame Generation',
  dlss_ray_reconstruction: 'DLSS Ray Reconstruction',
  nvidia_streamline: 'NVIDIA Streamline',
  nvidia_reflex: 'NVIDIA Reflex',
  intel_xess: 'Intel XeSS',
  intel_xefg: 'Intel XeFG',
  amd_fsr: 'AMD FSR',
  amd_fsr_frame_generation: 'AMD FSR Frame Generation',
  optiscaler: 'OptiScaler',
  DlssSuperResolution: 'DLSS Super Resolution',
  DlssFrameGeneration: 'DLSS Frame Generation',
  DlssRayReconstruction: 'DLSS Ray Reconstruction',
  NvidiaStreamline: 'NVIDIA Streamline',
  NvidiaReflex: 'NVIDIA Reflex',
  IntelXeSs: 'Intel XeSS',
  IntelXeFg: 'Intel XeFG',
  AmdFsr: 'AMD FSR',
  AmdFsrFrameGeneration: 'AMD FSR Frame Generation',
};

export function formatComponentLabel(value?: string | null): string {
  if (!value) {
    return 'Unknown';
  }

  return COMPONENT_LABELS[value] ?? humanizeToken(value, COMPONENT_UPPERCASE_WORDS);
}

export function formatLabel(value?: string | null): string {
  return formatComponentLabel(value);
}

export function formatLibrary(value?: string | null): string {
  return formatComponentLabel(value);
}
