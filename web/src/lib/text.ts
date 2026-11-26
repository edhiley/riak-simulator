// Short forms for text that the user types, so long input does not break
// the layout. The full text is shown on hover.

export const KEY_MAX = 10;

/** A key of more than 10 characters becomes its first 9 and an ellipsis. */
export function shortKey(key: string): string {
  return key.length > KEY_MAX ? `${key.slice(0, KEY_MAX - 1)}…` : key;
}
