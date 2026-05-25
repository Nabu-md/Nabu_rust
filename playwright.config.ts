import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false, // Electron tests must run serially — one app instance per worker
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1, // Electron tests require single-worker execution
  reporter: 'html',
  timeout: 30_000,
  use: {
    trace: 'on-first-retry'
  }
  // No 'projects' block — Electron E2E tests use the _electron API directly,
  // not a browser project.  The default project (no name) handles all specs.
})
