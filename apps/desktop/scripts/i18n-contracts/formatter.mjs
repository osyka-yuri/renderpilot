import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const OXFMT_CLI = path.join(
  path.dirname(fileURLToPath(import.meta.resolve('oxfmt/package.json'))),
  'bin',
  'oxfmt',
);

export function formatGeneratedSource(filePath, source, configPath, appRoot) {
  return new Promise((resolve, reject) => {
    const formatter = spawn(
      process.execPath,
      [OXFMT_CLI, `--config=${configPath}`, `--stdin-filepath=${filePath}`, '--threads=1'],
      {
        cwd: path.dirname(configPath),
        stdio: ['pipe', 'pipe', 'pipe'],
        windowsHide: true,
      },
    );
    let stdout = '';
    let stderr = '';

    formatter.stdout.setEncoding('utf8');
    formatter.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    formatter.stderr.setEncoding('utf8');
    formatter.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    formatter.once('error', (cause) => {
      reject(
        new Error(
          `i18n contract generation failed: could not start oxfmt for ${path.relative(appRoot, filePath)}`,
          { cause },
        ),
      );
    });
    formatter.once('close', (code, signal) => {
      if (code === 0) {
        resolve(stdout);
        return;
      }

      const reason = stderr.trim() || `exit code ${String(code)}, signal ${String(signal)}`;
      reject(
        new Error(
          `i18n contract generation failed: could not format ${path.relative(appRoot, filePath)}: ${reason}`,
        ),
      );
    });
    formatter.stdin.end(source, 'utf8');
  });
}
