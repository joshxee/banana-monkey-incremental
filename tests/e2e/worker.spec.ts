import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

/// A worker delivers every 47.5 s, and its starting phase is jittered across the
/// whole cycle, so the wait for a first delivery is bounded by one cycle plus
/// the hand-harvesting that pays for the hire.
const CYCLE_SECONDS = 47.5;

type Point = { x: number; y: number };
type Monkey = {
  x: number;
  y: number;
  segment: "to-grove" | "pick" | "to-depot" | "unload";
  carrying: boolean;
};
type GameState = {
  ready: boolean;
  bananas: number;
  workers: number;
  nextCost: number;
  hireRequired: number;
  canHire: boolean;
  grossPerSec: number;
  wagesPerSec: number;
  netPerSec: number;
  monkeys: Monkey[];
  menu: string;
  viewport: Point;
  harvest: Point;
  deposit: Point;
  buttons: {
    hireWorker: Point;
    restart: Point;
    confirmRestart: Point;
  };
};

test.beforeEach(async ({ page }) => installDevicePixelContentBoxFix(page));

async function touchTap(page: Page, point: Point): Promise<void> {
  const viewport = (await state(page)).viewport;
  const client = await page.context().newCDPSession(page);
  const target = await page
    .locator("#banana-monkey-canvas")
    .evaluate((canvas, source) => {
      const bounds = canvas.getBoundingClientRect();
      return {
        x: bounds.left + source.point.x * (bounds.width / source.viewport.x),
        y: bounds.top + source.point.y * (bounds.height / source.viewport.y),
      };
    }, { point, viewport });
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ ...target, id: 1 }],
  });
  await page.waitForTimeout(200);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await client.detach();
}

async function state(page: Page): Promise<GameState> {
  return page.evaluate(() => {
    const raw = (window as typeof window & {
      __BANANA_MONKEY_TEST_STATE__?: string;
    }).__BANANA_MONKEY_TEST_STATE__;
    if (!raw) {
      throw new Error("game test state is not ready");
    }
    return JSON.parse(raw) as GameState;
  });
}

async function openFreshGame(page: Page): Promise<void> {
  // Guarded, because init scripts re-run on every navigation: clearing
  // unconditionally would wipe the save the reload test is there to check.
  await page.addInitScript(
    ({ saveKey, guardKey }) => {
      if (sessionStorage.getItem(guardKey) !== "true") {
        localStorage.removeItem(saveKey);
        sessionStorage.setItem(guardKey, "true");
      }
    },
    { saveKey: SAVE_KEY, guardKey: `${SAVE_KEY}.test-cleared` },
  );
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
  await page.locator("#banana-monkey-canvas").focus();
}

/// Hand-harvest until the shop reports the hire is affordable. The gate is
/// cost + wage reserve, not cost alone.
async function harvestUntilAffordable(page: Page): Promise<void> {
  for (let attempt = 0; attempt < 40; attempt += 1) {
    if ((await state(page)).canHire) {
      return;
    }
    const before = (await state(page)).bananas;
    await page.keyboard.press("h");
    await expect.poll(async () => (await state(page)).bananas).toBe(before + 1);
  }
  throw new Error("never became affordable");
}

