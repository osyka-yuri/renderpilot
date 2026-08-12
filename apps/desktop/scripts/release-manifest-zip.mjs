import { inflateRawSync } from 'node:zlib';

import { fail } from './release-manifest-common.mjs';

const CENTRAL_DIRECTORY_SIGNATURE = 0x02014b50;
const END_OF_CENTRAL_DIRECTORY_SIGNATURE = 0x06054b50;
const LOCAL_FILE_HEADER_SIGNATURE = 0x04034b50;
const UTF8_FLAG = 0x0800;
const SUPPORTED_FLAGS = UTF8_FLAG;
const ZIP64_EXTRA_FIELD = 0x0001;
const ZIP64_UINT16_SENTINEL = 0xffff;
const ZIP64_UINT32_SENTINEL = 0xffffffff;

function readUInt32(buffer, offset, label) {
  if (offset < 0 || offset > buffer.length - 4) {
    fail(`${label} is truncated.`);
  }
  return buffer.readUInt32LE(offset);
}

function readUInt16(buffer, offset, label) {
  if (offset < 0 || offset > buffer.length - 2) {
    fail(`${label} is truncated.`);
  }
  return buffer.readUInt16LE(offset);
}

function endOffset(buffer, offset, length, label) {
  if (offset < 0 || length < 0 || offset > buffer.length - length) {
    fail(`${label} is truncated.`);
  }
  return offset + length;
}

function crc32(buffer) {
  let result = 0xffffffff;
  for (const byte of buffer) {
    result ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      result = (result >>> 1) ^ (0xedb88320 & -(result & 1));
    }
  }
  return (result ^ 0xffffffff) >>> 0;
}

function findEndOfCentralDirectory(archive) {
  if (archive.length < 22) {
    fail('Portable ZIP is shorter than its end-of-central-directory record.');
  }
  const minimumOffset = Math.max(0, archive.length - 65_557);
  for (let offset = archive.length - 22; offset >= minimumOffset; offset -= 1) {
    if (archive.readUInt32LE(offset) !== END_OF_CENTRAL_DIRECTORY_SIGNATURE) {
      continue;
    }
    const commentLength = readUInt16(archive, offset + 20, 'Portable ZIP comment length');
    if (offset + 22 + commentLength === archive.length) {
      return offset;
    }
  }
  fail('Portable ZIP does not contain an exact end-of-central-directory record.');
}

function rejectZip64Sentinel(value, sentinel, label) {
  if (value === sentinel) {
    fail(`${label} uses an unsupported Zip64 sentinel.`);
  }
}

function validateExtraFields(extra, label) {
  let offset = 0;
  while (offset < extra.length) {
    const fieldEnd = endOffset(extra, offset, 4, `${label} extra-field header`);
    const fieldId = readUInt16(extra, offset, `${label} extra-field ID`);
    const fieldLength = readUInt16(extra, offset + 2, `${label} extra-field length`);
    const valueEnd = endOffset(extra, fieldEnd, fieldLength, `${label} extra field`);
    if (fieldId === ZIP64_EXTRA_FIELD) {
      fail(`${label} contains an unsupported Zip64 extra field.`);
    }
    offset = valueEnd;
  }
}

function assertSupportedFlags(flags, label) {
  if ((flags & ~SUPPORTED_FLAGS) !== 0) {
    fail(`${label} uses unsupported general-purpose flags 0x${flags.toString(16)}.`);
  }
}

function assertSupportedCompression(compression, label) {
  if (compression !== 0 && compression !== 8) {
    fail(`${label} uses unsupported compression method ${compression}.`);
  }
}

function decodeEntryName(bytes, flags, label) {
  if ((flags & UTF8_FLAG) === 0) {
    if (bytes.some((byte) => byte > 0x7f)) {
      fail(`${label} must use ASCII unless the UTF-8 flag is set.`);
    }
    return bytes.toString('ascii');
  }
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch (error) {
    fail(`${label} has invalid UTF-8: ${error.message}`);
  }
}

function validateCanonicalName(name, label) {
  if (!name || name.includes('\0')) {
    fail(`${label} is empty or contains a NUL.`);
  }
  if (name.includes('\\') || name.startsWith('/') || /^[A-Za-z]:/.test(name)) {
    fail(`${label} is not a canonical relative ZIP path.`);
  }
  const directory = name.endsWith('/');
  const parts = name.split('/');
  if (directory) {
    parts.pop();
  }
  if (
    parts.length === 0 ||
    parts.some((part) => part.length === 0 || part === '.' || part === '..')
  ) {
    fail(`${label} is not a canonical relative ZIP path.`);
  }
  return { directory, name };
}

