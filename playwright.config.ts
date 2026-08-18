import { defineConfig } from "@playwright/test";

const executablePath = process.env.PLAYWRIGHT_CHROMIUM_PATH;
const port = Number(process.env.PLAYWRIGHT_PORT ?? "5190");

if (!Number.isInteger(port) || port < 1024 || port > 65_535) {
  throw new Error("PLAYWRIGHT_PORT must be an integer from 1024 through 65535");
}

const runRoot = `test-results/${port}`;

export default defineConfig({
  testDir: "./tests/e2e",
  outputDir: `${runRoot}/playwright`,
  timeout: 90_000,
  expect: {
    timeout: 15_000,
  },
  fullyParallel: false,
  workers: 1,
  forbidOnly: true,
  reporter: "list",
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    browserName: "chromium",
    launchOptions: executablePath ? { executablePath } : undefined,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command:
      `CARGO_TARGET_DIR=target/playwright-e2e-${port} trunk serve --dist ${runRoot}/trunk-dist --port ${port} --no-autoreload=true --watch src --watch assets --watch Cargo.toml --watch index.html --watch diagnostics.js`,
    url: `http://127.0.0.1:${port}`,
    reuseExistingServer: false,
    timeout: 600_000,
  },
  projects: [
    {
      name: "desktop",
      use: {
        viewport: { width: 1280, height: 720 },
      },
    },
    {
      name: "mobile",
      use: {
        viewport: { width: 390, height: 844 },
        hasTouch: true,
        isMobile: true,
        deviceScaleFactor: 2,
      },
    },
    {
      name: "mobile-fractional-dpr",
      testMatch: /harvest\.spec\.ts/,
      use: {
        viewport: { width: 475, height: 653 },
        hasTouch: true,
        isMobile: true,
        deviceScaleFactor: 2.625,
      },
    },
    {
      name: "mobile-dpr3",
      grep: /touch drag accepts the harvest zone on a phone/,
      use: {
        viewport: { width: 390, height: 844 },
        hasTouch: true,
        isMobile: true,
        deviceScaleFactor: 3,
      },
    },
  ],
});
