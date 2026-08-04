import path from 'node:path';

const LOCALE_FORMAT_METHODS = new Set([
  'toLocaleString',
  'toLocaleDateString',
  'toLocaleTimeString',
]);

const INTL_IMPORT_ALIASES = ['@shared/intl', '@/shared/intl'];

export function createIntlBoundariesRule({ projectRoot, sourceRoot }) {
  const normalizedSourceRoot = sourceRoot.replaceAll('\\', '/').replace(/^\.?\//u, '');

  function isInsideSharedSegment(filename, segment) {
    if (filename.length === 0 || filename === '<input>' || filename === '<text>') {
      return false;
    }

    const relativePath = path.relative(projectRoot, filename).replaceAll(path.sep, '/');
    return relativePath.startsWith(`${normalizedSourceRoot}/shared/${segment}/`);
  }

  return {
    meta: {
      type: 'problem',
      docs: {
        description:
          'Keep Intl construction in shared/intl and expose it through semantic shared APIs.',
      },
      schema: [],
      messages: {
        directIntl: 'Use Intl runtime APIs only in shared/intl; use a semantic shared API.',
        directIntlImport:
          'Import @shared/intl only from shared/format, shared/i18n, or shared/text; UI consumers must use a semantic shared API.',
        localeMethod:
          'Do not format values with toLocale* directly; use the @shared/format semantic API.',
      },
    },

    create(context) {
      const filename = context.filename;
      const ownsIntlConstruction = isInsideSharedSegment(filename, 'intl');
      const mayImportIntl =
        ownsIntlConstruction ||
        isInsideSharedSegment(filename, 'format') ||
        isInsideSharedSegment(filename, 'i18n') ||
        isInsideSharedSegment(filename, 'text');
      const sourceCode = context.sourceCode;

      function staticPropertyName(node) {
        if (!node.computed && node.property.type === 'Identifier') {
          return node.property.name;
        }
        if (node.computed && node.property.type === 'Literal') {
          return typeof node.property.value === 'string' ? node.property.value : null;
        }
        if (
          node.computed &&
          node.property.type === 'TemplateLiteral' &&
          node.property.expressions.length === 0
        ) {
          return node.property.quasis[0]?.value.cooked ?? null;
        }
        return null;
      }

      function isGlobalIdentifier(node, name) {
        if (node.type !== 'Identifier' || node.name !== name) {
          return false;
        }

        for (let scope = sourceCode.getScope(node); scope !== null; scope = scope.upper) {
          const variable = scope.set.get(name);
          if (variable !== undefined) {
            return variable.defs.length === 0;
          }
        }

        return true;
      }

      function isGlobalIntlIdentifier(node) {
        return isGlobalIdentifier(node, 'Intl');
      }

      function isGlobalThisIntl(node) {
        return (
          node.type === 'MemberExpression' &&
          isGlobalIdentifier(node.object, 'globalThis') &&
          staticPropertyName(node) === 'Intl'
        );
      }

      function isIntlNamespace(node) {
        return isGlobalIntlIdentifier(node) || isGlobalThisIntl(node);
      }

      function isRegistryImport(value) {
        return (
          typeof value === 'string' &&
          INTL_IMPORT_ALIASES.some((alias) => value === alias || value.startsWith(`${alias}/`))
        );
      }

      function checkRegistryModuleSource(source) {
        if (!mayImportIntl && source !== null && isRegistryImport(source.value)) {
          context.report({ node: source, messageId: 'directIntlImport' });
        }
      }

      return {
        ImportDeclaration(node) {
          checkRegistryModuleSource(node.source);
        },

        ExportNamedDeclaration(node) {
          checkRegistryModuleSource(node.source);
        },

        ExportAllDeclaration(node) {
          checkRegistryModuleSource(node.source);
        },

        ImportExpression(node) {
          checkRegistryModuleSource(node.source);
        },

        Identifier(node) {
          if (ownsIntlConstruction || !isGlobalIntlIdentifier(node)) {
            return;
          }

          const parent = node.parent;
          const isMemberPart =
            parent?.type === 'MemberExpression' &&
            (parent.object === node || (!parent.computed && parent.property === node));
          const isTypeOnlyReference =
            parent?.type === 'TSQualifiedName' || parent?.type === 'TSTypeQuery';

          if (!isMemberPart && !isTypeOnlyReference) {
            context.report({ node, messageId: 'directIntl' });
          }
        },

        MemberExpression(node) {
          if (ownsIntlConstruction) {
            return;
          }

          const propertyName = staticPropertyName(node);
          if (isIntlNamespace(node.object)) {
            context.report({ node, messageId: 'directIntl' });
            return;
          }

          if (propertyName !== null && LOCALE_FORMAT_METHODS.has(propertyName)) {
            context.report({ node, messageId: 'localeMethod' });
            return;
          }

          if (isGlobalThisIntl(node)) {
            const parent = node.parent;
            const isNamespaceAccess = parent?.type === 'MemberExpression' && parent.object === node;
            if (!isNamespaceAccess) {
              context.report({ node, messageId: 'directIntl' });
            }
          }
        },
      };
    },
  };
}