function parseEndOfCentralDirectory(archive) {
  const offset = findEndOfCentralDirectory(archive);
  const diskNumber = readUInt16(archive, offset + 4, 'Portable ZIP disk number');
  const centralDirectoryDisk = readUInt16(
    archive,
    offset + 6,
    'Portable ZIP central-directory disk number',
  );
  const entriesOnDisk = readUInt16(archive, offset + 8, 'Portable ZIP entries on disk');
  const entryCount = readUInt16(archive, offset + 10, 'Portable ZIP entry count');
  const centralDirectorySize = readUInt32(
    archive,
    offset + 12,
    'Portable ZIP central-directory size',
  );
  const centralDirectoryOffset = readUInt32(
    archive,
    offset + 16,
    'Portable ZIP central-directory offset',
  );
  for (const [value, sentinel, label] of [
    [diskNumber, ZIP64_UINT16_SENTINEL, 'Portable ZIP disk number'],
    [centralDirectoryDisk, ZIP64_UINT16_SENTINEL, 'Portable ZIP central-directory disk number'],
    [entriesOnDisk, ZIP64_UINT16_SENTINEL, 'Portable ZIP entries on disk'],
    [entryCount, ZIP64_UINT16_SENTINEL, 'Portable ZIP entry count'],
    [centralDirectorySize, ZIP64_UINT32_SENTINEL, 'Portable ZIP central-directory size'],
    [centralDirectoryOffset, ZIP64_UINT32_SENTINEL, 'Portable ZIP central-directory offset'],
  ]) {
    rejectZip64Sentinel(value, sentinel, label);
  }
  if (diskNumber !== 0 || centralDirectoryDisk !== 0 || entriesOnDisk !== entryCount) {
    fail('Portable ZIP must be a single-disk archive.');
  }
  if (centralDirectoryOffset + centralDirectorySize !== offset) {
    fail('Portable ZIP central-directory size and offset must end exactly at the EOCD.');
  }
  return { centralDirectoryOffset, centralDirectorySize, entryCount };
}

function parseCentralDirectory(archive, end) {
  const centralEnd = end.centralDirectoryOffset + end.centralDirectorySize;
  const entries = [];
  let offset = end.centralDirectoryOffset;

  for (let index = 0; index < end.entryCount; index += 1) {
    if (
      readUInt32(archive, offset, 'Portable ZIP central-directory record') !==
      CENTRAL_DIRECTORY_SIGNATURE
    ) {
      fail('Portable ZIP has an invalid central-directory record.');
    }
    const fixedEnd = endOffset(archive, offset, 46, 'Portable ZIP central-directory record');
    const flags = readUInt16(archive, offset + 8, 'Portable ZIP entry flags');
    const compression = readUInt16(archive, offset + 10, 'Portable ZIP entry compression method');
    const expectedCrc = readUInt32(archive, offset + 16, 'Portable ZIP entry CRC-32');
    const compressedSize = readUInt32(archive, offset + 20, 'Portable ZIP compressed size');
    const uncompressedSize = readUInt32(archive, offset + 24, 'Portable ZIP uncompressed size');
    const nameLength = readUInt16(archive, offset + 28, 'Portable ZIP filename length');
    const extraLength = readUInt16(archive, offset + 30, 'Portable ZIP extra-field length');
    const commentLength = readUInt16(archive, offset + 32, 'Portable ZIP entry comment length');
    const diskStart = readUInt16(archive, offset + 34, 'Portable ZIP entry disk number');
    const localHeaderOffset = readUInt32(archive, offset + 42, 'Portable ZIP local-header offset');
    for (const [value, sentinel, label] of [
      [compressedSize, ZIP64_UINT32_SENTINEL, 'Portable ZIP compressed size'],
      [uncompressedSize, ZIP64_UINT32_SENTINEL, 'Portable ZIP uncompressed size'],
      [localHeaderOffset, ZIP64_UINT32_SENTINEL, 'Portable ZIP local-header offset'],
      [diskStart, ZIP64_UINT16_SENTINEL, 'Portable ZIP entry disk number'],
    ]) {
      rejectZip64Sentinel(value, sentinel, label);
    }
    if (diskStart !== 0) {
      fail('Portable ZIP entry is not on the first disk.');
    }
    assertSupportedFlags(flags, 'Portable ZIP entry');
    assertSupportedCompression(compression, 'Portable ZIP entry');

    const nameEnd = endOffset(archive, fixedEnd, nameLength, 'Portable ZIP filename');
    const extraEnd = endOffset(archive, nameEnd, extraLength, 'Portable ZIP central extra field');
    const recordEnd = endOffset(archive, extraEnd, commentLength, 'Portable ZIP entry comment');
    if (recordEnd > centralEnd) {
      fail('Portable ZIP central-directory record extends beyond its declared size.');
    }
    const name = decodeEntryName(
      archive.subarray(fixedEnd, nameEnd),
      flags,
      'Portable ZIP central filename',
    );
    validateCanonicalName(name, 'Portable ZIP central filename');
    validateExtraFields(archive.subarray(nameEnd, extraEnd), 'Portable ZIP central record');
    entries.push({
      compression,
      compressedSize,
      expectedCrc,
      flags,
      localHeaderOffset,
      name,
      uncompressedSize,
    });
    offset = recordEnd;
  }

  if (offset !== centralEnd) {
    fail('Portable ZIP central-directory count does not match its declared size.');
  }
  return entries;
}

