(() => {
  const key = "characterModAppearance";
  let theme = "dark";
  try {
    const saved = localStorage.getItem(key);
    if (saved === "light" || saved === "dark") theme = saved;
  } catch {
  }
  document.documentElement.dataset.theme = theme;
  window.characterModAppearance = {
    set(value) {
      if (value !== "light" && value !== "dark") return;
      document.documentElement.dataset.theme = value;
      try {
        localStorage.setItem(key, value);
      } catch {
      }
    }
  };
})();
