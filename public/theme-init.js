(function () {
  try {
    var theme = localStorage.getItem("theme") || "system";
    var dark = theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    if (dark) document.documentElement.classList.add("dark");
  } catch (e) {}
  try {
    if (window.__TAURI_INTERNALS__.metadata.currentWindow.label === "menubar") document.documentElement.classList.add("menubar-panel");
  } catch (e) {}
})();
