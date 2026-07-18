import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// crypto.randomUUID() is used in ~30 places (session ids, tab keys, …) with no
// fallback. It requires a secure context and webkit2gtk ≥ 2.36; on an older or
// oddly-configured Linux webview it can be undefined, which would throw in a
// useState initializer and white-screen the app on mount. Install a v4 fallback
// once, before anything runs, so the app degrades instead of dying.
if (typeof crypto !== "undefined" && typeof (crypto as { randomUUID?: unknown }).randomUUID !== "function") {
  (crypto as { randomUUID: () => string }).randomUUID = () => {
    const b = new Uint8Array(16);
    if (crypto.getRandomValues) crypto.getRandomValues(b);
    else for (let i = 0; i < 16; i++) b[i] = Math.floor(Math.random() * 256);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    const h = Array.from(b, (x) => x.toString(16).padStart(2, "0"));
    return `${h[0]}${h[1]}${h[2]}${h[3]}-${h[4]}${h[5]}-${h[6]}${h[7]}-${h[8]}${h[9]}-${h[10]}${h[11]}${h[12]}${h[13]}${h[14]}${h[15]}`;
  };
}

// Disable the macOS WKWebView's auto-capitalization / autocorrect / spellcheck
// on every input and textarea. Operon's fields hold technical values — hosts,
// usernames, paths (e.g. /dfs3b/operonws/$USER), SLURM accounts, conda envs,
// module names, commands, search queries — where capitalizing the first letter
// or "correcting" the text is always wrong. We set it declaratively on <body>
// (index.html) AND at runtime here, with a MutationObserver, so inputs added
// dynamically (modals, the SSH form, Monaco/xterm helper textareas) are covered.
function hardenInput(el: Element) {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    el.setAttribute("autocapitalize", "off");
    el.setAttribute("autocorrect", "off");
    el.spellcheck = false;
  }
}
function hardenAllInputs() {
  document.querySelectorAll("input, textarea").forEach(hardenInput);
  const observer = new MutationObserver((mutations) => {
    for (const m of mutations) {
      for (const node of m.addedNodes) {
        if (node instanceof Element) {
          hardenInput(node);
          node.querySelectorAll("input, textarea").forEach(hardenInput);
        }
      }
    }
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

hardenAllInputs();
