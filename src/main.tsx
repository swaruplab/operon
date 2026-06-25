import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

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
