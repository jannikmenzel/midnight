const MIN_VISIBLE_MS = 900;

let gone: Promise<void> | null = null;

export function dismissSplash(immediate = false): Promise<void> {
  if (gone) return gone;
  const el = document.getElementById("splash");
  if (!el) return (gone = Promise.resolve());
  if (immediate) {
    el.remove();
    return (gone = Promise.resolve());
  }
  gone = new Promise((resolve) => {
    const remove = () => {
      el.remove();
      resolve();
    };
    setTimeout(() => {
      el.classList.add("out");
      el.addEventListener("animationend", remove, { once: true });
      setTimeout(remove, 600);
    }, Math.max(0, MIN_VISIBLE_MS - performance.now()));
  });
  return gone;
}
