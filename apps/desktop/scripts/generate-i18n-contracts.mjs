import {
  checkI18nContractOutputs,
  createI18nContractOutputs,
  writeI18nContractOutputs,
} from './i18n-contracts.mjs';

const mode = process.argv[2];
if (mode !== '--check' && mode !== '--write') {
  throw new Error('Usage: node scripts/generate-i18n-contracts.mjs --check|--write');
}

const outputs = await createI18nContractOutputs();
if (mode === '--write') {
  await writeI18nContractOutputs(outputs);
  console.log(`Generated ${outputs.size} i18n contract files.`);
} else {
  const stale = await checkI18nContractOutputs(outputs);
  if (stale.length > 0) {
    console.error('Generated i18n contracts are stale:');
    for (const file of stale) {
      console.error(`- ${file}`);
    }
    console.error('Run `pnpm i18n:generate` and commit the results.');
    process.exitCode = 1;
  } else {
    console.log(`Verified ${outputs.size} generated i18n contract files.`);
  }
}
