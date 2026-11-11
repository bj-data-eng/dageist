// Copies the static view files into dist/ after `tsc` has compiled the
// TypeScript sources there. Run through `npm run build`.
import { cpSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
cpSync(join(root, 'public'), join(root, 'dist'), { recursive: true });
