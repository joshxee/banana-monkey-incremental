import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

type Point = { x: number; y: number };
type UiState = {
  bananas: number;
  workers: number;
  interaction: string;
  storeExpanded: boolean;
  selectedTab: "MONKEYS" | "BUILDINGS" | "RESEARCH";
  storeScroll: number;
  viewport: Point;
  harvest: Point;
  deposit: Point;
  buttons: {
    hireWorker: Point;
    toggleStore: Point;
    previousTab: Point;
    nextTab: Point;
  };
};

test.beforeEach(async ({ page }) => installDevicePixelContentBoxFix(page));

async function state(page: Page): Promise<UiState> {
  return page.evaluate(() => {
    const raw = (window as typeof window & {
      __BANANA_MONKEY_TEST_STATE__?: string;
    }).__BANANA_MONKEY_TEST_STATE__;
    if (!raw) throw new Error("game test state is not ready");
    return JSON.parse(raw) as UiState;
  });
}

async function openFreshGame(page: Page): Promise<void> {
  await page.addInitScript((key) => localStorage.removeItem(key), SAVE_KEY);
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
  await page.locator("#banana-monkey-canvas").focus();
}

async function canvasPointToClient(
  page: Page,
  point: Point,
  viewport: Point,
): Promise<Point> {
  return page.locator("#banana-monkey-canvas").evaluate((canvas, source) => {
    const bounds = canvas.getBoundingClientRect();
    return {
      x: bounds.left + source.point.x * (bounds.width / source.viewport.x),
      y: bounds.top + source.point.y * (bounds.height / source.viewport.y),
    };
  }, { point, viewport });
}

async function touchDrag(page: Page, from: Point, to: Point): Promise<void> {
  const viewport = (await state(page)).viewport;
  const start = await canvasPointToClient(page, from, viewport);
  const finish = await canvasPointToClient(page, to, viewport);
  const client = await page.context().newCDPSession(page);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ ...start, id: 31 }],
  });
  for (let step = 1; step <= 10; step += 1) {
    const progress = step / 10;
    await client.send("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [{
        x: start.x + (finish.x - start.x) * progress,
        y: start.y + (finish.y - start.y) * progress,
        id: 31,
      }],
    });
  }
  await client.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await client.detach();
}

async function touchTap(page: Page, point: Point): Promise<void> {
  const viewport = (await state(page)).viewport;
  const client = await canvasPointToClient(page, point, viewport);
  await page.touchscreen.tap(client.x, client.y);
}

test("shop arrows and keyboard visit every tab without changing the economy", async ({ page }) => {
  await openFreshGame(page);
  const initial = await state(page);

  await page.mouse.click(initial.buttons.nextTab.x, initial.buttons.nextTab.y);
  await expect.poll(async () => (await state(page)).selectedTab).toBe("BUILDINGS");
  await page.keyboard.press("ArrowRight");
  await expect.poll(async () => (await state(page)).selectedTab).toBe("RESEARCH");
  await page.keyboard.press("ArrowLeft");
  await expect.poll(async () => (await state(page)).selectedTab).toBe("BUILDINGS");

  const after = await state(page);
  expect(after.bananas).toBe(initial.bananas);
  expect(after.workers).toBe(initial.workers);
});

test("horizontal tab swipe and vertical list scroll have separate intent", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await openFreshGame(page);

  const initial = await state(page);
  const tabCenter = {
    x: (initial.buttons.previousTab.x + initial.buttons.nextTab.x) * 0.5,
    y: initial.buttons.nextTab.y,
  };
  await touchDrag(page, { x: tabCenter.x + 55, y: tabCenter.y }, {
    x: tabCenter.x - 55,
    y: tabCenter.y + 3,
  });
  await expect.poll(async () => (await state(page)).selectedTab).toBe("BUILDINGS");
  expect((await state(page)).workers).toBe(0);

  await page.keyboard.press("ArrowLeft");
  await expect.poll(async () => (await state(page)).selectedTab).toBe("MONKEYS");
  for (let index = 0; index < 4; index += 1) {
    const before = (await state(page)).bananas;
    await page.keyboard.press("h");
    await expect.poll(async () => (await state(page)).bananas).toBe(before + 1);
  }

  const affordable = await state(page);
  await touchDrag(
    page,
    affordable.buttons.hireWorker,
    { x: affordable.buttons.hireWorker.x + 3, y: affordable.buttons.hireWorker.y - 120 },
  );
  await expect.poll(async () => (await state(page)).storeScroll).toBeGreaterThan(0);
  const after = await state(page);
  expect(after.selectedTab).toBe("MONKEYS");
  expect(after.workers).toBe(0);
  expect(after.bananas).toBe(affordable.bananas);
});

test("expanded drawer owns touches over the covered world", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await openFreshGame(page);

  for (let index = 0; index < 4; index += 1) {
    const before = (await state(page)).bananas;
    await page.keyboard.press("h");
    await expect.poll(async () => (await state(page)).bananas).toBe(before + 1);
  }

  const collapsed = await state(page);
  await touchTap(page, collapsed.buttons.toggleStore);
  await expect.poll(async () => (await state(page)).storeExpanded).toBe(true);

  const expanded = await state(page);
  await touchDrag(page, expanded.harvest, expanded.deposit);
  await expect.poll(async () => (await state(page)).interaction).toBe("idle");
  expect((await state(page)).bananas).toBe(expanded.bananas);

  const beforeButtonDrag = await state(page);
  await touchDrag(
    page,
    { x: beforeButtonDrag.buttons.hireWorker.x, y: 150 },
    beforeButtonDrag.buttons.hireWorker,
  );
  const afterButtonDrag = await state(page);
  expect(afterButtonDrag.workers).toBe(0);
  expect(afterButtonDrag.bananas).toBe(beforeButtonDrag.bananas);
});
