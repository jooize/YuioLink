// Progressive enhancement for the landing + result pages.
//
// The create form works WITHOUT JavaScript (it posts to `POST /` and a result page
// comes back). This script is the enhancement layer: an in-place result, the kind
// named on the button, keyboard shortcuts, copy, and a created-link history.
//
// It targets modern browsers deliberately. Anything too old to run this is served
// the no-JS path instead — that is the universal fallback, so there is no ES5 /
// legacy shimming here. `try/catch` guards only cover modern APIs that can still
// fail at runtime (localStorage in private mode, clipboard when denied).
(() => {
    "use strict";

    // Which backend to call; empty meta = same origin. Lets a hosted frontend point
    // at another backend (e.g. one with encryption enabled).
    const metaBase = document.querySelector('meta[name="yuiolink-api-base"]');
    const API_BASE = metaBase?.content ? metaBase.content.replace(/\/+$/, "") : "";

    // --- redirect-vs-text detection, mirroring yuiolink_core::detect_kind ---
    const hasScheme = (s) => {
        const i = s.indexOf(":");
        return i > 0 && /^[a-zA-Z][a-zA-Z0-9+.-]*$/.test(s.slice(0, i));
    };
    const looksLikeDomain = (s) => {
        if (/\s/.test(s)) return false;
        const host = s.split(/[/?#]/)[0].split(":")[0];
        const labels = host.split(".");
        if (labels.length < 2) return false;
        // Unicode-aware so internationalized domains (åäö.se, münchen.de) match,
        // mirroring yuiolink_core::looks_like_domain.
        const tld = labels.at(-1);
        if (!/^\p{L}{2,}$/u.test(tld)) return false;
        return labels.every((l) => /^[\p{L}\p{N}-]+$/u.test(l));
    };
    const detectKind = (value) => {
        const t = value.trim();
        if (t === "") return "text";
        if (t.includes("\n")) return "text";
        return hasScheme(t) || looksLikeDomain(t) ? "redirect" : "text";
    };
    const kindLabel = (k) => (k === "redirect" ? "Redirect" : "Text");
    const normalizeTarget = (s) => (hasScheme(s) ? s : `https://${s}`);

    // Platform copy shortcut shown beside a freshly created (and selected) link.
    const isMac = /mac|iphone|ipad|ipod/i.test(navigator.platform || navigator.userAgent || "");
    const COPY_HINT = `${isMac ? "⌘" : "Ctrl+"}C to copy`;

    // Flash "Copied" on a button; `restore` re-derives its label afterwards (the
    // create-page Copy button toggles between "+ Copy" and "Copy").
    const flashCopied = (button, restore) => {
        const prev = button.textContent;
        button.classList.add("copied");
        button.textContent = "Copied";
        setTimeout(() => {
            button.classList.remove("copied");
            if (restore) restore(); else button.textContent = prev;
        }, 1500);
    };
    const copyToClipboard = async (text, button, restore) => {
        if (!text) return;
        try {
            await navigator.clipboard.writeText(text);
            flashCopied(button, restore);
        } catch {
            // Clipboard unavailable (insecure context) or permission denied.
        }
    };
    // Simple copy wiring for the result/text pages (the label never changes).
    const wireCopy = (button, getText) => {
        button.disabled = false;
        button.addEventListener("click", () => copyToClipboard(getText(), button));
    };

    // --- created-link history ---
    // Kept in memory for the session by default. Persisted to localStorage ONLY when
    // the user explicitly opts in via "Save on this device"; opting back out deletes
    // the stored copy.
    const HISTORY_KEY = "yuiolink:history";
    const PERSIST_KEY = "yuiolink:history:persist";
    const HISTORY_MAX = 20;
    let memHistory = [];
    let persistEnabled = false;
    // The link currently shown in the result panel; excluded from the history list
    // below so it is not displayed twice.
    let currentResultUrl = null;

    const lsGet = (k) => { try { return localStorage.getItem(k); } catch { return null; } };
    const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch { /* full or blocked */ } };
    const lsDel = (k) => { try { localStorage.removeItem(k); } catch { /* blocked */ } };

    const usesSuffix = (uses) => {
        if (uses === 1) return " · one-time";
        if (uses) return ` · max ${uses} uses`;
        return "";
    };
    // SQLite "YYYY-MM-DD HH:MM:SS" is UTC; make it explicit so Date parses correctly.
    const parseUtc = (s) => (s ? new Date(`${s.replace(" ", "T")}Z`) : null);
    const humanizeUntil = (iso) => {
        const d = parseUtc(iso);
        if (!d || Number.isNaN(d.getTime())) return "";
        const ms = d.getTime() - Date.now();
        if (ms <= 0) return "expired";
        const mins = Math.round(ms / 60000);
        if (mins < 60) return `expires in ${mins} min`;
        const hrs = Math.round(mins / 60);
        if (hrs < 48) return `expires in ${hrs} h`;
        return `expires in ${Math.round(hrs / 24)} d`;
    };
    const pruneHistory = (list) => list.filter((it) => {
        const d = parseUtc(it.expires);
        return !d || Number.isNaN(d.getTime()) || d.getTime() > Date.now();
    });

    const loadPersisted = () => {
        persistEnabled = lsGet(PERSIST_KEY) === "1";
        if (persistEnabled) {
            try { memHistory = pruneHistory(JSON.parse(lsGet(HISTORY_KEY)) ?? []); }
            catch { memHistory = []; }
        }
    };
    const persistNow = () => { if (persistEnabled) lsSet(HISTORY_KEY, JSON.stringify(memHistory)); };
    const setPersist = (on) => {
        persistEnabled = on;
        if (on) { lsSet(PERSIST_KEY, "1"); persistNow(); }
        else { lsDel(PERSIST_KEY); lsDel(HISTORY_KEY); } // forget what was stored
    };
    const addHistory = (entry) => {
        memHistory = pruneHistory(memHistory).filter((it) => it.url !== entry.url);
        memHistory.unshift(entry);
        if (memHistory.length > HISTORY_MAX) memHistory.length = HISTORY_MAX;
        persistNow();
    };

    const renderHistory = () => {
        memHistory = pruneHistory(memHistory);
        persistNow();
        // The link in the result panel is shown there, not repeated in the list.
        const shown = currentResultUrl
            ? memHistory.filter((it) => it.url !== currentResultUrl)
            : [...memHistory];
        const n = shown.length;

        const indicator = document.getElementById("storage-indicator");
        if (indicator) {
            indicator.classList.toggle("shown", n > 0);
            if (n > 0) {
                const where = persistEnabled ? "saved on this device" : "lost when this tab closes";
                indicator.textContent = `History · ${n} ${n === 1 ? "link" : "links"} (${where}) ›`;
            } else {
                indicator.textContent = "";
            }
        }

        const section = document.getElementById("history");
        const listEl = document.getElementById("history-list");
        if (!section || !listEl) return;
        const persistBox = document.getElementById("history-persist");
        if (persistBox) persistBox.checked = persistEnabled;

        listEl.replaceChildren();
        if (n === 0) { section.hidden = true; return; }
        section.hidden = false;
        for (const it of shown) {
            const li = document.createElement("li");
            li.className = "history-item";

            const text = document.createElement("div");
            text.className = "history-text";
            const url = document.createElement("code");
            url.className = "history-url";
            url.textContent = it.url;
            const meta = document.createElement("small");
            meta.className = "history-meta";
            meta.textContent = `${kindLabel(it.kind)} · ${humanizeUntil(it.expires)}${usesSuffix(it.uses)}`;
            text.append(url, meta);

            const copy = document.createElement("button");
            copy.className = "history-copy";
            copy.type = "button";
            copy.textContent = "Copy";
            copy.addEventListener("click", () => copyToClipboard(it.url, copy));

            li.append(text, copy);
            listEl.append(li);
        }
    };

    const initCreate = () => {
        const content = document.getElementById("content");
        const form = document.getElementById("create-form");
        const encrypt = document.getElementById("encrypt"); // null when encryption is off
        const copyBtn = document.getElementById("copy");
        const submitBtn = document.getElementById("submit");
        const linkEl = document.getElementById("link-element");
        const metaEl = document.getElementById("link-expiry");
        const panel = document.getElementById("link-panel");
        const ttlCustomValue = document.getElementById("ttl-custom-value");
        const ttlCustomUnit = document.getElementById("ttl-custom-unit");
        const limitCustomValue = document.getElementById("limit-custom-value");

        const UNIT_SECS = { m: 60, h: 3600, d: 86400 };
        const UNIT_NAME = { m: "minute", h: "hour", d: "day" };

        const checkedValue = (name, fallback) =>
            document.querySelector(`input[name="${name}"]:checked`)?.value ?? fallback;

        // Expiry as { secs, label }, honoring the Custom field.
        const ttlInfo = () => {
            const v = checkedValue("ttl_seconds", "3600");
            if (v === "custom") {
                let n = Number.parseInt(ttlCustomValue.value, 10);
                if (Number.isNaN(n) || n < 0) n = 0;
                const u = ttlCustomUnit.value;
                return {
                    secs: n * (UNIT_SECS[u] ?? 60),
                    label: `${n} ${UNIT_NAME[u]}${n === 1 ? "" : "s"}`,
                };
            }
            const radio = document.querySelector('input[name="ttl_seconds"]:checked');
            return { secs: Number.parseInt(v, 10), label: radio?.nextElementSibling?.textContent.trim() ?? "1 hour" };
        };

        // Use limit: 1, a custom positive count, or null (unlimited).
        const maxUses = () => {
            const v = checkedValue("limit", "unlimited");
            if (v === "1") return 1;
            if (v === "custom") {
                const n = Number.parseInt(limitCustomValue.value, 10);
                return !Number.isNaN(n) && n > 0 ? n : null;
            }
            return null;
        };

        const metaNote = (kind, ttl, uses) =>
            `${kindLabel(kind)} · expires in ${ttl.label}${usesSuffix(uses)}`;

        // The primary button names what it will create.
        const updateSubmitLabel = () => {
            submitBtn.textContent = content.value.trim() === ""
                ? "Create Link"
                : `Create ${kindLabel(detectKind(content.value))} Link`;
        };

        // "+ Copy" creates first, then copies. Once the current input (content +
        // settings) already has a link, it drops the "+" and just copies.
        let lastSig = null;
        let lastUrl = null;
        const currentSig = () => JSON.stringify({
            c: content.value,
            t: ttlInfo().secs,
            m: maxUses(),
            e: !!encrypt?.checked,
        });
        const isCreated = () => lastUrl !== null && lastSig === currentSig();
        const refreshCopyLabel = () => { copyBtn.textContent = isCreated() ? "Copy" : "+ Copy"; };

        const autosize = () => {
            content.style.height = "auto";
            const h = content.scrollHeight;
            content.style.height = `${h}px`;
            // Only allow scrolling once the content actually exceeds the max height;
            // otherwise a single line (or fully-shown text) never scrolls.
            const maxPx = Number.parseFloat(getComputedStyle(content).maxHeight);
            content.style.overflowY = !Number.isNaN(maxPx) && h > maxPx ? "auto" : "hidden";
        };

        content.addEventListener("input", () => {
            autosize();
            updateSubmitLabel();
            refreshCopyLabel();
        });
        // Any settings change invalidates the "already created" state.
        form.addEventListener("change", refreshCopyLabel);
        form.addEventListener("input", refreshCopyLabel);

        // Focus the Custom field as soon as its segment is picked.
        const ttlCustomRadio = document.getElementById("ttl-custom");
        ttlCustomRadio?.addEventListener("change", () => {
            if (ttlCustomRadio.checked) ttlCustomValue.focus();
        });
        const limitCustomRadio = document.getElementById("limit-custom");
        limitCustomRadio?.addEventListener("change", () => {
            if (limitCustomRadio.checked) limitCustomValue.focus();
        });

        // Redirect: Enter submits. Text: Enter = newline, Cmd/Ctrl-Enter submits.
        content.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") return;
            if (detectKind(content.value) === "redirect") {
                if (!event.shiftKey) { event.preventDefault(); form.requestSubmit(); }
            } else if (event.metaKey || event.ctrlKey) {
                event.preventDefault();
                form.requestSubmit();
            }
        });

        // Focusing the field places the caret at the end (clicking still positions it).
        content.addEventListener("focus", () => {
            const n = content.value.length;
            content.setSelectionRange(n, n);
        });

        const showReady = (url, note) => {
            linkEl.textContent = url;
            metaEl.textContent = note;
            panel.hidden = false;
            const range = document.createRange();
            range.selectNodeContents(linkEl);
            const sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
            // Focus the panel (which precedes the form in the DOM) so the link
            // selection survives for ⌘C and the next Tab lands on the input.
            panel.focus({ preventScroll: true });
        };

        // Create the link via the JSON API and show it in place. Returns the final
        // URL (with any encryption key fragment), or null on empty/failed input.
        const createLink = async () => {
            const raw = content.value;
            if (raw.trim() === "") { content.focus(); return null; }

            const kind = detectKind(raw);
            const payload = kind === "redirect" ? normalizeTarget(raw.trim()) : raw;
            const ttl = ttlInfo();
            const uses = maxUses();

            const restoreLabel = submitBtn.textContent;
            submitBtn.disabled = true;
            copyBtn.disabled = true;
            submitBtn.textContent = "Creating…";
            try {
                let bodyContent = payload;
                let fragment = "";
                if (encrypt?.checked) {
                    const key = YuioCrypto.generateKey();
                    bodyContent = await YuioCrypto.seal(payload, key);
                    fragment = `#${YuioCrypto.keyToFragment(key)}`;
                }
                const resp = await fetch(`${API_BASE}/api/v1/links`, {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        kind,
                        content: bodyContent,
                        encrypted: !!encrypt?.checked,
                        ttl_seconds: ttl.secs,
                        max_uses: uses,
                    }),
                });
                if (!resp.ok) {
                    const err = await resp.json().catch(() => ({}));
                    throw new Error(err.error || "Request failed");
                }
                const data = await resp.json();
                const url = data.url + fragment;
                showReady(url, metaNote(kind, ttl, uses));
                lastSig = currentSig();
                lastUrl = url;
                currentResultUrl = url;
                addHistory({ url, kind, uses, expires: data.expires_at });
                renderHistory();
                return url;
            } catch (e) {
                alert(e.message || "Could not create the link.");
                return null;
            } finally {
                submitBtn.disabled = false;
                copyBtn.disabled = false;
                submitBtn.textContent = restoreLabel;
                refreshCopyLabel();
            }
        };

        form.addEventListener("submit", (event) => {
            // Intercept for the in-place result; without JS this same form posts to
            // `POST /` and renders a result page instead.
            event.preventDefault();
            createLink();
        });

        // NB: not an async handler — the clipboard call must run synchronously inside
        // the click so it keeps the user activation it requires.
        copyBtn.addEventListener("click", () => {
            if (isCreated() && lastUrl) {
                copyToClipboard(lastUrl, copyBtn, refreshCopyLabel);
                return;
            }
            // First click has to create the link first; a clipboard write *after* the
            // network await loses the activation it needs (the "first click does
            // nothing" bug). Hand the clipboard a Promise of the text up front so the
            // write stays bound to this click. createLink is called exactly once.
            const urlPromise = createLink();
            const fallback = () =>
                urlPromise.then((url) => { if (url) copyToClipboard(url, copyBtn, refreshCopyLabel); });
            if (typeof ClipboardItem !== "undefined") {
                navigator.clipboard.write([
                    new ClipboardItem({
                        "text/plain": urlPromise.then((url) => {
                            if (!url) throw new Error("no url");
                            return new Blob([url], { type: "text/plain" });
                        }),
                    }),
                ]).then(() => flashCopied(copyBtn, refreshCopyLabel), fallback);
            } else {
                fallback();
            }
        });
        copyBtn.disabled = false;

        const resultHint = document.getElementById("result-hint");
        if (resultHint) resultHint.textContent = COPY_HINT;

        // Jump to the history without leaving "#history" in the address bar.
        document.getElementById("storage-indicator")?.addEventListener("click", (event) => {
            event.preventDefault();
            document.getElementById("history")?.scrollIntoView({ behavior: "smooth", block: "start" });
            history.replaceState(null, "", location.pathname + location.search);
        });

        const persistBox = document.getElementById("history-persist");
        persistBox?.addEventListener("change", () => {
            setPersist(persistBox.checked);
            renderHistory();
        });
        document.getElementById("history-clear")?.addEventListener("click", () => {
            memHistory = [];
            lsDel(HISTORY_KEY);
            renderHistory();
        });

        autosize();
        updateSubmitLabel();
        refreshCopyLabel();
        renderHistory();
        content.focus();
    };

    // The no-JS result page: record the just-created link so it shows in history
    // when the user returns to the landing page (persisted only if opted in).
    const recordResultPage = (linkEl) => {
        const url = linkEl.textContent.trim();
        if (!url) return;
        const metaText = document.getElementById("link-expiry")?.textContent ?? "";
        const when = metaText.match(/expires (\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) UTC/);
        const usesMatch = metaText.match(/max (\d+) uses/);
        addHistory({
            url,
            kind: /^Text/.test(metaText) ? "text" : "redirect",
            uses: /one-time/.test(metaText) ? 1 : (usesMatch ? Number.parseInt(usesMatch[1], 10) : null),
            expires: when ? when[1] : null,
        });
    };

    document.addEventListener("DOMContentLoaded", () => {
        loadPersisted();
        if (document.getElementById("create-form")) initCreate();

        // Result page (no-JS create landed here): wire its copy button and record it.
        const resultCopy = document.getElementById("copy-link");
        const resultLink = document.getElementById("link-element");
        if (resultCopy && resultLink) {
            wireCopy(resultCopy, () => resultLink.textContent.trim());
            recordResultPage(resultLink);
            // Select the link and show the copy shortcut, matching the in-place result.
            const range = document.createRange();
            range.selectNodeContents(resultLink);
            const sel = window.getSelection();
            sel.removeAllRanges();
            sel.addRange(range);
            const hint = document.getElementById("result-hint");
            if (hint) hint.textContent = COPY_HINT;
        }
    });
})();
