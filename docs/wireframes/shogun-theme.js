/* Appearance switch shared by the SHOGUN prototypes.

   Three states, same as the Settings segment and apps/desktop: Dark / Light / Auto. The choice
   is written to <html data-theme> and persisted, so moving between the prototypes keeps it.
   Auto follows the system and updates live if the system flips while you're looking. */

(() => {
  const KEY = "shogun-appearance";
  const root = document.documentElement;
  const mq = window.matchMedia("(prefers-color-scheme: light)");

  const resolve = pref => (pref === "auto" ? (mq.matches ? "light" : "dark") : pref);

  function apply(pref){
    root.setAttribute("data-theme", resolve(pref));
    root.setAttribute("data-appearance", pref);
    try { localStorage.setItem(KEY, pref) } catch {}
    document.querySelectorAll(".theme-switch button").forEach(b =>
      b.classList.toggle("on", b.dataset.pref === pref));
    // keep any Settings "Theme" segment in sync with the floating switch
    document.querySelectorAll("[data-theme-seg] span").forEach(s =>
      s.classList.toggle("on", s.dataset.pref === pref));
  }

  let pref = "dark";
  try { pref = localStorage.getItem(KEY) || "dark" } catch {}

  const ICONS = {
    dark:  '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/></svg>',
    light: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>',
    auto:  '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 3v18" /><path d="M12 3a9 9 0 0 1 0 18z" fill="currentColor" stroke="none"/></svg>'
  };

  function mount(){
    if (!document.querySelector(".theme-switch")){
      const el = document.createElement("div");
      el.className = "theme-switch";
      el.setAttribute("role", "group");
      el.setAttribute("aria-label", "Appearance");
      el.innerHTML = ["dark","light","auto"].map(p =>
        `<button data-pref="${p}" title="${p[0].toUpperCase()+p.slice(1)}" aria-label="${p}">${ICONS[p]}</button>`
      ).join("");
      document.body.appendChild(el);
    }
    document.querySelectorAll(".theme-switch button").forEach(b =>
      b.addEventListener("click", () => { pref = b.dataset.pref; apply(pref) }));
    document.querySelectorAll("[data-theme-seg] span").forEach(s =>
      s.addEventListener("click", () => { pref = s.dataset.pref; apply(pref) }));
    apply(pref);
  }

  mq.addEventListener("change", () => { if (pref === "auto") apply("auto") });
  apply(pref);                                  // before paint, so there's no flash
  document.readyState === "loading"
    ? document.addEventListener("DOMContentLoaded", mount)
    : mount();
})();
