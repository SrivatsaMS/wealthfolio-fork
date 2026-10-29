import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { isTauri } from "@/adapters";
import {
  connectServer,
  getServerConnection,
  testServerConnection,
  type ServerConnection,
  type ServerTestResult,
} from "@/features/server-mode/api";
import { disconnectFromShell, shellReturnOrigin } from "@/features/server-mode/shell-return";
import { Button, Checkbox, Icons, Input, Label, Spinner, useDateFormatting } from "@wealthfolio/ui";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@wealthfolio/ui/components/ui/card";

import { SettingsHeader } from "../settings-header";

function TestResult({ result }: { result: ServerTestResult }) {
  const { t } = useTranslation();
  if (!result.reachable) {
    return (
      <p role="alert" className="text-destructive text-sm">
        {result.error ?? t("settings:serverMode.unreachableTitle")}
      </p>
    );
  }
  return (
    <p className="text-success flex items-center gap-2 text-sm">
      <Icons.CheckCircle className="h-4 w-4" />
      {result.authRequired === false
        ? t("settings:serverMode.reachableNoAuth")
        : t("settings:serverMode.reachable")}
    </p>
  );
}

/** Local app: connect this device to a self-hosted server. */
function ConnectCard() {
  const { t } = useTranslation();
  const dates = useDateFormatting();
  const [saved, setSaved] = useState<ServerConnection | null>(null);
  const [url, setUrl] = useState("");
  const [result, setResult] = useState<ServerTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<"test" | "connect" | null>(null);
  const [understood, setUnderstood] = useState(false);

  useEffect(() => {
    void getServerConnection()
      .then((connection) => {
        setSaved(connection);
        if (connection) setUrl(connection.serverUrl);
      })
      .catch(() => setSaved(null));
  }, []);

  const run = async (kind: "test" | "connect") => {
    setBusy(kind);
    setError(null);
    setResult(null);
    try {
      const next = kind === "test" ? await testServerConnection(url) : await connectServer(url);
      setResult(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("settings:serverMode.connectTitle")}</CardTitle>
        <CardDescription>{t("settings:serverMode.connectDescription")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-1.5">
          <Label htmlFor="server-url">{t("settings:serverMode.urlLabel")}</Label>
          <Input
            id="server-url"
            inputMode="url"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            placeholder="http://100.101.102.103:8088"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
          <p className="text-muted-foreground text-xs">{t("settings:serverMode.urlHint")}</p>
        </div>

        {saved?.lastVerifiedAt && (
          <p className="text-muted-foreground text-xs">
            {t("settings:serverMode.lastVerified", {
              when: dates.formatDateTime(saved.lastVerifiedAt),
            })}
          </p>
        )}

        <div className="bg-muted/40 space-y-2 rounded-md border p-3 text-sm">
          <p className="font-medium">{t("settings:serverMode.existingDataTitle")}</p>
          <p className="text-muted-foreground">{t("settings:serverMode.existingDataBody")}</p>
          <p className="text-muted-foreground">{t("settings:serverMode.offlineBody")}</p>
          <label className="flex items-start gap-2">
            <Checkbox
              checked={understood}
              onCheckedChange={(v) => setUnderstood(v === true)}
              aria-label={t("settings:serverMode.understood")}
            />
            <span>{t("settings:serverMode.understood")}</span>
          </label>
        </div>

        {result && <TestResult result={result} />}
        {error && (
          <p role="alert" className="text-destructive text-sm">
            {error}
          </p>
        )}

        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            disabled={!url.trim() || busy !== null}
            onClick={() => void run("test")}
          >
            {busy === "test" && <Spinner className="me-2 h-4 w-4" />}
            {t("settings:serverMode.test")}
          </Button>
          <Button
            disabled={!url.trim() || !understood || busy !== null}
            onClick={() => void run("connect")}
          >
            {busy === "connect" && <Spinner className="me-2 h-4 w-4" />}
            {t("settings:serverMode.connect")}
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

/** Server's web app: shows the connection and, inside the app, how to leave it. */
function ConnectedCard() {
  const { t } = useTranslation();
  const inApp = shellReturnOrigin() !== null;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("settings:serverMode.connectedTitle")}</CardTitle>
        <CardDescription>
          {t("settings:serverMode.connectedDescription", { origin: window.location.origin })}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3 text-sm">
        <p className="text-muted-foreground">{t("settings:serverMode.liveSync")}</p>
        {inApp ? (
          <>
            <p className="text-muted-foreground">{t("settings:serverMode.disconnectHint")}</p>
            <Button variant="outline" onClick={disconnectFromShell}>
              {t("settings:serverMode.disconnect")}
            </Button>
          </>
        ) : (
          <p className="text-muted-foreground">{t("settings:serverMode.browserHint")}</p>
        )}
      </CardContent>
    </Card>
  );
}

export default function ServerSettingsPage() {
  const { t } = useTranslation();
  return (
    <div className="space-y-6">
      <SettingsHeader
        heading={t("settings:serverMode.heading")}
        text={t("settings:serverMode.subheading")}
      />
      {isTauri ? <ConnectCard /> : <ConnectedCard />}
    </div>
  );
}
