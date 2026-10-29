import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { createAlternativeAsset, updateAlternativeAssetMetadata } from "@/adapters";
import { useSettingsContext } from "@/lib/settings-provider";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Spinner,
  Textarea,
} from "@wealthfolio/ui";

import { invalidateAlternativeAssetQueries } from "../hooks/use-alternative-asset-mutations";
import type { ProductEditTarget } from "./fixed-deposit-form-dialog";
import {
  emptyPolicy,
  policyErrors,
  policyFromMetadata,
  policyMetadata,
  withoutEmpty,
  type PolicyFormValues,
} from "./product-metadata";
import { Field, SelectField, TextField, errorMessage, localToday } from "./product-fields";

interface InsurancePolicyFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editing?: ProductEditTarget;
  onSaved?: (assetId: string) => void;
}

const POLICY_TYPES = [
  "endowment",
  "money_back",
  "whole_life",
  "term",
  "ulip",
  "pension",
  "other",
] as const;

export function InsurancePolicyFormDialog({
  open,
  onOpenChange,
  editing,
  onSaved,
}: InsurancePolicyFormDialogProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { settings } = useSettingsContext();
  const baseCurrency = settings?.baseCurrency ?? "INR";
  const [values, setValues] = useState<PolicyFormValues>(() =>
    emptyPolicy(baseCurrency, localToday()),
  );
  const [showErrors, setShowErrors] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setShowErrors(false);
    setServerError(null);
    setValues(
      editing
        ? policyFromMetadata(editing.name, editing.currency, editing.notes, editing.metadata)
        : emptyPolicy(baseCurrency, localToday()),
    );
  }, [open, editing, baseCurrency]);

  const set = <K extends keyof PolicyFormValues>(key: K, value: PolicyFormValues[K]) =>
    setValues((prev) => ({ ...prev, [key]: value }));

  const errors = policyErrors(values);
  const err = (field: string) =>
    showErrors && errors.includes(field) ? t(`asset:products.policy.errors.${field}`) : null;

  const save = async () => {
    setShowErrors(true);
    setServerError(null);
    if (errors.length > 0) return;
    setSaving(true);
    try {
      let assetId: string;
      if (editing) {
        await updateAlternativeAssetMetadata(
          editing.assetId,
          policyMetadata(values),
          values.name.trim(),
          values.notes.trim() || null,
          undefined,
          editing.updatedAt,
        );
        assetId = editing.assetId;
      } else {
        const paidBefore = Number(values.premiumsPaidBefore || "0");
        const response = await createAlternativeAsset({
          kind: "other",
          name: values.name.trim(),
          currency: values.currency.trim().toUpperCase(),
          // Only used if the policy is switched to manual valuation later.
          currentValue: String(paidBefore * Number(values.premiumAmount)),
          valueDate: values.commencementDate,
          metadata: withoutEmpty(policyMetadata(values)),
        });
        assetId = response.assetId;
      }
      await invalidateAlternativeAssetQueries(queryClient);
      toast.success(editing ? t("asset:products.saved") : t("asset:products.policy.created"));
      onSaved?.(assetId);
      onOpenChange(false);
    } catch (error) {
      setServerError(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const frequencyOptions = (
    ["monthly", "quarterly", "half_yearly", "yearly", "single", "custom"] as const
  ).map((value) => ({ value, label: t(`asset:products.frequency.${value}`) }));

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {editing ? t("asset:products.policy.editTitle") : t("asset:products.policy.addTitle")}
          </DialogTitle>
          <DialogDescription>{t("asset:products.policy.description")}</DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <TextField
            id="policy-name"
            label={t("asset:products.policy.name")}
            value={values.name}
            onChange={(v) => set("name", v)}
            error={err("name")}
            placeholder="Jeevan Anand"
          />
          <TextField
            id="policy-number"
            label={t("asset:products.policy.number")}
            value={values.policyNumber}
            onChange={(v) => set("policyNumber", v)}
          />
          <TextField
            id="policy-insurer"
            label={t("asset:products.policy.insurer")}
            value={values.insurer}
            onChange={(v) => set("insurer", v)}
          />
          <SelectField
            id="policy-type"
            label={t("asset:products.policy.type")}
            value={
              (POLICY_TYPES as readonly string[]).includes(values.policyType)
                ? values.policyType
                : "other"
            }
            onChange={(v) => set("policyType", v)}
            options={POLICY_TYPES.map((value) => ({
              value,
              label: t(`asset:products.policy.types.${value}`),
            }))}
          />
          <TextField
            id="policy-premium"
            label={t("asset:products.policy.premium")}
            value={values.premiumAmount}
            inputMode="decimal"
            onChange={(v) => set("premiumAmount", v)}
            error={err("premiumAmount")}
          />
          <TextField
            id="policy-currency"
            label={t("common:currency")}
            value={values.currency}
            onChange={(v) => set("currency", v.toUpperCase())}
          />
          <SelectField
            id="policy-frequency"
            label={t("asset:products.policy.frequency")}
            value={values.frequency}
            onChange={(v) => set("frequency", v)}
            options={frequencyOptions}
          />
          {values.frequency === "custom" && (
            <TextField
              id="policy-frequency-months"
              label={t("asset:products.policy.frequencyMonths")}
              value={values.frequencyMonths}
              inputMode="numeric"
              onChange={(v) => set("frequencyMonths", v)}
              error={err("frequencyMonths")}
            />
          )}
          <TextField
            id="policy-commencement"
            type="date"
            label={t("asset:products.policy.commencement")}
            value={values.commencementDate}
            onChange={(v) => set("commencementDate", v)}
            error={err("commencementDate")}
          />
          <TextField
            id="policy-premium-start"
            type="date"
            label={t("asset:products.policy.premiumStart")}
            hint={t("asset:products.policy.premiumStartHint")}
            value={values.premiumStartDate}
            onChange={(v) => set("premiumStartDate", v)}
            error={err("premiumStartDate")}
          />
          <TextField
            id="policy-term"
            label={t("asset:products.policy.policyTerm")}
            value={values.policyTermYears}
            inputMode="numeric"
            onChange={(v) => set("policyTermYears", v)}
            error={err("policyTermYears")}
          />
          {values.frequency !== "single" && (
            <TextField
              id="policy-premium-term"
              label={t("asset:products.policy.premiumTerm")}
              value={values.premiumTermYears}
              inputMode="numeric"
              onChange={(v) => set("premiumTermYears", v)}
              error={err("premiumTermYears")}
            />
          )}
          <TextField
            id="policy-maturity"
            type="date"
            label={t("asset:products.policy.maturityDate")}
            hint={t("asset:products.policy.maturityDateHint")}
            value={values.maturityDate}
            onChange={(v) => set("maturityDate", v)}
            error={err("maturityDate")}
          />
          <TextField
            id="policy-paid-before"
            label={t("asset:products.policy.paidBefore")}
            hint={t("asset:products.policy.paidBeforeHint")}
            value={values.premiumsPaidBefore}
            inputMode="numeric"
            onChange={(v) => set("premiumsPaidBefore", v)}
            error={err("premiumsPaidBefore")}
          />
          <TextField
            id="policy-sum-assured"
            label={t("asset:products.policy.sumAssured")}
            value={values.sumAssured}
            inputMode="decimal"
            onChange={(v) => set("sumAssured", v)}
          />
          <TextField
            id="policy-bonus"
            label={t("asset:products.policy.bonus")}
            hint={t("asset:products.policy.bonusHint")}
            value={values.bonus}
            inputMode="decimal"
            onChange={(v) => set("bonus", v)}
          />
          <TextField
            id="policy-expected"
            label={t("asset:products.policy.expectedMaturity")}
            hint={t("asset:products.policy.expectedMaturityHint")}
            value={values.expectedMaturity}
            inputMode="decimal"
            onChange={(v) => set("expectedMaturity", v)}
          />
          <TextField
            id="policy-assumed-return"
            label={t("asset:products.policy.assumedReturn")}
            hint={t("asset:products.policy.assumedReturnHint")}
            value={values.assumedReturn}
            inputMode="decimal"
            onChange={(v) => set("assumedReturn", v)}
          />
          <SelectField
            id="policy-status"
            label={t("asset:products.status")}
            value={values.status}
            onChange={(v) => set("status", v)}
            options={(["in_force", "paid_up", "lapsed", "matured", "surrendered"] as const).map(
              (value) => ({ value, label: t(`asset:products.policy.statuses.${value}`) }),
            )}
          />
          {(values.status === "matured" || values.status === "surrendered") && (
            <TextField
              id="policy-closed"
              type="date"
              label={t("asset:products.closedDate")}
              value={values.closedDate}
              onChange={(v) => set("closedDate", v)}
            />
          )}
          <SelectField
            id="policy-valuation"
            label={t("asset:products.policy.valuation")}
            hint={t("asset:products.policy.valuationHint")}
            value={values.valuation}
            onChange={(v) => set("valuation", v)}
            options={[
              { value: "premiums_paid", label: t("asset:products.policy.valuationPremiums") },
              { value: "manual", label: t("asset:products.policy.valuationManual") },
            ]}
          />
          <div className="sm:col-span-2">
            <Field id="policy-notes" label={t("asset:products.notes")}>
              <Textarea
                id="policy-notes"
                value={values.notes}
                onChange={(e) => set("notes", e.target.value)}
              />
            </Field>
          </div>
        </div>

        <p className="text-muted-foreground text-xs">{t("asset:products.policy.estimateNote")}</p>
        {serverError && (
          <p role="alert" className="text-destructive text-sm">
            {serverError}
          </p>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={saving}>
            {t("common:cancel")}
          </Button>
          <Button onClick={() => void save()} disabled={saving}>
            {saving && <Spinner className="me-2 h-4 w-4" />}
            {t("common:save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
