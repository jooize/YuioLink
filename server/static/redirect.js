// Encrypted-redirect viewer: decrypt the target with the key from the URL
// fragment, refuse anything but an allowlisted scheme, then navigate.
(function () {
    var ALLOWED = /^(https?|mailto|tel|sms|ftp|ftps|magnet|spotify|xmpp|irc|ircs|matrix):/i;

    function fail(message) {
        var note = document.getElementById("status");
        if (note) note.textContent = message;
    }

    document.addEventListener("DOMContentLoaded", async function () {
        var payload = document.getElementById("payload");
        var sealed = payload ? payload.dataset.sealed : "";
        var fragment = location.hash.replace(/^#/, "");

        if (!sealed) {
            fail("This link is missing its encrypted payload.");
            return;
        }
        if (!fragment) {
            fail("This link is missing its decryption key (the part after #).");
            return;
        }

        try {
            var url = await YuioCrypto.open(sealed, YuioCrypto.fragmentToKey(fragment));
            if (!ALLOWED.test(url)) {
                fail("Refusing to open a link with a disallowed scheme.");
                return;
            }
            location.replace(url);
        } catch (e) {
            fail("Could not decrypt this link — the key may be wrong or the link corrupted.");
        }
    });
})();