test.describe("worker monkey", () => {
  test.beforeEach(async ({ page }) => openFreshGame(page));

  test("hiring spawns a monkey that walks the route and delivers", async ({
    page,
  }) => {
    // One full cycle, plus harvesting time and browser slack.
    test.setTimeout(160_000);

    const start = await state(page);
    expect(start.workers).toBe(0);
    expect(start.monkeys).toHaveLength(0);
    // The first hire costs 4 but needs 6.85: cost plus a 2.85 wage reserve, so
    // the treasury cannot be spent to exactly zero.
    expect(start.nextCost).toBeCloseTo(4, 6);
    expect(start.hireRequired).toBeCloseTo(6.85, 6);
    expect(start.canHire).toBe(false);

    await harvestUntilAffordable(page);
    const beforeHire = await state(page);
    expect(beforeHire.bananas).toBeGreaterThanOrEqual(beforeHire.hireRequired);

    await page.keyboard.press("b");

    // The monkey exists, and the treasury paid exactly the quoted price.
    await expect.poll(async () => (await state(page)).workers).toBe(1);
    await expect.poll(async () => (await state(page)).monkeys.length).toBe(1);
    const afterHire = await state(page);
    const debited = beforeHire.bananas - beforeHire.nextCost;
    // Not exact: wages start draining the moment the worker exists, so the
    // balance is the debit less a little feeding.
    expect(afterHire.bananas).toBeLessThanOrEqual(debited);
    expect(afterHire.bananas).toBeGreaterThan(debited - 0.5);
    // The reserve is what keeps this above zero rather than in the red.
    expect(afterHire.bananas).toBeGreaterThan(0);

    // Invariant I1: the hire raises net, and the readout can show all three.
    expect(afterHire.grossPerSec).toBeCloseTo(5 / CYCLE_SECONDS, 6);
    expect(afterHire.wagesPerSec).toBeCloseTo(0.03, 6);
    expect(afterHire.netPerSec).toBeGreaterThan(0);

    // It is on the route between the two zones, standing on the ground.
    const monkey = afterHire.monkeys[0];
    expect(monkey.x).toBeGreaterThan(afterHire.harvest.x);
    expect(monkey.x).toBeLessThan(afterHire.deposit.x);

    // It moves without any further input.
    const startX = monkey.x;
    await expect
      .poll(async () => Math.abs((await state(page)).monkeys[0].x - startX), {
        timeout: 20_000,
      })
      .toBeGreaterThan(4);

    // And it eventually hands over a full payload of 5 at the stall.
    const beforeDelivery = (await state(page)).bananas;
    await expect
      .poll(async () => (await state(page)).bananas, {
        timeout: (CYCLE_SECONDS + 12) * 1000,
        intervals: [500],
      })
      .toBeGreaterThan(beforeDelivery + 3);

    // Between deliveries the pile shrinks: the monkey is being fed.
    const settled = (await state(page)).bananas;
    await page.waitForTimeout(3_000);
    expect((await state(page)).bananas).toBeLessThan(settled);
  });

  test("a monkey only carries a banana on the way back", async ({ page }) => {
    test.setTimeout(160_000);

    await harvestUntilAffordable(page);
    await page.keyboard.press("b");
    await expect.poll(async () => (await state(page)).workers).toBe(1);

    // Sample the whole cycle and check the carried banana never contradicts the
    // segment: empty on the way out and while picking, loaded on the way home.
    const seen = new Set<string>();
    const deadline = Date.now() + (CYCLE_SECONDS + 8) * 1000;
    while (Date.now() < deadline) {
      const monkey = (await state(page)).monkeys[0];
      seen.add(monkey.segment);
      const shouldCarry =
        monkey.segment === "to-depot" || monkey.segment === "unload";
      expect(monkey.carrying).toBe(shouldCarry);
      await page.waitForTimeout(250);
    }

    // A full round trip visits all four segments.
    expect([...seen].sort()).toEqual(["pick", "to-depot", "to-grove", "unload"]);
  });

  test("tapping the shop card hires exactly one worker", async ({
    page,
  }, testInfo) => {
    test.setTimeout(120_000);

    // Every other hire in this suite goes through the B shortcut, but tapping
    // the card is the only route a touch player has.
    await harvestUntilAffordable(page);
    const before = await state(page);
    expect(before.canHire).toBe(true);

    if (testInfo.project.name.startsWith("mobile")) {
      await touchTap(page, before.buttons.hireWorker);
    } else {
      await page.mouse.click(
        before.buttons.hireWorker.x,
        before.buttons.hireWorker.y,
      );
    }

    await expect.poll(async () => (await state(page)).workers).toBe(1);
    // Exactly one. `handle_menu` gathers presses from bevy's `Interaction` and
    // from a manual touch hit-test, and those can resolve on different frames,
    // so one tap must not buy two workers.
    await page.waitForTimeout(1_000);
    expect((await state(page)).workers).toBe(1);
    await expect.poll(async () => (await state(page)).monkeys.length).toBe(1);
  });

  test("the shop button refuses a hire the player cannot afford", async ({
    page,
  }, testInfo) => {
    test.setTimeout(120_000);

    // 4 bananas covers the 4.0 price but not the 2.85 wage reserve.
    for (let i = 0; i < 4; i += 1) {
      const before = (await state(page)).bananas;
      await page.keyboard.press("h");
      await expect.poll(async () => (await state(page)).bananas).toBe(before + 1);
    }
    const blocked = await state(page);
    expect(blocked.bananas).toBeGreaterThanOrEqual(blocked.nextCost);
    expect(blocked.canHire).toBe(false);

    if (testInfo.project.name.startsWith("mobile")) {
      await touchTap(page, blocked.buttons.hireWorker);
    } else {
      await page.mouse.click(
        blocked.buttons.hireWorker.x,
        blocked.buttons.hireWorker.y,
      );
    }
    await page.waitForTimeout(500);

    expect((await state(page)).workers).toBe(0);
    expect((await state(page)).bananas).toBe(blocked.bananas);
  });

  test("workers and a fractional treasury survive a reload", async ({ page }) => {
    test.setTimeout(120_000);

    await harvestUntilAffordable(page);
    await page.keyboard.press("b");
    await expect.poll(async () => (await state(page)).workers).toBe(1);

    // Let wages make the treasury fractional, which the old integer save
    // format would have silently truncated on the way out.
    await page.waitForTimeout(2_000);
    const before = await state(page);
    expect(before.bananas % 1).not.toBe(0);

    await page.reload();
    await page.waitForFunction(
      () =>
        typeof (window as typeof window & {
          __BANANA_MONKEY_TEST_STATE__?: string;
        }).__BANANA_MONKEY_TEST_STATE__ === "string",
    );

    await expect.poll(async () => (await state(page)).workers).toBe(1);
    await expect.poll(async () => (await state(page)).monkeys.length).toBe(1);
    // Within a banana: the save is throttled to a 5 s cadence, so the last
    // fraction of a second of wages may not have reached storage.
    const after = await state(page);
    expect(Math.abs(after.bananas - before.bananas)).toBeLessThan(1);
  });

  test("restart dismisses every worker, and the scrim covers the shop", async ({
    page,
  }, testInfo) => {
    test.skip(testInfo.project.name !== "desktop", "mouse project only");
    test.setTimeout(120_000);

    await harvestUntilAffordable(page);
    await page.keyboard.press("b");
    await expect.poll(async () => (await state(page)).workers).toBe(1);

    await page.keyboard.press("Escape");
    await expect.poll(async () => (await state(page)).menu).toBe("open");

    // The shop card sits underneath the scrim. Clicking where it is must not
    // reach it - the touch hit-test iterates every button regardless of what
    // is actually on screen.
    const menu = await state(page);
    await page.mouse.click(menu.buttons.hireWorker.x, menu.buttons.hireWorker.y);
    await page.waitForTimeout(300);
    expect((await state(page)).workers).toBe(1);
    // And pressing B while paused must not hire either.
    await page.keyboard.press("b");
    await page.waitForTimeout(300);
    expect((await state(page)).workers).toBe(1);

    let current = await state(page);
    await page.mouse.click(current.buttons.restart.x, current.buttons.restart.y);
    await expect.poll(async () => (await state(page)).menu).toBe("confirm-restart");
    current = await state(page);
    await page.mouse.click(
      current.buttons.confirmRestart.x,
      current.buttons.confirmRestart.y,
    );

    await expect.poll(async () => (await state(page)).workers).toBe(0);
    await expect.poll(async () => (await state(page)).monkeys.length).toBe(0);
    await expect.poll(async () => (await state(page)).bananas).toBe(0);
    // With nobody to feed, the wage bill is zero again.
    await expect.poll(async () => (await state(page)).wagesPerSec).toBe(0);
  });
});
