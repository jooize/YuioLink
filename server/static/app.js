// Progressive enhancement for the landing + result pages. The create form works
// without JavaScript (it posts to `POST /` and a result page comes back). When JS
// is present we intercept the submit for a slicker in-place result, add live type
// detection, keyboard shortcuts, and copy. Encryption (if the operator enabled it)
// is the one path that *requires* JS, since the content is sealed in the browser.
(function () {
    // Which backend to call; empty meta = same origin. Lets a hosted frontend
    // point at another backend (e.g. one with encryption enabled).
    var metaBase = document.querySelector('meta[name="yuiolink-api-base"]');
    var API_BASE = (metaBase && metaBase.content) ? metaBase.content.replace(/\/+$/, "") : "";

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

    function wireCopy(button, getText) {
        button.disabled = false;
        button.addEventListener("click", function () {
            var text = getText();
            if (!text || !navigator.clipboard || !navigator.clipboard.writeText) return;
            navigator.clipboard.writeText(text).then(function () {
                var original = button.textContent;
                button.classList.add("copied");
                button.textContent = "Copied";
                setTimeout(function () {
                    button.classList.remove("copied");
                    button.textContent = original;
                }, 1500);
            }, function () {});
        });
    }

    function initCreate() {
        var content = document.getElementById("content");
        var form = document.getElementById("create-form");
        var encrypt = document.getElementById("encrypt"); // null when encryption is off
        var copyBtn = document.getElementById("copy");
        var submitBtn = document.getElementById("submit");
        var linkEl = document.getElementById("link-element");
        var expiryEl = document.getElementById("link-expiry");
        var panel = document.getElementById("link-panel");
        var maxUsesInput = document.getElementById("max-uses");
        var hint = document.getElementById("detected-hint");
        var autoRadio = document.getElementById("kind-auto");

        function checkedValue(name, fallback) {
            var el = document.querySelector('input[name="' + name + '"]:checked');
            return el ? el.value : fallback;
        }
        function effectiveKind() {
            var v = checkedValue("kind", "auto");
            return v === "auto" ? detectKind(content.value) : v;
        }
        function ttlLabel() {
            var el = document.querySelector('input[name="ttl_seconds"]:checked');
            var label = el && el.nextElementSibling;
            return label ? label.textContent.trim() : "the chosen time";
        }

        function updateHint() {
            if (!hint) return;
            if (autoRadio && autoRadio.checked && content.value.trim() !== "") {
                hint.textContent = "— looks like " + (detectKind(content.value) === "redirect" ? "a redirect" : "text");
            } else {
                hint.textContent = "";
            }
        }

        function autosize() {
            content.style.height = "auto";
            content.style.height = content.scrollHeight + "px";
        }

        content.addEventListener("input", function () {
            autosize();
            updateHint();
        });
        document.querySelectorAll('input[name="kind"]').forEach(function (r) {
            r.addEventListener("change", updateHint);
        });

        // Redirect: Enter submits. Text: Enter = newline, Cmd/Ctrl-Enter submits.
        content.addEventListener("keydown", function (event) {
            if (event.key !== "Enter") return;
            if (effectiveKind() === "redirect") {
                if (!event.shiftKey) { event.preventDefault(); form.requestSubmit(); }
            } else if (event.metaKey || event.ctrlKey) {
                event.preventDefault();
                form.requestSubmit();
            }
        });

        function showReady(url, note) {
            linkEl.textContent = url;
            expiryEl.textContent = note;
            panel.hidden = false;
            var range = document.createRange();
            range.selectNodeContents(linkEl);
            var sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
            copyBtn.focus();
        }
        wireCopy(copyBtn, function () { return linkEl.textContent.trim(); });

        form.addEventListener("submit", async function (event) {
            // Intercept for the in-place result; if JS were off, this same form
            // would post to `POST /` and render a result page instead.
            event.preventDefault();
            var raw = content.value;
            if (raw.trim() === "") { content.focus(); return; }

            var kind = effectiveKind();
            var payload = kind === "redirect" ? normalizeTarget(raw.trim()) : raw;
            var maxUses = null;
            if (maxUsesInput && maxUsesInput.value.trim() !== "") {
                var n = parseInt(maxUsesInput.value, 10);
                if (!isNaN(n) && n > 0) maxUses = n;
            }
            var ttl = parseInt(checkedValue("ttl_seconds", "86400"), 10);
            var label = ttlLabel();

            submitBtn.disabled = true;
            submitBtn.textContent = "Creating…";
            try {
                var bodyContent = payload;
                var fragment = "";
                if (encrypt && encrypt.checked) {
                    var key = YuioCrypto.generateKey();
                    bodyContent = await YuioCrypto.seal(payload, key);
                    fragment = "#" + YuioCrypto.keyToFragment(key);
                }
                var resp = await fetch(API_BASE + "/api/v1/links", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        kind: kind,
                        content: bodyContent,
                        encrypted: !!(encrypt && encrypt.checked),
                        ttl_seconds: ttl,
                        max_uses: maxUses,
                    }),
                });
                if (!resp.ok) {
                    var err = await resp.json().catch(function () { return {}; });
                    throw new Error(err.error || "Request failed");
                }
                var data = await resp.json();
                var note = "Expires in " + label;
                if (maxUses) note += " · burns after " + maxUses + (maxUses === 1 ? " use" : " uses");
                showReady(data.url + fragment, note);
            } catch (e) {
                alert(e.message || "Could not create the link.");
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = "Create Link";
            }
        });

        autosize();
        updateHint();
        content.focus();
    }

    document.addEventListener("DOMContentLoaded", function () {
        if (document.getElementById("create-form")) initCreate();

        // Result page (no-JS create landed here): wire its copy button.
        var resultCopy = document.getElementById("copy-link");
        var resultLink = document.getElementById("link-element");
        if (resultCopy && resultLink) {
            wireCopy(resultCopy, function () { return resultLink.textContent.trim(); });
        }
    });
})();
