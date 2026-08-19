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
    // `--features test-hooks` builds in the `?speed=` clock scale that lets a
    // spec watch a 50-second harvest cycle in a couple of real seconds. The
    // feature is off by default, so the released build has no such switch.
    command:
      `CARGO_TARGET_DIR=target/playwright-e2e-${port} trunk serve --features test-hooks --dist ${runRoot}/trunk-dist --port ${port} --no-autoreload=true --watch src --watch assets --watch Cargo.toml --watch index.html --watch diagnostics.js`,
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
      // Emulated touch at a fractional DPR is by far the slowest project here:
      // every `mouse.move` step is a CDP round trip against a re-rendering wgpu
      // canvas, and the three-drag test measures 1.9 min on an idle machine. It
      // was passing under the 90 s default with no headroom at all, so it failed
      // the moment the box was busy. The tests are not doing anything they
      // should not; the budget was simply wrong for this project.
      timeout: 240_000,
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
