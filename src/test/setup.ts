import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, beforeAll } from "vitest";

// Tauri's IPC generates callback ids via crypto.getRandomValues, which jsdom
// lacks. The snippet in Tauri's docs replaces window.crypto wholesale, which
// destroys randomUUID/subtle - both of which we will need for the planned
// AES-256-GCM work. Spread the original instead so only the gap is filled.
beforeAll(() => {
  const existing = globalThis.crypto ?? ({} as Crypto);
  if (typeof existing.getRandomValues !== "function") {
    Object.defineProperty(globalThis, "crypto", {
      configurable: true,
      value: {
        ...existing,
        getRandomValues: <T extends ArrayBufferView | null>(buffer: T): T => {
          if (buffer && "length" in buffer) {
            const view = buffer as unknown as Uint8Array;
            for (let i = 0; i < view.length; i++) {
              view[i] = Math.floor(Math.random() * 256);
            }
          }
          return buffer;
        },
      },
    });
  }
});

// mockIPC installs window.__TAURI_INTERNALS__ on the shared global, and Vitest
// does not hand each test a fresh window. Without this, a handler registered in
// one test silently serves the next one.
afterEach(() => {
  clearMocks();
  cleanup();
});
