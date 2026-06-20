// Text-link viewer. For a plaintext link the <pre> is already filled by the
// server, so this only wires the Copy button. For an encrypted link it decrypts
// the sealed payload with the key from the URL fragment and writes the result
// with textContent (never innerHTML) so the plaintext stays inert.
(() => {
    "use strict";

    const fail = (message) => {
        const note = document.getElementById("status");
        if (note) note.textContent = message;
    };

    document.addEventListener("DOMContentLoaded", async () => {
        const pre = document.getElementById("text-body");
        const copyBtn = document.getElementById("copy-text");
        const sealed = document.getElementById("payload")?.dataset.sealed ?? "";

        if (sealed) {
            const fragment = location.hash.replace(/^#/, "");
            if (!fragment) {
                fail("This link is missing its decryption key (the part after #).");
                return;
            }
            try {
                pre.textContent = await YuioCrypto.open(sealed, YuioCrypto.fragmentToKey(fragment));
                pre.hidden = false;
                if (copyBtn) copyBtn.hidden = false;
                fail("");
            } catch {
                fail("Could not decrypt this text — the key may be wrong or the link corrupted.");
                return;
            }
        }

        copyBtn?.addEventListener("click", async () => {
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
