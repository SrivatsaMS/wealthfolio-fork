import { afterEach, describe, expect, it } from "vitest";

import { captureShellReturn, isAppOrigin, shellReturnOrigin } from "./shell-return";

describe("server mode shell return", () => {
  afterEach(() => {
    localStorage.clear();
    window.history.replaceState(null, "", "/");
  });

  it("accepts only the app's own origins", () => {
    expect(isAppOrigin("tauri://localhost")).toBe(true);
    expect(isAppOrigin("http://tauri.localhost")).toBe(true);
    expect(isAppOrigin("https://evil.example.com")).toBe(false);
    expect(isAppOrigin(null)).toBe(false);
  });

  it("captures the return origin and cleans the address bar", () => {
    window.history.replaceState(null, "", "/?wf_shell=tauri%3A%2F%2Flocalhost&x=1");
    captureShellReturn();
    expect(shellReturnOrigin()).toBe("tauri://localhost");
    expect(window.location.search).toBe("?x=1");
  });

  it("ignores a foreign origin", () => {
    window.history.replaceState(null, "", "/?wf_shell=https%3A%2F%2Fevil.example.com");
    captureShellReturn();
    expect(shellReturnOrigin()).toBeNull();
    expect(window.location.search).toBe("");
  });
});
