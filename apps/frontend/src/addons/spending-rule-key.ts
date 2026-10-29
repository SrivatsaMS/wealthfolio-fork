import { sha256Digest, toHex } from "@/lib/sha256";

/**
 * Derives a stable categorization-rule id from an addon-supplied `ruleKey`.
 * The `addonId` prefix keeps ids from different addons distinct even when
 * they pick the same ruleKey text, and is readable provenance when a user
 * browses rules in Wealthfolio's own Settings UI. Hashing only `ruleKey`
 * (not the addonId too) keeps the id compact regardless of ruleKey length
 * without needing a delimiter between the two.
 */
export async function deriveRuleId(addonId: string, ruleKey: string): Promise<string> {
  const hex = toHex(await sha256Digest(new TextEncoder().encode(ruleKey)));
  return `addon:${addonId}:${hex.slice(0, 24)}`;
}
