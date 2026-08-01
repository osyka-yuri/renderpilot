import { analyzeMessageTemplate as analyzeSharedMessageTemplate } from '../../ui/src/shared/i18n/messages/template.ts';

function fail(message) {
  throw new Error(`i18n contract generation failed: ${message}`);
}

export function analyzeMessageTemplate(template) {
  const analysis = analyzeSharedMessageTemplate(template);
  return analysis.valid
    ? { valid: true, placeholders: [...analysis.placeholders] }
    : { valid: false, placeholders: [] };
}

function messagePlaceholders(template, context) {
  const analysis = analyzeMessageTemplate(template);
  if (!analysis.valid) {
    fail(`${context} contains invalid placeholder syntax`);
  }
  return [...analysis.placeholders].sort();
}

function assertPlaceholderName(value, context) {
  const analysis = analyzeMessageTemplate(`{${value}}`);
  if (!analysis.valid || analysis.placeholders.length !== 1 || analysis.placeholders[0] !== value) {
    fail(`${context} has invalid placeholder name ${JSON.stringify(value)}`);
  }
}

function duplicateValues(values) {
  return values.filter((value, index) => values.indexOf(value) !== index);
}

export function sortRecord(record) {
  return Object.fromEntries(
    Object.entries(record).sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0)),
  );
}

export function validateEnglishContract(entries) {
  const messages = {};
  for (const [key, value] of entries) {
    if (Object.hasOwn(messages, key)) {
      fail(`English catalog contains duplicate key ${key}`);
    }
    if (value.kind === 'string') {
      messages[key] = {
        kind: 'string',
        placeholders: messagePlaceholders(value.template, `English message ${key}`),
      };
      continue;
    }
    if (value.helper !== 'plural' && value.helper !== 'select') {
      fail(`English message ${key} uses unsupported helper ${value.helper}()`);
    }

    assertPlaceholderName(value.argument, `English message ${key} argument`);
    const branchNames = value.branches.map(([name]) => name);
    if (duplicateValues(branchNames).length > 0) {
      fail(`English message ${key} contains duplicate branches`);
    }
    const sortedBranchNames = [...branchNames].sort();
    if (value.helper === 'plural' && sortedBranchNames.join(',') !== 'one,other') {
      fail(`English message ${key} plural branches must be exactly one and other`);
    }
    if (value.helper === 'select' && (!branchNames.includes('other') || branchNames.length < 2)) {
      fail(`English message ${key} select branches must include other and at least one named case`);
    }

    messages[key] = {
      kind: value.helper,
      argument: value.argument,
      branches: sortRecord(
        Object.fromEntries(
          value.branches.map(([name, template]) => [
            name,
            messagePlaceholders(template, `English message ${key}.${name}`),
          ]),
        ),
      ),
    };
  }
  return sortRecord(messages);
}

export function validatePluralCategories(entries) {
  const categories = {};
  for (const [locale, values] of entries) {
    if (Object.hasOwn(categories, locale)) {
      fail(`PLURAL_CATEGORIES contains duplicate locale ${locale}`);
    }
    if (values.length === 0) {
      fail(`PLURAL_CATEGORIES.${locale} must be a non-empty literal array`);
    }
    const uniqueCategories = new Set(values);
    if (
      uniqueCategories.size !== values.length ||
      !uniqueCategories.has('other') ||
      [...uniqueCategories].some(
        (category) => !['zero', 'one', 'two', 'few', 'many', 'other'].includes(category),
      )
    ) {
      fail(`PLURAL_CATEGORIES.${locale} contains invalid or duplicate categories`);
    }
    categories[locale] = values;
  }
  return sortRecord(categories);
}

function assertIdentifier(value, context) {
  if (!/^[a-z][A-Za-z0-9]*$/.test(value)) {
    fail(`${context} has invalid identifier ${JSON.stringify(value)}`);
  }
}

function assertMessageId(value, prefix, context) {
  if (
    typeof value !== 'string' ||
    !new RegExp(`^${prefix}\\.[a-z0-9-]+\\.[a-z0-9_]+$`).test(value)
  ) {
    fail(`${context} has invalid message ID ${JSON.stringify(value)}`);
  }
}

export function validateLumaContract(entries) {
  const groups = {};
  const ids = new Set();
  for (const [phrase, phraseIds] of entries) {
    assertIdentifier(phrase, 'Luma phrase');
    if (!Array.isArray(phraseIds) || phraseIds.length === 0) {
      fail(`Luma phrase ${phrase} must contain at least one message ID`);
    }
    groups[phrase] = phraseIds.map((id) => {
      assertMessageId(id, 'luma', `Luma phrase ${phrase}`);
      if (ids.has(id)) {
        fail(`duplicate Luma message ID ${id}`);
      }
      ids.add(id);
      return id;
    });
  }
  if (Object.keys(groups).length === 0) {
    fail('Luma contract must contain at least one phrase');
  }
  return { groups, ids: [...ids].sort() };
}

function nonEmptyString(value, context) {
  if (typeof value !== 'string' || value.trim() === '') {
    fail(`${context} must be a non-empty string`);
  }
  return value;
}

export function validateNvapiContract(settings) {
  const sourceCatalog = {};
  const messages = {};
  const settingKeys = new Set();

  const addMessage = (key, value) => {
    sourceCatalog[key] = nonEmptyString(value, key);
    messages[key] = messagePlaceholders(sourceCatalog[key], key);
  };

  for (const setting of settings) {
    if (!setting || typeof setting !== 'object') {
      fail('NVAPI setting must be an object');
    }
    const settingKey = nonEmptyString(setting.key, 'NVAPI setting key');
    if (!/^[a-z0-9_]+$/.test(settingKey) || settingKeys.has(settingKey)) {
      fail(`invalid or duplicate NVAPI setting key ${settingKey}`);
    }
    settingKeys.add(settingKey);
    const prefix = `nvapi.${settingKey}`;
    addMessage(`${prefix}.label`, setting.label);
    if (setting.description !== undefined) {
      addMessage(`${prefix}.description`, setting.description);
    }
    if (setting.values !== undefined && !Array.isArray(setting.values)) {
      fail(`${settingKey}.values must be an array`);
    }

    const wires = new Set();
    for (const option of setting.values ?? []) {
      if (!option || typeof option !== 'object') {
        fail(`${settingKey}.values must contain objects`);
      }
      const wire = nonEmptyString(option.wire, `${settingKey}.value.wire`);
      if (!/^[a-z0-9_]+$/.test(wire) || wires.has(wire)) {
        fail(`invalid or duplicate wire value ${settingKey}.${wire}`);
      }
      wires.add(wire);
      addMessage(`${prefix}.value.${wire}`, option.label);
    }
  }

  return {
    sourceCatalog: sortRecord(sourceCatalog),
    messages: sortRecord(messages),
  };
}
