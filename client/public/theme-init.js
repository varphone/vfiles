// Apply the saved theme before first paint; otherwise CSS uses prefers-color-scheme.
(function () {
  try {
    var mode = localStorage.getItem("vfiles:theme");
    if (mode === "light" || mode === "dark") {
      document.documentElement.dataset.theme = mode;
    }
  } catch {
    // localStorage may be unavailable in private browsing contexts.
  }
})();
