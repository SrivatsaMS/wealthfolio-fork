import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { deleteActivity } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import type { AlternativeAssetHolding, FixedDepositSummary, PolicySummary } from "@/lib/types";
import { AmountDisplay, Button, Icons, useDateFormatting } from "@wealthfolio/ui";
import { Badge } from "@wealthfolio/ui/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@wealthfolio/ui/components/ui/card";

import { invalidateAlternativeAssetQueries } from "../hooks/use-alternative-asset-mutations";
import { FixedDepositFormDialog, type ProductEditTarget } from "./fixed-deposit-form-dialog";
import { InsurancePolicyFormDialog } from "./insurance-policy-form-dialog";
import { RecordPremiumDialog } from "./record-premium-dialog";
import { errorMessage } from "./product-fields";

interface RowProps {
  label: string;
  children: React.ReactNode;
  hint?: string;
}

function Row({ label, children, hint }: RowProps) {
  return (
    <div className="flex items-start justify-between gap-4 py-1.5 text-sm">
      <div>
        <div className="text-muted-foreground">{label}</div>
        {hint && <div className="text-muted-foreground/80 text-xs">{hint}</div>}
      </div>
      <div className="text-end font-medium tabular-nums">{children}</div>
    </div>
  );
}

function useProductDate() {
  const dates = useDateFormatting();
  return (value: string | null | undefined) => (value ? dates.formatCalendarDate(value) : "—");
}

function editTarget(holding: AlternativeAssetHolding, updatedAt?: string): ProductEditTarget {
  return {
    assetId: holding.id,
    name: holding.name,
    currency: holding.currency,
    notes: holding.notes,
    metadata: holding.metadata,
    updatedAt,
  };
}

interface FixedDepositOverviewProps {
  holding: AlternativeAssetHolding;
  deposit: FixedDepositSummary;
  updatedAt?: string;
}

