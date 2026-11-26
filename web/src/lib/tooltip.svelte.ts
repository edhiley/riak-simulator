// One tooltip for the page, so definitions show above scrolling panels.
export const tooltip = $state<{ text: string; x: number; y: number; bottom: number; visible: boolean }>({
  text: "",
  x: 0,
  y: 0,
  bottom: 0,
  visible: false,
});

export function showTooltip(el: Element, text: string) {
  const r = el.getBoundingClientRect();
  tooltip.text = text;
  tooltip.x = r.left + r.width / 2;
  tooltip.y = r.top;
  tooltip.bottom = r.bottom;
  tooltip.visible = true;
}

export function hideTooltip() {
  tooltip.visible = false;
}
