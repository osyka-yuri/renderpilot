import ts from 'typescript';

function fail(message) {
  throw new Error(`i18n contract generation failed: ${message}`);
}

function sourceFile(fileName, sourceText) {
  const source = ts.createSourceFile(
    fileName,
    sourceText,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );
  if (source.parseDiagnostics.length > 0) {
    const diagnostic = source.parseDiagnostics[0];
    const location =
      diagnostic.start === undefined
        ? 'unknown location'
        : (() => {
            const { line, character } = source.getLineAndCharacterOfPosition(diagnostic.start);
            return `${line + 1}:${character + 1}`;
          })();
    fail(
      `TypeScript syntax error at ${location}: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')}`,
    );
  }
  return source;
}

export function parseJsonSource(sourceText) {
  try {
    return JSON.parse(sourceText);
  } catch (cause) {
    throw new Error('i18n contract generation failed: invalid JSON', { cause });
  }
}

function unwrapExpression(expression) {
  let current = expression;
  while (
    ts.isAsExpression(current) ||
    ts.isSatisfiesExpression(current) ||
    ts.isParenthesizedExpression(current)
  ) {
    current = current.expression;
  }
  return current;
}

function propertyName(name, context) {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) {
    return name.text;
  }
  fail(`${context} contains a computed property`);
}

function stringValue(expression, context) {
  const value = unwrapExpression(expression);
  if (ts.isStringLiteral(value) || ts.isNoSubstitutionTemplateLiteral(value)) {
    return value.text;
  }
  fail(`${context} must be a string literal`);
}

function objectEntries(expression, context) {
  const value = unwrapExpression(expression);
  if (!ts.isObjectLiteralExpression(value)) {
    fail(`${context} must be an object literal`);
  }
  return value.properties.map((property) => {
    if (!ts.isPropertyAssignment(property)) {
      fail(`${context} may contain only property assignments`);
    }
    return [propertyName(property.name, context), property.initializer];
  });
}

function variableInitializer(source, variableName) {
  for (const statement of source.statements) {
    if (!ts.isVariableStatement(statement)) {
      continue;
    }
    for (const declaration of statement.declarationList.declarations) {
      if (ts.isIdentifier(declaration.name) && declaration.name.text === variableName) {
        if (!declaration.initializer) {
          fail(`${variableName} has no initializer`);
        }
        return unwrapExpression(declaration.initializer);
      }
    }
  }
  fail(`could not find ${variableName}`);
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
  const source = sourceFile(fileName, sourceText);
  const initializer = variableInitializer(source, 'en');
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
  const source = sourceFile(fileName, sourceText);
  const initializer = variableInitializer(source, 'PLURAL_CATEGORIES');
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

export function parseLumaSource(value) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    fail('Luma contract must be an object');
  }
  return Object.entries(value);
}

export function parseNvapiSource(value) {
  if (!value || typeof value !== 'object' || !Array.isArray(value.settings)) {
    fail('NVAPI source must contain a settings array');
  }
  return value.settings;
}
