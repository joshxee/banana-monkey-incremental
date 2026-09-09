import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

/// A fresh hire delivers 47.5 s after it sets out and eats 2.5 s after that.
/// The cycle itself is pinned tick by tick in `src/sim_tests.rs`; this suite
/// only covers what needs a browser - the shop card under a pointer, and the
/// save in localStorage - and scales the clock so the reload test does not
/// wait out a real trip.
const CYCLE_SECONDS = 50;
const PAYLOAD = 5;
const MEAL = 1.5;

type Point = { x: number; y: number };
type Monkey = {
  x: number;
  y: number;
  segment: "to-grove" | "pick" | "to-depot" | "unload" | "snack";
  carrying: boolean;
};
type GameState = {
  ready: boolean;
  bananas: number;
  workers: number;
  nextCost: number;
  meal: number;
  canHire: boolean;
  committed: number;
  staff: Array<{
    role: string;
    owned: number;
    hungry: number;
    nextCost: number;
    gainPerMin: number;
    canHire: boolean;
  }>;
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
    hireChef: Point;
    hireUnpacker: Point;
    hireTechnologist: Point;
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
    const spawnPosition = (await state(page)).monkeys[0].x;

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
    // A random phase can land near the stall by chance, so retry a few resumes
    // to make this assertion reliable without exposing a test-only seed.
    let after: GameState | undefined;
    let atReload = saved;
    let resumedOffStall = false;
    for (let attempt = 0; attempt < 5; attempt += 1) {
      // Re-read immediately before *this* reload. The throttled save keeps
      // running between attempts, and a worker's meal settles a couple of
      // seconds after the delivery that funds it - so the value on disk when a
      // later attempt reloads is legitimately lower than the one captured
      // before the first. Comparing a fifth reload against the first snapshot
      // is what made this test flaky, not anything it was written to catch.
      atReload = await savedBananas(page);
      await page.reload();
      await waitForGame(page);

      await expect.poll(async () => (await state(page)).workers).toBe(1);
      await expect.poll(async () => (await state(page)).monkeys.length).toBe(1);

      after = await state(page);
      resumedOffStall ||=
        after.monkeys[0].segment !== "to-grove" ||
        after.monkeys[0].x < spawnPosition - 20;
      if (resumedOffStall) break;
    }

    // A band, not an equality. Booting the page back up costs a second or two
    // of wall clock, and at 25x that is whole cycles of production. A restored
    // worker can also settle its already-reserved meal before the bridge is
    // readable, putting the live balance exactly one meal below the save.
    // Fraction preservation is asserted separately below.
    expect(after!.bananas).toBeGreaterThanOrEqual(atReload! - MEAL);
    expect(after!.bananas).toBeLessThanOrEqual(atReload! + 5 * (PAYLOAD - MEAL));
    // The fraction is the point of the test: an integer save format would have
    // truncated it on the way out, whatever the magnitude.
    expect(saved! % 1).not.toBe(0);

    // Cycle phase is deliberately not persisted (see `persistence.rs`), but a
    // restored worker must still be placed somewhere in its cycle rather than
    // reset to the phase-zero stall position every time.
    expect(resumedOffStall).toBe(true);
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
