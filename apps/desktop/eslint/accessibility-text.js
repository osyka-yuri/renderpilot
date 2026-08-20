const ACCESSIBLE_TEXT_ATTRIBUTES = new Set([
  'alt',
  'aria-description',
  'aria-label',
  'aria-placeholder',
  'aria-roledescription',
  'aria-valuetext',
  'placeholder',
  'title',
]);

function attributeName(attribute) {
  const name = attribute.key?.name;
  return typeof name === 'string' ? name : name?.name;
}

function hasNonWhitespaceText(value) {
  return typeof value === 'string' && value.trim().length > 0;
}

function hasStaticTextExpression(expression) {
  if (expression?.type === 'Literal' && typeof expression.value === 'string') {
    return expression.value;
  }

  if (expression?.type === 'TemplateLiteral') {
    let value = '';

    for (let index = 0; index < expression.quasis.length; index += 1) {
      value += expression.quasis[index].value.cooked ?? expression.quasis[index].value.raw;

      if (index < expression.expressions.length) {
        const nested = hasStaticTextExpression(expression.expressions[index]);
        if (nested === undefined) {
          return undefined;
        }
        value += nested;
      }
    }

    return value;
  }

  if (expression?.type === 'BinaryExpression' && expression.operator === '+') {
    const left = hasStaticTextExpression(expression.left);
    const right = hasStaticTextExpression(expression.right);
    return left === undefined || right === undefined ? undefined : left + right;
  }

  return undefined;
}

function hasHardcodedTextExpression(expression) {
  if (expression?.type === 'Literal' && typeof expression.value === 'string') {
    return hasNonWhitespaceText(expression.value);
  }

  if (expression?.type === 'TemplateLiteral') {
    return (
      expression.quasis.some((quasi) =>
        hasNonWhitespaceText(quasi.value.cooked ?? quasi.value.raw),
      ) || expression.expressions.some((nested) => hasHardcodedTextExpression(nested))
    );
  }

  if (expression?.type === 'BinaryExpression' && expression.operator === '+') {
    return (
      hasHardcodedTextExpression(expression.left) || hasHardcodedTextExpression(expression.right)
    );
  }

  if (expression?.type === 'ConditionalExpression' || expression?.type === 'LogicalExpression') {
    return (
      hasHardcodedTextExpression(expression.consequent ?? expression.left) ||
      hasHardcodedTextExpression(expression.alternate ?? expression.right)
    );
  }

  return false;
}

function hasHardcodedAttributeText(attribute) {
  if (!Array.isArray(attribute.value)) {
    return false;
  }

  return attribute.value.some((value) => {
    if (value.type === 'SvelteLiteral') {
      return hasNonWhitespaceText(value.value);
    }

    return value.type === 'SvelteMustacheTag' && hasHardcodedTextExpression(value.expression);
  });
}

const SR_ONLY_CLASS = 'sr-only';
const NOT_SR_ONLY_CLASS = 'not-sr-only';

function unknownClassSummary() {
  return { guaranteesSrOnly: false, mayConflict: true };
}

function summarizeClassText(text) {
  const tokens = text.split(/\s+/u).filter(Boolean);
  return {
    guaranteesSrOnly: tokens.includes(SR_ONLY_CLASS),
    mayConflict: tokens.includes(NOT_SR_ONLY_CLASS),
  };
}

function combineClassSummaries(summaries) {
  return {
    guaranteesSrOnly: summaries.some((summary) => summary.guaranteesSrOnly),
    mayConflict: summaries.some((summary) => summary.mayConflict),
  };
}

function summarizeObjectClass(expression) {
  let srOnlyState = 'unresolved';
  let notSrOnlyState = 'unresolved';

  for (let index = expression.properties.length - 1; index >= 0; index -= 1) {
    const property = expression.properties[index];
    if (property.type === 'SpreadElement') {
      if (srOnlyState === 'unresolved') {
        srOnlyState = 'unknown';
      }
      if (notSrOnlyState === 'unresolved') {
        notSrOnlyState = 'unknown';
      }
      continue;
    }

    if (property.type !== 'Property' || property.kind !== 'init') {
      if (srOnlyState === 'unresolved') {
        srOnlyState = 'unknown';
      }
      if (notSrOnlyState === 'unresolved') {
        notSrOnlyState = 'unknown';
      }
      continue;
    }

    const key = property.computed
      ? hasStaticTextExpression(property.key)
      : property.key.type === 'Identifier'
        ? property.key.name
        : hasStaticTextExpression(property.key);

    if (key === undefined) {
      if (srOnlyState === 'unresolved') {
        srOnlyState = 'unknown';
      }
      if (notSrOnlyState === 'unresolved') {
        notSrOnlyState = 'unknown';
      }
      continue;
    }

    if (key !== SR_ONLY_CLASS && key !== NOT_SR_ONLY_CLASS) {
      continue;
    }

    const stateName = key === SR_ONLY_CLASS ? 'srOnlyState' : 'notSrOnlyState';
    if ({ srOnlyState, notSrOnlyState }[stateName] !== 'unresolved') {
      continue;
    }

    const value = property.value;
    const state =
      value.type === 'Literal' && value.value === true
        ? 'true'
        : value.type === 'Literal' && value.value === false
          ? 'false'
          : 'unknown';

    if (stateName === 'srOnlyState') {
      srOnlyState = state;
    } else {
      notSrOnlyState = state;
    }
  }

  const resolvedSrOnly = srOnlyState === 'unresolved' ? 'absent' : srOnlyState;
  const resolvedNotSrOnly = notSrOnlyState === 'unresolved' ? 'absent' : notSrOnlyState;

  return {
    guaranteesSrOnly:
      resolvedSrOnly === 'true' &&
      (resolvedNotSrOnly === 'absent' || resolvedNotSrOnly === 'false'),
    mayConflict:
      resolvedSrOnly === 'unknown' ||
      resolvedNotSrOnly === 'unknown' ||
      resolvedNotSrOnly === 'true',
  };
}

