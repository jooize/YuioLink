// Text-link viewer. For a plaintext link the <pre> is already filled by the
// server, so this only wires the Copy button. For an encrypted link it decrypts
// the sealed payload with the key from the URL fragment and writes the result
// with textContent (never innerHTML) so the plaintext stays inert.
(function () {
    function fail(message) {
        var note = document.getElementById("status");
        if (note) note.textContent = message;
    }

    document.addEventListener("DOMContentLoaded", async function () {
        var pre = document.getElementById("text-body");
        var copyBtn = document.getElementById("copy-text");
        var payload = document.getElementById("payload");
        var sealed = payload ? payload.dataset.sealed : "";

        if (sealed) {
            var fragment = location.hash.replace(/^#/, "");
            if (!fragment) {
                fail("This link is missing its decryption key (the part after #).");
                return;
            }
            try {
                var text = await YuioCrypto.open(sealed, YuioCrypto.fragmentToKey(fragment));
                pre.textContent = text;
                pre.hidden = false;
                if (copyBtn) copyBtn.hidden = false;
                fail("");
            } catch (e) {
                fail("Could not decrypt this text — the key may be wrong or the link corrupted.");
                return;
            }
        }

        if (copyBtn) {
            copyBtn.addEventListener("click", function () {
                var text = pre.textContent;
                if (navigator.clipboard && navigator.clipboard.writeText) {
                    navigator.clipboard.writeText(text).then(function () {
                        copyBtn.textContent = "Copied";
                        setTimeout(function () { copyBtn.textContent = "Copy"; }, 1500);
                    }, function () {});
                }
            });
        }
    });
})();
