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

    // Split a shoutkey name at its case boundaries ("runnyDUSK" -> "runny","DUSK")
    // and render each word in an alternating colour, so a multi-word name reads as
    // separate words. Returns a fragment of <span class="nw nw-0|nw-1"> words.
    const NAME_WORDS = /[a-z0-9]+(?:-[a-z0-9]+)*|[A-Z0-9]+(?:-[A-Z0-9]+)*/g;
    const nameSpans = (name) => {
        const frag = document.createDocumentFragment();
        (name.match(NAME_WORDS) || [name]).forEach((w, i) => {
            const s = document.createElement("span");
            s.className = `nw nw-${i % 2}`;
            s.textContent = w;
            frag.append(s);
        });
        return frag;
    };

    // Render a URL into `el` as styled parts: a dim scheme, a standout host, and the
    // memorable word (the link name) highlighted by word; any #fragment stays dim.
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
        if (path) {
            add("u-sep", "/");
            const nm = document.createElement("span");
            nm.className = "u-name";
            nm.append(nameSpans(path.slice(1)));
            el.append(nm);
        }
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

    // Minutes/hours/days show the SET value for a 30s grace, then count down by floor.
    const SET_GRACE_MS = 30000;
    const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
    // Remaining time as { text, compact, level }. Minutes/hours/days read the SET value for a
    // 30s grace (e.g. "1 hour"). DAYS then round UP and hold (a 7-day link reads "7 days"
    // until a full day passes, then "6 days" — coarse and steady at the top). HOURS and
    // MINUTES count down by FLOOR (truthful, skip-free): "1 day" -> "23 hours", "1 hour" ->
    // "59 minutes". A 5-minute link reads "5 minutes" for 30s, "4 minutes" for the next 30s,
    // then "3 minutes" from ~4:00. Seconds are floored & per-second; the last two minutes
    // count seconds ("1m59s"); under a minute is bare seconds. "expired" keys off the real
    // deadline (never early; last partial second reads "1 second"). "soon" (≤5 min) then "now".
    const formatCountdown = (expiresIso, createdMs) => {
        const d = parseUtc(expiresIso);
        if (!d || Number.isNaN(d.getTime())) return { text: "", compact: "", level: "" };
        const ms = d.getTime() - Date.now();
        // "expired" keys off the real deadline (not the floored seconds) so the label never
        // flips before the link is actually gone — the last partial second still reads
        // "1 second", matching the greying/Delete-disable which also use the true deadline.
        if (ms <= 0) return { text: "expired", compact: "expired", level: "now" };
        const s = Math.floor(ms / 1000);
        const created = Number(createdMs);
        const fresh = created && Date.now() - created < SET_GRACE_MS && ms >= 120000;
        let unit, short;
        if (fresh) {
            // The set value (what was chosen), held for the 30s grace. Round the TTL to its
            // natural unit via minutes first, so a 60-minute TTL reads "1 hour", not "60 min".
            const mins = Math.round((d.getTime() - created) / 60000);
            if (mins >= 1440) { const n = Math.round(mins / 1440); unit = plural(n, "day"); short = `${n}d`; }
            else if (mins >= 60) { const n = Math.round(mins / 60); unit = plural(n, "hour"); short = `${n}h`; }
            else { unit = plural(mins, "minute"); short = `${mins}m`; }
        } else if (s < 60) { const sec = s || 1; unit = plural(sec, "second"); short = `${sec}s`; } // floor can hit 0 in the last partial second; show 1
        // 1:59 -> 1:00 reads "1 minute 59 seconds" so the tail never shows "90 seconds".
        else if (s < 120) { const sec = s - 60; unit = sec ? `1 minute ${plural(sec, "second")}` : "1 minute"; short = sec ? `1m${sec}s` : "1m"; }
        else if (ms < 3600000) { const m = Math.floor(ms / 60000); unit = plural(m, "minute"); short = `${m}m`; }
        else if (ms < 86400000) { const h = Math.floor(ms / 3600000); unit = plural(h, "hour"); short = `${h}h`; }
        else { const days = Math.ceil(ms / 86400000); unit = plural(days, "day"); short = `${days}d`; } // days round up & hold (stay same until the day turns)
        const level = s < 60 ? "now" : s <= 300 ? "soon" : "";
        return { text: `${unit} left`, compact: short, level };
    };
    // Result spans show the full phrase; history spans set data-compact for "1h"/"4m".
    const updateCountdown = (span) => {
        const { text, compact, level } = formatCountdown(span.dataset.expires, span.dataset.created);
        span.textContent = span.dataset.compact ? compact : text;
        span.classList.toggle("expiring-soon", level === "soon");
        span.classList.toggle("expiring-now", level === "now");
        // A dead result strikes through its name word and URL (the history list dims
        // its rows instead) and disables Copy — the link no longer resolves. History
        // countdowns sit outside .result, so this no-ops for them. Live, since this runs
        // every tick via tickCountdowns().
        const panel = span.closest(".result");
        if (panel) {
            const dead = text === "expired";
            panel.classList.toggle("expired", dead);
            const copyBtn = panel.querySelector(".result-copy");
            if (copyBtn) {
                // An anchor has no `disabled`: drop the href (no target to copy)
                // and grey it via the class instead.
                copyBtn.classList.toggle("disabled", dead);
                if (dead) copyBtn.removeAttribute("href");
            }
        }
    };
    // Build "<Kind> · <green time left><uses>" into `metaEl` (no innerHTML): the kind
    // as a coloured word, then the green countdown, then any use limit.
    const buildMeta = (metaEl, kind, expiresIso, uses) => {
        metaEl.replaceChildren();
        metaEl.append(kindWord(kind), " · ");
        const span = document.createElement("span");
        span.className = "countdown";
        span.dataset.expires = expiresIso ?? "";
        span.dataset.created = String(Date.now()); // the result is shown at creation; grace runs from now
        updateCountdown(span);
        metaEl.append(span);
        const suffix = usesSuffix(uses);
        if (suffix) metaEl.append(suffix);
    };
    const tickCountdowns = () => {
        for (const span of document.querySelectorAll(".countdown")) updateCountdown(span);
    };
    // Re-tick only as often as the display actually changes: every second in the last
    // minute, otherwise at the next minute/hour/day boundary — so a long-lived link does
    // not wake the CPU every second. It also stops entirely once nothing is left to count,
    // and pauses while the tab is hidden — so an idle or backgrounded page draws no power.
    // Self-reschedules; call to (re)start it.
    let tickTimer = null;
    const scheduleTick = () => {
        if (tickTimer) { clearTimeout(tickTimer); tickTimer = null; }
        // Paused while the tab is hidden: nothing visible to update, so no battery spent.
        // visibilitychange (below) resumes and catches up the instant it is shown again.
        if (document.hidden) return;
        tickCountdowns();
        // Reflect any link that just expired: dim its row and reveal "Clear Expired".
        // Re-render whenever the live expired count diverges from what is shown dimmed —
        // not just the first time (the old `.hidden` guard fired once, so links expiring
        // after the button appeared showed "expired" text but never got greyed).
        const expiredNow = memHistory.filter(isExpired).length;
        const expiredShown = document.querySelectorAll(".history-item.expired").length;
        if (expiredNow !== expiredShown) renderHistory();
        let delay = Infinity;
        for (const span of document.querySelectorAll(".countdown")) {
            const d = parseUtc(span.dataset.expires);
            if (!d || Number.isNaN(d.getTime())) continue;
            const ms = d.getTime() - Date.now();
            if (ms <= 0) continue;         // expired: its text is final — nothing more to update
            // Wake on the next change: a minute-band link still in its 30s set-value grace
            // wakes when the grace ends (to switch to the floor countdown); otherwise on the
            // next second (≤2 min) or the next minute mark. The +1 lands just past the
            // boundary so the floored label shows its new (lower) value, and the final tick
            // lands on the deadline.
            const created = Number(span.dataset.created);
            const graceLeft = created ? created + SET_GRACE_MS - Date.now() : 0;
            let dd;
            if (graceLeft > 0 && ms >= 120000) dd = graceLeft;     // hold the set value
            else if (ms <= 120000) dd = (ms % 1000) + 1;           // per-second
            else if (ms < 3600000) dd = (ms % 60000) + 1;          // per-minute
            else if (ms < 86400000) dd = (ms % 3600000) + 1;       // per-hour
            else dd = (ms % 86400000) + 1;                          // per-day
            if (dd < delay) delay = dd;
        }
        // Re-arm only while a live countdown still needs updating; once everything is
        // expired (or there are none) the timer stops — zero wakeups until something
        // changes. Floor 50ms honours the sub-second final tick (only the last partial
        // second ever asks for under a second, so it is not a busy-wait).
        if (delay !== Infinity) tickTimer = setTimeout(scheduleTick, Math.max(50, Math.min(delay, 60000)));
    };
    // Stop ticking while hidden; resume (and catch up — a link may have expired off-screen)
    // the moment the tab is shown again.
    document.addEventListener("visibilitychange", () => {
        if (document.hidden) { if (tickTimer) { clearTimeout(tickTimer); tickTimer = null; } }
        else scheduleTick();
    });

    // Reveal and wire the result's Copy button (the link already exists, so this copy
    // is a plain synchronous writeText that works on the first click, incl. Safari).
    const setupResultCopy = (linkEl) => {
        const btn = document.getElementById("copy-result");
        if (btn) {
            btn.hidden = false;
            // A real link to the URL (right-click -> Copy Link); left click copies.
            btn.href = linkEl.textContent.trim();
            btn.addEventListener("click", (event) => {
                event.preventDefault();
                copyToClipboard(linkEl.textContent.trim(), btn, () => flashClass(linkEl, "copied"));
            });
        }
    };

    // ⌘C copies the link even with nothing visibly selected (tidier than a highlighted
    // selection). While the result panel holds focus and the user hasn't made their own
    // selection inside it, intercept ⌘C / Ctrl-C and copy the link URL, flashing Copy.
    const enableQuietCopy = (panel, linkEl) => {
        panel.addEventListener("keydown", (event) => {
            const isCopy = (event.metaKey || event.ctrlKey) && (event.key === "c" || event.key === "C");
            if (!isCopy) return;
            if (panel.classList.contains("expired")) return; // dead link: nothing to copy
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

    // --- cross-tab history: each entry carries a stable id so tabs can merge rather than
    // clobber each other's saved list (localStorage is the shared store). ---
    const newId = () => (self.crypto?.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2)}`);
    const withIds = (list) => (Array.isArray(list) ? list.map((e) => (e && e.id ? e : { ...e, id: newId() })) : []);
    const readStored = () => { try { return withIds(JSON.parse(lsGet(HISTORY_KEY))); } catch { return []; } };
    // Removing an entry can't be done by absence: another open tab still holds it in
    // memory and the next merge would union it back (resurrecting it). So a removal
    // leaves a "cleared" death-certificate that propagates through the merge like any
    // tombstone, then self-destructs once every open tab has had time to adopt it.
    const CLEARED_TTL_MS = 60000;
    const clearedMarker = (it) => ({ id: it.id, created: it.created, tombstone: "cleared", clearedAt: Date.now() });
    // Merge precedence for the same id: a "cleared" certificate beats every other state,
    // any other tombstone (forgotten/gone/broken/deleted) beats a live copy, and ties
    // keep whichever was seen first. This is what lets a removal win over a stale live copy.
    const mergeRank = (e) => (e.tombstone === "cleared" ? 2 : e.tombstone ? 1 : 0);
    // Merge two lists (this tab + storage/another tab): keyed by id with the precedence
    // above, expired "cleared" certificates garbage-collected, links de-duped by url,
    // newest (created) first.
    const mergeHistories = (a, b) => {
        const byId = new Map();
        for (const e of [...a, ...b]) {
            if (!e || !e.id) continue;
            const prev = byId.get(e.id);
            if (!prev || mergeRank(e) > mergeRank(prev)) byId.set(e.id, e);
        }
        const now = Date.now();
        const seenUrl = new Set();
        const out = [...byId.values()]
            // Drop death-certificates once they have had time to propagate — kept this
            // long only so other open tabs can adopt the removal before it vanishes.
            .filter((e) => !(e.tombstone === "cleared" && now - (e.clearedAt ?? 0) > CLEARED_TTL_MS))
            .sort((x, y) => (y.created ?? 0) - (x.created ?? 0))
            .filter((e) => !e.url || (!seenUrl.has(e.url) && seenUrl.add(e.url)));
        return out;
    };

    const isExpired = (it) => {
        const d = parseUtc(it.expires);
        return !!d && !Number.isNaN(d.getTime()) && d.getTime() <= Date.now();
    };
    const loadPersisted = () => {
        persistEnabled = lsGet(PERSIST_KEY) === "1";
        if (persistEnabled) {
            memHistory = mergeHistories(readStored(), []); // normalize + GC stale certificates
            historyOpen = lsGet(OPEN_KEY) !== "0"; // restore the open/closed choice (default open)
        } else {
            lsDel(OPEN_KEY); // history off: forget any saved openness
        }
    };
    // Write only when the value actually changed — so a tab adopting another tab's update
    // (via the storage event -> render -> persistNow) does not echo a write back and ping-pong.
    const persistNow = () => {
        if (!persistEnabled) return;
        const json = JSON.stringify(memHistory);
        if (json !== lsGet(HISTORY_KEY)) lsSet(HISTORY_KEY, json);
    };
    // Remember the open/closed choice only while persistence is on.
    const persistOpen = () => { if (persistEnabled) lsSet(OPEN_KEY, historyOpen ? "1" : "0"); };
    const applyHistoryOpen = () => {
        document.getElementById("history")?.classList.toggle("collapsed", !historyOpen);
    };
    const setHistoryOpen = (open) => { historyOpen = open; applyHistoryOpen(); persistOpen(); };
    const setPersist = (on) => {
        persistEnabled = on;
        // Turning on merges this tab's session links with whatever is already saved (and
        // other tabs') AND saves the current openness; turning off forgets both.
        if (on) { memHistory = mergeHistories(memHistory, readStored()); lsSet(PERSIST_KEY, "1"); persistNow(); persistOpen(); }
        else { lsDel(PERSIST_KEY); lsDel(HISTORY_KEY); lsDel(OPEN_KEY); }
    };
    const addHistory = (entry) => {
        entry.id = newId();
        if (persistEnabled) memHistory = mergeHistories(memHistory, readStored()); // pick up other tabs first
        memHistory = memHistory.filter((it) => it.url !== entry.url);
        memHistory.unshift(entry);
        persistNow();
    };

    const renderHistory = () => {
        persistNow();
        // "cleared" certificates are bookkeeping for the cross-tab merge — never shown.
        const shown = memHistory.filter((it) => it.tombstone !== "cleared");
        const n = shown.length;
        const linkCount = shown.filter((it) => !it.tombstone).length; // tombstones are not links

        // Split pill: left = status (and a link to the list), right = persistence toggle.
        // Both ship hidden (blank pills without JS); un-hide once filled.
        const status = document.getElementById("storage-status");
        if (status) {
            status.textContent = linkCount > 0 ? `History · ${linkCount} ${linkCount === 1 ? "Link" : "Links"} ›` : "No Links Yet";
            status.dataset.has = n > 0 ? "1" : "";
            status.hidden = false;
        }
        const toggle = document.getElementById("storage-toggle");
        if (toggle) {
            // HIG switch: the label names what it controls ("Local History"); the switch
            // shows on/off by colour + knob position, not by words. role/aria for a11y.
            if (!toggle.querySelector(".storage-switch")) {
                const label = document.createElement("span");
                label.className = "storage-toggle-label";
                label.textContent = "Local History";
                const sw = document.createElement("span");
                sw.className = "storage-switch";
                sw.setAttribute("aria-hidden", "true");
                toggle.replaceChildren(label, sw);
                toggle.setAttribute("role", "switch");
            }
            toggle.classList.toggle("on", persistEnabled);
            toggle.setAttribute("aria-checked", persistEnabled ? "true" : "false");
            toggle.hidden = false;
        }
        const warn = document.getElementById("storage-warning");
        if (warn) warn.hidden = !(warnArmed && !persistEnabled && n > 0);

        const section = document.getElementById("history");
        const listEl = document.getElementById("history-list");
        if (!section || !listEl) return;
        // The save-on-this-device switch beside the heading mirrors the top toggle:
        // state shown by the switch itself, so the title never grows a suffix.
        const hp = document.getElementById("history-persist");
        if (hp) {
            if (!hp.querySelector(".storage-switch")) {
                const sw = document.createElement("span");
                sw.className = "storage-switch";
                sw.setAttribute("aria-hidden", "true");
                hp.replaceChildren(sw);
                hp.setAttribute("role", "switch");
                hp.setAttribute("aria-label", "Save history on this device");
            }
            hp.classList.toggle("on", persistEnabled);
            hp.setAttribute("aria-checked", persistEnabled ? "true" : "false");
            hp.hidden = false;
        }

        listEl.replaceChildren();
        if (n === 0) { section.hidden = true; return; }
        section.hidden = false;
        for (const it of shown) {
            if (it.tombstone) {
                // A marker where a removed entry was. A just-forgotten live link keeps its
                // name + token for a short grace window so it can still be Deleted from the
                // server (an out for a mis-click); after that the purge strips them.
                const li = document.createElement("li");
                li.className = "history-item history-tomb";
                const msg = document.createElement("span");
                msg.className = "history-tomb-msg";
                msg.textContent = tombMessage(it);
                li.append(msg);
                if (it.tombstone === "forgotten" && it.until && it.name && it.token) {
                    const del = document.createElement("button");
                    del.className = "history-tomb-delete";
                    del.type = "button";
                    del.dataset.until = String(it.until);
                    del.textContent = graceLabel(it.until);
                    del.title = "Also delete it from the server — the link stops working for everyone.";
                    del.addEventListener("click", () => deleteForgotten(it, li));
                    li.append(del);
                }
                const clear = document.createElement("button");
                clear.className = "history-tomb-clear";
                clear.type = "button";
                clear.textContent = "Clear";
                clear.addEventListener("click", () => clearTombstone(it));
                li.append(clear);
                listEl.append(li);
                continue;
            }

            const li = document.createElement("li");
            li.className = "history-item";
            if (isExpired(it)) li.classList.add("expired");

            // Line 1: the full-width tri-colour URL (dim scheme, standout host, the
            // name highlighted by word) with a trailing green copy-check.
            const l1 = document.createElement("div");
            l1.className = "history-l1";
            const url = document.createElement("code");
            url.className = "history-url";
            renderUrlInto(url, it.url);
            const check = document.createElement("span");
            check.className = "history-check";
            check.setAttribute("aria-hidden", "true");
            l1.append(url, check);

            // Line 2: kind word + green time on the left, the actions on the right.
            const foot = document.createElement("div");
            foot.className = "history-foot";
            const meta = document.createElement("small");
            meta.className = "history-meta";
            meta.append(kindWord(it.kind), " · ");
            const span = document.createElement("span");
            span.className = "countdown";
            span.dataset.expires = it.expires ?? "";
            if (it.created) span.dataset.created = String(it.created);
            updateCountdown(span);
            meta.append(span);
            const suffix = usesSuffixShort(it.uses);
            if (suffix) meta.append(suffix);

            const actions = document.createElement("div");
            actions.className = "history-actions";
            // Copy and Preview are real links to the URL, so right-click offers
            // Copy Link / Open in New Tab; a left click on Copy copies instead.
            const copy = document.createElement("a");
            copy.className = "history-copy";
            copy.href = it.url;
            copy.textContent = "Copy";
            copy.addEventListener("click", (event) => {
                event.preventDefault();
                copyToClipboard(it.url, copy, () => flashClass(check, "show"));
            });
            const show = document.createElement("a");
            show.className = "history-show";
            show.href = it.url;
            show.target = "_blank";
            show.rel = "noopener noreferrer";
            show.textContent = "Preview";
            show.title = "Open this link's preview in a new tab";
            const remove = document.createElement("button");
            remove.className = "history-remove";
            remove.type = "button";
            remove.textContent = "Remove…";
            // Opens the confirm prompt over the row — not a toggle; the prompt carries
            // its own Cancel. openConfirm closes any other row's prompt first.
            remove.addEventListener("click", () => openConfirm(li, it));
            actions.append(show, copy, remove);
            foot.append(meta, actions);

            li.append(l1, foot);
            listEl.append(li);
        }
        syncClearMenu();
    };

    // --- "Clear…" fold: the two destructive actions stay hidden until asked for ---
    let clearMenuOpen = false;
    const syncClearMenu = () => {
        const opener = document.getElementById("history-clear-open");
        const expired = document.getElementById("history-clear-expired");
        const all = document.getElementById("history-clear");
        if (opener) opener.hidden = clearMenuOpen;
        // Both options always show while open; Expired goes inert (not hidden)
        // when nothing has expired, so the pair keeps its shape.
        if (expired) {
            expired.hidden = !clearMenuOpen;
            expired.disabled = !memHistory.some(isExpired);
        }
        if (all) all.hidden = !clearMenuOpen;
    };
    const setClearMenu = (open) => {
        clearMenuOpen = open;
        syncClearMenu();
    };

    // --- per-item removal: confirm over the row, then break-on-server or forget ---
    const closeConfirm = (li) => {
        li.classList.remove("confirming");
        li.querySelector(".history-confirm")?.remove();
    };
    // The message under a tombstone, by kind — it also tells the user where the link now
    // stands on the server.
    const tombMessage = (it) => {
        switch (it.tombstone) {
            case "deleted": return "Removed from this device and deleted from the server";
            case "gone": return "Removed from this device — the link has expired";
            case "forgotten": return "Removed from this device — the link still works";
            default: return "Removed from this device";
        }
    };
    // Label for a grace-window Delete button: the action plus the seconds left to use it.
    const graceLabel = (until) => `Delete · ${Math.max(1, Math.ceil((until - Date.now()) / 1000))}s`;
    // Pull in other tabs' changes, then locate an entry by its stable id (the passed `it`
    // may be a stale reference from a previous render). Returns the index, or -1.
    const findEntry = (it) => {
        if (persistEnabled) memHistory = mergeHistories(memHistory, readStored());
        return memHistory.findIndex((e) => e.id === it.id);
    };
    // Replace the entry in place with a tombstone, purging its url / name / token but keeping
    // the id (so the removal merges across tabs) and created (for ordering).
    const tombstone = (it, kind) => {
        const i = findEntry(it);
        if (i === -1) return;
        memHistory[i] = { id: memHistory[i].id, created: memHistory[i].created, tombstone: kind };
        persistNow();
        renderHistory();
    };
    const clearTombstone = (it) => {
        const i = findEntry(it);
        if (i === -1) return;
        // Leave a death-certificate rather than dropping the row, so the removal sticks
        // across tabs instead of being resurrected by another tab's stale copy.
        memHistory[i] = clearedMarker(memHistory[i]);
        persistNow();
        renderHistory();
    };
    // Forgetting removes the entry from this device at once. If the link is still live we
    // keep its name + token for FORGET_GRACE_MS so a mis-click can still Delete it from the
    // server, then purge them; an already-expired link has nothing on the server to keep.
    const FORGET_GRACE_MS = 15000;
    const forgetLink = (it) => {
        const i = findEntry(it);
        if (i === -1) return;
        const cur = memHistory[i];
        if (isExpired(cur)) {
            memHistory[i] = { id: cur.id, created: cur.created, tombstone: "gone" };
        } else if (cur.token && cur.name) {
            memHistory[i] = { id: cur.id, created: cur.created, tombstone: "forgotten", name: cur.name, token: cur.token, until: Date.now() + FORGET_GRACE_MS };
            tickForgetGrace();
        } else {
            memHistory[i] = { id: cur.id, created: cur.created, tombstone: "forgotten" };
        }
        persistNow();
        renderHistory();
    };
    // Drive the forget grace windows: each second, refresh the countdown shown on the grace
    // Delete buttons; when one lapses, strip its name/token (re-rendering so the button
    // goes), and stop ticking once none remain. Also covers graces restored from storage.
    let forgetGraceTimer = null;
    const tickForgetGrace = () => {
        if (forgetGraceTimer) { clearTimeout(forgetGraceTimer); forgetGraceTimer = null; }
        const now = Date.now();
        let purged = false;
        let active = false;
        for (const it of memHistory) {
            if (!it.until) continue;
            if (it.until <= now) { delete it.name; delete it.token; delete it.until; purged = true; }
            else active = true;
        }
        if (purged) { persistNow(); renderHistory(); }
        for (const btn of document.querySelectorAll(".history-tomb-delete[data-until]")) {
            btn.textContent = graceLabel(Number(btn.dataset.until));
        }
        if (active) forgetGraceTimer = setTimeout(tickForgetGrace, 1000);
    };

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
    // Best-effort DELETE on the server; true if the link is gone (204) or already gone
    // (404, expired/reaped). Shared by the confirm-menu Delete and the tombstone out.
    const serverDelete = async (name, token) => {
        try {
            const resp = await fetch(`${API_BASE}/api/v1/links/${encodeURIComponent(name)}`, {
                method: "DELETE",
                headers: { Authorization: `Bearer ${token}` },
            });
            return resp.ok || resp.status === 404;
        } catch {
            return false;
        }
    };
    const deleteFromServer = async (it, li) => {
        if (!it.token || !it.name) { tombstone(it, "broken"); return; }
        showConfirmBusy(li, "Deleting link…");
        if (await serverDelete(it.name, it.token)) tombstone(it, "deleted");
        else showConfirmError(li, it);
    };
    // The grace-window out: delete a just-forgotten (still-live) link from the server.
    const deleteForgotten = async (it, li) => {
        if (!it.token || !it.name) return;
        const msgEl = li.querySelector(".history-tomb-msg");
        if (msgEl) msgEl.textContent = "Deleting from the server…";
        li.querySelector(".history-tomb-delete")?.remove();
        if (await serverDelete(it.name, it.token)) tombstone(it, "deleted");
        else renderHistory(); // restore the row; Delete reappears if still within grace
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
        // An expired link is already gone server-side, which changes both buttons below.
        const expired = isExpired(it);

        // Forget first; Delete Link (destructive) second.
        const forget = document.createElement("button");
        forget.type = "button";
        forget.className = "history-confirm-forget";
        forget.textContent = "Forget Link";
        forget.title = expired
            ? "Removes the record from this device — the link has already expired."
            : "Removes it from this device only — the link keeps working.";
        forget.addEventListener("click", () => forgetLink(it));
        actions.append(forget);
        // Server break needs the creation token; offer it only when we have one.
        if (it.token && it.name) {
            const server = document.createElement("button");
            server.type = "button";
            server.className = "history-confirm-server";
            server.textContent = "Delete Link";
            if (expired) {
                // An expired link is already erased from the server — nothing to delete,
                // so only forgetting the local record remains.
                server.disabled = true;
                server.title = "Already gone — an expired link is erased from the server.";
            } else {
                server.title = "Deletes it from the server — the link stops working for everyone.";
                server.addEventListener("click", () => deleteFromServer(it, li));
            }
            actions.append(server);
        }

        // Cancel (×) is the only way out now that Remove… is open-only; it sits at the
        // right end, after the destructive buttons.
        const cancel = document.createElement("button");
        cancel.type = "button";
        cancel.className = "history-confirm-cancel";
        cancel.textContent = "✕";
        cancel.title = "Cancel";
        cancel.setAttribute("aria-label", "Cancel");
        cancel.addEventListener("click", () => closeConfirm(li));
        actions.append(cancel);

        overlay.append(label, actions);
        li.append(overlay);
    };

    const initCreate = () => {
        const content = document.getElementById("content");
        const form = document.getElementById("create-form");
        const submitBtn = document.getElementById("submit");
        const clearBtn = document.getElementById("clear");
        const linkEl = document.getElementById("link-element");
        const linkWordEl = document.getElementById("link-word");
        const metaEl = document.getElementById("link-expiry");
        const panel = document.getElementById("link-panel");
        const resultNoteEl = document.getElementById("result-note");
        const ttlCustomValue = document.getElementById("ttl-custom-value");

        // --- Expires After: stepped slider + tappable readout ---
        // The stop ladder; must match TTL_STOPS in web.rs and the slider's range.
        const TTL_STOPS = [60, 120, 300, 600, 900, 1800, 2700, 3600, 7200, 10800,
            21600, 43200, 86400, 172800, 259200, 432000, 604800];
        const ttlSlider = document.getElementById("ttl-slider");
        const ttlReadout = document.getElementById("ttl-readout");
        const ttlCustomField = document.getElementById("ttl-custom-field");
        const fmtTtl = (secs) => {
            if (!Number.isFinite(secs)) return "Infinity years"; // digits past float range read as forever
            if (secs < 3600) { const m = Math.round(secs / 60); return `${m} minute${m === 1 ? "" : "s"}`; }
            if (secs < 86400) {
                const h = Math.floor(secs / 3600), m = Math.round((secs % 3600) / 60);
                return `${h} hour${h === 1 ? "" : "s"}${m ? ` ${m} min` : ""}`;
            }
            // Valid values stop at 7 days; the larger units only ever show a
            // struck-out over-limit fantasy, climbing weeks -> months -> years
            // and finally scientific notation so it always fits the readout.
            const days = secs / 86400;
            if (days < 14) {
                const d = Math.floor(days), h = Math.round((secs % 86400) / 3600);
                return `${d} day${d === 1 ? "" : "s"}${h ? ` ${h} h` : ""}`;
            }
            if (days < 61) {
                const w = Math.floor(days / 7), d = Math.round(days % 7);
                return `${w} week${w === 1 ? "" : "s"}${d ? ` ${d} d` : ""}`;
            }
            if (days < 730) {
                const mo = Math.floor(days / 30.44), d = Math.round(days % 30.44);
                return `${mo} month${mo === 1 ? "" : "s"}${d ? ` ${d} d` : ""}`;
            }
            const y = days / 365.25;
            if (y < 1e5) {
                const yr = Math.floor(y), mo = Math.round((days - yr * 365.25) / 30.44);
                return `${yr} year${yr === 1 ? "" : "s"}${mo ? ` ${mo} mo` : ""}`;
            }
            return `${y.toExponential(1)} years`;
        };
        // The concrete deadline under the duration, so "what is set" is unambiguous.
        const deadlineLabel = (secs) => {
            const t = new Date(Date.now() + secs * 1000);
            const days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            const hm = `${String(t.getHours()).padStart(2, "0")}:${String(t.getMinutes()).padStart(2, "0")}`;
            return `≈ deleted ${days[t.getDay()]} ${hm}`;
        };
        const ttlHint = document.querySelector("#ttl-custom-field .custom-hint");
        const ttlHintDefault = ttlHint?.textContent ?? "";
        // True when the exact field holds a value above the 7-day ceiling: the
        // field's synced `max` blocks submission, and the readout/hint show why.
        const ttlOverLimit = () => {
            const n = Number.parseInt(ttlCustomValue.value.trim(), 10);
            return !Number.isNaN(n) && n > Number(ttlCustomValue.max);
        };
        const updateTtlReadout = () => {
            if (!ttlReadout) return;
            // Over the ceiling: the typed duration is shown struck out in red
            // with no deletion time, the field and hint flag the value, and the
            // native bubble speaks on submit. badInput counts too — the browser
            // sets it for digit strings past ITS float range (~1e308), the same
            // "too big" one step further, so it reads as Infinity years rather
            // than a distinct error.
            const bad = ttlCustomValue.validity.badInput;
            const over = bad || ttlOverLimit();
            const secs = bad ? Infinity : ttlSeconds();
            // The duration in its own span so the wavy "you can specify this"
            // underline never runs under the deadline label.
            const dur = document.createElement("span");
            dur.className = "ttl-dur";
            dur.textContent = fmtTtl(secs);
            if (over) {
                ttlReadout.replaceChildren(dur);
            } else {
                const small = document.createElement("small");
                small.textContent = deadlineLabel(secs);
                ttlReadout.replaceChildren(dur, small);
            }
            ttlReadout.classList.toggle("ttl-over", over);
            // Striking out "45 days" corrects a plausible ask; striking out
            // "3.2e+300 years" belabors it. Absurd magnitudes (scientific
            // notation, Infinity) stay red but unstruck.
            ttlReadout.classList.toggle("ttl-absurd", over && secs / 86400 / 365.25 >= 1e5);
            ttlCustomField?.classList.toggle("over", over);
            if (ttlHint) ttlHint.textContent = over ? `Longest is ${fmtTtl(MAX_TTL)}.` : ttlHintDefault;
            ttlCustomValue.setCustomValidity(over && !bad ? `Links can last at most ${fmtTtl(MAX_TTL)}.` : "");
        };
        // The exact field's ceiling depends on the chosen unit (7 days = 168 hours
        // = 10080 minutes), so keep its `max` in step with the unit. requestSubmit()
        // then flags an over-limit value (e.g. 45 days) natively, before the POST,
        // instead of the user learning the ceiling from a server error.
        const MAX_TTL = TTL_STOPS[TTL_STOPS.length - 1];
        const syncCustomMax = () => {
            const unit = checkedValue("ttl_unit", "h");
            ttlCustomValue.max = Math.floor(MAX_TTL / (UNIT_SECS[unit] ?? 3600));
        };
        const setupTtl = () => {
            if (!ttlSlider || !ttlReadout) return;
            // The slider and ticks work without JS (ttl_stop posts natively);
            // JS adds the live readout and folds the exact field away behind a
            // readout tap (a filled exact field beats the slider).
            ttlReadout.hidden = false;
            ttlCustomField.hidden = true;
            syncCustomMax();
            ttlSlider.addEventListener("input", () => {
                ttlCustomValue.value = ""; // the slider takes back over
                updateTtlReadout();
            });
            ttlReadout.addEventListener("click", () => {
                // Closing the exact field on an over-limit value settles it at
                // the ceiling — tapping the number is accepting what it shows.
                if (!ttlCustomField.hidden && (ttlOverLimit() || ttlCustomValue.validity.badInput)) {
                    ttlCustomValue.value = ttlCustomValue.max;
                    updateTtlReadout();
                }
                ttlCustomField.hidden = !ttlCustomField.hidden;
                if (!ttlCustomField.hidden) ttlCustomValue.focus();
            });
            // The labeled landmarks jump straight to their stop.
            for (const tick of document.querySelectorAll(".ttl-tick"))
                tick.addEventListener("click", () => {
                    ttlSlider.value = tick.dataset.stop;
                    ttlCustomValue.value = "";
                    updateTtlReadout();
                });
            ttlCustomValue.addEventListener("input", updateTtlReadout);
            for (const r of document.querySelectorAll('input[name="ttl_unit"]'))
                r.addEventListener("change", () => { syncCustomMax(); updateTtlReadout(); });
            updateTtlReadout();
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
            // A filled exact field beats the slider (matching the server's form
            // precedence); otherwise the slider's stop governs.
            const raw = ttlCustomValue.value.trim();
            if (raw !== "") {
                const n = Number.parseInt(raw, 10);
                if (!Number.isNaN(n)) return n * (UNIT_SECS[checkedValue("ttl_unit", "h")] ?? 3600);
            }
            return TTL_STOPS[+(ttlSlider?.value ?? 16)] ?? 604800;
        };
        // One control picks the type: public (reusable, short, guessable), private
        // (reusable, long unguessable name), or once (single-use, long name).
        const linkType = () => checkedValue("link_type", "public");
        const maxUses = () => (linkType() === "once" ? 1 : null);
        const isPrivate = () => linkType() === "private";

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

        const showReady = (url, kind, expiresIso, uses) => {
            if (linkWordEl) linkWordEl.replaceChildren(nameSpans(url.split("#")[0].split("/").pop()));
            renderUrlInto(linkEl, url);
            const copyBtn = document.getElementById("copy-result");
            if (copyBtn) { copyBtn.href = url; copyBtn.classList.remove("disabled"); }
            buildMeta(metaEl, kind, expiresIso, uses);
            if (resultNoteEl) resultNoteEl.hidden = true; // reset; create path re-shows if crowded
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

            // The expiry Specify box's native validation has already run (a submit only
            // reaches here once it passes); the limit is now just once-or-unlimited.
            const ttl = ttlSeconds();
            const uses = maxUses();
            const priv = isPrivate();

            const kind = forceText ? "text" : detectKind(raw);
            const payload = kind === "redirect" ? normalizeTarget(raw.trim()) : raw;

            const restore = submitBtn.textContent;
            submitBtn.disabled = true;
            // Only show "Creating…" if the request runs long enough to notice; a fast
            // create would otherwise flash the label pointlessly.
            const creatingLabel = setTimeout(() => { submitBtn.textContent = "Creating…"; }, 150);
            try {
                const resp = await fetch(`${API_BASE}/api/v1/links`, {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        kind,
                        content: payload,
                        ttl_seconds: ttl,
                        max_uses: uses,
                        private: priv,
                    }),
                });
                if (!resp.ok) {
                    const err = await resp.json().catch(() => ({}));
                    // A 400 carries every field's problem at once; show each on
                    // its own line (.form-error is white-space: pre-line).
                    const msgs = Array.isArray(err.errors)
                        ? err.errors.map((e) => e?.message).filter(Boolean)
                        : [];
                    throw new Error(msgs.length ? msgs.join("\n") : (err.error || "Request failed"));
                }
                const data = await resp.json();
                const url = data.url;
                currentResultUrl = url;
                showReady(url, kind, data.expires_at, uses);
                // A public link is normally one word; more means the short tiers are
                // crowded right now — say so, since the name is longer than expected.
                if (resultNoteEl && !priv && !uses && data.words > 1) {
                    resultNoteEl.textContent = `Short names are in high demand right now, so this link uses ${data.words} words.`;
                    resultNoteEl.hidden = false;
                }
                // Keep the name + delete token so the history row can offer a real
                // server delete (token is undefined if the backend didn't send one).
                addHistory({ url, name: data.name, kind, uses, expires: data.expires_at, token: data.delete_token, created: Date.now() });
                renderHistory();
                // The input greys (still clickable); the result is in sync with it.
                resultSourceValue = raw;
                content.classList.add("submitted");
                panel.classList.remove("stale");
                scheduleTick();
            } catch (e) {
                showFormError(e.message || "Could not create the link.");
            } finally {
                clearTimeout(creatingLabel);
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

        // Clear ships hidden (dead without JS); reveal it as it gets its handler.
        if (clearBtn) {
            clearBtn.hidden = false;
            clearBtn.addEventListener("click", () => {
                content.value = "";
                autosize();
                updateSubmitLabel();
                content.focus();
            });
        }

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
        // The top toggle and the switch beside the History heading are the same
        // control in two places; both flip the localStorage opt-in.
        const flipPersist = () => {
            const turningOff = persistEnabled;
            setPersist(!persistEnabled);
            warnArmed = turningOff; // only warn when actively switching saving off
            renderHistory();
        };
        document.getElementById("storage-toggle")?.addEventListener("click", flipPersist);
        document.getElementById("history-persist")?.addEventListener("click", flipPersist);

        document.getElementById("history-clear-open")?.addEventListener("click", () => {
            setClearMenu(true);
        });
        document.getElementById("history-clear")?.addEventListener("click", () => {
            // Tombstone every row (don't just empty the list) so the clear propagates to
            // other tabs instead of being undone by their in-memory copies.
            memHistory = memHistory.map(clearedMarker);
            persistNow();
            setClearMenu(false);
            renderHistory();
        });
        document.getElementById("history-toggle")?.addEventListener("click", () => {
            setHistoryOpen(!historyOpen);
        });
        document.getElementById("history-clear-expired")?.addEventListener("click", () => {
            memHistory = memHistory.map((it) => (isExpired(it) ? clearedMarker(it) : it));
            persistNow();
            setClearMenu(false);
            renderHistory();
        });
        // Any click outside the head actions folds the Clear menu back up.
        document.addEventListener("click", (event) => {
            if (clearMenuOpen && !event.target.closest(".history-head-actions")) setClearMenu(false);
        });

        // Keyboard-shortcuts help: reveal the "?" corner button, name the modifier
        // keys for this platform, and open the dialog on click or a bare "?".
        const kbdHelp = document.getElementById("kbd-help");
        const kbdDialog = document.getElementById("kbd-dialog");
        if (kbdHelp && kbdDialog) {
            const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
            for (const el of kbdDialog.querySelectorAll(".k-mod")) el.textContent = isMac ? "⌘" : "Ctrl";
            for (const el of kbdDialog.querySelectorAll(".k-alt")) el.textContent = isMac ? "⌥ Option" : "Alt";
            kbdHelp.hidden = false;
            kbdHelp.addEventListener("click", () => kbdDialog.showModal());
            // A click on the backdrop (the dialog element itself, not its content)
            // closes it, alongside the native Esc.
            kbdDialog.addEventListener("click", (event) => {
                if (event.target === kbdDialog) kbdDialog.close();
            });
            window.addEventListener("keydown", (event) => {
                if (event.key !== "?" || kbdDialog.open) return;
                if (event.target.closest("input, textarea")) return; // typing, not asking
                event.preventDefault();
                kbdDialog.showModal();
            });
        }

        autosize();
        updateSubmitLabel();
        setupTtl();
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
            created: Date.now(), // the result page loads at ~creation; grace runs from now
        };
        addHistory(entry);
        return entry;
    };

    // Another tab changed the shared store: adopt its history (merge, so neither tab's links
    // are lost) or reflect a persistence on/off flip. The merged render re-persists only if
    // this tab still has something extra (persistNow's no-op guard prevents a ping-pong).
    window.addEventListener("storage", (event) => {
        if (event.key === HISTORY_KEY) {
            if (!persistEnabled) return;
            memHistory = mergeHistories(memHistory, readStored());
            renderHistory();
            tickForgetGrace();
            scheduleTick();
        } else if (event.key === PERSIST_KEY) {
            persistEnabled = lsGet(PERSIST_KEY) === "1";
            if (persistEnabled) memHistory = mergeHistories(memHistory, readStored());
            renderHistory();
        }
    });

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
        tickForgetGrace(); // clear/arm any grace windows restored from localStorage
        scheduleTick();
    });
})();
