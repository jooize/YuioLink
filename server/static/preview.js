// Redirect preview pages. The page works entirely without this; all it adds is
// ⌘C / Ctrl-C, which copies the destination — the one piece of the page a visitor
// would want to take with them.
//
// It runs only where the full destination is already on screen (an unlimited
// preview, or a revealed one). A limited link deliberately shows nothing but the
// domain until a use is spent, so there is nothing here to copy and the shortcut
// stays out of the way.
(() => {
    "use strict";

    document.addEventListener("DOMContentLoaded", () => {
        const dest = document.getElementById("destination");
        if (!dest) return;

        document.addEventListener("keydown", (event) => {
            if (!(event.metaKey || event.ctrlKey)) return;
            if (event.key !== "c" && event.key !== "C") return;
            // A selection the visitor made themselves always wins — copying it is
            // what they asked for, and this page has text worth selecting.
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed) return;
            const url = dest.textContent.trim();
            if (!url) return;
            event.preventDefault();
            navigator.clipboard.writeText(url).then(() => {
                dest.classList.add("copied");
                setTimeout(() => dest.classList.remove("copied"), 1500);
            }, () => {
                // Clipboard unavailable (insecure context) or permission denied.
            });
        });
    });
})();
