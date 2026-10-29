import { render, screen, waitFor } from "@/test/render";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { createActivity, createAlternativeAsset, updateAlternativeAssetMetadata } from "@/adapters";
import type { PolicySummary } from "@/lib/types";
import { FixedDepositFormDialog } from "./fixed-deposit-form-dialog";
import { RecordPremiumDialog } from "./record-premium-dialog";

vi.mock("@/adapters", () => ({
  createAlternativeAsset: vi.fn(() => Promise.resolve({ assetId: "FD1", quoteId: "q" })),
  updateAlternativeAssetMetadata: vi.fn(() => Promise.resolve()),
  createActivity: vi.fn(() => Promise.resolve({ id: "a1" })),
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));
vi.mock("@/lib/settings-provider", () => {
  const value = { settings: { baseCurrency: "INR" } };
  return { useSettingsContext: () => value };
});
vi.mock("@/hooks/use-accounts", () => {
  const accounts = [
    { id: "savings", name: "HDFC Savings", currency: "INR", accountType: "CASH", isActive: true },
  ];
  return { useAccounts: () => ({ accounts }) };
});
vi.mock("../hooks/use-alternative-asset-mutations", () => ({
  invalidateAlternativeAssetQueries: vi.fn(() => Promise.resolve()),
}));

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

async function fill(user: ReturnType<typeof userEvent.setup>, label: string, value: string) {
  const input = screen.getByLabelText(label);
  await user.clear(input);
  await user.type(input, value);
}

describe("FixedDepositFormDialog", () => {
  beforeEach(() => vi.clearAllMocks());

  it("creates a fixed deposit with its terms", async () => {
    const user = userEvent.setup();
    render(<FixedDepositFormDialog open onOpenChange={vi.fn()} />, { wrapper });
    await fill(user, "Deposit name", "SBI FD");
    await fill(user, "Principal amount", "100000");
    await fill(user, "Annual interest rate (%)", "7.1");
    await fill(user, "Start date", "2025-01-01");
    await fill(user, "Maturity date", "2026-01-01");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(createAlternativeAsset).toHaveBeenCalled());
    const request = vi.mocked(createAlternativeAsset).mock.calls[0][0];
    expect(request).toMatchObject({
      kind: "other",
      name: "SBI FD",
      currency: "INR",
      currentValue: "100000",
      valueDate: "2025-01-01",
      metadata: {
        sub_type: "fixed_deposit",
        fd_principal: "100000",
        fd_interest_rate: "7.1",
        fd_interest_method: "compound",
        fd_compounding: "quarterly",
        fd_start_date: "2025-01-01",
        fd_maturity_date: "2026-01-01",
      },
    });
  });

  it("refuses a maturity date before the start date", async () => {
    const user = userEvent.setup();
    render(<FixedDepositFormDialog open onOpenChange={vi.fn()} />, { wrapper });
    await fill(user, "Deposit name", "Bad FD");
    await fill(user, "Principal amount", "1000");
    await fill(user, "Annual interest rate (%)", "7");
    await fill(user, "Start date", "2025-06-01");
    await fill(user, "Maturity date", "2025-01-01");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(
      await screen.findByText("The maturity date must be after the start date."),
    ).toBeInTheDocument();
    expect(createAlternativeAsset).not.toHaveBeenCalled();
  });

  it("sends the loaded version when editing so stale edits are refused", async () => {
    const user = userEvent.setup();
    render(
      <FixedDepositFormDialog
        open
        onOpenChange={vi.fn()}
        editing={{
          assetId: "FD1",
          name: "SBI FD",
          currency: "INR",
          updatedAt: "2026-10-01T10:00:00.000",
          metadata: {
            sub_type: "fixed_deposit",
            fd_principal: "100000",
            fd_interest_rate: "7",
            fd_interest_method: "simple",
            fd_start_date: "2025-01-01",
            fd_maturity_date: "2026-01-01",
          },
        }}
      />,
      { wrapper },
    );
    await fill(user, "Annual interest rate (%)", "7.25");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(updateAlternativeAssetMetadata).toHaveBeenCalled());
    const call = vi.mocked(updateAlternativeAssetMetadata).mock.calls[0];
    expect(call[0]).toBe("FD1");
    expect(call[1]).toMatchObject({ fd_interest_rate: "7.25", fd_compounding: "" });
    expect(call[5]).toBe("2026-10-01T10:00:00.000");
  });
});

const policy = {
  premiumAmount: 10000,
  upcoming: [{ installment: 6, dueDate: "2026-01-01", amount: 10000, paid: false }],
} as unknown as PolicySummary;

describe("RecordPremiumDialog", () => {
  beforeEach(() => vi.clearAllMocks());

  it("records a premium as a tagged withdrawal with an idempotency key", async () => {
    const user = userEvent.setup();
    render(
      <RecordPremiumDialog
        open
        onOpenChange={vi.fn()}
        policyId="POL1"
        policyName="Jeevan Anand"
        currency="INR"
        policy={policy}
      />,
      { wrapper },
    );
    await user.click(screen.getByRole("button", { name: "Record premium" }));

    await waitFor(() => expect(createActivity).toHaveBeenCalled());
    expect(vi.mocked(createActivity).mock.calls[0][0]).toMatchObject({
      accountId: "savings",
      activityType: "WITHDRAWAL",
      amount: 10000,
      currency: "INR",
      idempotencyKey: "policy-premium:POL1:6",
      metadata: { policy_premium: { policy_id: "POL1", installment: 6 } },
    });
  });

  it("explains a duplicate instalment instead of creating it twice", async () => {
    vi.mocked(createActivity).mockRejectedValueOnce(
      new Error("Duplicate activity detected. A matching activity already exists."),
    );
    const user = userEvent.setup();
    render(
      <RecordPremiumDialog
        open
        onOpenChange={vi.fn()}
        policyId="POL1"
        policyName="Jeevan Anand"
        currency="INR"
        policy={policy}
      />,
      { wrapper },
    );
    await user.click(screen.getByRole("button", { name: "Record premium" }));
    expect(await screen.findByText("This instalment is already recorded.")).toBeInTheDocument();
  });
});
