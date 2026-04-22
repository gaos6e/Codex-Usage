import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['tests/unit/**/*.test.ts'],
    coverage: {
      reporter: ['text', 'html'],
      include: ['src/shared/**/*.ts', 'src/main/services/**/*.ts'],
      exclude: ['src/main/services/logger.ts'],
    },
  },
});