function validateExactEntrySet(entries, expectedName) {
  const expected = validateCanonicalName(expectedName, 'Expected Portable ZIP entry');
  if (expected.directory || expectedName !== 'RenderPilot/renderpilot-desktop.exe') {
    fail('Portable ZIP must use the canonical RenderPilot/renderpilot-desktop.exe entry name.');
  }
  if (entries.length !== 1) {
    fail('Portable ZIP must contain exactly one canonical portable executable entry.');
  }
  const [payload] = entries;
  if (payload.name !== expectedName) {
    fail(`Portable ZIP must contain only ${expectedName}.`);
  }
  return payload;
}

function parseLocalEntries(archive, entries, centralDirectoryOffset) {
  const localOffsets = new Set();
  const ranges = [];

  for (const entry of entries) {
    const localOffset = entry.localHeaderOffset;
    if (localOffset >= centralDirectoryOffset) {
      fail(`Portable ZIP entry ${entry.name} local header crosses into the central directory.`);
    }
    if (localOffsets.has(localOffset)) {
      fail('Portable ZIP contains duplicate local-header offsets.');
    }
    localOffsets.add(localOffset);
    if (
      readUInt32(archive, localOffset, 'Portable ZIP local-header signature') !==
      LOCAL_FILE_HEADER_SIGNATURE
    ) {
      fail(`Portable ZIP entry ${entry.name} has an invalid local header.`);
    }
    const fixedEnd = endOffset(archive, localOffset, 30, 'Portable ZIP local header');
    const flags = readUInt16(archive, localOffset + 6, 'Portable ZIP local flags');
    const compression = readUInt16(
      archive,
      localOffset + 8,
      'Portable ZIP local compression method',
    );
    const expectedCrc = readUInt32(archive, localOffset + 14, 'Portable ZIP local CRC-32');
    const compressedSize = readUInt32(
      archive,
      localOffset + 18,
      'Portable ZIP local compressed size',
    );
    const uncompressedSize = readUInt32(
      archive,
      localOffset + 22,
      'Portable ZIP local uncompressed size',
    );
    const nameLength = readUInt16(archive, localOffset + 26, 'Portable ZIP local filename length');
    const extraLength = readUInt16(
      archive,
      localOffset + 28,
      'Portable ZIP local extra-field length',
    );
    assertSupportedFlags(flags, 'Portable ZIP local header');
    assertSupportedCompression(compression, 'Portable ZIP local header');
    if (
      flags !== entry.flags ||
      compression !== entry.compression ||
      expectedCrc !== entry.expectedCrc ||
      compressedSize !== entry.compressedSize ||
      uncompressedSize !== entry.uncompressedSize
    ) {
      fail(`Portable ZIP entry ${entry.name} has inconsistent local metadata.`);
    }
    const nameEnd = endOffset(archive, fixedEnd, nameLength, 'Portable ZIP local filename');
    const extraEnd = endOffset(archive, nameEnd, extraLength, 'Portable ZIP local extra field');
    const dataEnd = endOffset(archive, extraEnd, compressedSize, 'Portable ZIP entry data');
    if (dataEnd > centralDirectoryOffset) {
      fail(`Portable ZIP entry ${entry.name} crosses into the central directory.`);
    }
    const localName = decodeEntryName(
      archive.subarray(fixedEnd, nameEnd),
      flags,
      'Portable ZIP local filename',
    );
    validateCanonicalName(localName, 'Portable ZIP local filename');
    if (localName !== entry.name) {
      fail(`Portable ZIP entry ${entry.name} has inconsistent local filename metadata.`);
    }
    validateExtraFields(archive.subarray(nameEnd, extraEnd), 'Portable ZIP local record');
    ranges.push({ end: dataEnd, name: entry.name, start: localOffset });
  }

  ranges.sort((left, right) => left.start - right.start);
  let expectedStart = 0;
  for (const range of ranges) {
    if (range.start !== expectedStart) {
      fail(`Portable ZIP local entry layout has a gap or overlap before ${range.name}.`);
    }
    expectedStart = range.end;
  }
  if (expectedStart !== centralDirectoryOffset) {
    fail('Portable ZIP local entry layout does not end at the central directory.');
  }
}

