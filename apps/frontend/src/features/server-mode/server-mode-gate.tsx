import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Button, Icons, Spinner } from "@wealthfolio/ui";

import { disconnectServer, getServerConnection, openSavedServer } from "./api";

type GateState =
  | { kind: "checking" }
  | { kind: "local" }
  | { kind: "opening"; serverUrl: string }
  | { kind: "failed"; serverUrl: string; error: string };

/**
 * Desktop/mobile app only. When this device is set to use a self-hosted
 * server, open it instead of the local data. If the server cannot be reached,
 * say so and offer to retry or to use this device's local data; the local
 * profile is not opened behind the user's back.
 */
export function ServerModeGate({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const [state, setState] = useState<GateState>({ kind: "checking" });

  const open = useCallback(async (serverUrl: string) => {
    setState({ kind: "opening", serverUrl });
    try {
      const result = await openSavedServer();
      // On success the window navigates to the server; keep the spinner.
      if (!result.reachable) {
        setState({ kind: "failed", serverUrl, error: result.error ?? "" });
      }
    } catch (error) {
      setState({ kind: "failed", serverUrl, error: String(error) });
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    getServerConnection()
      .then((connection) => {
        if (cancelled) return;
        if (connection) void open(connection.serverUrl);
        else setState({ kind: "local" });
      })
      // Without server-mode support (older builds, tests) the app stays local.
      .catch(() => !cancelled && setState({ kind: "local" }));
    return () => {
      cancelled = true;
    };
  }, [open]);

  if (state.kind === "local") return <>{children}</>;

  return (
    <div className="bg-background flex min-h-screen items-center justify-center p-6">
      <div className="w-full max-w-md space-y-4 text-center">
        {state.kind === "failed" ? (
          <>
            <Icons.AlertTriangle className="text-destructive mx-auto h-8 w-8" />
            <h1 className="text-lg font-semibold">{t("settings:serverMode.unreachableTitle")}</h1>
            <p className="text-muted-foreground text-sm break-all">{state.serverUrl}</p>
            {state.error && <p className="text-sm">{state.error}</p>}
            <div className="flex flex-col gap-2 sm:flex-row sm:justify-center">
              <Button onClick={() => void open(state.serverUrl)}>
                {t("settings:serverMode.retry")}
              </Button>
              <Button
                variant="outline"
                onClick={() => {
                  void disconnectServer().finally(() => setState({ kind: "local" }));
                }}
              >
                {t("settings:serverMode.useLocal")}
              </Button>
            </div>
            <p className="text-muted-foreground text-xs">{t("settings:serverMode.useLocalHint")}</p>
          </>
        ) : (
          <>
            <Spinner className="mx-auto h-6 w-6" />
            <p className="text-muted-foreground text-sm">
              {state.kind === "opening"
                ? t("settings:serverMode.connecting", { url: state.serverUrl })
                : t("settings:serverMode.checking")}
            </p>
          </>
        )}
      </div>
    </div>
  );
}
