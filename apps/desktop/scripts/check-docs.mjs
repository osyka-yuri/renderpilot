import { glob, readdir, readFile, stat } from 'node:fs/promises';
import path from 'node:path';

const repositoryRoot = path.resolve(import.meta.dirname, '../../..');
const documentationRoot = path.join(repositoryRoot, 'docs');
const readmePath = path.join(repositoryRoot, 'README.md');

const failures = [];

function relativeName(filePath) {
  return path.relative(repositoryRoot, filePath).replaceAll(path.sep, '/');
}

function report(filePath, message) {
  failures.push(`${relativeName(filePath)}: ${message}`);
}

function githubSlug(text) {
  return text
    .trim()
    .toLowerCase()
    .replace(/<[^>]+>/g, '')
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .replace(/\s+/g, '-');
}

export function markdownWithoutFencedCode(markdown) {
  let fenceCharacter;
  let fenceLength = 0;

  return markdown
    .split(/\r?\n/)
    .map((line) => {
      if (fenceCharacter === undefined) {
        const openingFence = /^ {0,3}(`{3,}|~{3,}).*$/.exec(line);
        if (!openingFence) {
          return line;
        }

        fenceCharacter = openingFence[1][0];
        fenceLength = openingFence[1].length;
        return '';
      }

      const closingFence = new RegExp(`^ {0,3}${fenceCharacter}{${fenceLength},}\\s*$`);
      if (closingFence.test(line)) {
        fenceCharacter = undefined;
        fenceLength = 0;
      }
      return '';
    })
    .join('\n');
}

export function headingAnchors(markdown) {
  const anchors = new Set();
  const counts = new Map();

  for (const line of markdownWithoutFencedCode(markdown).split('\n')) {
    const match = /^(#{1,6})\s+(.+?)\s*#*\s*$/.exec(line);
    if (!match) {
      continue;
    }

    const base = githubSlug(match[2]);
    const count = counts.get(base) ?? 0;
    counts.set(base, count + 1);
    anchors.add(count === 0 ? base : `${base}-${count}`);
  }

  return anchors;
}

async function caseSensitiveTarget(targetPath) {
  const relative = path.relative(repositoryRoot, targetPath);
  if (relative.startsWith('..') || path.isAbsolute(relative)) {
    return false;
  }

  let cursor = repositoryRoot;
  for (const segment of relative.split(path.sep).filter(Boolean)) {
    const entries = await readdir(cursor);
    if (!entries.includes(segment)) {
      return false;
    }
    cursor = path.join(cursor, segment);
  }
  return true;
}

export function localReferences(markdown) {
  const references = [];
  const markdownLink = /(!?)\[([^\]]*)\]\(([^)\s]+)(?:\s+["'][^)]*["'])?\)/g;
  const htmlReference = /<(a|img)\b[^>]*?\b(href|src)="([^"]+)"[^>]*>/gi;
  const prose = markdownWithoutFencedCode(markdown);

  for (const match of prose.matchAll(markdownLink)) {
    references.push({
      destination: match[3],
      isImage: match[1] === '!',
      alt: match[2].trim(),
    });
  }

  for (const match of prose.matchAll(htmlReference)) {
    const isImage = match[1].toLowerCase() === 'img';
    const altMatch = /\balt="([^"]*)"/i.exec(match[0]);
    references.push({
      destination: match[3],
      isImage,
      alt: isImage ? (altMatch?.[1].trim() ?? '') : undefined,
    });
  }

  return references;
}

async function validateReferences(filePath, markdown, markdownByPath) {
  for (const reference of localReferences(markdown)) {
    if (reference.isImage && !reference.alt) {
      report(filePath, `image has empty alt text: ${reference.destination}`);
    }

    if (/^(?:https?:|mailto:)/i.test(reference.destination)) {
      continue;
    }

    const hashIndex = reference.destination.indexOf('#');
    const rawPath =
      hashIndex === -1 ? reference.destination : reference.destination.slice(0, hashIndex);
    const rawAnchor = hashIndex === -1 ? '' : reference.destination.slice(hashIndex + 1);
    const decodedPath = decodeURIComponent(rawPath);
    const targetPath = decodedPath ? path.resolve(path.dirname(filePath), decodedPath) : filePath;

    try {
      if (!(await caseSensitiveTarget(targetPath))) {
        report(filePath, `relative target is missing or has incorrect case: ${rawPath}`);
        continue;
      }
      await stat(targetPath);
    } catch {
      report(filePath, `relative target does not exist: ${rawPath}`);
      continue;
    }

    if (!rawAnchor) {
      continue;
    }
    const targetMarkdown = markdownByPath.get(targetPath) ?? (await readFile(targetPath, 'utf8'));
    const anchor = decodeURIComponent(rawAnchor).toLowerCase();
    if (!headingAnchors(targetMarkdown).has(anchor)) {
      report(filePath, `anchor does not exist in ${relativeName(targetPath)}: #${rawAnchor}`);
    }
  }
}

async function validateReadmeScreenshot(markdown) {
  const screenshotReferences = localReferences(markdown).filter(
    ({ destination, isImage }) => isImage && destination === 'docs/screenshot.webp',
  );
  if (screenshotReferences.length !== 1) {
    report(
      readmePath,
      `must reference docs/screenshot.webp exactly once; found ${screenshotReferences.length}`,
    );
  }

  const screenshot = await readFile(path.join(documentationRoot, 'screenshot.webp'));
  if (
    screenshot.length < 12 ||
    screenshot.subarray(0, 4).toString('ascii') !== 'RIFF' ||
    screenshot.subarray(8, 12).toString('ascii') !== 'WEBP'
  ) {
    report(readmePath, 'docs/screenshot.webp is not a valid RIFF WebP file');
  }
}

async function main() {
  const documentationFiles = (await Array.fromAsync(glob('**/*.md', { cwd: documentationRoot })))
    .map((filePath) => path.join(documentationRoot, filePath))
    .toSorted();
  const markdownFiles = [
    readmePath,
    path.join(repositoryRoot, 'CONTRIBUTING.md'),
    ...documentationFiles,
  ];
  const markdownByPath = new Map();

  for (const filePath of markdownFiles) {
    markdownByPath.set(filePath, await readFile(filePath, 'utf8'));
  }

  for (const [filePath, markdown] of markdownByPath) {
    await validateReferences(filePath, markdown, markdownByPath);
  }
  await validateReadmeScreenshot(markdownByPath.get(readmePath));

  if (failures.length > 0) {
    console.error(
      `Documentation checks failed (${failures.length}) across ${markdownFiles.length} Markdown files:`,
    );
    for (const failure of failures) {
      console.error(`- ${failure}`);
    }
    process.exitCode = 1;
  } else {
    console.log(`Documentation checks passed for ${markdownFiles.length} Markdown files.`);
  }
}

if (process.argv[1] !== undefined && path.resolve(process.argv[1]) === import.meta.filename) {
  await main();
}
