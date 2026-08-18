import type { Page } from "@playwright/test";

// Headless Chromium can expose devicePixelContentBoxSize in CSS pixels even when
// a device scale factor is emulated. Winit treats that API as physical pixels.
// Correct only the inconsistent response so browser tests match real devices.
export async function installDevicePixelContentBoxFix(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const descriptor = Object.getOwnPropertyDescriptor(
      ResizeObserverEntry.prototype,
      "devicePixelContentBoxSize",
    );
    const nativeGetter = descriptor?.get;
    if (!descriptor || !nativeGetter) {
      return;
    }

    Object.defineProperty(
      ResizeObserverEntry.prototype,
      "devicePixelContentBoxSize",
      {
        ...descriptor,
        get(this: ResizeObserverEntry): readonly ResizeObserverSize[] {
          const sizes = nativeGetter.call(this) as readonly ResizeObserverSize[];
          const first = sizes[0];
          const scale = window.devicePixelRatio;
          if (
            !first ||
            scale <= 1 ||
            Math.abs(first.inlineSize - this.contentRect.width) >= 1 ||
            Math.abs(first.blockSize - this.contentRect.height) >= 1
          ) {
            return sizes;
          }

          return Array.from(sizes, (size) => ({
            inlineSize: size.inlineSize * scale,
            blockSize: size.blockSize * scale,
          }));
        },
      },
    );
  });
}
