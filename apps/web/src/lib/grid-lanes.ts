/**
 * Cards per row for a `repeat(auto-fill, minmax(minCard, 1fr))` grid of the
 * given width. Pure so it can be unit-tested; garbage in (zero/NaN) yields
 * one lane rather than nonsense.
 */
export function lanesForWidth(gridWidth: number, minCard: number, gap: number): number {
  if (!Number.isFinite(gridWidth) || gridWidth <= 0) return 1;
  if (!Number.isFinite(minCard) || minCard <= 0) return 1;
  const safeGap = Number.isFinite(gap) && gap >= 0 ? gap : 0;
  return Math.max(1, Math.floor((gridWidth + safeGap) / (minCard + safeGap)));
}
