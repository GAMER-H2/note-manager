// Publishes the on-screen keyboard height as a CSS variable
// (`--keyboard-height`) on <html>, so layouts can lift content above the
// keyboard on mobile. Returns a cleanup function.
export function initVisualViewport() {
  const vv = typeof window !== "undefined" ? window.visualViewport : null;
  const root = document.documentElement;
  root.style.setProperty("--keyboard-height", "0px");
  if (!vv) return () => {};

  const update = () => {
    // Portion of the layout viewport hidden by the keyboard.
    const hidden = Math.max(0, window.innerHeight - vv.height - vv.offsetTop);
    root.style.setProperty("--keyboard-height", `${Math.round(hidden)}px`);
  };

  vv.addEventListener("resize", update);
  vv.addEventListener("scroll", update);
  update();

  return () => {
    vv.removeEventListener("resize", update);
    vv.removeEventListener("scroll", update);
  };
}
