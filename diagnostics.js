(() => {
  "use strict";

  const DIAGNOSTICS_VERSION = "banana-input-diag-v2";
  const MAX_ENTRIES = 200;
  const MOVE_SAMPLE_MS = 100;
  const CRITICAL_RUST_EVENTS = new Set([
    "touch_start",
    "touch_release",
    "touch_cancel",
    "touch_missing",
    "mouse_start",
    "mouse_release",
    "mouse_missing",
    "drag_begin",
    "input_blocked",
    "settlement",
  ]);
  const TERMINAL_RUST_EVENTS = new Set([
    "touch_release",
    "touch_cancel",
    "mouse_release",
    "mouse_missing",
    "settlement",
  ]);

  const entries = [];
  const pointers = new Map();
  const gesturesBySource = new Map();
  const browserMoveAt = new Map();
  const rustMoveAt = new Map();
  let sequence = 0;
  let gestureSequence = 0;
  let droppedEntries = 0;
  let droppedGestures = 0;
  let latestGestureId = null;
  let latestEnvironment = null;
  let panelOpen = false;
  let pendingPanelOpen = false;
  let previousFocus = null;

  const round = (value) =>
    Number.isFinite(value) ? Math.round(value * 100) / 100 : null;

  function targetName(target) {
    if (!(target instanceof Element)) {
      return "non-element";
    }
    if (target.id) {
      return `${target.tagName.toLowerCase()}#${target.id}`;
    }
    return target.tagName.toLowerCase();
  }

  function canvasElement() {
    const canvas = document.querySelector("#banana-monkey-canvas");
    return canvas instanceof HTMLCanvasElement ? canvas : null;
  }

  function canvasSnapshot() {
    const canvas = canvasElement();
    if (!canvas) {
      return { present: false };
    }
    const bounds = canvas.getBoundingClientRect();
    const style = getComputedStyle(canvas);
    return {
      present: true,
      bounds: {
        left: round(bounds.left),
        top: round(bounds.top),
        width: round(bounds.width),
        height: round(bounds.height),
      },
      backing: { width: canvas.width, height: canvas.height },
      client: { width: canvas.clientWidth, height: canvas.clientHeight },
      transform: style.transform,
      touchAction: style.touchAction,
    };
  }

  function viewportSnapshot() {
    const viewport = window.visualViewport;
    return {
      inner: { width: window.innerWidth, height: window.innerHeight },
      devicePixelRatio: round(window.devicePixelRatio),
      scroll: { x: round(window.scrollX), y: round(window.scrollY) },
      screen: {
        width: window.screen.width,
        height: window.screen.height,
        orientation: window.screen.orientation?.type ?? "unknown",
      },
      visualViewport: viewport
        ? {
            width: round(viewport.width),
            height: round(viewport.height),
            scale: round(viewport.scale),
            offsetLeft: round(viewport.offsetLeft),
            offsetTop: round(viewport.offsetTop),
            pageLeft: round(viewport.pageLeft),
            pageTop: round(viewport.pageTop),
          }
        : null,
      canvas: canvasSnapshot(),
    };
  }

  function trimEntries() {
    while (entries.length > MAX_ENTRIES) {
      const ungroupedNoise = entries.findIndex(
        (entry) => entry.gestureId === null && !entry.critical,
      );
      if (ungroupedNoise >= 0) {
        entries.splice(ungroupedNoise, 1);
        droppedEntries += 1;
        continue;
      }

      const activeGestures = new Set(
        Array.from(pointers.values(), (pointer) => pointer.gestureId),
      );
      for (const gestureId of gesturesBySource.values()) {
        activeGestures.add(gestureId);
      }
      if (latestGestureId !== null) {
        activeGestures.add(latestGestureId);
      }
      const oldGesture = entries.find(
        (entry) =>
          entry.gestureId !== null && !activeGestures.has(entry.gestureId),
      )?.gestureId;
      if (oldGesture !== undefined) {
        const before = entries.length;
        for (let index = entries.length - 1; index >= 0; index -= 1) {
          if (entries[index].gestureId === oldGesture) {
            entries.splice(index, 1);
          }
        }
        droppedEntries += before - entries.length;
        droppedGestures += 1;
        continue;
      }

      const removable = entries.findIndex((entry) => !entry.critical);
      entries.splice(removable >= 0 ? removable : 0, 1);
      droppedEntries += 1;
    }
  }

  function addEntry(
    source,
    event,
    data = {},
    critical = false,
    gestureId = null,
  ) {
    entries.push({
      sequence: ++sequence,
      performanceMs: round(performance.now()),
      source,
      event,
      critical,
      gestureId,
      data,
    });
    trimEntries();
    renderLog();
  }

  function exportReport() {
    return JSON.stringify(
      {
        diagnosticsVersion: DIAGNOSTICS_VERSION,
        exportedPerformanceMs: round(performance.now()),
        latestEnvironment,
        droppedEntries,
        droppedGestures,
        entryCount: entries.length,
        entries,
      },
      null,
      2,
    );
  }

  function clearGestureSampling(gestureId) {
    rustMoveAt.delete(`${gestureId}:touch_move`);
    rustMoveAt.delete(`${gestureId}:mouse_move`);
  }

  function forgetGestureSource(sourceKey, gestureId) {
    if (gesturesBySource.get(sourceKey) === gestureId) {
      gesturesBySource.delete(sourceKey);
    }
    clearGestureSampling(gestureId);
  }

  window.__BANANA_DIAG_PUSH__ = (event, message, inputSource = null) => {
    try {
      const eventName = String(event).slice(0, 80);
      const sourceKey =
        typeof inputSource === "string" ? inputSource.slice(0, 80) : null;
      const gestureId =
        sourceKey === null ? null : (gesturesBySource.get(sourceKey) ?? null);
      const isMove = eventName === "touch_move" || eventName === "mouse_move";
      const moveKey = `${gestureId ?? "ungrouped"}:${eventName}`;
      const now = performance.now();
      const firstMove = isMove && !rustMoveAt.has(moveKey);
      if (
        isMove &&
        !firstMove &&
        now - (rustMoveAt.get(moveKey) ?? 0) < MOVE_SAMPLE_MS
      ) {
        return;
      }
      if (isMove) {
        rustMoveAt.set(moveKey, now);
      }
      addEntry(
        "rust",
        eventName,
        { message: String(message).slice(0, 2000) },
        firstMove || CRITICAL_RUST_EVENTS.has(eventName),
        gestureId,
      );
      if (
        sourceKey !== null &&
        gestureId !== null &&
        TERMINAL_RUST_EVENTS.has(eventName)
      ) {
        queueMicrotask(() => forgetGestureSource(sourceKey, gestureId));
      }
    } catch (error) {
      console.warn("[BANANA-DIAG-v2] Rust bridge failed", error);
    }
  };
  window.__BANANA_DIAG_EXPORT__ = exportReport;
  window.__BANANA_DIAG_PANEL_OPEN__ = false;

  function coalescedCount(event) {
    try {
      return typeof event.getCoalescedEvents === "function"
        ? event.getCoalescedEvents().length
        : null;
    } catch (_error) {
      return -1;
    }
  }

  function eventIncludesCanvas(event, canvas) {
    try {
      return event.composedPath().includes(canvas);
    } catch (_error) {
      return event.target === canvas;
    }
  }

  function pointerSource(event) {
    return event.pointerType === "mouse"
      ? "mouse"
      : `${event.pointerType}:${event.pointerId}`;
  }

  function pointerData(event, pointer, canvas, startsOnCanvas, environment) {
    const bounds = canvas.getBoundingClientRect();
    const localX = event.clientX - bounds.left;
    const localY = event.clientY - bounds.top;
    const backingX = bounds.width
      ? localX * (canvas.width / bounds.width)
      : null;
    const backingY = bounds.height
      ? localY * (canvas.height / bounds.height)
      : null;
    return {
      pointerId: event.pointerId,
      pointerSequence: pointer?.pointerSequence ?? 1,
      pointerType: event.pointerType,
      isPrimary: event.isPrimary,
      target: targetName(event.target),
      composedPathIncludesCanvas: startsOnCanvas,
      client: { x: round(event.clientX), y: round(event.clientY) },
      canvasLocalCss: { x: round(localX), y: round(localY) },
      canvasBacking: { x: round(backingX), y: round(backingY) },
      buttons: event.buttons,
      button: event.button,
      pressure: round(event.pressure),
      cancelable: event.cancelable,
      defaultPrevented: event.defaultPrevented,
      coalescedCount: coalescedCount(event),
      canvasHasPointerCapture: canvas.hasPointerCapture(event.pointerId),
      environment,
    };
  }

  function showPendingPanel() {
    if (!pendingPanelOpen || pointers.size > 0) {
      return;
    }
    pendingPanelOpen = false;
    queueMicrotask(showPanel);
  }

  function observePointer(event) {
    if (panelOpen) {
      return;
    }
    const canvas = canvasElement();
    if (!canvas) {
      return;
    }

    if (event.type === "pointerdown") {
      const startsOnCanvas = eventIncludesCanvas(event, canvas);
      if (!startsOnCanvas) {
        return;
      }
      const environment = viewportSnapshot();
      latestEnvironment = environment;
      const gestureId = ++gestureSequence;
      const sourceKey = pointerSource(event);
      const previousGesture = gesturesBySource.get(sourceKey);
      if (previousGesture !== undefined) {
        clearGestureSampling(previousGesture);
      }
      const pointer = {
        gestureId,
        sourceKey,
        pointerSequence: 1,
        firstMoveSeen: false,
      };
      pointers.set(event.pointerId, pointer);
      gesturesBySource.set(sourceKey, gestureId);
      latestGestureId = gestureId;
      addEntry(
        "browser",
        event.type,
        pointerData(event, pointer, canvas, startsOnCanvas, environment),
        true,
        gestureId,
      );
      return;
    }

    const pointer = pointers.get(event.pointerId);
    if (!pointer) {
      return;
    }
    pointer.pointerSequence += 1;
    const terminal =
      event.type === "pointerup" ||
      event.type === "pointercancel" ||
      event.type === "lostpointercapture";
    const firstMove = event.type === "pointermove" && !pointer.firstMoveSeen;
    if (event.type === "pointermove") {
      const now = performance.now();
      const lastSample = browserMoveAt.get(event.pointerId) ?? 0;
      if (!firstMove && now - lastSample < MOVE_SAMPLE_MS) {
        return;
      }
      pointer.firstMoveSeen = true;
      browserMoveAt.set(event.pointerId, now);
    }

    addEntry(
      "browser",
      event.type,
      pointerData(event, pointer, canvas, true, undefined),
      firstMove || terminal,
      pointer.gestureId,
    );

    if (terminal) {
      pointers.delete(event.pointerId);
      browserMoveAt.delete(event.pointerId);
      setTimeout(
        () => forgetGestureSource(pointer.sourceKey, pointer.gestureId),
        1000,
      );
      showPendingPanel();
    }
  }

  for (const type of [
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointercancel",
    "lostpointercapture",
  ]) {
    window.addEventListener(type, observePointer, true);
  }

  function observeEnvironment(event) {
    const environment = viewportSnapshot();
    latestEnvironment = environment;
    addEntry(
      "browser",
      event,
      { visibility: document.visibilityState, environment },
      true,
    );
  }
  window.addEventListener("resize", () => observeEnvironment("resize"), true);
  window.addEventListener(
    "orientationchange",
    () => observeEnvironment("orientationchange"),
    true,
  );
  document.addEventListener(
    "visibilitychange",
    () => observeEnvironment("visibilitychange"),
    true,
  );

  const root = document.createElement("div");
  root.id = "banana-diagnostics";
  Object.assign(root.style, {
    position: "fixed",
    inset: "0",
    zIndex: "2147483647",
    pointerEvents: "none",
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
  });

  const panel = document.createElement("section");
  panel.id = "banana-diagnostics-panel";
  panel.hidden = true;
  panel.setAttribute("role", "dialog");
  panel.setAttribute("aria-modal", "true");
  panel.setAttribute("aria-labelledby", "banana-diagnostics-title");
  Object.assign(panel.style, {
    position: "absolute",
    inset: "0",
    display: "none",
    flexDirection: "column",
    gap: "8px",
    paddingTop: "calc(10px + env(safe-area-inset-top))",
    paddingRight: "calc(10px + env(safe-area-inset-right))",
    paddingBottom: "calc(10px + env(safe-area-inset-bottom))",
    paddingLeft: "calc(10px + env(safe-area-inset-left))",
    overflow: "hidden",
    background: "rgba(22, 12, 8, 0.97)",
    color: "#fff0b8",
    pointerEvents: "auto",
  });

  const title = document.createElement("h1");
  title.id = "banana-diagnostics-title";
  title.textContent = "INPUT LOGS";
  Object.assign(title.style, { margin: "0", fontSize: "20px" });

  const instructions = document.createElement("p");
  instructions.textContent =
    "CLEAR, CLOSE, RESUME, reproduce the problem, then return to Menu > Input Logs and COPY.";
  Object.assign(instructions.style, {
    margin: "0",
    font: "14px/1.3 system-ui, sans-serif",
  });

  const output = document.createElement("textarea");
  output.id = "banana-diagnostics-output";
  output.readOnly = true;
  output.wrap = "off";
  output.setAttribute("aria-label", "Diagnostic report");
  Object.assign(output.style, {
    flex: "1 1 auto",
    minHeight: "96px",
    width: "100%",
    resize: "none",
    overflow: "auto",
    border: "2px solid #fff0b8",
    borderRadius: "6px",
    background: "#140b08",
    color: "#f9e7a7",
    padding: "8px",
    font: "12px/1.35 ui-monospace, monospace",
  });

  const status = document.createElement("div");
  status.id = "banana-diagnostics-status";
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  Object.assign(status.style, { minHeight: "18px", fontSize: "13px" });

  const controls = document.createElement("div");
  Object.assign(controls.style, {
    display: "flex",
    flexWrap: "wrap",
    gap: "8px",
  });

  function controlButton(id, label) {
    const button = document.createElement("button");
    button.id = id;
    button.type = "button";
    button.textContent = label;
    Object.assign(button.style, {
      minHeight: "48px",
      flex: "1 1 100px",
      border: "2px solid #fff0b8",
      borderRadius: "8px",
      background: "#542414",
      color: "#fff0b8",
      font: "700 14px ui-monospace, monospace",
    });
    return button;
  }

  const copyButton = controlButton("banana-diagnostics-copy", "COPY");
  const downloadButton = controlButton(
    "banana-diagnostics-download",
    "DOWNLOAD",
  );
  const clearButton = controlButton("banana-diagnostics-clear", "CLEAR");
  const closeButton = controlButton("banana-diagnostics-close", "CLOSE");
  controls.append(copyButton, downloadButton, clearButton, closeButton);
  panel.append(title, instructions, output, status, controls);
  root.append(panel);
  document.body.append(root);

  function renderLog() {
    if (!panelOpen) {
      return;
    }
    output.value = exportReport();
    output.scrollTop = output.scrollHeight;
  }

  function showPanel() {
    if (panelOpen) {
      return;
    }
    panelOpen = true;
    pendingPanelOpen = false;
    window.__BANANA_DIAG_PANEL_OPEN__ = true;
    previousFocus = document.activeElement;
    const canvas = canvasElement();
    if (canvas) {
      canvas.inert = true;
      canvas.setAttribute("aria-hidden", "true");
    }
    panel.hidden = false;
    panel.style.display = "flex";
    status.textContent = "";
    renderLog();
    closeButton.focus();
    setTimeout(() => {
      if (panelOpen) {
        closeButton.focus();
      }
    }, 0);
  }

  function hidePanel() {
    if (!panelOpen) {
      return;
    }
    panelOpen = false;
    window.__BANANA_DIAG_PANEL_OPEN__ = false;
    panel.hidden = true;
    panel.style.display = "none";
    const canvas = canvasElement();
    if (canvas) {
      canvas.inert = false;
      canvas.removeAttribute("aria-hidden");
    }
    if (previousFocus instanceof HTMLElement && previousFocus.isConnected) {
      previousFocus.focus();
    } else {
      canvas?.focus();
    }
  }

  window.__BANANA_DIAG_OPEN__ = () => {
    if (panelOpen || pendingPanelOpen) {
      return;
    }
    pendingPanelOpen = true;
    showPendingPanel();
  };

  async function copyReport() {
    const report = exportReport();
    output.value = report;
    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("Clipboard API unavailable");
      }
      await navigator.clipboard.writeText(report);
      status.textContent = "Copied. Paste the report into chat.";
    } catch (_error) {
      output.focus();
      output.select();
      let copied = false;
      try {
        copied = document.execCommand("copy");
      } catch (_copyError) {
        copied = false;
      }
      status.textContent = copied
        ? "Copied. Paste the report into chat."
        : "Copy was blocked. Use your browser Copy command.";
    }
  }

  function downloadReport() {
    try {
      const blob = new Blob([exportReport()], { type: "application/json" });
      const objectUrl = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = objectUrl;
      link.download = "banana-input-diagnostics.json";
      document.body.append(link);
      link.click();
      link.remove();
      setTimeout(() => URL.revokeObjectURL(objectUrl), 5000);
      status.textContent = "Download requested.";
    } catch (_error) {
      status.textContent = "Download failed. Use COPY.";
    }
  }

  function clearReport() {
    entries.length = 0;
    sequence = 0;
    droppedEntries = 0;
    droppedGestures = 0;
    latestGestureId = null;
    gesturesBySource.clear();
    rustMoveAt.clear();
    status.textContent = "Log cleared. Close, resume, and reproduce the problem.";
    renderLog();
  }

  panel.addEventListener("pointerdown", (event) => event.stopPropagation());
  panel.addEventListener("click", (event) => event.stopPropagation());
  closeButton.addEventListener("click", hidePanel);
  copyButton.addEventListener("click", copyReport);
  downloadButton.addEventListener("click", downloadReport);
  clearButton.addEventListener("click", clearReport);

  window.addEventListener(
    "keydown",
    (event) => {
      if (!panelOpen) {
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopImmediatePropagation();
        hidePanel();
        return;
      }
      if (event.key !== "Tab") {
        return;
      }

      const focusable = [
        output,
        copyButton,
        downloadButton,
        clearButton,
        closeButton,
      ];
      const current = focusable.indexOf(document.activeElement);
      const direction = event.shiftKey ? -1 : 1;
      const next =
        (current + direction + focusable.length) % focusable.length;
      event.preventDefault();
      event.stopImmediatePropagation();
      focusable[next].focus();
    },
    true,
  );

  latestEnvironment = viewportSnapshot();
  addEntry(
    "browser",
    "session_start",
    {
      diagnosticsVersion: DIAGNOSTICS_VERSION,
      userAgent: navigator.userAgent,
      language: navigator.language,
      platform: navigator.platform,
      environment: latestEnvironment,
    },
    true,
  );
})();
