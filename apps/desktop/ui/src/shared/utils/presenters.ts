const LABELS: Record<string, string> = {
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
  NativeWindows: 'Native Windows',
  NativeLinux: 'Native Linux',
  BattleNet: 'Battle.net',
  MacOs: 'macOS',
  low: 'Low',
  medium: 'Medium',
  high: 'High',
  blocked: 'Blocked',
  unknown: 'Unknown',
  planned: 'Planned',
  completed: 'Completed',
  failed: 'Failed',
  rolled_back: 'Rolled Back',
  rollback_required: 'Rollback Required',
  replace_component: 'Replace Component',
};

const UPPERCASE_WORDS = new Set([
  'ACL',
  'AMD',
  'API',
  'CPU',
  'DLSS',
  'DLL',
  'DXVK',
  'EA',
  'FG',
  'FSR',
  'GPU',
  'GOG',
  'IO',
  'JSON',
  'NVAPI',
  'PRD',
  'RR',
  'SHA256',
  'UI',
  'VKD3D',
  'XEFG',
  'XESS',
]);

export function formatLabel(value?: string | null): string {
  if (!value) {
    return 'Unknown';
  }

  return LABELS[value] ?? humanizeToken(value);
}

export function formatTechnology(value?: string | null): string {
  return formatLabel(value);
}

export function formatRisk(value?: string | null): string {
  return formatLabel(value);
}

export function riskTone(value?: string | null): 'low' | 'medium' | 'high' | 'blocked' | 'unknown' {
  switch (value) {
    case 'low':
      return 'low';
    case 'medium':
      return 'medium';
    case 'high':
      return 'high';
    case 'blocked':
      return 'blocked';
    default:
      return 'unknown';
  }
}

export function riskBadgeTone(value?: string | null): 'success' | 'warning' | 'danger' | 'muted' {
  switch (riskTone(value)) {
    case 'low':
      return 'success';
    case 'medium':
      return 'warning';
    case 'high':
    case 'blocked':
      return 'danger';
    default:
      return 'muted';
  }
}

export function statusTone(value?: string | null): 'neutral' | 'warning' | 'danger' | 'success' {
  switch (value) {
    case 'completed':
    case 'rolled_back':
      return 'success';
    case 'planned':
    case 'validating':
    case 'backup_created':
    case 'replacing':
      return 'neutral';
    case 'rollback_required':
      return 'warning';
    case 'failed':
    case 'blocked':
      return 'danger';
    default:
      return 'neutral';
  }
}

export function titleMonogram(title: string): string {
  const words = title
    .split(/\s+/)
    .map((word) => word.trim())
    .filter(Boolean);

  if (words.length === 0) {
    return 'RP';
  }

  if (words.length === 1) {
    return words[0].slice(0, 2).toUpperCase();
  }

  return `${words[0][0]}${words[1][0]}`.toUpperCase();
}

export function formatTimestamp(value?: number | null): string {
  if (!value) {
    return 'No timestamp yet';
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(value);
}

function humanizeToken(value: string): string {
  const spaced = value
    .replace(/_/g, ' ')
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
    .trim();

  return spaced
    .split(/\s+/)
    .map((word) => {
      const upper = word.toUpperCase();
      if (UPPERCASE_WORDS.has(upper)) {
        return upper;
      }

      return upper[0] + upper.slice(1).toLowerCase();
    })
    .join(' ');
}