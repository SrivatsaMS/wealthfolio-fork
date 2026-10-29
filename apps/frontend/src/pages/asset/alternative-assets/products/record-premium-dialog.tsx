import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { createActivity } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { QueryKeys } from "@/lib/query-keys";
import type { PolicySummary } from "@/lib/types";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Spinner,
} from "@wealthfolio/ui";

import { paymentMethodOptions } from "@/features/spending/lib/payment-methods";
import { PAYMENT_METHOD_METADATA_KEY } from "@/features/spending/types/payment-method";
import { invalidateAlternativeAssetQueries } from "../hooks/use-alternative-asset-mutations";
import { premiumPaymentTag } from "./product-metadata";
import { SelectField, TextField, errorMessage, localToday } from "./product-fields";

interface RecordPremiumDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  policyId: string;
  policyName: string;
  currency: string;
  policy: PolicySummary;
}

/**
 * Records a premium as a withdrawal from the paying account, tagged to the
 * policy and keyed by instalment so a retry or a second device cannot create
 * the same payment twice.
 */
export function RecordPremiumDialog({
  open,
  onOpenChange,
  policyId,
  policyName,
  currency,
  policy,
}: RecordPremiumDialogProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { accounts } = useAccounts();
  const nextInstallment = policy.upcoming[0]?.installment ?? null;
  const [accountId, setAccountId] = useState("");
  const [date, setDate] = useState(localToday());
  const [amount, setAmount] = useState(String(policy.premiumAmount));
  const [installment, setInstallment] = useState(nextInstallment ? String(nextInstallment) : "");
  const [paymentMethod, setPaymentMethod] = useState("none");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setAccountId((prev) => prev || accounts[0]?.id || "");
    setDate(localToday());
    setAmount(String(policy.premiumAmount));
    setInstallment(nextInstallment ? String(nextInstallment) : "");
    setError(null);
  }, [open, accounts, policy.premiumAmount, nextInstallment]);

  const save = async () => {
    setError(null);
    const value = Number(amount);
    const number = installment.trim() ? Number(installment) : null;
    if (!accountId) return setError(t("asset:products.premium.errors.account"));
    if (!Number.isFinite(value) || value <= 0)
      return setError(t("asset:products.premium.errors.amount"));
    if (number !== null && (!Number.isInteger(number) || number < 1))
      return setError(t("asset:products.premium.errors.installment"));
    setSaving(true);
    try {
      const tag = premiumPaymentTag(policyId, number);
      const account = accounts.find((a) => a.id === accountId);
      await createActivity({
        accountId,
        activityType: "WITHDRAWAL",
        activityDate: new Date(`${date}T12:00:00`).toISOString(),
        amount: value,
        currency: account?.currency ?? currency,
        comment: t("asset:products.premium.note", { name: policyName }),
        idempotencyKey: tag.idempotencyKey,
        metadata: {
          ...tag.metadata,
          ...(paymentMethod !== "none" ? { [PAYMENT_METHOD_METADATA_KEY]: paymentMethod } : {}),
        },
      });
      await Promise.all([
        invalidateAlternativeAssetQueries(queryClient),
        queryClient.invalidateQueries({ queryKey: [QueryKeys.ACTIVITY_DATA] }),
      ]);
      toast.success(t("asset:products.premium.recorded"));
      onOpenChange(false);
    } catch (e) {
      const message = errorMessage(e);
      setError(/duplicate/i.test(message) ? t("asset:products.premium.errors.duplicate") : message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{t("asset:products.premium.title")}</DialogTitle>
          <DialogDescription>{t("asset:products.premium.description")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <SelectField
            id="premium-account"
            label={t("asset:products.premium.account")}
            value={accountId}
            onChange={setAccountId}
            options={accounts.map((a) => ({ value: a.id, label: `${a.name} (${a.currency})` }))}
          />
          <TextField
            id="premium-date"
            type="date"
            label={t("common:date")}
            value={date}
            onChange={setDate}
          />
          <TextField
            id="premium-amount"
            label={t("common:amount")}
            value={amount}
            inputMode="decimal"
            onChange={setAmount}
          />
          <TextField
            id="premium-installment"
            label={t("asset:products.premium.installment")}
            hint={t("asset:products.premium.installmentHint")}
            value={installment}
            inputMode="numeric"
            onChange={setInstallment}
          />
          <SelectField
            id="premium-method"
            label={t("spending:paymentMethod.label")}
            value={paymentMethod}
            onChange={setPaymentMethod}
            options={[
              { value: "none", label: t("spending:paymentMethod.notSet") },
              ...paymentMethodOptions(t),
            ]}
          />
          {error && (
            <p role="alert" className="text-destructive text-sm">
              {error}
            </p>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            {t("common:cancel")}
          </Button>
          <Button onClick={() => void save()} disabled={saving}>
            {saving && <Spinner className="me-2 h-4 w-4" />}
            {t("asset:products.premium.record")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
