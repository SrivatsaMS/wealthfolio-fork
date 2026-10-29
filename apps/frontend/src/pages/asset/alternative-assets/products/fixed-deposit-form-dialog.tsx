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
import {
  emptyFixedDeposit,
  fixedDepositErrors,
  fixedDepositFromMetadata,
  fixedDepositMetadata,
  withoutEmpty,
  type FixedDepositFormValues,
} from "./product-metadata";
import { Field, SelectField, TextField, errorMessage, localToday } from "./product-fields";

export interface ProductEditTarget {
  assetId: string;
  name: string;
  currency: string;
  notes?: string | null;
  metadata?: Record<string, unknown>;
  /** The asset's `updatedAt` when loaded; a newer server version refuses the save. */
  updatedAt?: string;
}

interface FixedDepositFormDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Omit to create a new deposit. */
  editing?: ProductEditTarget;
  onSaved?: (assetId: string) => void;
}

export function FixedDepositFormDialog({
  open,
  onOpenChange,
  editing,
  onSaved,
}: FixedDepositFormDialogProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { settings } = useSettingsContext();
  const baseCurrency = settings?.baseCurrency ?? "INR";
  const [values, setValues] = useState<FixedDepositFormValues>(() =>
    emptyFixedDeposit(baseCurrency, localToday()),
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
        ? fixedDepositFromMetadata(editing.name, editing.currency, editing.notes, editing.metadata)
        : emptyFixedDeposit(baseCurrency, localToday()),
    );
  }, [open, editing, baseCurrency]);

  const set = <K extends keyof FixedDepositFormValues>(key: K, value: FixedDepositFormValues[K]) =>
    setValues((prev) => ({ ...prev, [key]: value }));

  const errors = fixedDepositErrors(values);
  const err = (field: string) =>
    showErrors && errors.includes(field) ? t(`asset:products.fd.errors.${field}`) : null;

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
          fixedDepositMetadata(values),
          values.name.trim(),
          values.notes.trim() || null,
          undefined,
          editing.updatedAt,
        );
        assetId = editing.assetId;
      } else {
        const response = await createAlternativeAsset({
          kind: "other",
          name: values.name.trim(),
          currency: values.currency.trim().toUpperCase(),
          currentValue: values.principal.trim(),
          valueDate: values.startDate,
          metadata: withoutEmpty(fixedDepositMetadata(values)),
        });
        assetId = response.assetId;
      }
      await invalidateAlternativeAssetQueries(queryClient);
      toast.success(editing ? t("asset:products.saved") : t("asset:products.fd.created"));
      onSaved?.(assetId);
      onOpenChange(false);
    } catch (error) {
      setServerError(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {editing ? t("asset:products.fd.editTitle") : t("asset:products.fd.addTitle")}
          </DialogTitle>
          <DialogDescription>{t("asset:products.fd.description")}</DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <TextField
            id="fd-name"
            label={t("asset:products.fd.name")}
            value={values.name}
            onChange={(v) => set("name", v)}
            error={err("name")}
          />
          <TextField
            id="fd-issuer"
            label={t("asset:products.fd.issuer")}
            value={values.issuer}
            onChange={(v) => set("issuer", v)}
            placeholder="State Bank of India"
          />
          <TextField
            id="fd-principal"
            label={t("asset:products.fd.principal")}
            value={values.principal}
            inputMode="decimal"
            onChange={(v) => set("principal", v)}
            error={err("principal")}
          />
          <TextField
            id="fd-currency"
            label={t("common:currency")}
            value={values.currency}
            onChange={(v) => set("currency", v.toUpperCase())}
          />
          <TextField
            id="fd-rate"
            label={t("asset:products.fd.rate")}
            value={values.interestRate}
            inputMode="decimal"
            onChange={(v) => set("interestRate", v)}
            error={err("interestRate")}
          />
          <SelectField
            id="fd-method"
            label={t("asset:products.fd.method")}
            value={values.interestMethod}
            onChange={(v) => set("interestMethod", v)}
            options={[
              { value: "compound", label: t("asset:products.fd.methodCompound") },
              { value: "simple", label: t("asset:products.fd.methodSimple") },
            ]}
          />
          {values.interestMethod === "compound" && (
            <SelectField
              id="fd-compounding"
              label={t("asset:products.fd.compounding")}
              value={values.compounding}
              onChange={(v) => set("compounding", v)}
              options={[
                { value: "monthly", label: t("asset:products.frequency.monthly") },
                { value: "quarterly", label: t("asset:products.frequency.quarterly") },
                { value: "half_yearly", label: t("asset:products.frequency.half_yearly") },
                { value: "yearly", label: t("asset:products.frequency.yearly") },
              ]}
            />
          )}
          <SelectField
            id="fd-day-count"
            label={t("asset:products.fd.dayCount")}
            hint={t("asset:products.fd.dayCountHint")}
            value={values.dayCount}
            onChange={(v) => set("dayCount", v)}
            options={[
              { value: "act365", label: "Actual/365" },
              { value: "act_act", label: "Actual/Actual" },
              { value: "act360", label: "Actual/360" },
              { value: "thirty360", label: "30/360" },
            ]}
          />
          <SelectField
            id="fd-payout"
            label={t("asset:products.fd.payout")}
            value={values.payout}
            onChange={(v) => set("payout", v)}
            options={[
              { value: "cumulative", label: t("asset:products.fd.payoutCumulative") },
              { value: "periodic", label: t("asset:products.fd.payoutPeriodic") },
            ]}
          />
          <TextField
            id="fd-start"
            type="date"
            label={t("asset:products.fd.startDate")}
            value={values.startDate}
            onChange={(v) => set("startDate", v)}
            error={err("startDate")}
          />
          <TextField
            id="fd-maturity"
            type="date"
            label={t("asset:products.fd.maturityDate")}
            value={values.maturityDate}
            onChange={(v) => set("maturityDate", v)}
            error={err("maturityDate")}
          />
          <TextField
            id="fd-actual"
            label={t("asset:products.fd.actualMaturity")}
            hint={t("asset:products.fd.actualMaturityHint")}
            value={values.actualMaturityAmount}
            inputMode="decimal"
            onChange={(v) => set("actualMaturityAmount", v)}
            error={err("actualMaturityAmount")}
          />
          <TextField
            id="fd-tax"
            label={t("asset:products.fd.taxRate")}
            value={values.taxRate}
            inputMode="decimal"
            onChange={(v) => set("taxRate", v)}
            error={err("taxRate")}
          />
          <SelectField
            id="fd-status"
            label={t("asset:products.status")}
            value={values.status}
            onChange={(v) => set("status", v)}
            options={[
              { value: "active", label: t("asset:products.fd.statusActive") },
              { value: "closed", label: t("asset:products.fd.statusClosed") },
            ]}
          />
          {values.status === "closed" && (
            <TextField
              id="fd-closed"
              type="date"
              label={t("asset:products.closedDate")}
              hint={t("asset:products.fd.closedDateHint")}
              value={values.closedDate}
              onChange={(v) => set("closedDate", v)}
            />
          )}
          <div className="sm:col-span-2">
            <Field id="fd-notes" label={t("asset:products.notes")}>
              <Textarea
                id="fd-notes"
                value={values.notes}
                onChange={(e) => set("notes", e.target.value)}
              />
            </Field>
          </div>
        </div>

        <p className="text-muted-foreground text-xs">{t("asset:products.fd.calculationNote")}</p>
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
