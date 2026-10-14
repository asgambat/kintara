/** Fallback when a stored highlight color is missing or fails validation. */
export const DEFAULT_HIGHLIGHT_COLOR = "rgba(234, 179, 8, 0.4)";

const HEX = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/;
const RGB = /^rgba?\([0-9,.\s%/]+\)$/;

/**
 * Returns a highlight color safe to put in a style prop or attribute.
 *
 * Mirrors the server allowlist (`annotations.rs::valid_color`): hex or numeric
 * `rgb()`/`rgba()` only, max 64 chars. Anything else — quotes, semicolons,
 * angle brackets, `url(`/`expression(` — falls back, so a crafted value stored
 * before validation existed (or written straight into the DB) can never break
 * out of its element at render time.
 */
export function safeHighlightColor(
  value: string | null | undefined,
  fallback: string = DEFAULT_HIGHLIGHT_COLOR,
): string {
  if (!value) return fallback;
  const v = value.trim();
  if (v.length === 0 || v.length > 64) return fallback;
  if (HEX.test(v) || RGB.test(v)) return v;
  return fallback;
}
