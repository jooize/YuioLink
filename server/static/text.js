// Text-link viewer. The <pre> is already filled by the server; this only wires
// the Copy button, which ships hidden (it is dead without JavaScript).
(() => {
    "use strict";

    document.addEventListener("DOMContentLoaded", () => {
        const pre = document.getElementById("text-body");
        const copyBtn = document.getElementById("copy-text");
        if (!pre || !copyBtn) return;

        copyBtn.hidden = false;
        copyBtn.addEventListener("click", async () => {
            try {
                await navigator.clipboard.writeText(pre.textContent);
                copyBtn.textContent = "Copied";
                setTimeout(() => { copyBtn.textContent = "Copy"; }, 1500);
            } catch {
                // Clipboard unavailable (insecure context) or permission denied.
            }
        });
    });
})();
