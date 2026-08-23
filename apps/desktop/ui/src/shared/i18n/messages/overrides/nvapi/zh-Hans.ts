import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': '渲染预设',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    '强制使用指定的 DLSS 超分辨率预设。某些游戏可能还需要设置“强制模型预设配置文件”才能应用自定义预设。',
  'Off (game default)': '关闭（游戏默认值）',
  'Preset A (CNN)': '预设 A（CNN）',
  'Preset B (CNN)': '预设 B（CNN）',
  'Preset C (CNN)': '预设 C（CNN）',
  'Preset D (CNN)': '预设 D（CNN）',
  'Preset E (CNN)': '预设 E（CNN）',
  'Preset F (CNN)': '预设 F（CNN）',
  'Preset G (unused)': '预设 G（未使用）',
  'Preset H (unused)': '预设 H（未使用）',
  'Preset I (unused)': '预设 I（未使用）',
  'Preset J (Transformer Gen 1)': '预设 J（Transformer Gen 1）',
  'Preset K (Transformer Gen 1)': '预设 K（Transformer Gen 1）',
  'Preset L (Transformer Gen 2)': '预设 L（Transformer Gen 2）',
  'Preset M (Transformer Gen 2)': '预设 M（Transformer Gen 2）',
  'Preset N (unused)': '预设 N（未使用）',
  'Preset O (unused)': '预设 O（未使用）',
  Recommended: '推荐',
  'Forced Quality Level': '强制质量级别',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    '覆盖游戏内选择的 DLSS 超分辨率质量级别。',
  Performance: '性能',
  Balanced: '平衡',
  Quality: '质量',
  'N/A': '不适用',
  'Ultra Performance': '超级性能',
  Custom: '自定义',
  'Forced Scaling Ratio': '强制缩放比例',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    '设置自定义渲染分辨率比例。需要将“强制质量级别”设为“自定义”。',
  Off: '关闭',
  '33% (Ultra Performance)': '33%（超级性能）',
  '50% (Performance)': '50%（性能）',
  '58% (Balanced)': '58%（平衡）',
  '67% (Quality)': '67%（质量）',
  '77% (Ultra Quality)': '77%（超级质量）',
  'Enable DLL Override': '开启 DLL 覆盖',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    '强制游戏使用系统中安装的最新版 DLSS-SR。大多数支持 DLSS 2 或更高版本的游戏均可使用。',
  'On (use latest installed)': '开启（使用已安装的最新版本）',
  'Forced Model Preset Profile': '强制模型预设配置文件',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    '允许在默认情况下“渲染预设”不生效的游戏中应用自定义预设。',
  'Force DLAA (full-resolution)': '强制 DLAA（全分辨率）',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    '以全分辨率渲染每种 DLSS 质量模式，使其作为抗锯齿方案（DLAA）运行。',
  On: '开启',
  'Remap Performance to Ultra Performance': '将“性能”重映射为“超级性能”',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    '强制“性能”质量模式使用“超级性能”渲染路径。',
  'Forces a specific DLSS Frame Generation preset.': '强制使用指定的 DLSS 帧生成预设。',
  'Preset A': '预设 A',
  'Preset B': '预设 B',
  'Preset C (unused)': '预设 C（未使用）',
  'Preset D (unused)': '预设 D（未使用）',
  'Preset E (unused)': '预设 E（未使用）',
  'Preset F (unused)': '预设 F（未使用）',
  'Preset J (unused)': '预设 J（未使用）',
  'Preset K (unused)': '预设 K（未使用）',
  'Preset L (unused)': '预设 L（未使用）',
  'Preset M (unused)': '预设 M（未使用）',
  'Forced Mode': '强制模式',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    '设置帧生成模式。动态模式需要 595.97 或更高版本的驱动程序。',
  Fixed: '固定',
  Dynamic: '动态',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    '强制游戏使用系统中安装的最新版 DLSS-FG。大多数 DLSS 3 游戏均可使用。',
  'Multi-Frame Generation — Fixed Count': '多帧生成 — 固定数量',
  'Sets a fixed number of generated frames per rendered frame.':
    '设置每个渲染帧对应的固定生成帧数。',
  'Multi-Frame Generation — Dynamic Count': '多帧生成 — 动态数量',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    '设置帧生成处于动态模式时的最大生成帧数。',
  'Up to 2x': '最高 2x',
  'Up to 3x': '最高 3x',
  'Up to 4x': '最高 4x',
  'Up to 5x': '最高 5x',
  'Up to 6x': '最高 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate': '多帧生成 — 目标动态帧率',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    '设置动态帧生成要保持的目标帧率。',
  'Max Refresh Rate': '最大刷新率',
  'Forces a specific DLSS Ray Reconstruction preset.': '强制使用指定的 DLSS 光线重建预设。',
  'Preset D (Transformer Gen 1)': '预设 D（Transformer Gen 1）',
  'Preset E (Transformer Gen 1)': '预设 E（Transformer Gen 1）',
  'Preset F (Transformer Gen 2)': '预设 F（Transformer Gen 2）',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    '覆盖游戏内选择的 DLSS 光线重建质量级别。',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    '强制游戏使用系统中安装的最新版 DLSS-RR。大多数支持光线重建的游戏均可使用。',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'zh-Hans', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
