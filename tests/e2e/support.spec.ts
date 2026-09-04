import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

type Point = { x: number; y: number };
type GameState = {
  ready: boolean;
  bananas: number;
  workers: number;
  canHire: boolean;
  avatarsDrawn: number;
  storeScroll: number;
  staff: Array<{
    role: string;
    owned: number;
    nextCost: number;
    canHire: boolean;
  }>;
  buttons: {
    hireWorker: Point;
    hireChef: Point;
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

// Mouse wheel events do not register on a `hasTouch`/`isMobile` emulated
// page - the mobile project needs a real touch-drag gesture instead, the
// same one a phone player would use.
async function touchDrag(page: Page, from: Point, to: Point): Promise<void> {
  const viewport = (await page.evaluate(() =>
    JSON.parse(
      (window as typeof window & { __BANANA_MONKEY_TEST_STATE__?: string })
        .__BANANA_MONKEY_TEST_STATE__!,
    )
  )).viewport as Point;
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
  await client.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });
  await client.detach();
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
  test.beforeEach(async ({ page }) => {
    await installDevicePixelContentBoxFix(page);
    await openFreshGame(page);
  });

  test("a chef hired before any worker still gets an avatar", async ({
    page,
  }, testInfo) => {
    // The mobile project's touch-drag scroll is ten CDP round trips against a
    // re-rendering wgpu canvas on top of the hand-harvest loop below - the
    // same combination `harvest.spec.ts` gives `mobile-fractional-dpr` a
    // 240 s budget for. The 90 s default has no headroom left for it.
    test.setTimeout(180_000);

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

    // The store's resting height shows the Worker row in full and only the
    // top sliver of the Chef row beneath it - the panel's own
    // `Overflow::clip_y`. `buttons.hireChef` reports the button's true
    // center, which sits inside that clipped-away remainder and is not
    // actually reachable by a click until the list is scrolled, exactly as a
    // real player would have to.
    const collapsed = await state(page);
    if (testInfo.project.name.startsWith("mobile")) {
      await touchDrag(
        page,
        collapsed.buttons.hireWorker,
        { x: collapsed.buttons.hireWorker.x, y: collapsed.buttons.hireWorker.y - 120 },
      );
    } else {
      await page.mouse.move(collapsed.buttons.hireWorker.x, collapsed.buttons.hireWorker.y);
      await page.mouse.wheel(0, 100);
    }
    await expect.poll(async () => (await state(page)).storeScroll).toBeGreaterThan(0);

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
