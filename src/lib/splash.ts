const MIN_VISIBLE_MS = 900;

export function dismissSplash(immediate = false) {
  const el = document.getElementById("splash");
  if (!el || el.dataset.leaving !== undefined) return;
  el.dataset.leaving = "";
  if (immediate) return el.remove();
  setTimeout(() => {
    el.classList.add("out");
    el.addEventListener("animationend", () => el.remove(), { once: true });
    setTimeout(() => el.remove(), 600);
  }, Math.max(0, MIN_VISIBLE_MS - performance.now()));
}
