import ts from 'typescript';

export function parseTypeScriptSource(sourceText, fileName, fail) {
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

export function unwrapExpression(expression) {
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

export function propertyName(name, context, fail) {
  if (ts.isIdentifier(name) || ts.isStringLiteral(name) || ts.isNumericLiteral(name)) {
    return name.text;
  }
  fail(`${context} contains a computed property`);
}

export function stringLiteralValue(expression, context, fail) {
  const value = unwrapExpression(expression);
  if (ts.isStringLiteral(value) || ts.isNoSubstitutionTemplateLiteral(value)) {
    return value.text;
  }
  fail(`${context} must be a string literal`);
}

export function objectLiteralEntries(expression, context, fail) {
  const value = unwrapExpression(expression);
  if (!ts.isObjectLiteralExpression(value)) {
    fail(`${context} must be an object literal`);
  }
  return value.properties.map((property) => {
    if (!ts.isPropertyAssignment(property)) {
      fail(`${context} may contain only property assignments`);
    }
    return [propertyName(property.name, context, fail), property.initializer];
  });
}

export function variableInitializer(source, variableName, fail) {
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
