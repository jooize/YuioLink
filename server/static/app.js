// Progressive enhancement for the landing + result pages.
//
// The create form works WITHOUT JavaScript (it posts to `POST /` and a result page
// comes back). This script is the enhancement layer: an in-place result, the kind
// named on the button, keyboard shortcuts, copy, a live expiry countdown, and a
// created-link history.
//
// It targets modern browsers deliberately. Anything too old to run this is served
// the no-JS path instead — that is the universal fallback, so there is no ES5 /
// legacy shimming here. `try/catch` guards only cover modern APIs that can still
// fail at runtime (localStorage in private mode, clipboard when denied).
(() => {
    "use strict";

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

    // --- clipboard ---
    const flashCopied = (button) => {
        button.classList.add("copied");
        button.textContent = "Copied";
        setTimeout(() => {
            button.classList.remove("copied");
            button.textContent = "Copy";
        }, 1500);
    };
    const copyToClipboard = async (text, button) => {
        if (!text) return;
        try {
            await navigator.clipboard.writeText(text);
            flashCopied(button);
        } catch {
            // Clipboard unavailable (insecure context) or permission denied.
        }
    };

    // --- expiry countdown (live; shared by the result and the history list) ---
    const usesSuffix = (uses) => {
        if (uses === 1) return " · one-time";
        if (uses) return ` · max ${uses} uses`;
        return "";
    };
    // SQLite "YYYY-MM-DD HH:MM:SS" is UTC; make it explicit so Date parses correctly.
    const parseUtc = (s) => (s ? new Date(`${s.replace(" ", "T")}Z`) : null);

    // Remaining time as { text, level }. floor minutes (so "1 min" holds for the whole
    // last minute, then seconds tick to "1s" — never a jump to 0), and the hour band
    // starts at 59 min so a 1-hour link reads "1 hour" for its first minute, then
    // "58 min". Flags "soon" (≤5 min, yellow) then "now" (last minute, red).
    const formatCountdown = (expiresIso) => {
        const d = parseUtc(expiresIso);
        if (!d || Number.isNaN(d.getTime())) return { text: "", level: "" };
        const s = Math.round((d.getTime() - Date.now()) / 1000);
        if (s <= 0) return { text: "expired", level: "now" };
        let text;
        if (s < 60) text = `${s}s`;                            // last minute: seconds
        else if (s < 3540) { const m = Math.floor(s / 60); text = `${m} minute${m === 1 ? "" : "s"}`; }
        else if (s < 82800) { const h = Math.round(s / 3600); text = `${h} hour${h === 1 ? "" : "s"}`; }
        else { const days = Math.round(s / 86400); text = `${days} day${days === 1 ? "" : "s"}`; }
        const level = s < 60 ? "now" : s <= 300 ? "soon" : "";
        return { text, level };
    };
    const updateCountdown = (span) => {
        const { text, level } = formatCountdown(span.dataset.expires);
        span.textContent = text;
        span.classList.toggle("expiring-soon", level === "soon");
        span.classList.toggle("expiring-now", level === "now");
    };
    // Build "<kind> · expires in <live countdown><uses>" into `metaEl` (no innerHTML).
    const buildMeta = (metaEl, kind, expiresIso, uses) => {
        metaEl.replaceChildren();
        metaEl.append(`${kindLabel(kind)} · expires in `);
        const span = document.createElement("span");
        span.className = "countdown";
        span.dataset.expires = expiresIso ?? "";
        updateCountdown(span);
        metaEl.append(span);
        const suffix = usesSuffix(uses);
        if (suffix) metaEl.append(suffix);
    };
    const tickCountdowns = () => {
        for (const span of document.querySelectorAll(".countdown")) updateCountdown(span);
    };
    // Re-tick only as often as the display actually changes: every second in the last
    // minute, otherwise at the next minute/hour/day boundary — so a long-lived link
    // does not wake the CPU every second. Self-reschedules; call to (re)start it.
    let tickTimer = null;
    const scheduleTick = () => {
        if (tickTimer) clearTimeout(tickTimer);
        tickCountdowns();
        let delay = 60000;
        for (const span of document.querySelectorAll(".countdown")) {
            const d = parseUtc(span.dataset.expires);
            if (!d || Number.isNaN(d.getTime())) continue;
            const s = Math.round((d.getTime() - Date.now()) / 1000);
            let dd;
            if (s <= 0) dd = 60000;
            else if (s < 60) dd = 1000;
            else if (s < 3540) dd = (s % 60 + 1) * 1000;
            else if (s < 82800) dd = (s % 3600 + 1) * 1000;
            else dd = (s % 86400 + 1) * 1000;
            if (dd < delay) delay = dd;
        }
        tickTimer = setTimeout(scheduleTick, Math.max(1000, Math.min(delay, 60000)));
    };

    // Reveal and wire the result's Copy button (the link already exists, so this copy
    // is a plain synchronous writeText that works on the first click, incl. Safari).
    const setupResultCopy = (linkEl) => {
        const btn = document.getElementById("copy-result");
        if (btn) {
            btn.hidden = false;
            btn.addEventListener("click", () => copyToClipboard(linkEl.textContent.trim(), btn));
        }
    };

    // --- created-link history ---
    // Kept in memory for the session by default. Persisted to localStorage ONLY when
    // the user explicitly opts in ("Enable local history"); opting back out deletes
    // the stored copy.
    const HISTORY_KEY = "yuiolink:history";
    const PERSIST_KEY = "yuiolink:history:persist";
    const HISTORY_MAX = 20;
    let memHistory = [];
    let persistEnabled = false;
    // The link currently shown in the result panel; excluded from the history list.
    let currentResultUrl = null;

    const lsGet = (k) => { try { return localStorage.getItem(k); } catch { return null; } };
    const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch { /* full or blocked */ } };
    const lsDel = (k) => { try { localStorage.removeItem(k); } catch { /* blocked */ } };

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
        const shown = currentResultUrl
            ? memHistory.filter((it) => it.url !== currentResultUrl)
            : [...memHistory];
        const n = shown.length;

        // Split pill: left = status (and a link to the list), right = persistence toggle.
        const status = document.getElementById("storage-status");
        if (status) {
            status.textContent = n > 0 ? `History · ${n} ${n === 1 ? "link" : "links"} ›` : "No links yet";
            status.dataset.has = n > 0 ? "1" : "";
        }
        const toggle = document.getElementById("storage-toggle");
        if (toggle) {
            toggle.textContent = persistEnabled ? "Local history on" : "Enable local history";
            toggle.classList.toggle("on", persistEnabled);
        }

        const section = document.getElementById("history");
        const listEl = document.getElementById("history-list");
        if (!section || !listEl) return;

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
            buildMeta(meta, it.kind, it.expires, it.uses);
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
        const submitBtn = document.getElementById("submit");
        const clearBtn = document.getElementById("clear");
        const linkEl = document.getElementById("link-element");
        const metaEl = document.getElementById("link-expiry");
        const panel = document.getElementById("link-panel");
        const ttlCustomValue = document.getElementById("ttl-custom-value");
        const ttlCustomUnit = document.getElementById("ttl-custom-unit");
        const limitCustomValue = document.getElementById("limit-custom-value");

        const UNIT_SECS = { m: 60, h: 3600, d: 86400 };
        // The input value that produced the current result; the result dims when the
        // input drifts away from it.
        let resultSourceValue = null;

        const checkedValue = (name, fallback) =>
            document.querySelector(`input[name="${name}"]:checked`)?.value ?? fallback;

        const ttlSeconds = () => {
            const v = checkedValue("ttl_seconds", "3600");
            if (v === "custom") {
                let n = Number.parseInt(ttlCustomValue.value, 10);
                if (Number.isNaN(n) || n < 0) n = 0;
                return n * (UNIT_SECS[ttlCustomUnit.value] ?? 60);
            }
            return Number.parseInt(v, 10);
        };
        const maxUses = () => {
            const v = checkedValue("limit", "unlimited");
            if (v === "1") return 1;
            if (v === "custom") {
                const n = Number.parseInt(limitCustomValue.value, 10);
                return !Number.isNaN(n) && n > 0 ? n : null;
            }
            return null;
        };

        // The primary button names what it will create.
        const updateSubmitLabel = () => {
            submitBtn.textContent = content.value.trim() === ""
                ? "Create Link"
                : `Create ${kindLabel(detectKind(content.value))} Link`;
        };

        const autosize = () => {
            content.style.height = "auto";
            const h = content.scrollHeight;
            content.style.height = `${h}px`;
            // Only scroll once the content exceeds max-height; a single line never does.
            const maxPx = Number.parseFloat(getComputedStyle(content).maxHeight);
            content.style.overflowY = !Number.isNaN(maxPx) && h > maxPx ? "auto" : "hidden";
        };

        content.addEventListener("input", () => {
            autosize();
            updateSubmitLabel();
            content.classList.remove("submitted"); // editing re-activates the field
            if (currentResultUrl) panel.classList.toggle("stale", content.value !== resultSourceValue);
        });

        // Focus the Custom field as soon as its segment is picked.
        const ttlCustomRadio = document.getElementById("ttl-custom");
        ttlCustomRadio?.addEventListener("change", () => { if (ttlCustomRadio.checked) ttlCustomValue.focus(); });
        const limitCustomRadio = document.getElementById("limit-custom");
        limitCustomRadio?.addEventListener("change", () => { if (limitCustomRadio.checked) limitCustomValue.focus(); });

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

        // Focusing the field un-greys it and places the caret at the end.
        content.addEventListener("focus", () => {
            content.classList.remove("submitted");
            const n = content.value.length;
            content.setSelectionRange(n, n);
        });

        const showReady = (url, kind, expiresIso, uses) => {
            linkEl.textContent = url;
            buildMeta(metaEl, kind, expiresIso, uses);
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

        const createLink = async () => {
            const raw = content.value;
            if (raw.trim() === "") { content.focus(); return; }
            const kind = detectKind(raw);
            const payload = kind === "redirect" ? normalizeTarget(raw.trim()) : raw;
            const ttl = ttlSeconds();
            const uses = maxUses();

            const restore = submitBtn.textContent;
            submitBtn.disabled = true;
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
                        ttl_seconds: ttl,
                        max_uses: uses,
                    }),
                });
                if (!resp.ok) {
                    const err = await resp.json().catch(() => ({}));
                    throw new Error(err.error || "Request failed");
                }
                const data = await resp.json();
                const url = data.url + fragment;
                currentResultUrl = url;
                showReady(url, kind, data.expires_at, uses);
                addHistory({ url, kind, uses, expires: data.expires_at });
                renderHistory();
                // The input greys (still clickable); the result is in sync with it.
                resultSourceValue = raw;
                content.classList.add("submitted");
                panel.classList.remove("stale");
                scheduleTick();
            } catch (e) {
                alert(e.message || "Could not create the link.");
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = restore;
            }
        };

        form.addEventListener("submit", (event) => {
            // Intercept for the in-place result; without JS this same form posts to
            // `POST /` and renders a result page instead.
            event.preventDefault();
            createLink();
        });

        clearBtn?.addEventListener("click", () => {
            content.value = "";
            autosize();
            updateSubmitLabel();
            content.focus();
        });

        setupResultCopy(linkEl);

        // Split pill: left status jumps to the list (without leaving #history in the
        // address bar); right toggle flips local persistence.
        const status = document.getElementById("storage-status");
        status?.addEventListener("click", (event) => {
            event.preventDefault();
            if (!status.dataset.has) return;
            document.getElementById("history")?.scrollIntoView({ behavior: "smooth", block: "start" });
            history.replaceState(null, "", location.pathname + location.search);
        });
        document.getElementById("storage-toggle")?.addEventListener("click", () => {
            setPersist(!persistEnabled);
            renderHistory();
        });

        document.getElementById("history-clear")?.addEventListener("click", () => {
            memHistory = [];
            lsDel(HISTORY_KEY);
            renderHistory();
        });

        autosize();
        updateSubmitLabel();
        renderHistory();
        content.focus();
    };

    // The no-JS result page: record the just-created link and return its fields.
    const recordResultPage = (linkEl) => {
        const url = linkEl.textContent.trim();
        if (!url) return null;
        const metaText = document.getElementById("link-expiry")?.textContent ?? "";
        const when = metaText.match(/expires (\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) UTC/);
        const usesMatch = metaText.match(/max (\d+) uses/);
        const entry = {
            url,
            kind: /^Text/.test(metaText) ? "text" : "redirect",
            uses: /one-time/.test(metaText) ? 1 : (usesMatch ? Number.parseInt(usesMatch[1], 10) : null),
            expires: when ? when[1] : null,
        };
        addHistory(entry);
        return entry;
    };

    document.addEventListener("DOMContentLoaded", () => {
        loadPersisted();
        if (document.getElementById("create-form")) {
            initCreate();
        } else {
            // Result page: turn the server-rendered meta into a live countdown, wire
            // copy + ⌘C, and select the link.
            const resultLink = document.getElementById("link-element");
            if (resultLink) {
                const entry = recordResultPage(resultLink);
                if (entry) {
                    const metaEl = document.getElementById("link-expiry");
                    if (metaEl) buildMeta(metaEl, entry.kind, entry.expires, entry.uses);
                    setupResultCopy(resultLink);
                    const range = document.createRange();
                    range.selectNodeContents(resultLink);
                    const sel = window.getSelection();
                    sel.removeAllRanges();
                    sel.addRange(range);
                }
            }
        }
        scheduleTick();
    });
})();
