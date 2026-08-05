import ts from 'typescript';

import {
  objectLiteralEntries,
  parseTypeScriptSource,
  stringLiteralValue,
  unwrapExpression,
  variableInitializer,
} from '../typescript-ast.mjs';

function fail(message) {
  throw new Error(`i18n contract generation failed: ${message}`);
}

export function parseJsonSource(sourceText) {
  try {
    return JSON.parse(sourceText);
  } catch (cause) {
    throw new Error('i18n contract generation failed: invalid JSON', { cause });
  }
}

function stringValue(expression, context) {
  return stringLiteralValue(expression, context, fail);
}

function objectEntries(expression, context) {
  return objectLiteralEntries(expression, context, fail);
}

function parseMessageExpression(expression, context) {
  const value = unwrapExpression(expression);
  if (ts.isStringLiteral(value) || ts.isNoSubstitutionTemplateLiteral(value)) {
    return { kind: 'string', template: value.text };
  }
  if (!ts.isCallExpression(value) || !ts.isIdentifier(value.expression)) {
    fail(`${context} must be a string, plural(), or select()`);
  }
  if (value.arguments.length !== 2) {
    fail(`${context} ${value.expression.text}() must receive an argument name and branches`);
  }

  return {
    kind: 'tagged',
    helper: value.expression.text,
    argument: stringValue(value.arguments[0], `${context} argument`),
    branches: objectEntries(value.arguments[1], `${context} branches`).map(([name, branch]) => [
      name,
      stringValue(branch, `${context}.${name}`),
    ]),
  };
}

export function parseEnglishSource(sourceText, fileName) {
  const source = parseTypeScriptSource(sourceText, fileName, fail);
  const initializer = variableInitializer(source, 'en', fail);
  if (
    !ts.isCallExpression(initializer) ||
    !ts.isIdentifier(initializer.expression) ||
    initializer.expression.text !== 'defineSourceCatalog' ||
    initializer.arguments.length !== 1
  ) {
    fail('en must be declared with defineSourceCatalog({...})');
  }

  return objectEntries(initializer.arguments[0], 'English catalog').map(([key, expression]) => [
    key,
    parseMessageExpression(expression, `English message ${key}`),
  ]);
}

export function parsePluralCategorySource(sourceText, fileName) {
  const source = parseTypeScriptSource(sourceText, fileName, fail);
  const initializer = variableInitializer(source, 'PLURAL_CATEGORIES', fail);
  return objectEntries(initializer, 'PLURAL_CATEGORIES').map(([locale, expression]) => {
    const value = unwrapExpression(expression);
    if (!ts.isArrayLiteralExpression(value)) {
      fail(`PLURAL_CATEGORIES.${locale} must be a literal array`);
    }
    return [
      locale,
      value.elements.map((element) => stringValue(element, `PLURAL_CATEGORIES.${locale}`)),
    ];
  });
}
