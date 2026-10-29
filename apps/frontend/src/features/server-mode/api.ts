import { invoke } from "#platform";

/**
 * Server mode commands. They exist only in the Tauri app (desktop and mobile);
 * the web build is the server's own app and never calls them.
 */
export interface ServerConnection {
  serverUrl: string;
  lastVerifiedAt: string | null;
}

export interface ServerTestResult {
  serverUrl: string;
  reachable: boolean;
  authRequired: boolean | null;
  httpStatus: number | null;
  error: string | null;
}

export const getServerConnection = () =>
  invoke<ServerConnection | null>("get_server_connection", {});

export const testServerConnection = (serverUrl: string) =>
  invoke<ServerTestResult>("test_server_connection", { serverUrl });

/** Saves the server and, when it answers, opens it in this window. */
export const connectServer = (serverUrl: string) =>
  invoke<ServerTestResult>("connect_server", { serverUrl });

/** Opens the saved server at startup; the result says why when it cannot. */
export const openSavedServer = () => invoke<ServerTestResult>("open_saved_server", {});

export const disconnectServer = () => invoke<void>("disconnect_server", {});
