/** Mirrors `wealthfolio_spending::payment_method`. */
export const PAYMENT_METHODS = [
  "upi",
  "cash",
  "debit_card",
  "credit_card",
  "bank_transfer",
  "other",
] as const;

export type PaymentMethod = (typeof PAYMENT_METHODS)[number];

/** Filter token for rows that have no payment method. */
export const UNSPECIFIED_PAYMENT_METHOD = "unspecified";

/** Activity metadata key holding the payment method. */
export const PAYMENT_METHOD_METADATA_KEY = "payment_method";

export interface PaymentMethodSummaryRequest {
  startDate?: string;
  endDate?: string;
  accountIds?: string[];
  paymentMethods?: string[];
}

export interface PaymentMethodCategoryTotal {
  categoryId: string | null;
  total: number;
  count: number;
}

export interface PaymentMethodTotal {
  paymentMethod: PaymentMethod | typeof UNSPECIFIED_PAYMENT_METHOD;
  spent: number;
  spentCount: number;
  received: number;
  receivedCount: number;
  categories: PaymentMethodCategoryTotal[];
}

export interface PaymentMethodSummary {
  currency: string | null;
  unconvertedCount: number;
  methods: PaymentMethodTotal[];
}
