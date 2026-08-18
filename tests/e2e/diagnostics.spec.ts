import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

import { installDevicePixelContentBoxFix } from "./device-pixel-content-box";

type Point = { x: number; y: number };
type GameState = {
  ready: boolean;
  bananas: number;
  menu: "closed" | "open" | "confirm-restart";
  viewport: Point;
  activeTouches: number;
  harvest: Point;
  deposit: Point;
  buttons: {
    logs: Point;
    resume: Point;
  };
};
type DiagnosticEntry = {
  sequence: number;
  source: string;
  event: string;
  critical: boolean;
  gestureId: number | null;
  data: {
    message?: string;
    pointerId?: number;
  };
};
type DiagnosticReport = {
  diagnosticsVersion: string;
  droppedEntries: number;
  droppedGestures: number;
  entryCount: number;
  entries: DiagnosticEntry[];
};

test.beforeEach(async ({ page }) => installDevicePixelContentBoxFix(page));

async function waitForGame(page: Page): Promise<void> {
  await page.waitForFunction(
    () =>
      typeof (window as typeof window & {
        __BANANA_MONKEY_TEST_STATE__?: string;
      }).__BANANA_MONKEY_TEST_STATE__ === "string",
  );
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

async function realTouchDrag(
  page: Page,
  from: Point,
  to: Point,
  whilePressed?: () => Promise<void>,
): Promise<void> {
  const viewport = (await state(page)).viewport;
  const start = await canvasPointToClient(page, from, viewport);
  const finish = await canvasPointToClient(page, to, viewport);
  const client = await page.context().newCDPSession(page);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ ...start, id: 17 }],
  });
  await expect.poll(async () => (await state(page)).activeTouches).toBe(1);
  await whilePressed?.();
  for (let step = 1; step <= 8; step += 1) {
    const progress = step / 8;
    await client.send("Input.dispatchTouchEvent", {
      type: "touchMove",
      touchPoints: [
        {
          x: start.x + (finish.x - start.x) * progress,
          y: start.y + (finish.y - start.y) * progress,
          id: 17,
        },
      ],
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
  const clientPoint = await canvasPointToClient(page, point, viewport);
  const client = await page.context().newCDPSession(page);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ ...clientPoint, id: 23 }],
  });
  await expect.poll(async () => (await state(page)).activeTouches).toBe(1);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await expect.poll(async () => (await state(page)).activeTouches).toBe(0);
  await client.detach();
}

async function report(page: Page): Promise<DiagnosticReport> {
  return page.evaluate(() => {
    const exportReport = (window as typeof window & {
      __BANANA_DIAG_EXPORT__?: () => string;
    }).__BANANA_DIAG_EXPORT__;
    if (!exportReport) {
      throw new Error("diagnostic export bridge is unavailable");
    }
    return JSON.parse(exportReport()) as DiagnosticReport;
  });
}

test("normal play keeps diagnostics hidden until opened from the menu", async ({ page }) => {
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await waitForGame(page);

  await expect(page.locator("#banana-diagnostics")).toHaveCount(1);
  await expect(page.locator("#banana-diagnostics-panel")).toBeHidden();
  await expect(page.locator("#banana-diagnostics-button")).toHaveCount(0);
  expect(
    await page.evaluate(
      () =>
        typeof (window as typeof window & {
          __BANANA_DIAG_PUSH__?: unknown;
        }).__BANANA_DIAG_PUSH__,
    ),
  ).toBe("function");
  expect(
    await page.evaluate(
      () =>
        typeof (window as typeof window & {
          __BANANA_DIAG_OPEN__?: unknown;
        }).__BANANA_DIAG_OPEN__,
    ),
  ).toBe("function");
});