export function FixedDepositOverview({ holding, deposit, updatedAt }: FixedDepositOverviewProps) {
  const { t } = useTranslation();
  const { isBalanceHidden } = useBalancePrivacy();
  const formatDate = useProductDate();
  const [editing, setEditing] = useState(false);
  const money = (value: number | null | undefined) =>
    value == null ? (
      "—"
    ) : (
      <AmountDisplay value={value} currency={holding.currency} isHidden={isBalanceHidden} />
    );

  const years = Math.floor(deposit.daysToMaturity / 365);
  const days = deposit.daysToMaturity % 365;

  return (
    <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
      <Card className="md:col-span-2">
        <CardHeader className="flex flex-row items-start justify-between space-y-0">
          <div>
            <CardTitle className="text-xl font-bold">{money(deposit.currentValue)}</CardTitle>
            <p className="text-muted-foreground text-sm">{t("asset:products.fd.currentValue")}</p>
          </div>
          <div className="flex items-center gap-2">
            <Badge variant="secondary">{t(`asset:products.fd.status.${deposit.status}`)}</Badge>
            <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
              <Icons.Pencil className="me-2 h-3 w-3" />
              {t("asset:products.editTerms")}
            </Button>
          </div>
        </CardHeader>
        <CardContent className="divide-y">
          <Row label={t("asset:products.fd.principal")}>{money(deposit.principal)}</Row>
          <Row
            label={t("asset:products.fd.expectedMaturity")}
            hint={t("asset:products.fd.expectedMaturityHint")}
          >
            {money(deposit.expectedMaturityAmount)}
          </Row>
          <Row label={t("asset:products.fd.expectedInterest")}>
            {money(deposit.expectedInterest)}
          </Row>
          {deposit.actualMaturityAmount != null && (
            <Row
              label={t("asset:products.fd.actualMaturity")}
              hint={t("asset:products.fd.actualMaturityShownHint")}
            >
              {money(deposit.actualMaturityAmount)}
            </Row>
          )}
          {deposit.actualInterest != null && (
            <Row label={t("asset:products.fd.actualInterest")}>{money(deposit.actualInterest)}</Row>
          )}
          <Row label={t("asset:products.fd.accruedInterest")}>{money(deposit.accruedInterest)}</Row>
          {deposit.estimatedTax != null && (
            <Row label={t("asset:products.fd.estimatedTax")}>{money(deposit.estimatedTax)}</Row>
          )}
          {deposit.interestAfterTax != null && (
            <Row label={t("asset:products.fd.interestAfterTax")}>
              {money(deposit.interestAfterTax)}
            </Row>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-md">{t("asset:products.terms")}</CardTitle>
        </CardHeader>
        <CardContent className="divide-y">
          <Row label={t("asset:products.fd.issuer")}>{deposit.issuer ?? "—"}</Row>
          <Row label={t("asset:products.fd.rate")}>{deposit.annualRatePct}%</Row>
          <Row label={t("asset:products.fd.method")}>
            {deposit.interestMethod === "simple"
              ? t("asset:products.fd.methodSimple")
              : `${t("asset:products.fd.methodCompound")} · ${t(
                  `asset:products.frequency.${deposit.compounding ?? "quarterly"}`,
                )}`}
          </Row>
          <Row label={t("asset:products.fd.startDate")}>{formatDate(deposit.startDate)}</Row>
          <Row label={t("asset:products.fd.maturityDate")}>{formatDate(deposit.maturityDate)}</Row>
          <Row label={t("asset:products.fd.remaining")}>
            {deposit.daysToMaturity === 0
              ? t("asset:products.fd.matured")
              : t("asset:products.fd.remainingValue", { years, days })}
          </Row>
        </CardContent>
      </Card>

      <FixedDepositFormDialog
        open={editing}
        onOpenChange={setEditing}
        editing={editTarget(holding, updatedAt)}
      />
    </div>
  );
}

interface PolicyOverviewProps {
  holding: AlternativeAssetHolding;
  policy: PolicySummary;
  updatedAt?: string;
}

export function PolicyOverview({ holding, policy, updatedAt }: PolicyOverviewProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { isBalanceHidden } = useBalancePrivacy();
  const { accounts } = useAccounts({ filterActive: false, includeArchived: true });
  const formatDate = useProductDate();
  const [editing, setEditing] = useState(false);
  const [recording, setRecording] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);
  const money = (value: number | null | undefined) =>
    value == null ? (
      "—"
    ) : (
      <AmountDisplay value={value} currency={holding.currency} isHidden={isBalanceHidden} />
    );
  const accountName = (id: string) => accounts.find((a) => a.id === id)?.name ?? id;
  const frequency =
    policy.frequency.kind === "custom"
      ? t("asset:products.policy.everyMonths", { count: policy.frequency.months })
      : t(`asset:products.frequency.${policy.frequency.kind}`);

  const removePayment = async (activityId: string) => {
    setDeleting(activityId);
    try {
      await deleteActivity(activityId);
      await invalidateAlternativeAssetQueries(queryClient);
      toast.success(t("asset:products.premium.deleted"));
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setDeleting(null);
    }
  };

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        <Card className="md:col-span-2">
          <CardHeader className="flex flex-row items-start justify-between space-y-0">
            <div>
              <CardTitle className="text-xl font-bold">
                {money(Number(holding.marketValue))}
              </CardTitle>
              <p className="text-muted-foreground text-sm">
                {policy.valuation === "manual"
                  ? t("asset:products.policy.valuedManually")
                  : t("asset:products.policy.valuedAtPremiums")}
              </p>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-2">
              <Badge variant="secondary">
                {t(`asset:products.policy.statuses.${policy.status}`)}
              </Badge>
              <Button size="sm" onClick={() => setRecording(true)}>
                <Icons.Plus className="me-2 h-3 w-3" />
                {t("asset:products.premium.record")}
              </Button>
              <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
                <Icons.Pencil className="me-2 h-3 w-3" />
                {t("asset:products.editTerms")}
              </Button>
            </div>
          </CardHeader>
          <CardContent className="divide-y">
            <Row label={t("asset:products.policy.nextDue")}>
              {policy.nextPremiumDue ? formatDate(policy.nextPremiumDue) : "—"}
            </Row>
            <Row label={t("asset:products.policy.premiumsPaid")}>
              {policy.premiumsPaid} / {policy.totalScheduledPremiums}
            </Row>
            <Row label={t("asset:products.policy.premiumsRemaining")}>
              {policy.premiumsRemaining}
            </Row>
            {policy.premiumsOverdue > 0 && (
              <Row label={t("asset:products.policy.overdue")}>
                <span className="text-destructive">
                  {policy.premiumsOverdue} · {money(policy.outstandingAmount)}
                </span>
              </Row>
            )}
            <Row label={t("asset:products.policy.totalPaid")}>
              {money(policy.totalPremiumsPaid)}
            </Row>
            <Row label={t("asset:products.policy.totalScheduled")}>
              {money(policy.totalScheduledAmount)}
            </Row>
            <Row label={t("asset:products.policy.maturityDate")}>
              {formatDate(policy.maturityDate)}
            </Row>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-md">
              {t("asset:products.policy.maturityEstimates")}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-muted-foreground text-xs">
              {t("asset:products.policy.notGuaranteed")}
            </p>
            <div className="divide-y">
              <Row
                label={t("asset:products.policy.fromDocuments")}
                hint={t("asset:products.policy.fromDocumentsHint")}
              >
                {money(policy.userMaturityEstimate?.amount)}
              </Row>
              <Row
                label={t("asset:products.policy.calculated")}
                hint={
                  policy.assumedReturnPct != null
                    ? t("asset:products.policy.calculatedHint", {
                        rate: policy.assumedReturnPct,
                      })
                    : t("asset:products.policy.calculatedMissing")
                }
              >
                {money(policy.calculatedMaturityEstimate?.amount)}
              </Row>
              <Row
                label={t("asset:products.policy.sumAssuredBonus")}
                hint={t("asset:products.policy.sumAssuredBonusHint")}
              >
                {money(policy.sumAssuredWithBonus)}
              </Row>
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-md">{t("asset:products.policy.upcoming")}</CardTitle>
          </CardHeader>
          <CardContent>
            {policy.upcoming.length === 0 ? (
              <p className="text-muted-foreground text-sm">
                {t("asset:products.policy.noUpcoming")}
              </p>
            ) : (
              <ul className="divide-y text-sm">
                {policy.upcoming.map((due) => (
                  <li key={due.installment} className="flex justify-between gap-2 py-1.5">
                    <span>
                      #{due.installment} · {formatDate(due.dueDate)}
                    </span>
                    <span className="tabular-nums">{money(due.amount)}</span>
                  </li>
                ))}
              </ul>
            )}
            <p className="text-muted-foreground mt-2 text-xs">
              {t("asset:products.policy.frequencyLabel", { frequency })}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-md">{t("asset:products.policy.history")}</CardTitle>
          </CardHeader>
          <CardContent>
            {policy.payments.length === 0 ? (
              <p className="text-muted-foreground text-sm">
                {t("asset:products.policy.noPayments")}
              </p>
            ) : (
              <ul className="divide-y text-sm">
                {[...policy.payments].reverse().map((payment) => (
                  <li
                    key={payment.activityId}
                    className="flex items-center justify-between gap-2 py-1.5"
                  >
                    <span>
                      {payment.installment != null ? `#${payment.installment} · ` : ""}
                      {formatDate(payment.date)}
                      <span className="text-muted-foreground">
                        {" "}
                        · {accountName(payment.accountId)}
                      </span>
                    </span>
                    <span className="flex items-center gap-2 tabular-nums">
                      {money(payment.amount)}
                      <Button
                        size="icon"
                        variant="ghost"
                        className="h-7 w-7"
                        aria-label={t("asset:products.premium.delete")}
                        disabled={deleting === payment.activityId}
                        onClick={() => void removePayment(payment.activityId)}
                      >
                        <Icons.Trash className="h-3.5 w-3.5" />
                      </Button>
                    </span>
                  </li>
                ))}
              </ul>
            )}
            {policy.premiumsPaid > policy.payments.length && (
              <p className="text-muted-foreground mt-2 text-xs">
                {t("asset:products.policy.paidBeforeTracking", {
                  count: policy.premiumsPaid - policy.payments.length,
                })}
              </p>
            )}
          </CardContent>
        </Card>
      </div>

      <InsurancePolicyFormDialog
        open={editing}
        onOpenChange={setEditing}
        editing={editTarget(holding, updatedAt)}
      />
      <RecordPremiumDialog
        open={recording}
        onOpenChange={setRecording}
        policyId={holding.id}
        policyName={holding.name}
        currency={holding.currency}
        policy={policy}
      />
    </div>
  );
}
