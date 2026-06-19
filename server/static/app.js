// Landing-page logic: one input that auto-detects redirect vs Text (with a
// manual override), a TTL picker, optional burn-after-read and encryption, then
// create the ephemeral link and arm "+ Copy" so the next Enter copies it.
(function () {
    // --- redirect-vs-text detection, mirroring yuiolink_core::detect_kind ---
    function hasScheme(s) {
        var i = s.indexOf(":");
        if (i <= 0) return false;
        return /^[a-zA-Z][a-zA-Z0-9+.-]*$/.test(s.slice(0, i));
    }
    function looksLikeDomain(s) {
        if (/\s/.test(s)) return false;
        var host = s.split(/[\/?#]/)[0].split(":")[0];
        var labels = host.split(".");
        if (labels.length < 2) return false;
        var tld = labels[labels.length - 1];
        if (tld.length < 2 || !/^[a-zA-Z]+$/.test(tld)) return false;
        return labels.every(function (l) { return l.length > 0 && /^[a-zA-Z0-9-]+$/.test(l); });
    }
    function detectKind(value) {
        var t = value.trim();
        if (t === "") return "text";
        if (t.indexOf("\n") !== -1) return "text";
        return (hasScheme(t) || looksLikeDomain(t)) ? "redirect" : "text";
    }
    function normalizeTarget(s) {
        return hasScheme(s) ? s : "https://" + s;
    }

    document.addEventListener("DOMContentLoaded", function () {
        var content = document.getElementById("content");
        var form = document.getElementById("create-form");
        var encrypt = document.getElementById("encrypt");
        var copyBtn = document.getElementById("copy");
        var submitBtn = document.getElementById("submit");
        var linkEl = document.getElementById("link-element");
        var expiryEl = document.getElementById("link-expiry");
        var panel = document.getElementById("link-panel");
        var maxUsesInput = document.getElementById("max-uses");

        var modeButtons = {
            redirect: document.getElementById("mode-redirect"),
            text: document.getElementById("mode-text"),
        };
        var ttlButtons = Array.prototype.slice.call(
            document.querySelectorAll("#ttl-toggle .seg-btn")
        );

        var autoMode = true; // until the user taps a mode, follow detection
        var mode = "text";
        var ttlSeconds = 86400;
        var ttlLabel = "1 day";

        // --- mode (Redirect | Text) ---
        function applyMode(next) {
            mode = next;
            modeButtons.redirect.classList.toggle("active", mode === "redirect");
            modeButtons.text.classList.toggle("active", mode === "text");
        }
        function syncAutoMode() {
            if (autoMode) applyMode(detectKind(content.value));
        }
        Object.keys(modeButtons).forEach(function (key) {
            modeButtons[key].addEventListener("click", function () {
                autoMode = false; // manual tap wins
                applyMode(key);
                content.focus();
            });
        });

        // --- TTL picker ---
        ttlButtons.forEach(function (btn) {
            btn.addEventListener("click", function () {
                ttlButtons.forEach(function (b) { b.classList.remove("active"); });
                btn.classList.add("active");
                ttlSeconds = parseInt(btn.dataset.ttl, 10);
                ttlLabel = btn.textContent.trim();
            });
        });

        // --- auto-growing textarea ---
        function autosize() {
            content.style.height = "auto";
            content.style.height = content.scrollHeight + "px";
        }
        content.addEventListener("input", function () {
            autosize();
            syncAutoMode();
        });

        // --- keyboard: Redirect Enter submits; Text Enter = newline, Cmd/Ctrl-Enter submits ---
        content.addEventListener("keydown", function (event) {
            if (event.key !== "Enter") return;
            if (mode === "redirect") {
                if (!event.shiftKey) {
                    event.preventDefault();
                    form.requestSubmit();
                }
            } else if (event.metaKey || event.ctrlKey) {
                event.preventDefault();
                form.requestSubmit();
            }
        });

        // --- copy ---
        function hasLink() { return linkEl.textContent.trim() !== ""; }
        function selectLinkText() {
            var range = document.createRange();
            range.selectNodeContents(linkEl);
            var sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
        }
        function showReady(url, expiresLabel) {
            linkEl.textContent = url;
            expiryEl.textContent = expiresLabel;
            panel.hidden = false;
            copyBtn.disabled = false;
            selectLinkText();
            copyBtn.focus();
        }
        function doCopy() {
            if (!hasLink()) return;
            var text = linkEl.textContent.trim();
            if (navigator.clipboard && navigator.clipboard.writeText) {
                navigator.clipboard.writeText(text).then(function () {
                    copyBtn.classList.add("copied");
                    copyBtn.textContent = "Copied";
                    setTimeout(function () {
                        copyBtn.classList.remove("copied");
                        copyBtn.textContent = "+ Copy";
                    }, 1500);
                }, function () {});
            }
        }
        copyBtn.addEventListener("click", doCopy);

        // --- submit ---
        form.addEventListener("submit", async function (event) {
            event.preventDefault();
            var raw = content.value;
            if (raw.trim() === "") {
                content.focus();
                return;
            }

            // For redirect we normalize a bare host to https://; text is sent as typed.
            var payload = mode === "redirect" ? normalizeTarget(raw.trim()) : raw;

            var maxUses = null;
            if (maxUsesInput.value.trim() !== "") {
                var n = parseInt(maxUsesInput.value, 10);
                if (!isNaN(n) && n > 0) maxUses = n;
            }

            submitBtn.disabled = true;
            submitBtn.textContent = "Creating…";

            try {
                var body = payload;
                var fragment = "";
                if (encrypt.checked) {
                    var rawKey = YuioCrypto.generateKey();
                    body = await YuioCrypto.seal(payload, rawKey);
                    fragment = "#" + YuioCrypto.keyToFragment(rawKey);
                }

                var resp = await fetch("/api/v1/links", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        kind: mode,
                        content: body,
                        encrypted: encrypt.checked,
                        ttl_seconds: ttlSeconds,
                        max_uses: maxUses,
                    }),
                });

                if (!resp.ok) {
                    var err = await resp.json().catch(function () { return {}; });
                    throw new Error(err.error || "Request failed");
                }

                var data = await resp.json();
                var note = "Expires in " + ttlLabel;
                if (maxUses) note += " · burns after " + maxUses + (maxUses === 1 ? " use" : " uses");
                showReady(data.url + fragment, note);
            } catch (e) {
                alert(e.message || "Could not create the link.");
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = "Create Link";
            }
        });

        // initial state
        autosize();
        syncAutoMode();
        content.focus();
    });
})();