test("diagnostics keep simultaneous pointer sources in separate gestures", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await waitForGame(page);

  await page.evaluate(() => {
    const canvas = document.querySelector("#banana-monkey-canvas");
    if (!(canvas instanceof HTMLCanvasElement)) {
      throw new Error("game canvas is unavailable");
    }
    const push = (window as typeof window & {
      __BANANA_DIAG_PUSH__?: (
        event: string,
        message: string,
        inputSource?: string,
      ) => void;
    }).__BANANA_DIAG_PUSH__!;
    const emit = (type: string, pointerId: number) =>
      canvas.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          clientX: 100 + pointerId,
          clientY: 300,
          pointerId,
          pointerType: "touch",
        }),
      );

    emit("pointerdown", 41);
    push("touch_start", "source=touch:41", "touch:41");
    emit("pointerdown", 42);
    push("touch_start", "source=touch:42", "touch:42");
    emit("pointerup", 41);
    for (let index = 0; index < 250; index += 1) {
      push("test_noise", `source=touch:42 noise=${index}`, "touch:42");
    }
    push("touch_start", "source=touch:41 after-terminal", "touch:41");
    emit("pointerup", 42);
  });

  const captured = await report(page);
  const first = captured.entries.find(
    (entry) => entry.data.message === "source=touch:41",
  );
  const second = captured.entries.find(
    (entry) => entry.data.message === "source=touch:42",
  );
  const firstAfterTerminal = captured.entries.find(
    (entry) => entry.data.message === "source=touch:41 after-terminal",
  );
  const down41 = captured.entries.find(
    (entry) => entry.event === "pointerdown" && entry.data.pointerId === 41,
  );
  const down42 = captured.entries.find(
    (entry) => entry.event === "pointerdown" && entry.data.pointerId === 42,
  );
  expect(first?.gestureId).toBe(down41?.gestureId);
  expect(firstAfterTerminal?.gestureId).toBe(down41?.gestureId);
  expect(second?.gestureId).toBe(down42?.gestureId);
  expect(first?.gestureId).not.toBe(second?.gestureId);
  expect(captured.entryCount).toBe(200);
  expect(captured.droppedEntries).toBeGreaterThan(0);
});

