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
    // The kind as a colour-coded word (Redirect blue, Text yellow), shared by the
    // history rows and the result meta.
    const kindWord = (k) => {
        const el = document.createElement("span");
        el.className = `kind-word ${k === "redirect" ? "redirect" : "text"}`;
        el.textContent = kindLabel(k);
        return el;
    };
    const normalizeTarget = (s) => (hasScheme(s) ? s : `https://${s}`);

    // --- clipboard ---
    const flashCopied = (button) => {
        if (!button) return;
        button.classList.add("copied");
        button.textContent = "Copied";
        setTimeout(() => {
            button.classList.remove("copied");
            button.textContent = "Copy";
        }, 1500);
    };
    // Add `cls` to `el` for the same 1.5s as the button's "Copied" flash — used for the
    // green copy check that trails the result URL and the history rows.
    const flashClass = (el, cls) => {
        if (!el) return;
        el.classList.add(cls);
        setTimeout(() => el.classList.remove(cls), 1500);
    };
    // `onCopied`, when given, runs on a successful copy (e.g. to flash a green check).
    const copyToClipboard = async (text, button, onCopied) => {
        if (!text) return;
        try {
            await navigator.clipboard.writeText(text);
            flashCopied(button);
            onCopied?.();
        } catch {
            // Clipboard unavailable (insecure context) or permission denied.
        }
    };

    // Render a URL into `el` as styled parts: a dim scheme, a standout host, and the
    // memorable word (the link name) most highlighted; any #fragment stays dim.
    // textContent still returns the whole URL, so copy and ⌘C are unaffected.
    const renderUrlInto = (el, url) => {
        el.replaceChildren();
        const m = url.match(/^([a-z][a-z0-9+.-]*:\/\/)([^/]+)(\/[^#]*)?(#.*)?$/i);
        if (!m) { el.textContent = url; return; }
        const [, scheme, host, path, frag] = m;
        const add = (cls, text) => {
            if (!text) return;
            const s = document.createElement("span");
            s.className = cls;
            s.textContent = text;
            el.append(s);
        };
        add("u-scheme", scheme);
        add("u-host", host);
        if (path) { add("u-sep", "/"); add("u-name", path.slice(1)); }
        add("u-frag", frag);
    };

    // --- expiry countdown (live; shared by the result and the history list) ---
    const usesSuffix = (uses) => {
        if (uses === 1) return " · one-time";
        if (uses) return ` · max ${uses.toLocaleString()} uses`;
        return "";
    };
    // Compact form for the tight history rows.
    const usesSuffixShort = (uses) => {
        if (uses === 1) return " · once";
        if (uses) return ` · ${uses}×`;
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
        if (!d || Number.isNaN(d.getTime())) return { text: "", compact: "", level: "" };
        const s = Math.round((d.getTime() - Date.now()) / 1000);
        if (s <= 0) return { text: "expired", compact: "expired", level: "now" };
        let unit, short;
        if (s < 180) { unit = `${s} second${s === 1 ? "" : "s"}`; short = `${s}s`; } // last 3 minutes: seconds
        else if (s < 3540) { const m = Math.floor(s / 60); unit = `${m} minute${m === 1 ? "" : "s"}`; short = `${m}m`; }
        else if (s < 82800) { const h = Math.round(s / 3600); unit = `${h} hour${h === 1 ? "" : "s"}`; short = `${h}h`; }
        else { const days = Math.round(s / 86400); unit = `${days} day${days === 1 ? "" : "s"}`; short = `${days}d`; }
        const level = s < 60 ? "now" : s <= 300 ? "soon" : "";
        return { text: `${unit} left`, compact: short, level };
    };
    // Result spans show the full phrase; history spans set data-compact for "1h"/"4m".
    const updateCountdown = (span) => {
        const { text, compact, level } = formatCountdown(span.dataset.expires);
        span.textContent = span.dataset.compact ? compact : text;
        span.classList.toggle("expiring-soon", level === "soon");
        span.classList.toggle("expiring-now", level === "now");
    };
    // Build "<Kind> · <green time left><uses>" into `metaEl` (no innerHTML): the kind
    // as a coloured word, then the green countdown, then any use limit.
    const buildMeta = (metaEl, kind, expiresIso, uses) => {
        metaEl.replaceChildren();
        metaEl.append(kindWord(kind), " · ");
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
        // Reflect a link that just expired: dim it and reveal "Clear Expired".
        const expiredBtn = document.getElementById("history-clear-expired");
        if (expiredBtn && expiredBtn.hidden && memHistory.some(isExpired)) renderHistory();
        let delay = 60000;
        for (const span of document.querySelectorAll(".countdown")) {
            const d = parseUtc(span.dataset.expires);
            if (!d || Number.isNaN(d.getTime())) continue;
            const s = Math.round((d.getTime() - Date.now()) / 1000);
            let dd;
            if (s <= 0) dd = 60000;
            else if (s < 180) dd = 1000;
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
            btn.addEventListener("click", () => copyToClipboard(linkEl.textContent.trim(), btn, () => flashClass(linkEl, "copied")));
        }
    };

    // ⌘C copies the link even with nothing visibly selected (tidier than a highlighted
    // selection). While the result panel holds focus and the user hasn't made their own
    // selection inside it, intercept ⌘C / Ctrl-C and copy the link URL, flashing Copy.
    const enableQuietCopy = (panel, linkEl) => {
        panel.addEventListener("keydown", (event) => {
            const isCopy = (event.metaKey || event.ctrlKey) && (event.key === "c" || event.key === "C");
            if (!isCopy) return;
            // Respect a real selection the user made within the panel — let it copy natively.
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed && panel.contains(sel.anchorNode)) return;
            const url = linkEl.textContent.trim();
            if (!url) return;
            event.preventDefault();
            copyToClipboard(url, document.getElementById("copy-result"), () => flashClass(linkEl, "copied"));
        });
    };

    // --- created-link history ---
    // Kept in memory for the session by default. Persisted to localStorage ONLY when
    // the user explicitly opts in ("Enable local history"); opting back out deletes
    // the stored copy.
    const HISTORY_KEY = "yuiolink:history";
    const PERSIST_KEY = "yuiolink:history:persist";
    const OPEN_KEY = "yuiolink:history:open";
    const HISTORY_MAX = 20;
    let memHistory = [];
    let persistEnabled = false;
    // The history panel is open by default each visit; the open/closed choice is only
    // remembered across visits while Local History is on (persistOpen / setPersist).
    let historyOpen = true;
    // The URL currently shown in the result panel — a flag for the "input out of sync"
    // dimming. (The link itself still counts in the history list.)
    let currentResultUrl = null;
    // Set when the user turns persistence off while links exist, to show the warning.
    let warnArmed = false;

    const lsGet = (k) => { try { return localStorage.getItem(k); } catch { return null; } };
    const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch { /* full or blocked */ } };
    const lsDel = (k) => { try { localStorage.removeItem(k); } catch { /* blocked */ } };

    const isExpired = (it) => {
        const d = parseUtc(it.expires);
        return !!d && !Number.isNaN(d.getTime()) && d.getTime() <= Date.now();
    };
    const loadPersisted = () => {
        persistEnabled = lsGet(PERSIST_KEY) === "1";
        if (persistEnabled) {
            try { memHistory = JSON.parse(lsGet(HISTORY_KEY)) ?? []; }
            catch { memHistory = []; }
            historyOpen = lsGet(OPEN_KEY) !== "0"; // restore the open/closed choice (default open)
        } else {
            lsDel(OPEN_KEY); // history off: forget any saved openness
        }
    };
    const persistNow = () => { if (persistEnabled) lsSet(HISTORY_KEY, JSON.stringify(memHistory)); };
    // Remember the open/closed choice only while persistence is on.
    const persistOpen = () => { if (persistEnabled) lsSet(OPEN_KEY, historyOpen ? "1" : "0"); };
    const applyHistoryOpen = () => {
        document.getElementById("history")?.classList.toggle("collapsed", !historyOpen);
    };
    const setHistoryOpen = (open) => { historyOpen = open; applyHistoryOpen(); persistOpen(); };
    const setPersist = (on) => {
        persistEnabled = on;
        // Turning on (re)saves the list AND the current openness — so flipping it off then
        // back on restores the panel state; turning off forgets both.
        if (on) { lsSet(PERSIST_KEY, "1"); persistNow(); persistOpen(); }
        else { lsDel(PERSIST_KEY); lsDel(HISTORY_KEY); lsDel(OPEN_KEY); }
    };
    const addHistory = (entry) => {
        memHistory = memHistory.filter((it) => it.url !== entry.url);
        memHistory.unshift(entry);
        if (memHistory.length > HISTORY_MAX) memHistory.length = HISTORY_MAX;
        persistNow();
    };

    const renderHistory = () => {
        persistNow();
        const shown = [...memHistory];
        const n = shown.length;
        const linkCount = shown.filter((it) => !it.tombstone).length; // tombstones are not links

        // Split pill: left = status (and a link to the list), right = persistence toggle.
        const status = document.getElementById("storage-status");
        if (status) {
            status.textContent = linkCount > 0 ? `History · ${linkCount} ${linkCount === 1 ? "Link" : "Links"} ›` : "No Links Yet";
            status.dataset.has = n > 0 ? "1" : "";
        }
        const toggle = document.getElementById("storage-toggle");
        if (toggle) {
            toggle.textContent = persistEnabled ? "Local History On" : "Enable Local History";
            toggle.classList.toggle("on", persistEnabled);
        }
        const warn = document.getElementById("storage-warning");
        if (warn) warn.hidden = !(warnArmed && !persistEnabled && n > 0);

        const section = document.getElementById("history");
        const listEl = document.getElementById("history-list");
        if (!section || !listEl) return;
        const title = document.querySelector(".history-title");
        if (title) title.textContent = persistEnabled ? "Local History" : "Local History (Not Saved)";

        listEl.replaceChildren();
        if (n === 0) { section.hidden = true; return; }
        section.hidden = false;
        for (const it of shown) {
            if (it.tombstone) {
                // A marker where a removed entry was; no link details remain.
                const li = document.createElement("li");
                li.className = "history-item history-tomb";
                const msg = document.createElement("span");
                msg.className = "history-tomb-msg";
                msg.textContent = it.tombstone === "broken" ? "Link broken" : "Removed from this device";
                const clear = document.createElement("button");
                clear.className = "history-tomb-clear";
                clear.type = "button";
                clear.textContent = "Clear";
                clear.addEventListener("click", () => clearTombstone(it));
                li.append(msg, clear);
                listEl.append(li);
                continue;
            }

            const li = document.createElement("li");
            li.className = "history-item";
            if (isExpired(it)) li.classList.add("expired");

            // Two-line entry (mockup A): line 1 the full tri-colour URL (dim scheme,
            // standout host, accent name), line 2 the coloured kind word + green time
            // (mockup H3). Copy and × stay vertically centred beside the text block.
            const txt = document.createElement("div");
            txt.className = "history-text";

            const l1 = document.createElement("div");
            l1.className = "history-l1";
            const url = document.createElement("code");
            url.className = "history-url";
            renderUrlInto(url, it.url);
            const check = document.createElement("span");
            check.className = "history-check";
            check.setAttribute("aria-hidden", "true");
            l1.append(url, check);

            const meta = document.createElement("small");
            meta.className = "history-meta";
            meta.append(kindWord(it.kind), " · ");
            const span = document.createElement("span");
            span.className = "countdown";
            span.dataset.expires = it.expires ?? "";
            updateCountdown(span);
            meta.append(span);
            const suffix = usesSuffixShort(it.uses);
            if (suffix) meta.append(suffix);

            txt.append(l1, meta);

            const copy = document.createElement("button");
            copy.className = "history-copy";
            copy.type = "button";
            copy.textContent = "Copy";
            copy.addEventListener("click", () => copyToClipboard(it.url, copy, () => flashClass(check, "show")));

            const del = document.createElement("button");
            del.className = "history-delete";
            del.type = "button";
            del.setAttribute("aria-label", "Remove");
            del.textContent = "×"; // ×
            // The same × toggles the prompt — it stays put (sits above the overlay).
            del.addEventListener("click", () => {
                if (li.classList.contains("confirming")) closeConfirm(li);
                else openConfirm(li, it);
            });

            li.append(txt, copy, del);
            listEl.append(li);
        }
        const clearExpired = document.getElementById("history-clear-expired");
        if (clearExpired) clearExpired.hidden = !memHistory.some(isExpired);
    };

    // --- per-item removal: confirm over the row, then break-on-server or forget ---
    const closeConfirm = (li) => {
        li.classList.remove("confirming");
        li.querySelector(".history-confirm")?.remove();
    };
    // Replace the entry in place with a tombstone, fully purging its url / name /
    // token from memory and storage. The tombstone holds no link details and can
    // itself be cleared.
    const tombstone = (it, kind) => {
        const i = memHistory.indexOf(it);
        if (i === -1) return;
        memHistory[i] = { tombstone: kind };
        persistNow();
        renderHistory();
    };
    const clearTombstone = (it) => {
        memHistory = memHistory.filter((h) => h !== it);
        persistNow();
        renderHistory();
    };
    const forgetLink = (it) => tombstone(it, "forgotten");

    // While the server is contacted, swap the overlay to a spinner; on failure offer
    // a retry (the entry is left intact so the token survives for another try).
    const showConfirmBusy = (li, msg) => {
        const overlay = li.querySelector(".history-confirm");
        if (!overlay) return;
        const spin = document.createElement("span");
        spin.className = "history-spinner";
        const label = document.createElement("span");
        label.className = "history-confirm-label";
        label.textContent = msg;
        overlay.replaceChildren(spin, label);
    };
    const showConfirmError = (li, it) => {
        const overlay = li.querySelector(".history-confirm");
        if (!overlay) return;
        const label = document.createElement("span");
        label.className = "history-confirm-label error";
        label.textContent = "Server didn't respond.";
        const retry = document.createElement("button");
        retry.type = "button";
        retry.className = "history-confirm-server";
        retry.textContent = "Try Again";
        retry.addEventListener("click", () => deleteFromServer(it, li));
        const actions = document.createElement("div");
        actions.className = "history-confirm-actions";
        actions.append(retry);
        overlay.replaceChildren(label, actions);
    };
    const deleteFromServer = async (it, li) => {
        if (!it.token || !it.name) { tombstone(it, "broken"); return; }
        showConfirmBusy(li, "Deleting link…");
        try {
            const resp = await fetch(`${API_BASE}/api/v1/links/${encodeURIComponent(it.name)}`, {
                method: "DELETE",
                headers: { Authorization: `Bearer ${it.token}` },
            });
            // 204 = deleted; 404 = already gone (expired/reaped) — either way it is gone.
            if (resp.ok || resp.status === 404) { tombstone(it, "broken"); return; }
            throw new Error("delete failed");
        } catch {
            showConfirmError(li, it);
        }
    };
    const openConfirm = (li, it) => {
        for (const el of document.querySelectorAll(".history-item.confirming")) closeConfirm(el);
        li.classList.add("confirming");

        const overlay = document.createElement("div");
        overlay.className = "history-confirm";
        const label = document.createElement("span");
        label.className = "history-confirm-label";
        label.textContent = "Remove this link?";
        const actions = document.createElement("div");
        actions.className = "history-confirm-actions";

        // Forget first; Delete Link (destructive) second.
        const forget = document.createElement("button");
        forget.type = "button";
        forget.className = "history-confirm-forget";
        forget.textContent = "Forget Link";
        forget.title = "Removes it from this device only — the link keeps working.";
        forget.addEventListener("click", () => forgetLink(it));
        actions.append(forget);
        // Server break needs the creation token; offer it only when we have one.
        if (it.token && it.name) {
            const server = document.createElement("button");
            server.type = "button";
            server.className = "history-confirm-server";
            server.textContent = "Delete Link";
            server.title = "Deletes it from the server — the link stops working for everyone.";
            server.addEventListener("click", () => deleteFromServer(it, li));
            actions.append(server);
        }

        // No cancel button — the row's × toggles the prompt shut, so it never moves.
        overlay.append(label, actions);
        li.append(overlay);
    };

    const initCreate = () => {
        const content = document.getElementById("content");
        const form = document.getElementById("create-form");
        const encrypt = document.getElementById("encrypt"); // null when encryption is off
        const submitBtn = document.getElementById("submit");
        const clearBtn = document.getElementById("clear");
        const linkEl = document.getElementById("link-element");
        const linkWordEl = document.getElementById("link-word");
        const metaEl = document.getElementById("link-expiry");
        const panel = document.getElementById("link-panel");
        const ttlCustomValue = document.getElementById("ttl-custom-value");
        const limitCustomValue = document.getElementById("limit-custom-value");

        // Field-level problems (not a number, below 1, not whole) are left to the
        // number inputs' native validation — no hand-rolled checks. A Specify box is
        // only meaningful while its segment is selected, so disable the inactive ones:
        // a disabled control is skipped by native validation and not submitted, which
        // keeps a stale hidden value from silently blocking the form.
        const syncCustomEnabled = () => {
            ttlCustomValue.disabled = checkedValue("ttl_seconds", "3600") !== "custom";
            limitCustomValue.disabled = checkedValue("limit", "unlimited") !== "custom";
        };

        // Whole-form errors (a failed request, a server rejection) shown on the page
        // under the action — never an alert popup.
        const formError = document.getElementById("form-error");
        const showFormError = (msg) => {
            if (!formError) return;
            formError.textContent = msg;
            formError.hidden = false;
            // Bring it on screen in case the user had scrolled to the expiry/limit pickers.
            formError.scrollIntoView({ behavior: "smooth", block: "nearest" });
        };
        const clearFormError = () => { if (formError) formError.hidden = true; };

        const UNIT_SECS = { m: 60, h: 3600, d: 86400 };
        // The input value that produced the current result; the result dims when the
        // input drifts away from it.
        let resultSourceValue = null;

        const checkedValue = (name, fallback) =>
            document.querySelector(`input[name="${name}"]:checked`)?.value ?? fallback;

        const ttlSeconds = () => {
            const v = checkedValue("ttl_seconds", "3600");
            if (v === "custom") {
                // An empty Specify box accepts the default (5); a typed value is used
                // as-is (the input's native min/step already barred 0 and fractions).
                const raw = ttlCustomValue.value.trim();
                const n = raw === "" ? 5 : Number.parseInt(raw, 10);
                return (Number.isNaN(n) ? 5 : n) * (UNIT_SECS[checkedValue("ttl_unit", "m")] ?? 60);
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

        // Holding Option/Alt forces a Text link even when the input looks like a URL.
        // We track whether it is held so the button label reflects what a submit makes.
        let altHeld = false;

        // The primary button names what it will create (Option flips a URL to Text).
        const updateSubmitLabel = () => {
            const empty = content.value.trim() === "";
            const kind = altHeld ? "text" : detectKind(content.value);
            submitBtn.textContent = empty ? "Create Link" : `Create ${kindLabel(kind)} Link`;
            // Hint the hidden override only when it would change the outcome (a URL that
            // would otherwise redirect), so the tooltip is never misleading.
            submitBtn.title = (!empty && !altHeld && detectKind(content.value) === "redirect")
                ? "Hold Option to share as a Text link instead"
                : "";
        };
        const setAlt = (on) => { if (altHeld !== on) { altHeld = on; updateSubmitLabel(); } };
        window.addEventListener("keydown", (e) => { if (e.key === "Alt") setAlt(true); });
        window.addEventListener("keyup", (e) => { if (e.key === "Alt") setAlt(false); });
        window.addEventListener("blur", () => setAlt(false));

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
            clearFormError();
            content.classList.remove("submitted"); // editing re-activates the field
            if (currentResultUrl) panel.classList.toggle("stale", content.value !== resultSourceValue);
        });

        // Switching segments toggles which Specify box native validation sees, and
        // picking "Specify" focuses its field.
        for (const r of document.querySelectorAll('input[name="ttl_seconds"]'))
            r.addEventListener("change", () => {
                syncCustomEnabled();
                if (document.getElementById("ttl-custom")?.checked) ttlCustomValue.focus();
            });
        for (const r of document.querySelectorAll('input[name="limit"]'))
            r.addEventListener("change", () => {
                syncCustomEnabled();
                if (document.getElementById("limit-custom")?.checked) limitCustomValue.focus();
            });

        // Keep the view limit to a whole number: digits only, no leading zeros.
        limitCustomValue.addEventListener("input", () => {
            const cleaned = limitCustomValue.value.replace(/\D+/g, "").replace(/^0+(?=\d)/, "");
            if (cleaned !== limitCustomValue.value) limitCustomValue.value = cleaned;
        });

        // Redirect: Enter submits (Shift-Enter inserts a newline). Text: Enter = newline,
        // Cmd/Ctrl-Enter submits. Holding Option forces Text, so a URL then behaves like
        // text — Option-Cmd-Enter submits it as a Text link (plain Enter just newlines).
        content.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") return;
            const kind = event.altKey ? "text" : detectKind(content.value);
            if (kind === "redirect") {
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

        const showReady = (url, kind, expiresIso, uses, defaultedOnce) => {
            if (linkWordEl) linkWordEl.textContent = url.split("#")[0].split("/").pop();
            renderUrlInto(linkEl, url);
            buildMeta(metaEl, kind, expiresIso, uses);
            const note = document.getElementById("result-note");
            if (note) {
                note.hidden = !defaultedOnce;
                if (defaultedOnce) note.textContent = "Limit not specified, so this link opens once.";
            }
            panel.hidden = false;
            // Focus the panel (it precedes the form in the DOM) so ⌘C copies the link
            // (the quiet-copy handler) and the next Tab lands on the input — with no
            // visible text selection, which looks tidier.
            panel.focus({ preventScroll: true });
        };

        // forceText (Option held) overrides detection so a URL is stored as Text.
        const createLink = async (forceText = altHeld) => {
            const raw = content.value;
            clearFormError();
            if (raw.trim() === "") { content.focus(); return; }

            // The number inputs' native validation has already run (a submit only
            // reaches here once it passes), so this only fills the gaps it can't: an
            // empty Specify-limit takes the default of Once, noted on the result.
            const ttl = ttlSeconds();
            let uses = maxUses();
            let defaultedOnce = false;
            if (checkedValue("limit", "unlimited") === "custom" && limitCustomValue.value.trim() === "") {
                uses = 1;
                defaultedOnce = true;
            }

            const kind = forceText ? "text" : detectKind(raw);
            const payload = kind === "redirect" ? normalizeTarget(raw.trim()) : raw;

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
                showReady(url, kind, data.expires_at, uses, defaultedOnce);
                // Keep the name + delete token so the history row can offer a real
                // server delete (token is undefined if the backend didn't send one).
                addHistory({ url, name: data.name, kind, uses, expires: data.expires_at, token: data.delete_token });
                renderHistory();
                // The input greys (still clickable); the result is in sync with it.
                resultSourceValue = raw;
                content.classList.add("submitted");
                panel.classList.remove("stale");
                scheduleTick();
            } catch (e) {
                showFormError(e.message || "Could not create the link.");
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
        enableQuietCopy(panel, linkEl);

        // Split pill: left status jumps to the list (without leaving #history in the
        // address bar); right toggle flips local persistence.
        const status = document.getElementById("storage-status");
        status?.addEventListener("click", (event) => {
            event.preventDefault();
            if (!status.dataset.has) return;
            const section = document.getElementById("history");
            if (!section) return;
            setHistoryOpen(true); // expand (and persist the openness if Local History is on)
            section.scrollIntoView({ behavior: "smooth", block: "start" });
            history.replaceState(null, "", location.pathname + location.search);
        });
        document.getElementById("storage-toggle")?.addEventListener("click", () => {
            const turningOff = persistEnabled;
            setPersist(!persistEnabled);
            warnArmed = turningOff; // only warn when actively switching saving off
            renderHistory();
        });

        document.getElementById("history-clear")?.addEventListener("click", () => {
            memHistory = [];
            lsDel(HISTORY_KEY);
            renderHistory();
        });
        document.getElementById("history-toggle")?.addEventListener("click", () => {
            setHistoryOpen(!historyOpen);
        });
        document.getElementById("history-clear-expired")?.addEventListener("click", () => {
            memHistory = memHistory.filter((it) => !isExpired(it));
            persistNow();
            renderHistory();
        });

        autosize();
        updateSubmitLabel();
        syncCustomEnabled();
        renderHistory();
        applyHistoryOpen(); // open by default on load (or the remembered state)
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
            // The name is the last path segment (minus any #fragment). No token on the
            // no-JS path, so this row can only be forgotten on device, not server-deleted.
            name: url.split("#")[0].split("/").pop(),
            kind: /^Text/.test(metaText) ? "text" : "redirect",
            uses: /one-time/.test(metaText) ? 1 : (usesMatch ? Number.parseInt(usesMatch[1], 10) : null),
            expires: when ? when[1] : null,
            token: null,
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
                    renderUrlInto(resultLink, entry.url); // re-style after reading its text
                    const metaEl = document.getElementById("link-expiry");
                    if (metaEl) buildMeta(metaEl, entry.kind, entry.expires, entry.uses);
                    setupResultCopy(resultLink);
                    // Quiet ⌘C: focus the panel and copy on ⌘C without a visible selection.
                    const panel = document.getElementById("link-panel");
                    if (panel) {
                        enableQuietCopy(panel, resultLink);
                        panel.focus({ preventScroll: true });
                    }
                }
            }
        }
        scheduleTick();
    });
})();
