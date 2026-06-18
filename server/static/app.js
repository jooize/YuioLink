// Landing-page logic: create a (optionally encrypted) redirect, then select the
// link and arm the "+ Copy" segment so the next Enter copies it.
(function () {
    document.addEventListener("DOMContentLoaded", function () {
        var input = document.getElementById("uri");
        var form = document.getElementById("create-form");
        var encrypt = document.getElementById("encrypt");
        var copyBtn = document.getElementById("copy");
        var submitBtn = document.getElementById("submit");
        var linkEl = document.getElementById("link-element");
        var panel = document.getElementById("link-panel");

        function hasLink() {
            return linkEl.textContent.trim() !== "";
        }

        function selectLinkText() {
            var range = document.createRange();
            range.selectNodeContents(linkEl);
            var sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
        }

        // Reveal the link, select its text, and focus Copy so Enter copies it.
        function showReady() {
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

        form.addEventListener("submit", async function (event) {
            event.preventDefault();
            var uri = input.value.trim();
            if (!uri) {
                input.focus();
                return;
            }

            submitBtn.disabled = true;
            submitBtn.textContent = "Creating…";

            try {
                var content = uri;
                var fragment = "";
                if (encrypt.checked) {
                    var rawKey = YuioCrypto.generateKey();
                    content = await YuioCrypto.seal(uri, rawKey);
                    fragment = "#" + YuioCrypto.keyToFragment(rawKey);
                }

                var resp = await fetch("/api/v1/links", {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        kind: "redirect",
                        content: content,
                        encrypted: encrypt.checked,
                    }),
                });

                if (!resp.ok) {
                    var err = await resp.json().catch(function () { return {}; });
                    throw new Error(err.error || "Request failed");
                }

                var data = await resp.json();
                linkEl.textContent = data.url + fragment;
                showReady();
            } catch (e) {
                input.setCustomValidity ? input.setCustomValidity("") : null;
                alert(e.message || "Could not create the link.");
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = "Create Link";
            }
        });

        input.focus();
        input.select();
    });
})();
