(() => {
  const icons = {
    fullSwapTab: "contact",
    configsTab: "layers",
    bodyTab: "person-standing",
    hairTab: "scissors",
    headbandTab: "stretch-horizontal",
    previewTab: "box",
    cliTab: "terminal",
    healthTab: "shield-check"
  };
  for (const [id, name] of Object.entries(icons)) {
    const button = document.getElementById(id);
    if (!button) continue;
    const icon = document.createElement("i");
    icon.dataset.lucide = name;
    button.querySelector("span")?.replaceWith(icon);
    button.title = button.textContent.trim();
  }
  const choices = [...document.querySelectorAll("[data-theme-choice]")];
  function sync() {
    for (const button of choices) {
      const active = button.dataset.themeChoice === document.documentElement.dataset.theme;
      button.classList.toggle("active", active);
      button.setAttribute("aria-pressed", String(active));
    }
  }
  for (const button of choices) button.addEventListener("click", () => {
    window.characterModAppearance.set(button.dataset.themeChoice);
    sync();
  });
  window.lucide.createIcons({ attrs: { "aria-hidden": "true", focusable: "false", "stroke-width": 1.7 } });
  sync();
})();
