import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

/// Fresh hires start at the stall at phase zero, so a new hire delivers 47.5 s
/// later and eats 2.5 s after that. Restored workers are phased independently.
const CYCLE_SECONDS = 50;
const DELIVERY_AT_SECONDS = 47.5;
const PAYLOAD = 5;
const MEAL = 1.5;

type Point = { x: number; y: number };
type Monkey = {
  x: number;
  y: number;
  segment: "to-grove" | "pick" | "to-depot" | "unload" | "snack";
  carrying: boolean;
  hungry: boolean;
};
type GameState = {
  ready: boolean;
  bananas: number;
  workers: number;
  nextCost: number;
  meal: number;
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

async function savedBananas(page: Page): Promise<number | null> {
  const raw = await page.evaluate((key) => localStorage.getItem(key), SAVE_KEY);
  return raw === null ? null : (JSON.parse(raw).bananas as number);
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

/// How much faster than real time the simulation runs, unless a test asks for
/// real time. A worker's cycle is 50 *simulated* seconds either way - the tick
/// count and every economic figure are identical - so this only shortens the
/// wall clock, from 50 seconds a cycle to about two.
const FAST = 25;

async function openFreshGame(page: Page, speed = FAST): Promise<void> {
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
  await page.goto(speed === 1 ? "/" : `/?speed=${speed}`, {
    waitUntil: "domcontentloaded",
  });
  await waitForGame(page);
  await page.locator("#banana-monkey-canvas").focus();
}

async function waitForGame(page: Page): Promise<void> {
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
}

/// Hand-harvest until the shop reports the hire is affordable. The gate is the
/// signing fee and nothing else: a worker is fed out of its own deliveries, so
/// there is no wage reserve stacked on top of the quoted price.
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


  test("hiring spawns a monkey that walks the route and delivers @slow", async ({
    page,
  }) => {
    // One full cycle at real speed, plus harvesting time and browser slack.
    // Deliberately the only test that does: everything else scales the clock,
    // so this one is what proves the cycle really is 47.5 seconds to a delivery
    // and that the scaling is a test convenience rather than a balance change.
    test.setTimeout(160_000);
    await openFreshGame(page, 1);

    const start = await state(page);
    expect(start.workers).toBe(0);
    expect(start.monkeys).toHaveLength(0);
    // The price on the button is the whole requirement. It used to quote 4.0
    // and enforce 6.85, which is the confusion this cycle was redesigned around.
    expect(start.nextCost).toBeCloseTo(4, 6);
    expect(start.meal).toBeCloseTo(MEAL, 6);
    expect(start.canHire).toBe(false);

    await harvestUntilAffordable(page);
    const beforeHire = await state(page);
    expect(beforeHire.bananas).toBeGreaterThanOrEqual(beforeHire.nextCost);

    await page.keyboard.press("b");

    // The monkey exists, and the treasury paid exactly the quoted price -
    // exactly, because nothing drains between the hire and the first delivery.
    await expect.poll(async () => (await state(page)).workers).toBe(1);
    await expect.poll(async () => (await state(page)).monkeys.length).toBe(1);
    const afterHire = await state(page);
    expect(afterHire.bananas).toBeCloseTo(
      beforeHire.bananas - beforeHire.nextCost,
      6,
    );

    // Invariant I1: the hire raises net, and the readout can show all three.
    expect(afterHire.grossPerSec).toBeCloseTo(PAYLOAD / CYCLE_SECONDS, 6);
    expect(afterHire.wagesPerSec).toBeCloseTo(0.03, 6);
    expect(afterHire.netPerSec).toBeGreaterThan(0);

    // It starts at the stall and heads out empty-handed, which is what makes
    // the purchase legible: the click produces a monkey walking out of the shop.
    const monkey = afterHire.monkeys[0];
    expect(monkey.segment).toBe("to-grove");
    expect(monkey.carrying).toBe(false);
    expect(monkey.x).toBeGreaterThan(afterHire.harvest.x);
    expect(monkey.x).toBeLessThan(afterHire.deposit.x);
    expect(afterHire.deposit.x - monkey.x).toBeLessThan(
      (afterHire.deposit.x - afterHire.harvest.x) / 2,
    );

    // It walks towards the grove without any further input.
    const startX = monkey.x;
    await expect
      .poll(async () => (await state(page)).monkeys[0].x, { timeout: 20_000 })
      .toBeLessThan(startX - 4);

    // A full payload lands at the stall, and the treasury never dipped on the
    // way there: the trip itself costs nothing.
    const beforeDelivery = (await state(page)).bananas;
    await expect
      .poll(async () => (await state(page)).bananas, {
        timeout: (DELIVERY_AT_SECONDS + 15) * 1000,
        intervals: [250],
      })
      .toBeCloseTo(beforeDelivery + PAYLOAD, 6);

    // Then, a couple of seconds later, it eats its wage out of what it just
    // delivered - the visible dip that replaced a continuous invisible drain.
    await expect
      .poll(async () => (await state(page)).bananas, {
        timeout: 15_000,
        intervals: [250],
      })
      .toBeCloseTo(beforeDelivery + PAYLOAD - MEAL, 6);

    // Net of one full trip is strictly positive and the balance never went
    // below where the hire left it.
    expect((await state(page)).bananas).toBeGreaterThan(beforeDelivery);
  });

  test("a monkey only carries a banana on the way back", async ({ page }) => {
    test.setTimeout(90_000);
    // Much slower than the rest: this samples every segment, and Unload and
    // Snack are 2.5 simulated seconds each. At 25x that is a tenth of a real
    // second; even at 5x it is half a second, which a loaded machine steps
    // over. At 2x they are 1.25 s apiece and the sampler cannot miss them.
    const speed = 2;
    await openFreshGame(page, speed);

    await harvestUntilAffordable(page);
    await page.keyboard.press("b");
    await expect.poll(async () => (await state(page)).workers).toBe(1);

    // Sample the whole cycle and check the carried banana never contradicts the
    // segment: empty on the way out and while picking, loaded on the way home.
    const seen = new Set<string>();
    const deadline = Date.now() + (CYCLE_SECONDS / speed + 8) * 1000;
    while (Date.now() < deadline) {
      const monkey = (await state(page)).monkeys[0];
      seen.add(monkey.segment);
      // Held through the snack too: that banana is the meal, and seeing it in
      // hand is what connects the counter's dip to the monkey that caused it.
      const shouldCarry = ["to-depot", "unload", "snack"].includes(
        monkey.segment,
      );
      expect(monkey.carrying).toBe(shouldCarry);
      expect(monkey.hungry).toBe(false);
      await page.waitForTimeout(100);
    }

    // A full round trip visits all five segments.
    expect([...seen].sort()).toEqual([
      "pick",
      "snack",
      "to-depot",
      "to-grove",
      "unload",
    ]);
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

    // Three bananas is genuinely short of the 4.0 price - the button is greyed
    // because the player cannot pay, not because of a hidden second charge.
    for (let i = 0; i < 3; i += 1) {
      const before = (await state(page)).bananas;
      await page.keyboard.press("h");
      await expect.poll(async () => (await state(page)).bananas).toBe(before + 1);
    }
    const blocked = await state(page);
    expect(blocked.bananas).toBeLessThan(blocked.nextCost);
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
    test.setTimeout(90_000);

    await harvestUntilAffordable(page);
    await page.keyboard.press("b");
    await expect.poll(async () => (await state(page)).workers).toBe(1);

    // Wait out a full trip so the monkey's meal makes the treasury fractional,
    // which the old integer save format would have silently truncated, and then
    // wait again for the throttled save to carry that fraction to disk.
    await expect
      .poll(async () => (await state(page)).bananas % 1, {
        timeout: (CYCLE_SECONDS / FAST + 15) * 1000,
        intervals: [100],
      })
      .not.toBe(0);
    await expect
      .poll(async () => (await savedBananas(page))! % 1, {
        timeout: 15_000,
        intervals: [100],
      })
      .not.toBe(0);
    const saved = await savedBananas(page);
    expect(saved! % 1).not.toBe(0);

    // `reload` keeps the query string, so the restored run stays fast too.
    await page.reload();
    await waitForGame(page);

    await expect.poll(async () => (await state(page)).workers).toBe(1);
    await expect.poll(async () => (await state(page)).monkeys.length).toBe(1);

    // A band, not an equality. Booting the page back up costs a second or two
    // of wall clock, and at 25x that is whole cycles of production, so the live
    // balance has legitimately moved on before it can be read. What the band
    // still catches is the failure this test exists for: a truncating save
    // restores *below* what it recorded, and production cannot make `after`
    // start out smaller than `saved`.
    const after = await state(page);
    expect(after.bananas).toBeGreaterThanOrEqual(saved!);
    expect(after.bananas).toBeLessThanOrEqual(saved! + 5 * (PAYLOAD - MEAL));

    // The restored worker is walking, not stranded. Cycle phase is deliberately
    // *not* persisted (see `persistence.rs`), so it restarts its trip from the
    // stall - the check is that it is alive and productive, not that it resumed
    // mid-route.
    await expect
      .poll(async () => (await state(page)).monkeys[0].hungry, { timeout: 5_000 })
      .toBe(false);
    await expect
      .poll(async () => (await state(page)).bananas, {
        timeout: (CYCLE_SECONDS / FAST + 15) * 1000,
        intervals: [100],
      })
      .toBeGreaterThan(after.bananas);
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
