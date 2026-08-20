import path from 'node:path';

import { FSD_ALIAS_PREFIXES } from './fsd-boundaries.js';

const MESSAGES = Object.freeze({
  bitsUi:
    'Product UI must consume primitives through the shared UI public API. Add or extend a component in "@shared/ui" instead of importing "bits-ui" directly.',
  fsd: 'Do not import restricted FSD roots or internal paths through dynamic or CommonJS imports. Use a slice or segment public API.',
  sonner:
    'Dispatch notifications through @shared/notifications. Direct svelte-sonner imports belong only to the notification adapter and Toaster primitive.',
});

function exactDescriptor(value) {
  return { exact: value, prefix: value };
}

function unknownDescriptor() {
  return { prefix: '' };
}

function longestCommonPrefix(left, right) {
  const length = Math.min(left.length, right.length);
  let index = 0;

  while (index < length && left[index] === right[index]) {
    index += 1;
  }

  return left.slice(0, index);
}

function combineAlternatives(left, right) {
  const combinations = [];

  for (const leftAlternative of left) {
    for (const rightAlternative of right) {
      combinations.push(concatenateDescriptors(leftAlternative, rightAlternative));
      if (combinations.length > 128) {
        return [unknownDescriptor()];
      }
    }
  }

  return combinations;
}

function descriptorFromAlternatives(alternatives) {
  if (alternatives.length === 0) {
    return unknownDescriptor();
  }

  const exactValues = alternatives.map((alternative) => alternative.exact);
  if (exactValues.every((value) => value !== undefined && value === exactValues[0])) {
    return exactDescriptor(exactValues[0]);
  }

  return {
    prefix: alternatives.reduce(
      (prefix, alternative) => longestCommonPrefix(prefix, alternative.prefix),
      alternatives[0].prefix,
    ),
  };
}

function concatenateDescriptors(left, right) {
  if (left.exact !== undefined && right.exact !== undefined) {
    return exactDescriptor(left.exact + right.exact);
  }

  if (left.exact !== undefined) {
    return { prefix: left.exact + right.prefix };
  }

  return { prefix: left.prefix };
}

function describeTemplate(node) {
  let alternatives = [exactDescriptor('')];

  for (let index = 0; index < node.quasis.length; index += 1) {
    alternatives = alternatives.map((alternative) =>
      concatenateDescriptors(
        alternative,
        exactDescriptor(node.quasis[index].value.cooked ?? node.quasis[index].value.raw),
      ),
    );

    if (index < node.expressions.length) {
      alternatives = combineAlternatives(
        alternatives,
        describeAlternatives(node.expressions[index]),
      );
    }
  }

  return descriptorFromAlternatives(alternatives);
}

function describeAlternatives(node) {
  if (node?.type === 'Literal' && typeof node.value === 'string') {
    return [exactDescriptor(node.value)];
  }

  if (node?.type === 'TemplateLiteral') {
    return [describeTemplate(node)];
  }

  if (node?.type === 'BinaryExpression' && node.operator === '+') {
    return combineAlternatives(describeAlternatives(node.left), describeAlternatives(node.right));
  }

  if (node?.type === 'ConditionalExpression') {
    return [...describeAlternatives(node.consequent), ...describeAlternatives(node.alternate)];
  }

  if (node?.type === 'LogicalExpression') {
    const left = describeAlternatives(node.left);
    const right = describeAlternatives(node.right);

    if (node.operator === '&&') {
      return [unknownDescriptor(), ...right];
    }

    if (node.operator === '||' || node.operator === '??') {
      return [...left, ...right];
    }
  }

  if (
    node?.type === 'ChainExpression' ||
    node?.type === 'ParenthesizedExpression' ||
    node?.type === 'TSAsExpression' ||
    node?.type === 'TSTypeAssertion' ||
    node?.type === 'TSNonNullExpression' ||
    node?.type === 'TypeCastExpression'
  ) {
    return describeAlternatives(node.expression);
  }

  return [unknownDescriptor()];
}

function describeSpecifier(node) {
  const alternatives = describeAlternatives(node);
  return {
    ...descriptorFromAlternatives(alternatives),
    alternatives,
  };
}

function matchesPackage(descriptor, packageName) {
  return (
    descriptor.exact === packageName ||
    descriptor.exact?.startsWith(`${packageName}/`) === true ||
    (descriptor.exact === undefined && descriptor.prefix.startsWith(`${packageName}/`))
  );
}

function matchesFsdPath(descriptor) {
  return FSD_ALIAS_PREFIXES.some((alias) => {
    if (descriptor.exact === alias) {
      return true;
    }

    const deepPrefix = `${alias}/`;
    if (descriptor.exact?.startsWith(deepPrefix)) {
      return descriptor.exact.slice(deepPrefix.length).includes('/');
    }

    if (descriptor.exact === undefined && descriptor.prefix.startsWith(`${alias}/`)) {
      const remainder = descriptor.prefix.slice(deepPrefix.length);
      return remainder.includes('/');
    }

    return false;
  });
}

function anyAlternativeMatches(descriptor, matcher) {
  return descriptor.alternatives.some(matcher);
}

