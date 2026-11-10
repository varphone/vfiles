// Apply the theme before first paint. Always resolve to an explicit data-theme
// (saved preference, else the system scheme) so the CSS only needs one copy of
// each theme's variables instead of also shipping a prefers-color-scheme block.
(function () {
  try {
    var mode = localStorage.getItem("vfiles:theme");
    if (mode !== "light" && mode !== "dark") {
      mode = window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light";
    }
    document.documentElement.dataset.theme = mode;
  } catch {
    // localStorage may be unavailable in private browsing contexts; still honor
    // the system scheme so the single-copy CSS has an explicit theme.
    try {
      document.documentElement.dataset.theme = window.matchMedia(
        "(prefers-color-scheme: dark)",
      ).matches
        ? "dark"
        : "light";
    } catch {
      /* matchMedia unavailable: CSS defaults to light */
    }
  }
})();
