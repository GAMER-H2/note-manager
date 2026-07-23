// Lightweight platform detection based on the WebView user agent. Good enough
// to gate Android-only UI without pulling in an extra Tauri plugin.
export const isAndroid = () =>
  typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
