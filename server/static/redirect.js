// Encrypted-redirect viewer: decrypt the target with the key from the URL
// fragment, refuse anything but an allowlisted scheme, then navigate.
(() => {
    "use strict";

    const ALLOWED = /^(https?|mailto|tel|sms|ftp|ftps|magnet|spotify|xmpp|irc|ircs|matrix):/i;

    const fail = (message) => {
        const note = document.getElementById("status");
        if (note) note.textContent = message;
    };

    document.addEventListener("DOMContentLoaded", async () => {
        const sealed = document.getElementById("payload")?.dataset.sealed ?? "";
        const fragment = location.hash.replace(/^#/, "");

        if (!sealed) {
            fail("This link is missing its encrypted payload.");
            return;
        }
        if (!fragment) {
            fail("This link is missing its decryption key (the part after #).");
            return;
        }

        try {
            const url = await YuioCrypto.open(sealed, YuioCrypto.fragmentToKey(fragment));
            if (!ALLOWED.test(url)) {
                fail("Refusing to open a link with a disallowed scheme.");
                return;
            }
            location.replace(url);
        } catch {
            fail("Could not decrypt this link — the key may be wrong or the link corrupted.");
        }
    });
})();