function expandExactEntryPointGlob(pattern) {
  const openingBrace = pattern.indexOf('{');
  if (openingBrace < 0) {
    return [pattern];
  }

  const closingBrace = pattern.indexOf('}', openingBrace);
  if (closingBrace < 0) {
    return [pattern];
  }

  const prefix = pattern.slice(0, openingBrace);
  const suffix = pattern.slice(closingBrace + 1);
  return pattern
    .slice(openingBrace + 1, closingBrace)
    .split(',')
    .flatMap((alternative) => expandExactEntryPointGlob(`${prefix}${alternative}${suffix}`));
}

function isFoundationEntryPoint(filename, projectRoot, foundationEntryPointPaths) {
  const relative = relativeSourcePath(filename, projectRoot);
  return foundationEntryPointPaths.has(relative);
}

function relativeSourcePath(filename, projectRoot) {
  const relative = path.relative(projectRoot, filename);
  return relative.split(path.sep).join('/');
}

function isSonnerOwner(filename, projectRoot) {
  const relative = relativeSourcePath(filename, projectRoot);
  return (
    relative.startsWith('ui/src/shared/ui/sonner/') ||
    relative === 'ui/src/widgets/notifications-toaster/notification-adapter.ts' ||
    relative === 'ui/src/widgets/notifications-toaster/notification-adapter.test.ts'
  );
}

function isSharedUiOwner(filename, projectRoot) {
  return relativeSourcePath(filename, projectRoot).startsWith('ui/src/shared/ui/');
}

function isLexicallyShadowedRequire(sourceCode, node) {
  for (let scope = sourceCode.getScope(node); scope !== null; scope = scope.upper) {
    const variable = scope.set.get('require');
    if (variable?.defs.length > 0) {
      return true;
    }
  }

  return false;
}

function isStaticImportSyntax(kind) {
  return kind === 'import' || kind === 'export' || kind === 'import-equals';
}

function policyMatches(policy, descriptor, kind, filename, projectRoot, foundationEntryPointPaths) {
  if (
    policy.id === 'fsd' &&
    (isStaticImportSyntax(kind) ||
      isFoundationEntryPoint(filename, projectRoot, foundationEntryPointPaths))
  ) {
    return false;
  }

  if (policy.id === 'bits-ui' && isSharedUiOwner(filename, projectRoot)) {
    return false;
  }

  if (policy.id === 'sonner' && isSonnerOwner(filename, projectRoot)) {
    return false;
  }

  return anyAlternativeMatches(descriptor, policy.matcher);
}

export const importBoundaryMessages = MESSAGES;

export function createImportBoundariesRule({ projectRoot, foundationEntryPoints = [] }) {
  const foundationEntryPointPaths = new Set(
    foundationEntryPoints.flatMap(expandExactEntryPointGlob),
  );
  const policies = [
    {
      id: 'bits-ui',
      messageId: 'directBitsUiImport',
      matcher: (descriptor) => matchesPackage(descriptor, 'bits-ui'),
    },
    { id: 'fsd', messageId: 'restrictedFsdImport', matcher: matchesFsdPath },
    {
      id: 'sonner',
      messageId: 'directSonnerImport',
      matcher: (descriptor) => matchesPackage(descriptor, 'svelte-sonner'),
    },
  ];

  return {
    meta: {
      type: 'problem',
      docs: {
        description:
          'Enforce shared ownership for Bits UI, FSD aliases, and notification dispatch imports.',
      },
      schema: [],
      messages: {
        directBitsUiImport: MESSAGES.bitsUi,
        restrictedFsdImport: MESSAGES.fsd,
        directSonnerImport: MESSAGES.sonner,
      },
    },

    create(context) {
      const sourceCode = context.sourceCode;
      const reportedNodes = new Set();
      const filename = context.filename;

      function checkSpecifier(node, kind) {
        if (!node || reportedNodes.has(node)) {
          return;
        }

        const descriptor = describeSpecifier(node);
        const policy = policies.find((candidate) =>
          policyMatches(
            candidate,
            descriptor,
            kind,
            filename,
            projectRoot,
            foundationEntryPointPaths,
          ),
        );

        if (!policy) {
          return;
        }

        reportedNodes.add(node);
        context.report({ node, messageId: policy.messageId });
      }

      return {
        ImportDeclaration(node) {
          checkSpecifier(node.source, 'import');
        },
        ExportNamedDeclaration(node) {
          checkSpecifier(node.source, 'export');
        },
        ExportAllDeclaration(node) {
          checkSpecifier(node.source, 'export');
        },
        ImportExpression(node) {
          checkSpecifier(node.source, 'dynamic');
        },
        CallExpression(node) {
          if (
            node.callee.type === 'Identifier' &&
            node.callee.name === 'require' &&
            node.arguments.length === 1 &&
            !isLexicallyShadowedRequire(sourceCode, node)
          ) {
            checkSpecifier(node.arguments[0], 'require');
          }
        },
        TSImportEqualsDeclaration(node) {
          if (node.moduleReference?.type === 'TSExternalModuleReference') {
            checkSpecifier(node.moduleReference.expression, 'import-equals');
          }
        },
      };
    },
  };
}

export { describeSpecifier, matchesFsdPath, matchesPackage };
