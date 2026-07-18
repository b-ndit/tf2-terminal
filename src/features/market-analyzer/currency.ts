/** Formats a flat ref amount as backpack.tf's classifieds do: keys as the
 * headline unit, refined metal as the "change" — e.g. "2 keys, 5.33 ref"
 * — falling back to a plain ref amount below one key, or if no live key
 * rate is available yet. Mirrors `domain::currency::Currency::from_total_ref`'s
 * floor/remainder split exactly (src-tauri/src/domain/currency.rs). */
export function formatCurrency(totalRef: number | null, keyRateRef: number | null | undefined): string {
  if (totalRef === null) return "—";
  if (!keyRateRef || keyRateRef <= 0) {
    return `${totalRef.toFixed(2)} ref`;
  }

  const keys = Math.floor(totalRef / keyRateRef);
  const metalRef = totalRef - keys * keyRateRef;

  if (keys === 0) {
    return `${metalRef.toFixed(2)} ref`;
  }
  const keyLabel = `${keys} key${keys === 1 ? "" : "s"}`;
  // Below half a scrap (~0.005 ref) isn't worth showing as "change".
  return metalRef > 0.005 ? `${keyLabel}, ${metalRef.toFixed(2)} ref` : keyLabel;
}