test("menu diagnostics capture and export one real touch gesture", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await waitForGame(page);

  expect(
    await page.evaluate(
      () =>
        typeof (window as typeof window & {
          __BANANA_DIAG_PUSH__?: unknown;
        }).__BANANA_DIAG_PUSH__,
    ),
  ).toBe("function");

  const initial = await state(page);
  await realTouchDrag(page, initial.harvest, initial.deposit, async () => {
    await page.evaluate(() => {
      const push = (window as typeof window & {
        __BANANA_DIAG_PUSH__?: (
          event: string,
          message: string,
          inputSource?: string,
        ) => void;
      }).__BANANA_DIAG_PUSH__!;
      for (let index = 0; index < 250; index += 1) {
        push("test_noise", `held-touch-entry=${index}`, "touch:17");
      }
    });
  });
  await expect.poll(async () => (await state(page)).bananas).toBe(1);

  const captured = await report(page);
  const browserDown = captured.entries.filter(
    (entry) => entry.source === "browser" && entry.event === "pointerdown",
  );
  const browserTerminal = captured.entries.filter(
    (entry) =>
      entry.source === "browser" &&
      ["pointerup", "pointercancel", "lostpointercapture"].includes(entry.event),
  );
  const rustStart = captured.entries.find(
    (entry) => entry.source === "rust" && entry.event === "touch_start",
  );
  const rustSettlement = captured.entries.find(
    (entry) => entry.source === "rust" && entry.event === "settlement",
  );
  expect(browserDown).toHaveLength(1);
  expect(browserTerminal).toHaveLength(1);
  expect(rustStart).toBeDefined();
  expect(rustSettlement).toBeDefined();
  expect(browserDown[0].sequence).toBeLessThan(rustStart!.sequence);
  expect(captured.entryCount).toBe(200);
  expect(captured.droppedEntries).toBeGreaterThan(0);

  await page.keyboard.press("Escape");
  await expect.poll(async () => (await state(page)).menu).toBe("open");
  await touchTap(page, (await state(page)).buttons.logs);
  await expect(page.locator("#banana-diagnostics-panel")).toBeVisible();
  expect((await state(page)).activeTouches).toBe(0);
  await expect(page.locator("#banana-diagnostics-close")).toBeFocused();
  expect(
    await page.locator("#banana-monkey-canvas").evaluate((canvas) => ({
      inert: canvas.inert,
      ariaHidden: canvas.getAttribute("aria-hidden"),
    })),
  ).toEqual({ inert: true, ariaHidden: "true" });
  await page.keyboard.press("Tab");
  await expect(page.locator("#banana-diagnostics-output")).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(page.locator("#banana-diagnostics-close")).toBeFocused();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  await expect(page.locator("#banana-diagnostics-copy")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("#banana-diagnostics-status")).toContainText(
    /Copied|blocked/,
  );
  await page.keyboard.press("h");
  await page.waitForTimeout(500);
  expect((await state(page)).bananas).toBe(1);

  await page.locator("#banana-diagnostics-copy").click();
  await expect(page.locator("#banana-diagnostics-status")).toContainText(
    /Copied|blocked/,
  );

  await page.locator("#banana-diagnostics-output").evaluate((output) => {
    (window as typeof window & {
      __BANANA_COPY_DEFAULT_PREVENTED__?: boolean;
    }).__BANANA_COPY_DEFAULT_PREVENTED__ = true;
    output.addEventListener(
      "keydown",
      (event) => {
        if (event.ctrlKey && event.key.toLowerCase() === "c") {
          (window as typeof window & {
            __BANANA_COPY_DEFAULT_PREVENTED__?: boolean;
          }).__BANANA_COPY_DEFAULT_PREVENTED__ = event.defaultPrevented;
        }
      },
    );
  });
  await page.locator("#banana-diagnostics-output").focus();
  await page.keyboard.press("Control+c");
  expect(
    await page.evaluate(
      () =>
        (window as typeof window & {
          __BANANA_COPY_DEFAULT_PREVENTED__?: boolean;
        }).__BANANA_COPY_DEFAULT_PREVENTED__,
    ),
  ).toBe(false);

  const downloadPromise = page.waitForEvent("download");
  await page.locator("#banana-diagnostics-download").click();
  const download = await downloadPromise;
  const downloadPath = await download.path();
  expect(downloadPath).not.toBeNull();
  const downloaded = JSON.parse(
    await readFile(downloadPath!, "utf8"),
  ) as DiagnosticReport;
  expect(downloaded.entries.some((entry) => entry.source === "browser")).toBe(true);
  expect(downloaded.entries.some((entry) => entry.source === "rust")).toBe(true);

  await page.locator("#banana-diagnostics-close").click();
  await expect(page.locator("#banana-diagnostics-panel")).toBeHidden();
  await expect(page.locator("#banana-monkey-canvas")).toBeFocused();
  expect(
    await page.locator("#banana-monkey-canvas").evaluate((canvas) => canvas.inert),
  ).toBe(false);
  expect((await state(page)).menu).toBe("open");
  await page.evaluate(() => {
    const push = (window as typeof window & {
      __BANANA_DIAG_PUSH__?: (event: string, message: string) => void;
    }).__BANANA_DIAG_PUSH__!;
    for (let index = 0; index < 250; index += 1) {
      push("test_noise", `entry=${index}`);
    }
  });
  const capped = await report(page);
  expect(capped.entryCount).toBe(200);
  expect(capped.droppedEntries).toBeGreaterThan(0);
  expect(
    capped.entries.some(
      (entry) => entry.source === "browser" && entry.event === "pointerdown",
    ),
  ).toBe(true);
  expect(
    capped.entries.some(
      (entry) => entry.source === "rust" && entry.event === "touch_start",
    ),
  ).toBe(true);
  expect(
    capped.entries.some(
      (entry) => entry.source === "rust" && entry.event === "touch_release",
    ),
  ).toBe(true);
  expect(
    capped.entries.some(
      (entry) => entry.source === "rust" && entry.event === "settlement",
    ),
  ).toBe(true);

  await touchTap(page, (await state(page)).buttons.logs);
  await page.locator("#banana-diagnostics-clear").click();
  expect((await report(page)).entryCount).toBe(0);

  await page.keyboard.press("Escape");
  await expect(page.locator("#banana-diagnostics-panel")).toBeHidden();
  expect(
    await page.evaluate(
      () =>
        typeof (window as typeof window & {
          __BANANA_DIAG_PUSH__?: unknown;
        }).__BANANA_DIAG_PUSH__,
    ),
  ).toBe("function");

  await touchTap(page, (await state(page)).buttons.resume);
  await expect.poll(async () => (await state(page)).menu).toBe("closed");
  await page.keyboard.press("h");
  await expect.poll(async () => (await state(page)).bananas).toBe(2);
  const keyboardSettlement = (await report(page)).entries.findLast(
    (entry) =>
      entry.source === "rust" &&
      entry.event === "settlement" &&
      entry.data.message?.includes("source=Keyboard"),
  );
  expect(keyboardSettlement?.gestureId).toBeNull();
});

test("landscape menu opens diagnostics after the initiating touch ends", async ({ page }, testInfo) => {
  test.skip(!testInfo.project.name.startsWith("mobile"), "touch project only");
  await page.setViewportSize({ width: 844, height: 390 });
  await page.goto("/", { waitUntil: "domcontentloaded" });
  await waitForGame(page);

  await page.keyboard.press("Escape");
  await expect.poll(async () => (await state(page)).menu).toBe("open");
  await touchTap(page, (await state(page)).buttons.logs);

  await expect(page.locator("#banana-diagnostics-panel")).toBeVisible();
  expect((await state(page)).activeTouches).toBe(0);
});
