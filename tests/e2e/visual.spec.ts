import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

type VisualState = {
  ready: boolean;
  bananas: number;
  interaction: string;
  menu: string;
};

test.beforeEach(async ({ page }) => installDevicePixelContentBoxFix(page));

async function state(page: Page): Promise<VisualState> {
  return page.evaluate(() => {
    const raw = (window as typeof window & {
      __BANANA_MONKEY_TEST_STATE__?: string;
    }).__BANANA_MONKEY_TEST_STATE__;
    if (!raw) {
      throw new Error("game test state is not ready");
    }
    return JSON.parse(raw) as VisualState;
  });
}

async function openScene(page: Page): Promise<void> {
  await page.addInitScript((key) => localStorage.removeItem(key), SAVE_KEY);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
  await page.locator("#banana-monkey-canvas").focus();
  await page.waitForTimeout(750);
}

const screenshotOptions = {
  animations: "disabled" as const,
  maxDiffPixels: 4_000,
  scale: "css" as const,
};

test("idle, deposited, and menu visuals", async ({ page }) => {
  await openScene(page);
  await expect(page).toHaveScreenshot("idle.png", screenshotOptions);

  await page.keyboard.press("h");
  await expect.poll(async () => (await state(page)).bananas).toBe(1);
  await expect.poll(async () => (await state(page)).interaction).toBe("idle");
  await expect(page).toHaveScreenshot("deposited.png", screenshotOptions);

  await page.keyboard.press("Escape");
  await expect.poll(async () => (await state(page)).menu).toBe("open");
  await expect(page).toHaveScreenshot("menu.png", screenshotOptions);

  await page.keyboard.press("l");
  await expect(page.locator("#banana-diagnostics-panel")).toBeVisible();
  await expect(page).toHaveScreenshot("input-logs.png", {
    ...screenshotOptions,
    mask: [page.locator("#banana-diagnostics-output")],
    maskColor: "#140b08",
  });
  await page.locator("#banana-diagnostics-close").click();
  await expect(page.locator("#banana-diagnostics-panel")).toBeHidden();
  expect((await state(page)).menu).toBe("open");
});

test("landscape visual", async ({ page }) => {
  await page.setViewportSize({ width: 844, height: 390 });
  await openScene(page);
  await expect(page).toHaveScreenshot("landscape.png", screenshotOptions);

  await page.keyboard.press("Escape");
  await expect.poll(async () => (await state(page)).menu).toBe("open");
  await expect(page).toHaveScreenshot("landscape-menu.png", screenshotOptions);
});
