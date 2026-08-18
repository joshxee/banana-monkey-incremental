import { expect, type Page, test } from "@playwright/test";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

const SAVE_KEY = "banana-monkey-incremental.save-v1";

type Point = { x: number; y: number };
type Bounds = { min: Point; max: Point };
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
  monkeys: Array<{
    x: number;
    y: number;
    segment: "to-grove" | "pick" | "to-depot" | "unload";
    carrying: boolean;
  }>;
  interaction: "idle" | "dragging" | "keyboard-harvest";
  menu: "closed" | "open" | "confirm-restart";
  viewport: Point;
  activeTouches: number;
  touch: Point;
  banana: Point;
  harvest: Point;
  harvestBounds: Bounds;
  deposit: Point;
  buttons: {
    menu: Point;
    hireWorker: Point;
    resume: Point;
    restart: Point;
    confirmRestart: Point;
    cancelRestart: Point;
  };
};

test.beforeEach(async ({ page }) => installDevicePixelContentBoxFix(page));

async function savedBananas(page: Page): Promise<number | null> {
  const raw = await page.evaluate((key) => localStorage.getItem(key), SAVE_KEY);
  return raw === null ? null : (JSON.parse(raw).bananas as number);
}

async function savedWorkers(page: Page): Promise<number | null> {
  const raw = await page.evaluate((key) => localStorage.getItem(key), SAVE_KEY);
  return raw === null ? null : (JSON.parse(raw).workers as number);
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

async function waitForGame(page: Page): Promise<void> {
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
}

async function openFreshGame(page: Page): Promise<void> {
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
  await waitForGame(page);
  await page.locator("#banana-monkey-canvas").focus();
  expect((await state(page)).ready).toBe(true);
  expect((await state(page)).bananas).toBe(0);
  await expect
    .poll(async () => {
      const [current, canvas] = await Promise.all([
        state(page),
        page.locator("#banana-monkey-canvas").evaluate((element) => {
          const bounds = element.getBoundingClientRect();
          return { width: bounds.width, height: bounds.height };
        }),
      ]);
      return Math.max(
        Math.abs(current.viewport.x - canvas.width),
        Math.abs(current.viewport.y - canvas.height),
      );
    })
    .toBeLessThan(1);
}

async function keyboardHarvest(page: Page): Promise<void> {
  const before = (await state(page)).bananas;
  await page.keyboard.press("h");
  await expect.poll(async () => (await state(page)).bananas).toBe(before + 1);
  await expect.poll(async () => (await state(page)).interaction).toBe("idle");
}

async function mouseDrag(page: Page, from: Point, to: Point): Promise<void> {
  const viewport = (await state(page)).viewport;
  const clientFrom = await canvasPointToClient(page, from, viewport);
  const clientTo = await canvasPointToClient(page, to, viewport);
  await page.mouse.move(clientFrom.x, clientFrom.y);
  await page.mouse.down();
  await page.mouse.move(clientTo.x, clientTo.y, { steps: 12 });
  await page.mouse.up();
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
  const clientFrom = await canvasPointToClient(page, from, viewport);
  const clientTo = await canvasPointToClient(page, to, viewport);
  const canvasOrigin = await page
    .locator("#banana-monkey-canvas")
    .evaluate((canvas) => {
      const bounds = canvas.getBoundingClientRect();
      return { x: bounds.left, y: bounds.top };
    });
  const client = await page.context().newCDPSession(page);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ ...clientFrom, id: 1 }],
  });
  await expect.poll(async () => (await state(page)).activeTouches).toBe(1);
  await expect
    .poll(async () => {
      const touch = (await state(page)).touch;
      return Math.hypot(
        touch.x - (clientFrom.x - canvasOrigin.x),
        touch.y - (clientFrom.y - canvasOrigin.y),
      );
    })
    .toBeLessThan(1);
  await expect.poll(async () => (await state(page)).interaction).toBe("dragging");
  for (let step = 1; step <= 12; step += 1) {
    const progress = step / 12;
    await client.send("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [
        {
          x: clientFrom.x + (clientTo.x - clientFrom.x) * progress,
          y: clientFrom.y + (clientTo.y - clientFrom.y) * progress,
          id: 1,
        },
      ],
    });
  }
  await expect
    .poll(async () => {
      const current = await state(page);
      return Math.hypot(current.banana.x - to.x, current.banana.y - to.y);
    })
    .toBeLessThan(8);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await client.detach();
}

async function touchTap(page: Page, point: Point): Promise<void> {
  const viewport = (await state(page)).viewport;
  const clientPoint = await canvasPointToClient(page, point, viewport);
  const client = await page.context().newCDPSession(page);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ ...clientPoint, id: 1 }],
  });
  await page.waitForTimeout(250);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await client.detach();
}

