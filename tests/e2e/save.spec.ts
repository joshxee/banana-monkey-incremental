import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

/// What a run survives. Everything here needs a real browser: the save lives in
/// localStorage, and the promises the format makes - an older build's save
/// still opens, a newer build's save is not eaten, an unreadable one is kept
/// rather than overwritten - are all promises about that storage across a page
/// load. `src/persistence.rs` pins the encoding; this pins the storage.
const SAVE_KEY = "banana-monkey-incremental.save-v1";
const BACKUP_KEY = `${SAVE_KEY}.backup`;
const QUARANTINE_KEY = `${SAVE_KEY}.quarantine`;
/// A newer build's save is held apart from an unreadable one. Two different
/// accidents: a months-old corrupt payload must not be able to occupy the slot
/// that is protecting the run a player made yesterday.
const NEWER_KEY = `${SAVE_KEY}.newer`;

/// Below a minute away is a reload, not an absence. Comfortably past it.
const HOURS = 3600 * 1000;

type GameState = {
  ready: boolean;
  bananas: number;
  workers: number;
  menu: "closed" | "open" | "confirm-restart" | "welcome" | "confirm-import";
};

test.beforeEach(async ({ page }) => installDevicePixelContentBoxFix(page));

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

async function waitForGame(page: Page): Promise<void> {
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
}

async function slot(page: Page, key: string): Promise<string | null> {
  return page.evaluate((storageKey) => localStorage.getItem(storageKey), key);
}

/// Put a payload in the save slot and open the game on it. The slots are
/// cleared first so a previous test's quarantine cannot change the outcome.
///
/// The seed runs on the *first* navigation only, guarded through
/// `sessionStorage` exactly as `openFreshGame` guards its clear. An init script
/// runs on every navigation including `page.reload()`, so without the guard a
/// reload silently re-seeds the same save with the same timestamp - which makes
/// every "did it bank the absence twice" assertion pass regardless.
async function openWith(page: Page, save: string): Promise<void> {
  await page.addInitScript(
    ({ keys, payload, guard }) => {
      if (sessionStorage.getItem(guard) === "true") {
        return;
      }
      sessionStorage.setItem(guard, "true");
      for (const key of keys) {
        localStorage.removeItem(key);
      }
      localStorage.setItem(keys[0], payload);
    },
    {
      keys: [SAVE_KEY, BACKUP_KEY, QUARANTINE_KEY, NEWER_KEY],
      payload: save,
      guard: `${SAVE_KEY}.test-seeded`,
    },
  );
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await waitForGame(page);
  await expect.poll(async () => (await state(page)).ready).toBe(true);
}

async function reopen(page: Page): Promise<void> {
  await page.reload({ waitUntil: "domcontentloaded" });
  await waitForGame(page);
  await expect.poll(async () => (await state(page)).ready).toBe(true);
}

function save(fields: Record<string, unknown>): string {
  return JSON.stringify({ version: 4, ...fields });
}

test.describe("a run survives a new build", () => {
  test("a save from an older build keeps what that build had", async ({
    page,
  }) => {
    // The v2 schema, exactly as it shipped: no support staff, no carts, no
    // clock. It has to open, and its workforce has to be intact.
    await openWith(page, `{"version":2,"bananas":12.5,"workers":6}`);

    const opened = await state(page);
    expect(opened.workers).toBe(6);
    expect(opened.bananas).toBeGreaterThanOrEqual(12.5);
    // No clock in the payload, so no absence and no greeting.
    expect(opened.menu).toBe("closed");
  });

  test("a save from a newer build opens and is kept intact", async ({
    page,
  }) => {
    // A playtester who opens yesterday's build by mistake. The run has to
    // survive here, *and* the original has to be preserved so the newer build
    // can still have back the fields this one cannot represent.
    const future = JSON.stringify({
      version: 99,
      bananas: 480,
      workers: 5,
      zoos: 3,
      saved_at_ms: Date.now(),
    });
    await openWith(page, future);

    const opened = await state(page);
    expect(opened.workers).toBe(5);
    expect(opened.bananas).toBeGreaterThanOrEqual(480);
    expect(await slot(page, NEWER_KEY)).toBe(future);
    // And the player is told, rather than finding a field missing later.
    expect(opened.menu).toBe("welcome");
  });

  test("an unreadable save is kept rather than overwritten", async ({
    page,
  }) => {
    await openWith(page, "not json");

    expect((await state(page)).bananas).toBe(0);
    expect((await state(page)).menu).toBe("welcome");
    // The whole point: the run starts fresh, but the bytes that used to be
    // somebody's progress are still there to be recovered or sent in.
    expect(await slot(page, QUARANTINE_KEY)).toBe("not json");

    // And a fresh save writing over the top does not take the quarantine with
    // it. Playing on has to be safe.
    await page.locator("#banana-monkey-canvas").focus();
    await page.keyboard.press("KeyH");
    await expect.poll(async () => (await state(page)).bananas).toBe(1);
    await expect.poll(async () => slot(page, QUARANTINE_KEY)).toBe("not json");
  });

  test("the save that opened the session is backed up", async ({ page }) => {
    // One session of rollback, for a build that ruins a run while the player
    // is inside it.
    const original = save({ bananas: 200, workers: 3, saved_at_ms: Date.now() });
    await openWith(page, original);

    await expect.poll(async () => slot(page, BACKUP_KEY)).toBe(original);
  });
});

test.describe("time away", () => {
  test("a run is paid for the hours the tab was shut", async ({ page }) => {
    // Six monkeys for two hours. The rate is pinned exactly in
    // `src/domain.rs`; what this checks is that the clock in the save reaches
    // it at all across a real page load.
    await openWith(
      page,
      save({ bananas: 100, workers: 6, saved_at_ms: Date.now() - 2 * HOURS }),
    );

    const opened = await state(page);
    expect(opened.menu).toBe("welcome");
    expect(opened.bananas).toBeGreaterThan(100);
  });

  test("an absence is banked once and not again", async ({ page }) => {
    // The bug this guards is the expensive one: the absence is measured from
    // the timestamp in the save, so a save that is not rewritten immediately
    // pays the same night out on every reload for ever.
    await openWith(
      page,
      save({ bananas: 100, workers: 6, saved_at_ms: Date.now() - 2 * HOURS }),
    );
    const paid = (await state(page)).bananas;
    expect(paid).toBeGreaterThan(100);

    await reopen(page);

    const again = await state(page);
    // A second launch a moment later is a reload, not an absence. If the
    // credited run had not been written back with a fresh timestamp, this
    // launch would measure the same two hours all over again.
    expect(again.menu).toBe("closed");
    // Wages keep draining while the tab is open, so this cannot be an equality
    // - what it must never be is a second two hours' pay.
    expect(again.bananas).toBeLessThan(paid * 1.5);
  });

  test("a quick reload is not an absence", async ({ page }) => {
    await openWith(
      page,
      save({ bananas: 100, workers: 6, saved_at_ms: Date.now() - 5000 }),
    );

    const opened = await state(page);
    expect(opened.menu).toBe("closed");
    expect(opened.bananas).toBeLessThan(101);
  });

  test("a save with no clock is not paid for the epoch", async ({ page }) => {
    // Every pre-v4 save arrives with no timestamp. Reading that as "written in
    // 1970" would hand out fifty-six years of offline pay.
    await openWith(page, save({ bananas: 100, workers: 6 }));

    const opened = await state(page);
    expect(opened.menu).toBe("closed");
    expect(opened.bananas).toBeLessThan(101);
  });
});
