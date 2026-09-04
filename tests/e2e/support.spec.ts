import { expect, type Page, test } from "@playwright/test";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

type Point = { x: number; y: number };
type GameState = {
  ready: boolean;
  bananas: number;
  workers: number;
  canHire: boolean;
  avatarsDrawn: number;
  storeExpanded: boolean;
  staff: Array<{
    role: string;
    owned: number;
    nextCost: number;
    canHire: boolean;
  }>;
  buttons: {
    hireChef: Point;
    toggleStore: Point;
  };
};

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
  await page.addInitScript((key) => localStorage.removeItem(key), SAVE_KEY);
  // No `?speed=` here, unlike the worker suite: that scales the harvest
  // *cycle* clock so a test does not wait out a real 47.5 s delivery. This
  // test never waits on a cycle - hand-harvesting is instant per press either
  // way - so there is nothing for it to buy, and it is one less variable
  // between this repro and a real player's session.
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
  await page.locator("#banana-monkey-canvas").focus();
}

test.describe("support staff", () => {
  test.beforeEach(async ({ page }) => openFreshGame(page));

  test("a chef hired before any worker still gets an avatar", async ({
    page,
  }) => {
    // Regression guard for support rendering. A player who buys a Chef before
    // a Worker must still get a visible lo-fi avatar, even though the support
    // simulation and its bounded presentation pool are separate populations.
    const start = await state(page);
    expect(start.workers).toBe(0);
    expect(start.avatarsDrawn).toBe(0);

    // Hand-harvest to the Chef's base price without ever pressing "b". Paced,
    // not spammed: a keyboard harvest drives the same drag-and-drop
    // interaction a touch would, and firing "h" faster than that settles
    // mostly gets swallowed rather than banking extra bananas.
    while ((await state(page)).bananas < 25) {
      await page.keyboard.press("h");
      await page.waitForTimeout(20);
    }
    expect((await state(page)).workers).toBe(0);
    // Clear of the last harvest's own input-suppression window before the
    // shop gets a click of its own.
    await page.waitForTimeout(300);

    // The collapsed drawer's default height shows the Worker row in full and
    // only the top sliver of the Chef row beneath it - the shop's own
    // `Overflow::clip_y`. `buttons.hireChef` reports the button's true
    // center, which sits inside that clipped-away remainder and is not
    // actually reachable by a click until the drawer is pulled open, exactly
    // as a real player would have to.
    const collapsed = await state(page);
    await page.mouse.click(collapsed.buttons.toggleStore.x, collapsed.buttons.toggleStore.y);
    await expect.poll(async () => (await state(page)).storeExpanded).toBe(true);

    const before = await state(page);
    const chef = before.staff.find((role) => role.role === "CHEF")!;
    expect(chef.canHire).toBe(true);

    await page.mouse.click(before.buttons.hireChef.x, before.buttons.hireChef.y);

    await expect
      .poll(async () => (await state(page)).staff.find((role) => role.role === "CHEF")!.owned)
      .toBe(1);
    // The economic entity exists the instant the purchase lands; the avatar
    // pool reconciles on the very next `Update`, so this settles within a
    // couple of frames rather than a fixed-step tick.
    await expect.poll(async () => (await state(page)).avatarsDrawn).toBeGreaterThan(0);
    expect((await state(page)).workers).toBe(0);
  });
});
