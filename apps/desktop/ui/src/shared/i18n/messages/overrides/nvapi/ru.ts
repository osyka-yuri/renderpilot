import { defineLocalizedCatalog } from '../../contract';
import {
  expandNvapiTranslations,
  type NvapiSourceCatalog,
  type NvapiTranslations,
} from './contract.generated';

const translations = {
  'Render Preset': 'Пресет рендеринга',
  "Forces a specific DLSS Super Resolution preset. Some games may require 'Forced Model Preset Profile' to apply custom presets.":
    'Принудительно задаёт пресет DLSS Super Resolution. В некоторых играх для применения пользовательских пресетов также может потребоваться «Принудительный профиль пресета модели».',
  'Off (game default)': 'Выкл. (как в игре)',
  'Preset A (CNN)': 'Пресет A (CNN)',
  'Preset B (CNN)': 'Пресет B (CNN)',
  'Preset C (CNN)': 'Пресет C (CNN)',
  'Preset D (CNN)': 'Пресет D (CNN)',
  'Preset E (CNN)': 'Пресет E (CNN)',
  'Preset F (CNN)': 'Пресет F (CNN)',
  'Preset G (unused)': 'Пресет G (не используется)',
  'Preset H (unused)': 'Пресет H (не используется)',
  'Preset I (unused)': 'Пресет I (не используется)',
  'Preset J (Transformer Gen 1)': 'Пресет J (Transformer Gen 1)',
  'Preset K (Transformer Gen 1)': 'Пресет K (Transformer Gen 1)',
  'Preset L (Transformer Gen 2)': 'Пресет L (Transformer Gen 2)',
  'Preset M (Transformer Gen 2)': 'Пресет M (Transformer Gen 2)',
  'Preset N (unused)': 'Пресет N (не используется)',
  'Preset O (unused)': 'Пресет O (не используется)',
  Recommended: 'Рекомендуется',
  'Forced Quality Level': 'Принудительный уровень качества',
  'Overrides the in-game DLSS Super Resolution quality selection.':
    'Переопределяет выбранный в игре уровень качества DLSS Super Resolution.',
  Performance: 'Производительность',
  Balanced: 'Баланс',
  Quality: 'Качество',
  'N/A': 'Н/Д',
  'Ultra Performance': 'Ультрапроизводительность',
  Custom: 'Пользовательский',
  'Forced Scaling Ratio': 'Принудительный коэффициент масштабирования',
  "Sets a custom render-resolution ratio. Requires 'Forced Quality Level' to be set to Custom.":
    'Задаёт пользовательский коэффициент разрешения рендеринга. Для параметра «Принудительный уровень качества» требуется значение «Пользовательский».',
  Off: 'Выкл.',
  '33% (Ultra Performance)': '33% (ультрапроизводительность)',
  '50% (Performance)': '50% (производительность)',
  '58% (Balanced)': '58% (баланс)',
  '67% (Quality)': '67% (качество)',
  '77% (Ultra Quality)': '77% (ультракачество)',
  'Enable DLL Override': 'Включить переопределение DLL',
  'Forces the game to use the latest DLSS-SR version installed system-wide. Supported by most DLSS 2+ titles.':
    'Принудительно использует в игре новейшую установленную в системе версию DLSS-SR. Поддерживается большинством игр с DLSS 2 и новее.',
  'On (use latest installed)': 'Вкл. (использовать новейшую установленную)',
  'Forced Model Preset Profile': 'Принудительный профиль пресета модели',
  "Allows applying a custom preset in games where 'Render Preset' has no effect by default.":
    'Позволяет применять пользовательский пресет в играх, где «Пресет рендеринга» по умолчанию не действует.',
  'Force DLAA (full-resolution)': 'Принудительный DLAA (полное разрешение)',
  'Renders every DLSS quality mode at full resolution, acting as an anti-aliasing solution (DLAA).':
    'Рендерит каждый режим качества DLSS в полном разрешении, используя его как средство сглаживания (DLAA).',
  On: 'Вкл.',
  'Remap Performance to Ultra Performance':
    'Переназначить «Производительность» на «Ультрапроизводительность»',
  'Forces the Performance quality mode to use the Ultra Performance rendering path.':
    'Принудительно использует путь рендеринга «Ультрапроизводительность» для режима качества «Производительность».',
  'Forces a specific DLSS Frame Generation preset.':
    'Принудительно задаёт определённый пресет DLSS Frame Generation.',
  'Preset A': 'Пресет A',
  'Preset B': 'Пресет B',
  'Preset C (unused)': 'Пресет C (не используется)',
  'Preset D (unused)': 'Пресет D (не используется)',
  'Preset E (unused)': 'Пресет E (не используется)',
  'Preset F (unused)': 'Пресет F (не используется)',
  'Preset J (unused)': 'Пресет J (не используется)',
  'Preset K (unused)': 'Пресет K (не используется)',
  'Preset L (unused)': 'Пресет L (не используется)',
  'Preset M (unused)': 'Пресет M (не используется)',
  'Forced Mode': 'Принудительный режим',
  'Sets the Frame Generation mode. Dynamic mode requires driver 595.97 or newer.':
    'Задаёт режим Frame Generation. Для динамического режима требуется драйвер версии 595.97 или новее.',
  Fixed: 'Фиксированный',
  Dynamic: 'Динамический',
  'Forces the game to use the latest DLSS-FG version installed system-wide. Supported by most DLSS 3 titles.':
    'Принудительно использует в игре новейшую установленную в системе версию DLSS-FG. Поддерживается большинством игр с DLSS 3.',
  'Multi-Frame Generation — Fixed Count': 'Multi Frame Generation — фиксированное количество',
  'Sets a fixed number of generated frames per rendered frame.':
    'Задаёт фиксированное количество сгенерированных кадров на каждый отрендеренный кадр.',
  'Multi-Frame Generation — Dynamic Count': 'Multi Frame Generation — динамическое количество',
  'Sets an upper limit on generated frames when Frame Generation is in Dynamic mode.':
    'Задаёт верхний предел количества генерируемых кадров, когда Frame Generation работает в динамическом режиме.',
  'Up to 2x': 'До 2x',
  'Up to 3x': 'До 3x',
  'Up to 4x': 'До 4x',
  'Up to 5x': 'До 5x',
  'Up to 6x': 'До 6x',
  'Multi-Frame Generation — Target Dynamic Frame Rate':
    'Multi Frame Generation — целевая динамическая частота кадров',
  'Sets the target frame rate that Dynamic Frame Generation aims to maintain.':
    'Задаёт целевую частоту кадров, которую стремится поддерживать динамическая Frame Generation.',
  'Max Refresh Rate': 'Максимальная частота обновления',
  'Forces a specific DLSS Ray Reconstruction preset.':
    'Принудительно задаёт определённый пресет DLSS Ray Reconstruction.',
  'Preset D (Transformer Gen 1)': 'Пресет D (Transformer Gen 1)',
  'Preset E (Transformer Gen 1)': 'Пресет E (Transformer Gen 1)',
  'Preset F (Transformer Gen 2)': 'Пресет F (Transformer Gen 2)',
  'Overrides the in-game DLSS Ray Reconstruction quality selection.':
    'Переопределяет выбранный в игре уровень качества DLSS Ray Reconstruction.',
  'Forces the game to use the latest DLSS-RR version installed system-wide. Supported by most Ray Reconstruction titles.':
    'Принудительно использует в игре новейшую установленную в системе версию DLSS-RR. Поддерживается большинством игр с Ray Reconstruction.',
} as const satisfies NvapiTranslations;

export const nvapiOverrides = defineLocalizedCatalog<'ru', NvapiSourceCatalog>()(
  expandNvapiTranslations(translations),
);
