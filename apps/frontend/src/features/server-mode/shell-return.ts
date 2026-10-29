/**
 * Inside the desktop/mobile app's server mode, the server's web app is told
 * the app's own origin (`?wf_shell=`) so it can offer "Disconnect this app".
 * Only the app origins Tauri uses are accepted, so the parameter cannot be
 * used to redirect elsewhere.
 */
const SHELL_PARAM = "wf_shell";
const STORAGE_KEY = "wealthfolio.serverMode.shellOrigin";
const APP_ORIGINS = ["tauri://localhost", "http://tauri.localhost", "https://tauri.localhost"];

export function isAppOrigin(value: string | null | undefined): value is string {
  return !!value && APP_ORIGINS.includes(value);
}

/** Reads and removes `?wf_shell=` from the address bar; call once at startup. */
export function captureShellReturn(location: Location = window.location): void {
  const url = new URL(location.href);
  const origin = url.searchParams.get(SHELL_PARAM);
  if (origin === null) return;
  try {
    if (isAppOrigin(origin)) localStorage.setItem(STORAGE_KEY, origin);
  } catch {
    // Storage unavailable: the disconnect button is simply not offered.
  }
  url.searchParams.delete(SHELL_PARAM);
  window.history.replaceState(window.history.state, "", url.toString());
}

/** The app origin this page was opened from, if it runs inside the app. */
export function shellReturnOrigin(): string | null {
  try {
    const origin = localStorage.getItem(STORAGE_KEY);
    return isAppOrigin(origin) ? origin : null;
  } catch {
    return null;
  }
}

/** Leaves server mode: the app forgets the server and shows local data. */
export function disconnectFromShell(): void {
  const origin = shellReturnOrigin();
  if (!origin) return;
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // ignore
  }
  window.location.href = `${origin}/?wf_disconnect=1`;
}