function summarizeClassExpression(expression) {
  const text = hasStaticTextExpression(expression);
  if (text !== undefined) {
    return summarizeClassText(text);
  }

  if (expression?.type === 'ArrayExpression') {
    return combineClassSummaries(
      expression.elements.map((element) =>
        element?.type === 'SpreadElement'
          ? unknownClassSummary()
          : summarizeClassExpression(element),
      ),
    );
  }

  if (expression?.type === 'ObjectExpression') {
    return summarizeObjectClass(expression);
  }

  if (expression?.type === 'ConditionalExpression') {
    const consequent = summarizeClassExpression(expression.consequent);
    const alternate = summarizeClassExpression(expression.alternate);
    return {
      guaranteesSrOnly: consequent.guaranteesSrOnly && alternate.guaranteesSrOnly,
      mayConflict: consequent.mayConflict || alternate.mayConflict,
    };
  }

  if (expression?.type === 'LogicalExpression') {
    const left = summarizeClassExpression(expression.left);
    const right = summarizeClassExpression(expression.right);
    if (expression.operator === '&&') {
      return { guaranteesSrOnly: false, mayConflict: right.mayConflict };
    }
    if (expression.operator === '||') {
      return {
        guaranteesSrOnly: left.guaranteesSrOnly && right.guaranteesSrOnly,
        mayConflict: left.mayConflict || right.mayConflict,
      };
    }
  }

  if (
    expression?.type === 'CallExpression' &&
    expression.callee.type === 'Identifier' &&
    ['cn', 'clsx', 'cx'].includes(expression.callee.name)
  ) {
    return combineClassSummaries(
      expression.arguments.map((argument) =>
        argument.type === 'SpreadElement'
          ? unknownClassSummary()
          : summarizeClassExpression(argument),
      ),
    );
  }

  return unknownClassSummary();
}

function summarizeClassAttribute(attribute) {
  if (!Array.isArray(attribute.value)) {
    return unknownClassSummary();
  }

  return combineClassSummaries(
    attribute.value.map((value) => {
      if (value.type === 'SvelteLiteral') {
        return summarizeClassText(value.value);
      }
      if (value.type === 'SvelteMustacheTag') {
        return summarizeClassExpression(value.expression);
      }
      return unknownClassSummary();
    }),
  );
}

function summarizeClassDirective(attribute) {
  if (attribute.type !== 'SvelteDirective' || attribute.kind !== 'Class') {
    return undefined;
  }

  const name = attributeName(attribute);
  if (name !== SR_ONLY_CLASS && name !== NOT_SR_ONLY_CLASS) {
    return undefined;
  }

  const isStaticTrue =
    attribute.expression?.type === 'Literal' && attribute.expression.value === true;
  const isStaticFalse =
    attribute.expression?.type === 'Literal' && attribute.expression.value === false;

  return name === SR_ONLY_CLASS
    ? { guaranteesSrOnly: isStaticTrue, mayConflict: false }
    : { guaranteesSrOnly: false, mayConflict: !isStaticFalse };
}

function summarizeElementClasses(element) {
  return combineClassSummaries(
    element.startTag.attributes.flatMap((attribute) => {
      if (attribute.type === 'SvelteAttribute' && attributeName(attribute) === 'class') {
        return [summarizeClassAttribute(attribute)];
      }

      const directiveSummary = summarizeClassDirective(attribute);
      return directiveSummary === undefined ? [] : [directiveSummary];
    }),
  );
}

function hasStaticSrOnlyClass(element) {
  const summary = summarizeElementClasses(element);
  return summary.guaranteesSrOnly && !summary.mayConflict;
}

function isInsideStaticSrOnlyElement(node) {
  let current = node.parent;

  while (current) {
    if (current.type === 'SvelteElement' && hasStaticSrOnlyClass(current)) {
      return true;
    }
    current = current.parent;
  }

  return false;
}

function isSvelteAttributeValue(node) {
  return node.parent?.type === 'SvelteAttribute';
}

export const noHardcodedAccessibilityTextRule = {
  meta: {
    type: 'problem',
    docs: {
      description: 'Require translatable user-facing accessibility text in Svelte templates.',
    },
    schema: [],
    messages: {
      hardcodedAttribute:
        'Do not hardcode user-facing accessibility text in "{{ name }}". Use the typed i18n catalog.',
      hardcodedSrOnly: 'Do not hardcode screen-reader-only text. Use the typed i18n catalog.',
    },
  },

  create(context) {
    return {
      SvelteAttribute(node) {
        const name = attributeName(node);
        if (!ACCESSIBLE_TEXT_ATTRIBUTES.has(name)) {
          return;
        }

        if (!hasHardcodedAttributeText(node)) {
          return;
        }

        context.report({ node, messageId: 'hardcodedAttribute', data: { name } });
      },

      SvelteText(node) {
        if (node.value.trim().length > 0 && isInsideStaticSrOnlyElement(node)) {
          context.report({ node, messageId: 'hardcodedSrOnly' });
        }
      },

      SvelteMustacheTag(node) {
        if (isSvelteAttributeValue(node)) {
          return;
        }

        if (hasHardcodedTextExpression(node.expression) && isInsideStaticSrOnlyElement(node)) {
          context.report({ node, messageId: 'hardcodedSrOnly' });
        }
      },
    };
  },
};
