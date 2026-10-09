import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Product compatibility entries share source without duplicating CLI policy.
// Imported entries remain side-effect free; only the actual executable forwards.
export async function forwardCli(entryUrl: string, toolUrl: URL) {
  if (!process.argv[1] || entryUrl !== pathToFileURL(resolve(process.argv[1])).href) return;
  await new Promise<void>(resolveRun => {
    const child = spawn(process.execPath, [...process.execArgv, fileURLToPath(toolUrl), ...process.argv.slice(2)], {
      stdio: 'inherit', shell: false,
    });
    const interrupt = () => child.kill('SIGINT');
    const terminate = () => child.kill('SIGTERM');
    process.on('SIGINT', interrupt);
    process.on('SIGTERM', terminate);
    const finish = (code: number | null) => {
      process.off('SIGINT', interrupt);
      process.off('SIGTERM', terminate);
      process.exitCode = code ?? 1;
      resolveRun();
    };
    child.once('error', () => {
      console.error('Shared tool could not start. Initialize the fixed framework submodule.');
      finish(1);
    });
    child.once('close', finish);
  });
}
