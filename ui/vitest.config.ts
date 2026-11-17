import { defineConfig } from 'vitest/config';

// Vitest reads its configuration from this file's default export.
// oxlint-disable-next-line import/no-default-export
export default defineConfig({
  test: {
    // Only the view's data layers are tested: decoders and preparation functions, which are
    // plain TypeScript that needs no DOM. The drawing is not locked in by tests.
    include: ['apps/*/src/**/*.test.ts', 'packages/*/src/**/*.test.ts'],
    environment: 'node',
  },
});
