import type { Locale } from '../locale';

/**
 * Per-locale overrides for the NVAPI setting catalog, looked up dynamically via
 * `translateKey('nvapi.<setting_key>.{label|description|value.<wire>}', backendText)`
 * in NvapiSettingRow.svelte.
 *
 * English is intentionally OMITTED: the single source of English NVAPI text is
 * the backend data (`dlss_settings.json`), surfaced through the translateKey
 * fallback (`state.setting_label` / `state.description` / `option.label`). Other
 * locales provide overrides; any key absent here falls back to the backend
 * English. This keeps one source of English truth and avoids drift.
 */
export const nvapiOverrides: Partial<Record<Locale, Record<string, string>>> = {
  ru: {
    'nvapi.dlss_sr_render_preset.label': 'Пресет рендеринга',
    'nvapi.dlss_sr_render_preset.description':
      'Принудительно задает пресет DLSS. В некоторых играх также требуется изменить "Профиль пресета" (Model Preset Profile).',
    'nvapi.dlss_sr_render_preset.value.default': 'Выкл (как в игре)',
    'nvapi.dlss_sr_render_preset.value.recommended': 'Рекомендовано',

    'nvapi.dlss_sr_quality_level.label': 'Уровень качества',
    'nvapi.dlss_sr_quality_level.description':
      'Переопределяет выбранный в игре уровень качества DLSS.',
    'nvapi.dlss_sr_quality_level.value.custom': 'Свой',
    'nvapi.dlss_sr_quality_level.value.na': 'Н/Д',

    'nvapi.dlss_sr_scaling_ratio.label': 'Масштаб рендеринга',
    'nvapi.dlss_sr_scaling_ratio.description':
      'Задает свой масштаб (разрешение) для рендеринга. Требует установить "Уровень качества" в режим "Свой".',

    'nvapi.dlss_sr_dll_override.label': 'Переопределение версии DLL',
    'nvapi.dlss_sr_dll_override.description':
      'Заставляет игру использовать последнюю системную версию DLSS.',
    'nvapi.dlss_sr_dll_override.value.off': 'Выкл',
    'nvapi.dlss_sr_dll_override.value.on': 'Вкл (последняя системная)',

    'nvapi.dlss_sr_model_preset_profile.label': 'Профиль пресета',
    'nvapi.dlss_sr_model_preset_profile.description':
      'Позволяет применить кастомный пресет в играх, где настройка "Пресет рендеринга" не работает сама по себе.',
    'nvapi.dlss_sr_model_preset_profile.value.na': 'Н/Д',
    'nvapi.dlss_sr_model_preset_profile.value.recommended': 'Рекомендовано',
    'nvapi.dlss_sr_model_preset_profile.value.custom': 'Свой',

    'nvapi.dlss_sr_override_dlaa.label': 'Принудительный DLAA',
    'nvapi.dlss_sr_override_dlaa.description':
      'Заставляет все режимы качества DLSS рендериться в полном разрешении (как DLAA).',
    'nvapi.dlss_sr_override_dlaa.value.off': 'Выкл',
    'nvapi.dlss_sr_override_dlaa.value.on': 'Вкл',

    'nvapi.dlss_sr_override_perf_to_ultraperf.label': 'Performance в Ultra Performance',
    'nvapi.dlss_sr_override_perf_to_ultraperf.description':
      'Заставляет режим качества "Performance" использовать алгоритм рендеринга "Ultra Performance".',
    'nvapi.dlss_sr_override_perf_to_ultraperf.value.off': 'Выкл',
    'nvapi.dlss_sr_override_perf_to_ultraperf.value.on': 'Вкл',

    'nvapi.dlss_fg_render_preset.label': 'Пресет рендеринга',
    'nvapi.dlss_fg_render_preset.description':
      'Принудительно задает пресет генерации кадров (Frame Generation).',
    'nvapi.dlss_fg_render_preset.value.default': 'Выкл (как в игре)',
    'nvapi.dlss_fg_render_preset.value.recommended': 'Рекомендовано',

    'nvapi.dlss_fg_mode.label': 'Режим работы',
    'nvapi.dlss_fg_mode.description':
      'Устанавливает режим генерации кадров. Для динамического режима требуется драйвер 595.97 и новее.',
    'nvapi.dlss_fg_mode.value.na': 'Н/Д',
    'nvapi.dlss_fg_mode.value.fixed': 'Фиксированный',
    'nvapi.dlss_fg_mode.value.dynamic': 'Динамический',

    'nvapi.dlss_fg_dll_override.label': 'Переопределение версии DLL',
    'nvapi.dlss_fg_dll_override.description':
      'Заставляет игру использовать последнюю системную версию Frame Generation.',
    'nvapi.dlss_fg_dll_override.value.off': 'Выкл',
    'nvapi.dlss_fg_dll_override.value.on': 'Вкл (последняя системная)',

    'nvapi.dlss_mfg_fixed_count.label': 'Генерация кадров — Фиксированное кол-во',
    'nvapi.dlss_mfg_fixed_count.description':
      'Задает точное количество генерируемых кадров на каждый отрисованный кадр.',
    'nvapi.dlss_mfg_fixed_count.value.na': 'Н/Д',

    'nvapi.dlss_mfg_dynamic_count.label': 'Генерация кадров — Динамическое кол-во',
    'nvapi.dlss_mfg_dynamic_count.description':
      'Устанавливает верхний предел генерации кадров для динамического режима.',
    'nvapi.dlss_mfg_dynamic_count.value.na': 'Н/Д',

    'nvapi.dlss_mfg_target_frame_rate.label': 'Генерация кадров — Целевая частота (FPS)',
    'nvapi.dlss_mfg_target_frame_rate.description':
      'Задает частоту кадров, которую будет пытаться поддерживать динамический режим.',
    'nvapi.dlss_mfg_target_frame_rate.value.na': 'Н/Д',
    'nvapi.dlss_mfg_target_frame_rate.value.max_refresh': 'Макс. частота обновления монитора',

    'nvapi.dlss_rr_render_preset.label': 'Пресет рендеринга',
    'nvapi.dlss_rr_render_preset.description':
      'Принудительно задает пресет реконструкции лучей (Ray Reconstruction).',
    'nvapi.dlss_rr_render_preset.value.default': 'Выкл (как в игре)',
    'nvapi.dlss_rr_render_preset.value.recommended': 'Рекомендовано',

    'nvapi.dlss_rr_quality_level.label': 'Уровень качества',
    'nvapi.dlss_rr_quality_level.description':
      'Переопределяет выбранный в игре уровень качества Ray Reconstruction.',
    'nvapi.dlss_rr_quality_level.value.custom': 'Свой',
    'nvapi.dlss_rr_quality_level.value.na': 'Н/Д',

    'nvapi.dlss_rr_scaling_ratio.label': 'Масштаб рендеринга',
    'nvapi.dlss_rr_scaling_ratio.description':
      'Задает свой масштаб (разрешение) для рендеринга. Требует установить "Уровень качества" в режим "Свой".',

    'nvapi.dlss_rr_dll_override.label': 'Переопределение версии DLL',
    'nvapi.dlss_rr_dll_override.description':
      'Заставляет игру использовать последнюю системную версию Ray Reconstruction.',
    'nvapi.dlss_rr_dll_override.value.off': 'Выкл',
    'nvapi.dlss_rr_dll_override.value.on': 'Вкл (последняя системная)',
  },
};
