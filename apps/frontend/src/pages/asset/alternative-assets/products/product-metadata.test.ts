import { describe, expect, it } from "vitest";

import {
  emptyFixedDeposit,
  emptyPolicy,
  fixedDepositErrors,
  fixedDepositFromMetadata,
  fixedDepositMetadata,
  policyErrors,
  policyFromMetadata,
  policyMetadata,
  premiumPaymentTag,
  productSubtype,
  withoutEmpty,
} from "./product-metadata";

const fd = {
  ...emptyFixedDeposit("INR", "2025-01-01"),
  name: "SBI FD",
  principal: "100000",
  interestRate: "7",
  maturityDate: "2026-01-01",
};

describe("fixed deposit metadata", () => {
  it("round-trips terms through metadata", () => {
    const metadata = fixedDepositMetadata(fd);
    expect(metadata.sub_type).toBe("fixed_deposit");
    expect(metadata.fd_compounding).toBe("quarterly");
    expect(metadata.purchase_price).toBe("100000");
    const back = fixedDepositFromMetadata("SBI FD", "INR", null, metadata);
    expect(back).toEqual({ ...fd, notes: "" });
    expect(productSubtype(metadata)).toBe("fixed_deposit");
  });

  it("drops compounding for simple interest and empty keys on create", () => {
    const metadata = withoutEmpty(fixedDepositMetadata({ ...fd, interestMethod: "simple" }));
    expect(metadata.fd_compounding).toBeUndefined();
    expect(metadata.fd_actual_maturity_amount).toBeUndefined();
  });

  it("rejects invalid dates and amounts", () => {
    expect(fixedDepositErrors(fd)).toEqual([]);
    expect(fixedDepositErrors({ ...fd, maturityDate: "2024-12-31" })).toContain("maturityDate");
    expect(fixedDepositErrors({ ...fd, principal: "0" })).toContain("principal");
    expect(fixedDepositErrors({ ...fd, interestRate: "-1" })).toContain("interestRate");
    expect(fixedDepositErrors({ ...fd, actualMaturityAmount: "500" })).toContain(
      "actualMaturityAmount",
    );
  });
});

const policy = {
  ...emptyPolicy("INR", "2020-01-01"),
  name: "Jeevan Anand",
  premiumAmount: "10000",
  policyTermYears: "20",
  premiumTermYears: "15",
};

describe("policy metadata", () => {
  it("round-trips terms and clears premium term for single premium", () => {
    const back = policyFromMetadata("Jeevan Anand", "INR", null, policyMetadata(policy));
    expect(back).toEqual({ ...policy, notes: "" });
    const single = policyMetadata({ ...policy, frequency: "single" });
    expect(single.policy_premium_term_years).toBe("");
  });

  it("validates terms", () => {
    expect(policyErrors(policy)).toEqual([]);
    expect(policyErrors({ ...policy, premiumTermYears: "25" })).toContain("premiumTermYears");
    expect(policyErrors({ ...policy, frequency: "custom", frequencyMonths: "0" })).toContain(
      "frequencyMonths",
    );
    expect(policyErrors({ ...policy, premiumAmount: "" })).toContain("premiumAmount");
  });

  it("tags premium payments with a stable idempotency key", () => {
    expect(premiumPaymentTag("P1", 4)).toEqual({
      metadata: { policy_premium: { policy_id: "P1", installment: 4 } },
      idempotencyKey: "policy-premium:P1:4",
    });
    expect(premiumPaymentTag("P1", null).idempotencyKey).toBeUndefined();
  });
});