export function extractZipEntry(archive, expectedName) {
  const end = parseEndOfCentralDirectory(archive);
  const entries = parseCentralDirectory(archive, end);
  const payload = validateExactEntrySet(entries, expectedName);
  parseLocalEntries(archive, entries, end.centralDirectoryOffset);

  const localOffset = payload.localHeaderOffset;
  const localNameLength = readUInt16(
    archive,
    localOffset + 26,
    'Portable ZIP local filename length',
  );
  const localExtraLength = readUInt16(
    archive,
    localOffset + 28,
    'Portable ZIP local extra-field length',
  );
  const dataStart = localOffset + 30 + localNameLength + localExtraLength;
  const dataEnd = dataStart + payload.compressedSize;
  const compressed = archive.subarray(dataStart, dataEnd);
  let contents;
  if (payload.compression === 0) {
    contents = compressed;
  } else {
    try {
      contents = inflateRawSync(compressed);
    } catch (error) {
      fail(`Portable ZIP entry ${expectedName} cannot be decompressed: ${error.message}`);
    }
  }

  if (contents.length !== payload.uncompressedSize) {
    fail(`Portable ZIP entry ${expectedName} has an unexpected uncompressed size.`);
  }
  if (crc32(contents) !== payload.expectedCrc) {
    fail(`Portable ZIP entry ${expectedName} failed its CRC-32 check.`);
  }
  return contents;
}

/** Extracts an exact canonical entry set for the stored portable RPU layout. */
export function extractExactZipEntries(archive, expectedNames) {
  if (
    !Array.isArray(expectedNames) ||
    expectedNames.length === 0 ||
    new Set(expectedNames).size !== expectedNames.length
  ) {
    fail('Expected ZIP entry names must be a non-empty unique list.');
  }
  const end = parseEndOfCentralDirectory(archive);
  const entries = parseCentralDirectory(archive, end);
  if (
    entries.length !== expectedNames.length ||
    entries.some((entry) => !expectedNames.includes(entry.name))
  ) {
    fail('Portable RPU must contain exactly its canonical manifest and App entries.');
  }
  if (entries.some((entry) => entry.compression !== 0)) {
    fail('Portable RPU entries must use stored ZIP compression.');
  }
  parseLocalEntries(archive, entries, end.centralDirectoryOffset);
  const extracted = new Map();
  for (const entry of entries) {
    const localOffset = entry.localHeaderOffset;
    const localNameLength = readUInt16(
      archive,
      localOffset + 26,
      'Portable RPU local filename length',
    );
    const localExtraLength = readUInt16(
      archive,
      localOffset + 28,
      'Portable RPU local extra-field length',
    );
    const dataStart = localOffset + 30 + localNameLength + localExtraLength;
    const contents = archive.subarray(dataStart, dataStart + entry.compressedSize);
    if (contents.length !== entry.uncompressedSize || crc32(contents) !== entry.expectedCrc) {
      fail(`Portable RPU entry ${entry.name} failed its stored-size or CRC validation.`);
    }
    extracted.set(entry.name, contents);
  }
  return extracted;
}
