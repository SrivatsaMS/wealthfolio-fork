import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { openFileSaveDialog } from "@/adapters";
import { useAccounts } from "@/hooks/use-accounts";
import { useTaxonomy } from "@/hooks/use-taxonomies";
import {
  Button,
  Icons,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  Spinner,
  useAmountFormatting,
} from "@wealthfolio/ui";

import { getPaymentMethodSummary, searchCashActivities } from "../adapters/cash-activities";
import {
  paymentMethodLabel,
  paymentMethodRowsToCsv,
  readPaymentMethod,
  type PaymentMethodCsvRow,
} from "../lib/payment-methods";
import type { PaymentMethodSummaryRequest } from "../types/payment-method";

const SPENDING_TAXONOMY = "spending_categories";
const EXPORT_PAGE_SIZE = 1000;

interface PaymentMethodSummarySheetProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** ISO bounds of the period; the current month when omitted. */
  startDate?: string;
  endDate?: string;
  accountIds?: string[];
}

function currentMonthBounds(): { startDate: string; endDate: string } {
  const now = new Date();
  const start = new Date(now.getFullYear(), now.getMonth(), 1);
  const end = new Date(now.getFullYear(), now.getMonth() + 1, 0, 23, 59, 59, 999);
  return { startDate: start.toISOString(), endDate: end.toISOString() };
}

/**
 * Spending by payment method (UPI, cash, cards, …) for a period, grouped by
 * category, with a CSV export of the transactions behind each total.
 */
export function PaymentMethodSummarySheet({
  open,
  onOpenChange,
  startDate,
  endDate,
  accountIds,
}: PaymentMethodSummarySheetProps) {
  const { t } = useTranslation();
  const formatting = useAmountFormatting();
  const { accounts } = useAccounts();
  const spending = useTaxonomy(SPENDING_TAXONOMY);
  const [exporting, setExporting] = useState<string | null>(null);

  const request: PaymentMethodSummaryRequest = useMemo(() => {
    const fallback = currentMonthBounds();
    return {
      startDate: startDate ?? fallback.startDate,
      endDate: endDate ?? (startDate ? undefined : fallback.endDate),
      accountIds: accountIds && accountIds.length > 0 ? accountIds : undefined,
    };
  }, [startDate, endDate, accountIds]);

  const summary = useQuery({
    queryKey: ["spending", "payment-method-summary", request],
    queryFn: () => getPaymentMethodSummary(request),
    enabled: open,
  });

  const categoryName = useMemo(() => {
    const names = new Map<string, string>();
    (spending.data?.categories ?? []).forEach((c) => names.set(c.id, c.name));
    return (id: string | null) =>
      id ? (names.get(id) ?? id) : t("spending:paymentMethod.uncategorized");
  }, [spending.data?.categories, t]);

  const accountName = useMemo(() => {
    const names = new Map(accounts.map((a) => [a.id, a.name]));
    return (id: string) => names.get(id) ?? id;
  }, [accounts]);

  const exportMethod = async (method: string) => {
    setExporting(method);
    try {
      const rows: PaymentMethodCsvRow[] = [];
      let offset = 0;
      for (;;) {
        const page = await searchCashActivities({
          startDate: request.startDate,
          endDate: request.endDate,
          accountIds: request.accountIds,
          paymentMethods: [method],
          sortBy: "date",
          sortDir: "asc",
          offset,
          limit: EXPORT_PAGE_SIZE,
        });
        page.items.forEach((item) => {
          const categoryId =
            item.assignments.find((a) => a.taxonomyId === SPENDING_TAXONOMY)?.categoryId ?? null;
          rows.push({
            date: String(item.activityDate).slice(0, 10),
            account: accountName(item.accountId),
            payee: item.notes ?? "",
            category: categoryId ? categoryName(categoryId) : "",
            paymentMethod: paymentMethodLabel(t, readPaymentMethod(item.metadata) ?? method),
            amount: item.netAmount,
            currency: item.currency,
          });
        });
        offset += page.items.length;
        if (page.items.length === 0 || offset >= page.totalCount) break;
      }
      const saved = await openFileSaveDialog(
        paymentMethodRowsToCsv(rows),
        `${method}-transactions-${String(request.startDate).slice(0, 10)}.csv`,
      );
      if (saved) toast.success(t("spending:paymentMethod.exported", { count: rows.length }));
    } catch {
      toast.error(t("spending:paymentMethod.exportFailed"));
    } finally {
      setExporting(null);
    }
  };

  const currency = summary.data?.currency ?? "";
  const methods = summary.data?.methods ?? [];

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full overflow-y-auto sm:max-w-lg">
        <SheetHeader>
          <SheetTitle>{t("spending:paymentMethod.summaryTitle")}</SheetTitle>
          <SheetDescription>{t("spending:paymentMethod.summaryDescription")}</SheetDescription>
        </SheetHeader>

        {summary.isLoading && (
          <div className="flex justify-center py-8">
            <Spinner className="h-5 w-5" />
          </div>
        )}
        {summary.isError && (
          <p className="text-destructive py-4 text-sm">{t("spending:paymentMethod.loadFailed")}</p>
        )}
        {summary.data && methods.length === 0 && (
          <p className="text-muted-foreground py-4 text-sm">{t("spending:paymentMethod.empty")}</p>
        )}

        <div className="space-y-4 py-4">
          {methods.map((method) => (
            <section
              key={method.paymentMethod}
              className="rounded-lg border p-4"
              aria-label={paymentMethodLabel(t, method.paymentMethod)}
            >
              <div className="flex items-center justify-between gap-2">
                <div>
                  <h3 className="font-medium">{paymentMethodLabel(t, method.paymentMethod)}</h3>
                  <p className="text-muted-foreground text-xs">
                    {t("spending:paymentMethod.transactionsCount", { count: method.spentCount })}
                  </p>
                </div>
                <div className="text-end">
                  <div className="font-semibold tabular-nums">
                    {formatting.formatAmount(method.spent, currency)}
                  </div>
                  {method.received > 0 && (
                    <div className="text-muted-foreground text-xs tabular-nums">
                      {t("spending:paymentMethod.received", {
                        amount: formatting.formatAmount(method.received, currency),
                      })}
                    </div>
                  )}
                </div>
              </div>
              {method.categories.length > 0 && (
                <ul className="mt-3 space-y-1 text-sm">
                  {method.categories.map((category) => (
                    <li
                      key={category.categoryId ?? "uncategorized"}
                      className="flex justify-between gap-2"
                    >
                      <span className="truncate">{categoryName(category.categoryId)}</span>
                      <span className="tabular-nums">
                        {formatting.formatAmount(category.total, currency)}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              <Button
                variant="outline"
                size="sm"
                className="mt-3"
                disabled={exporting !== null}
                onClick={() => void exportMethod(method.paymentMethod)}
              >
                {exporting === method.paymentMethod ? (
                  <Spinner className="me-2 h-3 w-3" />
                ) : (
                  <Icons.Download className="me-2 h-3 w-3" />
                )}
                {t("spending:paymentMethod.exportCsv")}
              </Button>
            </section>
          ))}
          {(summary.data?.unconvertedCount ?? 0) > 0 && (
            <p className="text-muted-foreground text-xs">
              {t("spending:paymentMethod.unconverted", {
                count: summary.data?.unconvertedCount ?? 0,
              })}
            </p>
          )}
        </div>
      </SheetContent>
    </Sheet>
  );
}