test.describe("manual harvest", () => {
  test.beforeEach(async ({ page }) => openFreshGame(page));

  test("H harvest locks repeated input and persists after reload", async ({ page }) => {
    await page.keyboard.down("h");
    await page.keyboard.down("h");
    await page.keyboard.down("h");

    await expect.poll(async () => (await state(page)).bananas).toBe(1);
    await page.keyboard.up("h");
    await expect.poll(async () => savedBananas(page)).toBe(1);

    await page.reload();
    await waitForGame(page);
    await expect.poll(async () => (await state(page)).bananas).toBe(1);
  });

  test("mouse drag deposits once and invalid drop deposits nothing", async ({ page }, testInfo) => {
    test.skip(testInfo.project.name === "mobile", "mouse project only");

    const initial = await state(page);
    await mouseDrag(page, initial.banana, initial.deposit);
    await expect.poll(async () => (await state(page)).bananas).toBe(1);

    let reset = await state(page);
    await mouseDrag(page, reset.harvest, reset.deposit);
    await expect.poll(async () => (await state(page)).bananas).toBe(2);

    reset = await state(page);
    await mouseDrag(page, reset.banana, {
      x: reset.banana.x + 80,
      y: reset.banana.y - 100,
    });
    await expect.poll(async () => (await state(page)).interaction).toBe("idle");
    expect((await state(page)).bananas).toBe(2);
  });

  test("menu and confirmed restart clear the save", async ({ page }, testInfo) => {
    await keyboardHarvest(page);
    await page.keyboard.press("Escape");
    await expect.poll(async () => (await state(page)).menu).toBe("open");
    expect((await state(page)).bananas).toBe(1);

    let current = await state(page);
    if (testInfo.project.name.startsWith("mobile")) {
      await touchTap(page, current.buttons.restart);
    } else {
      await page.mouse.click(current.buttons.restart.x, current.buttons.restart.y);
    }
    await expect.poll(async () => (await state(page)).menu).toBe("confirm-restart");

    current = await state(page);
    if (testInfo.project.name.startsWith("mobile")) {
      await touchTap(page, current.buttons.confirmRestart);
    } else {
      await page.mouse.click(
        current.buttons.confirmRestart.x,
        current.buttons.confirmRestart.y,
      );
    }
    await expect.poll(async () => (await state(page)).bananas).toBe(0);
    await expect.poll(async () => (await state(page)).menu).toBe("closed");
    await expect.poll(async () => savedBananas(page)).toBe(0);
    await expect.poll(async () => savedWorkers(page)).toBe(0);
  });

  test("malformed save safely starts at zero", async ({ page }) => {
    await page.evaluate((key) => localStorage.setItem(key, "not json"), SAVE_KEY);
    await page.reload();
    await waitForGame(page);
    await expect.poll(async () => (await state(page)).bananas).toBe(0);
  });
});

test("touch drag deposits in portrait layout", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await openFreshGame(page);

  const initial = await state(page);
  expect(initial.banana.x).toBeLessThan(initial.deposit.x);
  await touchDrag(page, initial.banana, initial.deposit);

  await expect.poll(async () => (await state(page)).bananas).toBe(1);
});

test("touch outside the harvest bounds remains idle", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await openFreshGame(page);

  const initial = await state(page);
  const outside = await canvasPointToClient(
    page,
    {
      x: initial.harvestBounds.min.x - 2,
      y: initial.harvest.y,
    },
    initial.viewport,
  );
  const client = await page.context().newCDPSession(page);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ ...outside, id: 1 }],
  });

  await expect.poll(async () => (await state(page)).activeTouches).toBe(1);
  expect((await state(page)).interaction).toBe("idle");
  await client.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await client.detach();
  expect((await state(page)).bananas).toBe(0);
});

test("touch drag accepts the harvest zone on a phone", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await openFreshGame(page);

  const initial = await state(page);
  await touchDrag(page, initial.harvest, initial.deposit);

  await expect.poll(async () => (await state(page)).bananas).toBe(1);
});

test("responsive scene remains playable in landscape", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 844, height: 390 });
  await openFreshGame(page);

  const initial = await state(page);
  expect(initial.banana.x).toBeLessThan(initial.deposit.x);
  if (testInfo.project.name.startsWith("mobile")) {
    await touchDrag(page, initial.banana, initial.deposit);
  } else {
    await mouseDrag(page, initial.banana, initial.deposit);
  }
  await expect.poll(async () => (await state(page)).bananas).toBe(1);
});
