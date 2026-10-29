import { describe, expect, it } from "vitest";

import { paymentMethodRowsToCsv, readPaymentMethod, withPaymentMethod } from "./payment-methods";

describe("payment methods", () => {
  it("reads the payment method from object or JSON metadata", () => {
    expect(readPaymentMethod({ payment_method: "upi" })).toBe("upi");
    expect(readPaymentMethod('{"payment_method":"cash"}')).toBe("cash");
    // Existing transactions without the field keep working.
    expect(readPaymentMethod(undefined)).toBeNull();
    expect(readPaymentMethod({ flow: { is_external: true } })).toBeNull();
    expect(readPaymentMethod({ payment_method: "cheque" })).toBeNull();
    expect(readPaymentMethod("not json")).toBeNull();
  });

  it("creates an expense with UPI", () => {
    expect(withPaymentMethod(undefined, "upi", null, true)).toEqual({ payment_method: "upi" });
    expect(withPaymentMethod(undefined, null, null, true)).toBeUndefined();
    // Keeps the flow metadata the form computed.
    expect(withPaymentMethod({ flow: { is_external: true } }, "upi", null, true)).toEqual({
      flow: { is_external: true },
      payment_method: "upi",
    });
  });

  it("edits the payment method as a metadata patch", () => {
    expect(withPaymentMethod(undefined, "cash", "upi", false)).toEqual({ payment_method: "cash" });
    // Clearing sends null so the merged metadata drops the method.
    expect(withPaymentMethod(undefined, null, "upi", false)).toEqual({ payment_method: null });
    // Unchanged: nothing extra is sent.
    expect(withPaymentMethod(undefined, "upi", "upi", false)).toBeUndefined();
  });

  it("exports UPI rows as CSV", () => {
    const csv = paymentMethodRowsToCsv([
      {
        date: "2026-10-01",
        account: "HDFC Savings",
        payee: 'Cafe "Coffee", Day',
        category: "Food",
        paymentMethod: "UPI",
        amount: -250,
        currency: "INR",
      },
    ]);
    expect(csv.split("\n")).toEqual([
      "date,account,payee,category,payment_method,amount,currency",
      '2026-10-01,HDFC Savings,"Cafe ""Coffee"", Day",Food,UPI,-250,INR',
    ]);
  });
});
