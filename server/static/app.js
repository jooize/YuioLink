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
    // One token (a URL holds no whitespace; a space in one is written %20), and
    // either `//` after the scheme or a scheme a redirect may use (the server's
    // list, from the page's meta). "Shopping: oat milk", "Password:hunter2" and
    // "https://example.com is great" are Text. Mirrors yuiolink_core's
    // looks_like_url.
    const SCHEMES = new Set((document.querySelector('meta[name="yuiolink-schemes"]')?.content ?? "").split(" ").filter(Boolean));
    const looksLikeUrl = (s) => {
        if (!hasScheme(s)) return false;
        const i = s.indexOf(":");
        return !/\s/.test(s) && (s.startsWith("//", i + 1) || SCHEMES.has(s.slice(0, i).toLowerCase()));
    };
    // Why an input that tries to be an address cannot be a redirect, or "" when
    // it can. detectKind reads such input as Text, and the form says why under
    // the button. Mirrors the server's validate_redirect: the scheme must be on
    // its list, and the whole must parse.
    const addressProblem = (s) => {
        const scheme = s.slice(0, s.indexOf(":")).toLowerCase();
        if (!SCHEMES.has(scheme)) return `Links can't redirect to ${scheme}: addresses, so this will be a text link.`;
        // Asked of the server's table, not the browser: Chromium's parser takes
        // a joiner in a domain (as punycode) where the server's refuses it.
        if (domainHidden(s)?.refused) return "A hidden character in its domain makes this a text link.";
        if (!URL.canParse(s)) return "This isn't a valid address, so it will be a text link.";
        return "";
    };

    // --- hidden characters, as the server marks them ---
    // The server's table (views.rs char_table), so the form marks exactly what
    // a snippet marks: ranges of code points, the names and what each does.
    const CHARS = (() => {
        try {
            return JSON.parse(document.querySelector('meta[name="yuiolink-chars"]')?.content ?? "null");
        } catch {
            return null;
        }
    })();
    const inRanges = (list, cp) => Array.isArray(list) && list.some(([a, b]) => cp >= a && cp <= b);
    const isHidden = (cp) => inRanges(CHARS?.hidden, cp);
    // No-break spaces and soft hyphens: pasted text often holds them for a
    // reason, so they get a gray tick and Remove leaves them in.
    const isSoft = (cp) => inRanges(CHARS?.soft, cp);
    const hostDrops = (cp) => inRanges(CHARS?.hostDrops, cp);
    const EMOJI_BASES = new Set(CHARS?.emojiBases ?? []);
    const hex4 = (cp) => cp.toString(16).toUpperCase().padStart(4, "0");
    // [symbol, name], as views.rs char_dress gives them: the table's, else a
    // variation selector's number, else the code point.
    const charDress = (cp) => {
        const known = CHARS?.names?.[cp];
        if (Array.isArray(known)) return known;
        const vs = cp >= 0xfe00 && cp <= 0xfe0f ? cp - 0xfe00 + 1 : cp >= 0xe0100 && cp <= 0xe01ef ? cp - 0xe0100 + 17 : 0;
        return vs ? [`VS${vs}`, `variation selector-${vs}`] : [hex4(cp), "hidden character"];
    };
    const charMeaning = (cp) =>
        CHARS?.meanings?.find(([, ranges]) => inRanges(ranges, cp))?.[0] ?? "It draws nothing, so two texts that look the same can differ";
    // The hidden characters in an address's domain: the ones the server's
    // parser drops (the link goes to the domain without them) and the ones it
    // refuses (the address is not one, so the link is Text). `clean` is the
    // domain without any of them. Null when the domain holds none.
    const domainHidden = (s) => {
        const m = /^[a-z][a-z0-9+.-]*:\/\/([^/?#]*)/i.exec(s);
        if (!m) return null;
        const chars = [...m[1]];
        const cps = chars.map((c) => c.codePointAt(0)).filter(isHidden);
        if (!cps.length) return null;
        let clean = chars.filter((c) => !isHidden(c.codePointAt(0))).join("");
        try { clean = new URL(`https://${clean}`).host; } catch { /* shown as typed */ }
        return { refused: cps.filter((c) => !hostDrops(c)).length, dropped: cps.filter(hostDrops).length, clean };
    };
    const EXTENDED_PICTOGRAPHIC = /\p{Extended_Pictographic}/u;
    const isPictographic = (ch) => ch != null && EXTENDED_PICTOGRAPHIC.test(ch);
    // The text's hidden characters, as views.rs text_mark marks them, plus
    // the parts of an emoji: a joiner between two emoji (a family, a flag
    // with a rainbow), and the tags that spell a subdivision flag (Scotland's).
    // Each keeps `at`, its place in UTF-16 units (the field's selection counts
    // in those), and `i`, its place in its run of hidden characters.
    const scanHidden = (text) => {
        const chars = [...text];
        const exempt = new Set();
        for (let k = 0; k < chars.length; k += 1) {
            const cp = chars[k].codePointAt(0);
            if (cp === 0x200d) {
                let b = k - 1;
                while (b >= 0 && /[\u{fe0f}\u{1f3fb}-\u{1f3ff}]/u.test(chars[b])) b -= 1;
                if (isPictographic(chars[b]) && isPictographic(chars[k + 1])) exempt.add(k);
            } else if (cp === 0x1f3f4) {
                let e = k + 1;
                while (e < chars.length && chars[e].codePointAt(0) >= 0xe0020 && chars[e].codePointAt(0) <= 0xe007e) e += 1;
                if (e > k + 1 && chars[e]?.codePointAt(0) === 0xe007f) for (let t = k + 1; t <= e; t += 1) exempt.add(t);
            }
        }
        const marks = [];
        let at = 0;
        let run = 0;
        let paired = false;
        for (let k = 0; k < chars.length; k += 1) {
            const ch = chars[k];
            const cp = ch.codePointAt(0);
            // A selector completing a registered pair draws as part of its base.
            const pairs = !paired && (cp === 0xfe0e || cp === 0xfe0f) && k > 0 && EMOJI_BASES.has(chars[k - 1].codePointAt(0));
            paired = pairs;
            if (cp !== 0x0a && cp !== 0x09 && !pairs && !exempt.has(k) && isHidden(cp)) {
                marks.push({ ch, cp, at, i: run, soft: isSoft(cp), id: `u${cp.toString(16).padStart(4, "0")}` });
                run += 1;
            } else run = 0;
            at += ch.length;
        }
        return marks;
    };
    // Hidden characters side by side: what one tick marks and one caret opens.
    const runsOfMarks = (marks) => {
        const runs = [];
        for (const m of marks) {
            const last = runs.at(-1);
            if (m.i > 0 && last) { last.marks.push(m); last.end = m.at + m.ch.length; }
            else runs.push({ at: m.at, end: m.at + m.ch.length, marks: [m] });
        }
        return runs;
    };
    // What an opened run says: the character's name, counted for a run of one
    // kind ("3 × zero-width space").
    const runLabel = (cps) => {
        if (cps.every((c) => c === cps[0])) return `${cps.length > 1 ? `${cps.length} × ` : ""}${charDress(cps[0])[1]}`;
        return `${cps.length} hidden characters of ${new Set(cps).size} kinds`;
    };
    const looksLikeDomain = (s) => {
        if (/\s/.test(s)) return false;
        const host = s.split(/[/?#]/)[0].split(":")[0];
        const labels = host.split(".");
        if (labels.length < 2) return false;
        // Unicode-aware so internationalized domains (åäö.se, münchen.de) match,
        // mirroring yuiolink_core::looks_like_domain. A TLD is letters, or its
        // punycode form (xn--p1ai for рф).
        const tld = labels.at(-1);
        if (!/^\p{L}{2,}$/u.test(tld) && !/^xn--./i.test(tld)) return false;
        if (!labels.every((l) => /^[\p{L}\p{N}-]+$/u.test(l))) return false;
        // DNS caps a label at 63 bytes, counted as punycode; the URL parser
        // makes that form, and refuses an xn-- label that does not decode.
        try {
            return new URL(`https://${host}`).hostname.split(".").every((l) => l.length <= 63);
        } catch {
            return false;
        }
    };
    const detectKind = (value) => {
        const t = value.trim();
        if (t === "") return "text";
        if (t.includes("\n")) return "text";
        return (looksLikeUrl(t) && !addressProblem(t)) || looksLikeDomain(t) ? "redirect" : "text";
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

    // --- inline icons ---
    // Built as nodes rather than assigned as markup: the page runs under
    // `require-trusted-types-for 'script'`, which rejects a plain string handed to
    // innerHTML — the one DOM-XSS sink a Content-Security-Policy cannot see. These
    // shapes are ours and static, so nothing is lost by drawing them this way.
    // Each shape is `[element, attributes]`; the same geometry is server-rendered
    // in views.rs for the result panel.
    const SVG_NS = "http://www.w3.org/2000/svg";
    const icon = (size, viewBox, shapes) => {
        const svg = document.createElementNS(SVG_NS, "svg");
        svg.setAttribute("width", size);
        svg.setAttribute("height", size);
        svg.setAttribute("viewBox", viewBox);
        svg.setAttribute("fill", "none");
        svg.setAttribute("aria-hidden", "true");
        for (const [name, attrs] of shapes) {
            const shape = document.createElementNS(SVG_NS, name);
            for (const [k, v] of Object.entries(attrs)) shape.setAttribute(k, v);
            svg.append(shape);
        }
        return svg;
    };
    const round = { stroke: "currentColor", "stroke-linecap": "round", "stroke-linejoin": "round" };
    // Two sheets, the back one peeking out behind the front.
    const copyIcon = () =>
        icon(16, "0 0 14 14", [
            ["path", { stroke: "currentColor", "stroke-linecap": "round", d: "M9.6 2.2H3.6c-.77 0-1.4.63-1.4 1.4v6.2", "stroke-width": "1.6" }],
            ["rect", { stroke: "currentColor", x: "4.9", y: "4.7", width: "7", height: "7.2", rx: "1.3", "stroke-width": "1.6" }],
        ]);
    // An eye: show what a private row covers; struck through once it shows.
    const eyeIcon = (struck) =>
        icon(16, "0 0 14 14", [
            ["path", { ...round, d: "M1.3 7C2.8 4.3 4.7 3 7 3s4.2 1.3 5.7 4C11.2 9.7 9.3 11 7 11S2.8 9.7 1.3 7Z", "stroke-width": "1.3" }],
            ["circle", { ...round, cx: "7", cy: "7", r: "1.9", "stroke-width": "1.3" }],
            ...(struck ? [["path", { ...round, d: "M2.3 1.8 11.7 12.2", "stroke-width": "1.3" }]] : []),
        ]);
    // A padlock: what a covered history row shows where its contents would be.
    const lockIcon = () =>
        icon(13, "0 0 14 14", [
            ["rect", { ...round, x: "3", y: "6.2", width: "8", height: "5.8", rx: "1.4", "stroke-width": "1.4" }],
            ["path", { ...round, d: "M4.9 6.2V4.6a2.1 2.1 0 0 1 4.2 0v1.6", "stroke-width": "1.4" }],
        ]);
    // A right arrow: a redirect row's mark. Drawn rather than typed, since the
    // arrow glyph differs from font to font (Edge and Chrome pick different ones).
    const arrowIcon = () =>
        icon(14, "0 0 14 14", [["path", { ...round, d: "M2 7h9.6M7.6 3l4 4-4 4", "stroke-width": "1.7" }]]);
    // A pair of opening quotes: a Text row's mark, as the arrow is a redirect's.
    const quoteIcon = () =>
        icon(13, "0 0 14 14", [
            ["path", { fill: "currentColor", d: "M2 8.6C2 6 3.3 4.2 5.6 3.3l.5.9C4.8 4.9 4.1 5.9 4 7h1.2c.8 0 1.4.6 1.4 1.4v1.8c0 .8-.6 1.4-1.4 1.4H3.4C2.6 11.6 2 11 2 10.2V8.6Zm5.4 0c0-2.6 1.3-4.4 3.6-5.3l.5.9C10.2 4.9 9.5 5.9 9.4 7h1.2c.8 0 1.4.6 1.4 1.4v1.8c0 .8-.6 1.4-1.4 1.4H8.8c-.8 0-1.4-.6-1.4-1.4V8.6Z" }],
        ]);
    // A ringed exclamation: a Text row holds hidden characters (the Text
    // page's "Hidden Characters" fact wears the same).
    const alertIcon = () =>
        icon(12, "0 0 16 16", [
            ["circle", { cx: "8", cy: "8", r: "6.6", stroke: "currentColor", "stroke-width": "1.4" }],
            ["path", { d: "M8 4.6v4.2", stroke: "currentColor", "stroke-width": "1.6", "stroke-linecap": "round" }],
            ["circle", { cx: "8", cy: "11.2", r: ".95", fill: "currentColor" }],
        ]);
    // A ringed "i": the soft kinds, noted rather than warned about.
    const infoIcon = () =>
        icon(12, "0 0 16 16", [
            ["circle", { cx: "8", cy: "8", r: "6.6", stroke: "currentColor", "stroke-width": "1.4" }],
            ["circle", { cx: "8", cy: "4.9", r: ".95", fill: "currentColor" }],
            ["path", { d: "M8 7.4v4", stroke: "currentColor", "stroke-width": "1.6", "stroke-linecap": "round" }],
        ]);
    // A ringed check: the row's hidden characters are taken out.
    const checkIcon = () =>
        icon(12, "0 0 16 16", [
            ["circle", { cx: "8", cy: "8", r: "6.6", stroke: "currentColor", "stroke-width": "1.4" }],
            ["path", { ...round, d: "M5.2 8.2 7.2 10.1 10.9 6", "stroke-width": "1.6" }],
        ]);
    // A small down chevron: a cut row's note opens the rest.
    const chevronIcon = () =>
        icon(12, "0 0 10 10", [["path", { ...round, d: "M2.5 3.8 5 6.3l2.5-2.5", "stroke-width": "1.5" }]]);
    // A flame: burns after one use.
    const flameIcon = () =>
        icon(11, "0 0 14 14", [
            ["path", { fill: "currentColor", d: "M7.2 1c.5 2.6 3.8 4 3.8 7.6A4 4 0 0 1 3 8.6c0-2 1.2-3.3 2.2-4 0 1.4.5 2.5 1.5 3C6.4 5.4 6.8 3.2 7.2 1Z" }],
        ]);
    // The arrow out of a box's corner: opens elsewhere.
    const openIcon = () =>
        icon(15, "0 0 13 13", [["path", { ...round, d: "M2.5 10.5 10.5 2.5M4 2.5h6.5V9", "stroke-width": "1.8" }]]);
    // The can the result panel's Delete draws too, so the two places a link can be
    // removed from show one icon.
    const trashIcon = () =>
        icon(16, "0 0 14 14", [
            [
                "path",
                {
                    ...round,
                    d: "M2.6 3.9h8.8M5.6 3.9V2.7c0-.4.3-.7.7-.7h1.4c.4 0 .7.3.7.7v1.2M4 3.9l.5 7c0 .5.4.9.9.9h3.2c.5 0 .9-.4.9-.9l.5-7",
                    "stroke-width": "1.4",
                },
            ],
        ]);

    // --- clipboard ---
    const flashCopied = (button) => {
        if (!button) return;
        // Symbol buttons (the history row's copy sheets) keep their glyph and
        // flash green via .copied alone; a button with a word (the result's
        // Copy, in its `.label`) says "Copied" for the same moment.
        const label = button.querySelector(".label");
        button.classList.add("copied");
        if (label) label.textContent = "Copied";
        setTimeout(() => {
            button.classList.remove("copied");
            if (label) label.textContent = "Copy";
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

    // A one-line text's width in half-em steps, from its width in tenths of an
    // em, rounded up and capped: app.css turns it into a font size that fits
    // (`[data-w]`). Mirrors the server's `fit_bucket`.
    const fitBucket = (tenths) => String(Math.min(60, Math.ceil(tenths / 5)));

    // The result hero: the name's words in lowercase, alternating two blues, with
    // the width bucket that keeps it on one line. Mirrors the server's
    // `hero_name`. The URL keeps its capitals; only this display drops them.
    const fillHero = (el, name) => {
        const words = name.match(NAME_WORDS) || [name];
        el.replaceChildren();
        words.forEach((w, i) => {
            const s = document.createElement("span");
            s.className = `nw nw-${i % 2}`;
            s.textContent = w.toLowerCase();
            el.append(s);
        });
        const chars = words.reduce((n, w) => n + w.length, 0);
        el.dataset.w = fitBucket(chars * 6 + (words.length - 1) * 3 + 2);
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
        if (!d || Number.isNaN(d.getTime())) return { text: "", compact: "", level: "", unit: "" };
        const ms = d.getTime() - Date.now();
        // "expired" keys off the real deadline (not the floored seconds) so the label never
        // flips before the link is actually gone — the last partial second still reads
        // "1 second", matching the greying/Delete-disable which also use the true deadline.
        if (ms <= 0) return { text: "expired", compact: "expired", level: "now", unit: "" };
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
        // `unit` is the bare span ("6 days"). Prose that is not a countdown borrows it
        // rather than stripping the " left" back off `text`.
        return { text: `${unit} left`, compact: short, level, unit };
    };
    // Result spans show the full phrase; history spans set data-compact for "1h"/"4m".
    const updateCountdown = (span) => {
        const { text, compact, level, unit } = formatCountdown(span.dataset.expires, span.dataset.created);
        // A history row opens its line with the time, so it reads as a sentence
        // there: "Expires in 6 days", then "Expired".
        const phrase = text === "expired" ? "Expired" : `Expires in ${unit}`;
        span.textContent = span.dataset.phrase === "expires" ? phrase : span.dataset.compact ? compact : text;
        span.classList.toggle("expiring-soon", level === "soon");
        span.classList.toggle("expiring-now", level === "now");
        // A dead result strikes through its name word and URL (the history list dims
        // its rows instead) and disables Copy — the link no longer resolves. History
        // countdowns sit outside .result, so this no-ops for them. Live, since this runs
        // every tick via tickCountdowns().
        //
        // A link dies two ways and only one of them is on the clock: it runs out, or
        // its creator withdraws it. This function owns the first and must not overwrite
        // the second, so a withdrawal is remembered on the panel and read back here.
        // Without that, the tick after a delete quietly un-struck a link the server had
        // already stopped serving — and since ticks land on display boundaries (or the
        // moment the tab is shown again), it looked intermittent and looked like the
        // fault of whatever the user happened to click next.
        const panel = span.closest(".result");
        if (panel) {
            const dead = text === "expired" || panel.classList.contains("withdrawn");
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

    // The pair beneath Copy. Preview is a plain link to the created URL — it works
    // on the no-JS result page too, so all it needs here is its href and its hidden
    // attribute dropped. Delete needs the creation token, which only this path
    // holds, so it stays hidden without one.
    //
    // Deleting withdraws the link: it stops resolving and answers 410, but the row
    // survives and the name stays reserved until the link would have expired — a
    // withdrawn name is never handed to someone else. The prompt therefore promises
    // only that the link stops working, and never a name released.
    const setupResultActions = (entry) => {
        const preview = document.getElementById("preview-result");
        if (preview && entry.url) {
            preview.href = entry.url;
            preview.hidden = false;
        }
        const del = document.getElementById("delete-result");
        const box = document.getElementById("result-confirm");
        // Which link the hero is showing: the name for markPanelWithdrawn, the history
        // entry's id for clearResultFor (a tombstone keeps only the id).
        const heroPanel = document.getElementById("link-panel");
        if (heroPanel) {
            heroPanel.dataset.name = entry.name ?? "";
            heroPanel.dataset.entry = entry.id ?? "";
        }
        if (!del || !box) return;
        box.hidden = true;
        box.replaceChildren();
        // `onclick` rather than addEventListener: this runs again on every create,
        // and a stacked listener would delete whichever link was made first.
        if (!entry.name || !entry.token) { del.hidden = true; del.onclick = null; return; }
        del.hidden = false;
        del.onclick = () => {
            const label = document.createElement("span");
            label.className = "result-confirm-label";
            label.textContent = "Stop this link working?";
            const cancel = document.createElement("button");
            cancel.type = "button";
            cancel.className = "cancel";
            cancel.textContent = "Cancel";
            cancel.addEventListener("click", () => { box.hidden = true; box.replaceChildren(); });
            const go = document.createElement("button");
            go.type = "button";
            go.className = "delete";
            go.textContent = "Delete Link";
            go.addEventListener("click", async () => {
                label.textContent = "Deleting link…";
                cancel.remove();
                go.remove();
                if (await serverDelete(entry.name, entry.token)) {
                    // serverDelete has already struck the hero through
                    // markPanelWithdrawn — it is the same link.
                    //
                    // Name the time left, the way the 410 page does, since the expiry is
                    // right here. Deliberately the app.js countdown and not the server's
                    // humanizer: the two renderers exist separately by design and round
                    // differently (this one rounds days up and holds, the server rounds
                    // to the nearest), so the figure can differ by a day near a boundary.
                    // That is the accepted cost of not fusing them.
                    const left = formatCountdown(entry.expires, entry.created).unit;
                    label.textContent = left
                        ? `Deleted. The name stays reserved for ${left}.`
                        : "Deleted. The name stays reserved until it would have expired.";
                    del.hidden = true;
                    tombstone(entry, "deleted");
                } else {
                    label.textContent = "Could not delete the link.";
                    box.append(go);
                }
            });
            box.replaceChildren(label, cancel, go);
            box.hidden = false;
        };
    };

    // ⌘C copies the created link from anywhere on the page, with nothing visibly
    // selected (tidier than a highlighted selection). The one thing that beats it
    // is a real selection the user made — anywhere, including inside a field —
    // which always copies natively. Listens on the document, so it works long
    // after the result panel lost focus.
    const enableQuietCopy = (panel, linkEl) => {
        // Controls that hold no text. Focus sits on one of these most of the time
        // (the link-type radios, the expiry slider), and a quiet copy there is the
        // whole point of this handler — so they never block it.
        const NON_TEXT_INPUT = new Set([
            "button", "checkbox", "color", "file", "hidden", "image",
            "radio", "range", "reset", "submit",
        ]);

        const isTextEntry = (el) =>
            !!el && (el.isContentEditable || el.tagName === "TEXTAREA"
                || (el.tagName === "INPUT" && !NON_TEXT_INPUT.has((el.type || "text").toLowerCase())));

        // A selection inside an input/textarea is invisible to window.getSelection()
        // in some engines; ask the field itself. `selectionStart` is not available on
        // every text-entry type — Chromium and Edge throw outright on `number`, which
        // is the expiry amount — so an unreadable field counts as "might be holding a
        // selection": we would rather do nothing than copy the result URL out from
        // under a Ctrl-C aimed at the digits the user just selected.
        const fieldHasSelection = (el) => {
            if (!isTextEntry(el)) return false;
            if (el.isContentEditable) return false; // window.getSelection() sees these
            try {
                if (el.selectionStart == null) return true;
                return el.selectionStart !== el.selectionEnd;
            } catch { return true; }
        };
        document.addEventListener("keydown", (event) => {
            const isCopy = (event.metaKey || event.ctrlKey) && (event.key === "c" || event.key === "C");
            if (!isCopy) return;
            if (panel.hidden) return; // no result yet: nothing to copy
            if (panel.classList.contains("expired")) return; // dead link: nothing to copy
            if (fieldHasSelection(document.activeElement)) return;
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed) return;
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
    // The window's width once its edge has been dragged. Saved only while Local
    // History is on, like the open/closed choice: that switch is the consent to
    // store anything in this browser. PRE_PAINT_JS in views.rs reads it too.
    const WIDTH_KEY = "yuiolink:window-width";
    const WIDTH_MIN = 480;
    const WIDTH_MAX = 720;
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
    // Which of the two switches did it ("pill" | "history") — the warning appears at
    // that one, so the consequence lands where the user just clicked.
    let warnAt = null;

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
            // Between two copies of the same state, the one whose cover was
            // toggled last wins, so an eye clicked in one tab reaches the others
            // instead of being written back over by their older copy.
            const win = !prev || mergeRank(e) > mergeRank(prev)
                || (mergeRank(e) === mergeRank(prev) && (e.coverAt ?? 0) > (prev.coverAt ?? 0)) ? e : prev;
            // The hidden-character mode travels the same way, on its own stamp,
            // so a mode picked in one tab and an eye clicked in another both last.
            const other = win === e ? prev : e;
            if (other && !win.tombstone && (other.marksAt ?? 0) > (win.marksAt ?? 0)) {
                const merged = { ...win, marksAt: other.marksAt };
                if (other.marks) merged.marks = other.marks;
                else delete merged.marks;
                if (other.softRemoved) merged.softRemoved = true;
                else delete merged.softRemoved;
                byId.set(e.id, merged);
            } else byId.set(e.id, win);
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

    // --- what a link holds, as its history row shows it ---
    // As much as the row shows, which is up to ten lines, and no more: a Text
    // link can be long, and this sits in localStorage for as long as the row
    // does. Text keeps its line breaks, and one line past the ten, so the row
    // can end in an ellipsis when there is more.
    const SNIPPET_MAX = 1000;
    const SNIPPET_LINES = 10;
    const snippetOf = (kind, payload) =>
        (kind === "redirect"
            ? payload.trim()
            : payload.trim().split(/\r?\n/).slice(0, SNIPPET_LINES + 1).join("\n")
        ).slice(0, SNIPPET_MAX);
    // Text the server marked comes with a tree of the same opening (cut there
    // by the same rule), whose text is the snippet.
    const snippetFor = (kind, payload, display) =>
        (kind === "text" && Array.isArray(display) ? treeText(display) : snippetOf(kind, payload));
    // The pixels a row's contents are known to add to its one-line height before
    // it is drawn: Text's own line breaks, up to the ten shown (19px each), the
    // stacked Copy and eye a row of two lines or more gets (50px at least), and
    // the size note under a row cut short (23px). Wrapping only adds to it.
    // PRE_PAINT_JS in views.rs counts the same, to reserve the height.
    const knownExtraPx = (it) => {
        if (it.kind !== "text" || !it.snippet || it.tombstone || isExpired(it) || isCovered(it)) return 0;
        const lines = it.snippet.split("\n").length;
        const shown = Math.min(SNIPPET_LINES, lines);
        const body = shown > 1 ? Math.max(shown * 19, 50) : 19;
        return body - 19 + (lines > SNIPPET_LINES ? 23 : 0);
    };
    // How much the link holds, counted over the whole payload rather than the
    // snippet, so a covered row can say its size without holding any more of
    // it: numbers only.
    const sizeOf = (payload) => {
        const t = payload.trim();
        return { chars: [...t].length, lines: t.split("\n").length };
    };
    // Secret and one-time links hold what their creator did not want on show, so
    // their rows start covered. Any row can be covered or shown by its eye: the
    // choice is `it.cover`, kept with the entry (and saved with it), and absent
    // while the row is as it started.
    const isPrivate = (it) => !!it.secret || it.uses === 1;
    const isCovered = (it) => it.cover ?? isPrivate(it);
    // A link that has expired no longer holds anything, so neither does its row:
    // the contents leave this device when the link leaves the server. Returns
    // whether anything was dropped. (PRE_PAINT_JS in views.rs counts rows with
    // contents by the same rule, to reserve their height.)
    const dropExpiredSnippets = () => {
        let dropped = false;
        for (const it of memHistory) {
            if (it.snippet && isExpired(it)) { delete it.snippet; delete it.display; delete it.cast; dropped = true; }
        }
        return dropped;
    };
    // The dragged width, or null at the default. Lives on the root element as
    // `--window-width-user`, which app.css applies to the front page's window.
    let windowWidth = null;
    const clampWidth = (w) => Math.round(Math.min(WIDTH_MAX, Math.max(WIDTH_MIN, w)));
    const applyWindowWidth = () => {
        const root = document.documentElement;
        if (windowWidth) root.style.setProperty("--window-width-user", `${windowWidth}px`);
        else root.style.removeProperty("--window-width-user");
    };
    const persistWidth = () => {
        if (!persistEnabled) return;
        if (windowWidth) lsSet(WIDTH_KEY, String(windowWidth));
        else lsDel(WIDTH_KEY);
    };
    const loadPersisted = () => {
        persistEnabled = lsGet(PERSIST_KEY) === "1";
        if (persistEnabled) {
            memHistory = mergeHistories(readStored(), []); // normalize + GC stale certificates
            historyOpen = lsGet(OPEN_KEY) !== "0"; // restore the open/closed choice (default open)
            const w = Number(lsGet(WIDTH_KEY));
            windowWidth = w > WIDTH_MIN ? clampWidth(w) : null;
        } else {
            lsDel(OPEN_KEY); // history off: forget any saved openness and width
            lsDel(WIDTH_KEY);
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
        // On the root element, not the section: the pre-paint script has to be able
        // to set it before the section exists (see PRE_PAINT_JS in views.rs).
        document.documentElement.classList.toggle("history-collapsed", !historyOpen);
    };
    const setHistoryOpen = (open) => { historyOpen = open; applyHistoryOpen(); persistOpen(); };
    const setPersist = (on) => {
        persistEnabled = on;
        // Turning on merges this tab's session links with whatever is already saved (and
        // other tabs') AND saves the current openness; turning off forgets both.
        if (on) { memHistory = mergeHistories(memHistory, readStored()); lsSet(PERSIST_KEY, "1"); persistNow(); persistOpen(); persistWidth(); }
        else { lsDel(PERSIST_KEY); lsDel(HISTORY_KEY); lsDel(OPEN_KEY); lsDel(WIDTH_KEY); }
    };
    const addHistory = (entry) => {
        entry.id = newId();
        if (persistEnabled) memHistory = mergeHistories(memHistory, readStored()); // pick up other tabs first
        memHistory = memHistory.filter((it) => it.url !== entry.url);
        memHistory.unshift(entry);
        persistNow();
    };

    // The eye's choice for one row. Stamped with the time, so the cross-tab merge
    // takes the latest; set back to the row's default, the override goes.
    const setCover = (it, covered) => {
        if (covered === isPrivate(it)) delete it.cover;
        else it.cover = covered;
        it.coverAt = Date.now();
    };
    const toggleCover = (it) => {
        setCover(it, !isCovered(it));
        renderHistory();
        // The list was rebuilt; hand focus to this row's contents again.
        document.querySelector(`.history-well[data-for="${it.id}"] :is(.history-cover, .history-eye)`)?.focus();
    };
    const plainPlural = (n, word) => `${n.toLocaleString("en-US")} ${word}${n === 1 ? "" : "s"}`;
    // What a covered row says instead of its contents: why it is covered, and how
    // much it holds. Characters round up to the next ten, so the exact length
    // never shows; Text says its lines too when there is more than one. Rows saved
    // before sizes were kept say only why.
    const coverSize = (it) => {
        if (!it.chars) return "";
        const chars = plainPlural(Math.ceil(it.chars / 10) * 10, "character");
        return it.kind === "redirect" || !(it.lines > 1) ? chars : `${plainPlural(it.lines, "line")}, ${chars}`;
    };
    // Line 1 of a live row: what the link holds, in a well edged in its kind's
    // color, led by the kind's mark (the arrow, or quotes for Text). The eye in
    // the well covers it. Covered, the whole well is one button that shows it
    // again: striped in the type's color, with a lock, the reason and the size.
    const contentsWell = (it, rowName) => {
        const well = document.createElement("div");
        well.className = `history-well ${it.kind === "redirect" ? "redirect" : "text"}`;
        well.dataset.for = it.id;
        if (!it.snippet) {
            // Expired rows lose their contents with the link; rows saved before
            // contents were kept never had any.
            well.classList.add("none");
            const gone = document.createElement("span");
            gone.className = "history-well-gone";
            gone.textContent = isExpired(it) ? "Expired, its contents are gone" : "Contents not saved on this device";
            well.append(gone);
            return well;
        }
        const mark = () => {
            const m = document.createElement("span");
            m.className = "history-mark";
            m.append(it.kind === "redirect" ? arrowIcon() : quoteIcon());
            m.setAttribute("aria-hidden", "true");
            return m;
        };
        if (isCovered(it)) {
            const why = it.uses === 1 ? "One-Time" : it.secret ? "Secret" : "Concealed";
            well.classList.add("covered", it.uses === 1 ? "once" : it.secret ? "secret" : "hidden");
            const cover = document.createElement("button");
            cover.type = "button";
            cover.className = "history-cover";
            const reason = document.createElement("span");
            reason.className = "history-cover-why";
            reason.textContent = why;
            // The button's name opens with what it does; the words drawn on it
            // follow, so a screen reader hears the reason and the size too.
            const act = document.createElement("span");
            act.className = "visually-hidden";
            act.textContent = `Reveal Contents of ${rowName}: `;
            cover.append(act, mark(), lockIcon(), reason);
            const size = coverSize(it);
            if (size) {
                const sz = document.createElement("span");
                sz.className = "history-cover-size";
                sz.textContent = `\u00b7 ${size}`;
                cover.append(sz);
            }
            const eye = document.createElement("span");
            eye.className = "history-eye";
            eye.append(eyeIcon(false));
            cover.append(eye);
            cover.addEventListener("click", () => toggleCover(it));
            well.append(cover);
            return well;
        }
        // The contents and, under them when they are cut short, a note saying
        // how much there is in all; a click opens the rest. The chevron leads the
        // note from the mark's column, so the text lines up with the contents.
        const col = document.createElement("div");
        col.className = "history-well-col";
        const body = document.createElement("span");
        body.className = "history-well-body";
        fillBody(it, body);
        if (body.querySelector(".hid")) wireMarks(it, body);
        const more = document.createElement("button");
        more.type = "button";
        more.className = "history-more";
        more.hidden = true;
        const open = expanded.has(it.id);
        const moreText = document.createElement("span");
        moreText.textContent = open ? "Show Less" : fullSize(it);
        more.append(chevronIcon(), moreText);
        more.setAttribute("aria-expanded", String(open));
        more.addEventListener("click", () => toggleExpand(it));
        col.append(body, more);
        if (open) well.classList.add("open");
        // Copy and the eye, for the contents (the link has its own Copy below).
        // Side by side on a one-line row, stacked on a taller one (fitWells).
        const side = document.createElement("span");
        side.className = "history-well-side";
        const copy = document.createElement("button");
        copy.type = "button";
        copy.className = "history-well-btn history-well-copy";
        copy.append(copyIcon());
        copy.setAttribute("aria-label", `Copy Contents of ${rowName}`);
        copy.title = "Copy the contents";
        copy.addEventListener("click", () => copyContents(it, copy));
        const eye = document.createElement("button");
        eye.type = "button";
        eye.className = "history-well-btn history-eye";
        eye.append(eyeIcon(true));
        eye.setAttribute("aria-label", `Conceal Contents of ${rowName}`);
        eye.title = "Conceal the contents";
        eye.addEventListener("click", () => toggleCover(it));
        side.append(copy, eye);
        well.append(mark(), col, side);
        return well;
    };
    // What a row's contents show: a redirect's destination in the preview's
    // dress when the server sent it (rows saved before that show the host and
    // path plainly), or Text as typed, all of it once fetched to open. Text
    // with hidden characters comes as a tree too, drawn in the row's mode.
    const fillBody = (it, body) => {
        if (it.kind === "redirect") {
            if (Array.isArray(it.display)) {
                body.classList.add("pv-url");
                paintDisplay(it.display, body);
                return;
            }
            const t = it.snippet.replace(/^https:\/\//i, "");
            const cut = t.indexOf("/");
            const host = document.createElement("span");
            host.className = "history-dest-host";
            host.textContent = cut < 0 ? t : t.slice(0, cut);
            body.append(host);
            if (cut >= 0) {
                const path = document.createElement("span");
                path.className = "history-dest-path";
                path.textContent = t.slice(cut);
                body.append(path);
            }
            return;
        }
        const c = rowContents(it);
        if (!Array.isArray(c.display)) {
            body.textContent = c.text;
            return;
        }
        const mode = marksMode(it);
        if (mode !== "labels") body.classList.add("marks-ticks", "ticked");
        paintDisplay(c.display, body, dropFor(it));
        groupRuns(it, body, mode);
    };
    // Marks side by side make a run, as in the form: in Ticks one tick with a
    // count, and the run a click opened shows its label in place; in Labels
    // one label each. A label names the character.
    const groupRuns = (it, body, mode) => {
        const runs = [];
        for (const m of body.querySelectorAll(".hid[data-tell]")) {
            const last = runs.at(-1);
            if (last && m.previousSibling === last.at(-1)) last.push(m);
            else runs.push([m]);
        }
        const opened = castOpen.get(it.id)?.run;
        runs.forEach((run, k) => {
            const label = runLabel(run.map((m) => Number.parseInt(m.dataset.tell.slice(1), 16)));
            run.forEach((m, i) => {
                m.dataset.run = String(k);
                if (i > 0) { m.classList.add("rest"); return; }
                m.classList.add("first");
                m.dataset.n = String(run.length);
                m.dataset.esc = label;
            });
            if (run.every((m) => isSoft(Number.parseInt(m.dataset.tell.slice(1), 16)))) run.forEach((m) => m.classList.add("soft"));
            if (mode !== "labels" && opened === k) run.forEach((m) => m.classList.add("lab"));
        });
    };
    // The server's tree, built as elements: a string is text, ["wbr"] a
    // line-break opportunity, [class, child...] a span. An object after the
    // class names another tag and the element's data. Only those tags and
    // data names, class names and text come from it, never markup. `drop`
    // leaves out marks, and so their characters, by tier (`hard`, `soft`); a
    // no-break space left out becomes a plain space, as in the form.
    const TREE_TAGS = new Set(["b", "code", "div"]);
    const TREE_DATA = new Set(["esc", "tell", "cp", "name"]);
    const treeExtra = (n) => (n[1] && typeof n[1] === "object" && !Array.isArray(n[1]) ? n[1] : null);
    const isMarkNode = (n) => Array.isArray(n) && typeof n[0] === "string" && n[0].split(" ").includes("hid");
    const markCp = (n) => Number.parseInt(treeExtra(n)?.tell?.slice(1) ?? "", 16);
    // What a mark leaves in place of its character under `drop`: null to
    // keep the mark, else the text standing in for it.
    const dropped = (n, drop) => {
        if (!drop || !isMarkNode(n)) return null;
        const cp = markCp(n);
        if (!isSoft(cp)) return drop.hard ? "" : null;
        return drop.soft ? (cp === 0xad ? "" : " ") : null;
    };
    const paintDisplay = (nodes, parent, drop = null) => {
        for (const n of nodes) {
            if (typeof n === "string") parent.append(n);
            else if (Array.isArray(n) && n.length === 1 && n[0] === "wbr") parent.append(document.createElement("wbr"));
            else if (Array.isArray(n) && typeof n[0] === "string") {
                const instead = dropped(n, drop);
                if (instead != null) { parent.append(instead); continue; }
                const extra = treeExtra(n);
                const el = document.createElement(TREE_TAGS.has(extra?.tag) ? extra.tag : "span");
                if (n[0]) el.className = n[0];
                for (const [k, v] of Object.entries(extra ?? {})) {
                    if (TREE_DATA.has(k) && typeof v === "string") el.dataset[k] = v;
                }
                paintDisplay(n.slice(extra ? 2 : 1), el, drop);
                parent.append(el);
            }
        }
    };
    // A code point a mark carries in `data-cp` (a NUL, which HTML cannot hold
    // in its text), as the character, or nothing if it is not one.
    const fromCp = (cp) => {
        const n = Number(cp);
        return Number.isInteger(n) && n >= 0 && n <= 0x10ffff ? String.fromCodePoint(n) : "";
    };
    // The text a tree holds, exactly as stored, or with `drop` as drawn.
    const treeText = (nodes, drop = null) => {
        let out = "";
        for (const n of nodes) {
            if (typeof n === "string") out += n;
            else if (!Array.isArray(n) || typeof n[0] !== "string") continue;
            else if (dropped(n, drop) != null) out += dropped(n, drop);
            else if (treeExtra(n)?.cp !== undefined) out += fromCp(treeExtra(n).cp);
            else out += treeText(n.slice(treeExtra(n) ? 2 : 1), drop);
        }
        return out;
    };
    // How many marks a tree holds that are of the kinds counted in red
    // (`soft` false) or of the soft kinds (`soft` true).
    const markCount = (nodes, soft = false) =>
        nodes.reduce((sum, n) => sum + (isMarkNode(n) ? Number(isSoft(markCp(n)) === soft)
            : Array.isArray(n) ? markCount(n.slice(1), soft) : 0), 0);
    // The same, read back from the page: a mark's label is generated content,
    // never text, so only a NUL's empty mark needs its data-cp put back.
    const exactText = (root) => {
        let out = "";
        for (const node of root.childNodes) {
            if (node.nodeType === Node.TEXT_NODE) out += node.data;
            else if (node.nodeType !== Node.ELEMENT_NODE) continue;
            else if (node.dataset.cp !== undefined) out += fromCp(node.dataset.cp);
            else out += exactText(node);
        }
        return out;
    };

    // --- hidden characters in a Text row ---
    // A row draws its hidden characters in one of three modes, cycled by the
    // chip in its footer: Ticks (a tick where each run sits, as in the form;
    // the default), Labels (each run wears its name) and Removed (left out of
    // the row, of a selection and of the row's Copy). The mode is the row's
    // own, `it.marks` (absent for Ticks), stamped like the eye for the merge.
    // Removed leaves the soft kinds in; they have their own gray chip, which
    // takes them out too (`it.softRemoved`, on the same stamp).
    const MARK_MODES = ["ticks", "labels", "removed"];
    const marksMode = (it) => (MARK_MODES.includes(it.marks) ? it.marks : "ticks");
    const dropFor = (it) => {
        const hard = marksMode(it) === "removed";
        return hard || it.softRemoved ? { hard, soft: Boolean(it.softRemoved) } : null;
    };
    const softChip = (it, n) => {
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "history-hidden soft";
        chip.dataset.for = it.id;
        chip.dataset.tier = "soft";
        const what = plainPlural(n, "Special Character");
        const state = document.createElement("span");
        state.className = "history-hidden-mode";
        state.textContent = it.softRemoved ? "\u00b7 Put Back" : "\u00b7 Remove";
        if (it.softRemoved) {
            chip.dataset.mode = "removed";
            chip.append(checkIcon(), `${what} Removed`, state);
        } else chip.append(infoIcon(), what, state);
        chip.title = it.softRemoved ? "Put them back in the row" : "Take them out of the row";
        chip.addEventListener("click", () => {
            if (it.softRemoved) delete it.softRemoved;
            else it.softRemoved = true;
            it.marksAt = Date.now();
            renderHistory();
            document.querySelector(`.history-hidden[data-for="${it.id}"][data-tier="soft"]`)?.focus();
        });
        return chip;
    };
    // The entry a mark opens, under the row, this page view only: row id to
    // the mark's `data-tell` and its run's place in the row.
    const castOpen = new Map();
    const setMarksMode = (it, mode) => {
        if (mode === "ticks") delete it.marks;
        else it.marks = mode;
        it.marksAt = Date.now();
        if (mode === "removed") castOpen.delete(it.id);
    };
    const marksChip = (it, n) => {
        const mode = marksMode(it);
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "history-hidden";
        chip.dataset.mode = mode;
        chip.dataset.for = it.id;
        const what = plainPlural(n, "Hidden Character");
        if (mode === "removed") chip.append(checkIcon(), `${what} Removed`);
        else {
            const state = document.createElement("span");
            state.className = "history-hidden-mode";
            state.textContent = `\u00b7 ${mode === "ticks" ? "Ticks" : "Labels"}`;
            chip.append(alertIcon(), what, state);
        }
        chip.title = { ticks: "Show them as labels", labels: "Take them out of the row", removed: "Show them as ticks" }[mode];
        chip.addEventListener("click", () => {
            setMarksMode(it, MARK_MODES[(MARK_MODES.indexOf(mode) + 1) % MARK_MODES.length]);
            renderHistory();
            document.querySelector(`.history-hidden[data-for="${it.id}"]:not([data-tier])`)?.focus();
        });
        return chip;
    };
    // What a mark means, under the row: its cast entry, the Text page's own.
    const castPanel = (it) => {
        const tell = castOpen.get(it.id)?.tell;
        const cast = rowContents(it).cast;
        if (!tell || !Array.isArray(cast)) return null;
        const entry = cast.find((n) => Array.isArray(n) && treeExtra(n)?.name === tell);
        if (!entry) return null;
        const panel = document.createElement("div");
        panel.className = "history-cast pv-cast";
        paintDisplay([entry], panel);
        return panel;
    };
    // A mark's label ignores the pointer (in Safari a label that takes it
    // breaks a drag), so a click is matched to a mark by where it landed, as
    // on the Text page, and only a plain click counts: one that ends a
    // drag-selection leaves the entry alone. A click on the open mark puts
    // its entry away.
    const markAt = (body, x, y) => {
        // A tick takes no width: matched within 6px of where it stands.
        if (body.classList.contains("ticked")) return markNear(body, x, y);
        for (const m of body.querySelectorAll(".hid[data-tell]")) {
            for (const r of m.getClientRects()) {
                if (r.width > 0 && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return m;
            }
        }
        return null;
    };
    const wireMarks = (it, body) => {
        body.addEventListener("click", (event) => {
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed) return;
            const mark = markAt(body, event.clientX, event.clientY);
            if (!mark) return;
            const run = Number(mark.dataset.run);
            if (castOpen.get(it.id)?.run === run) castOpen.delete(it.id);
            else castOpen.set(it.id, { tell: mark.dataset.tell, run });
            renderHistory();
        });
        body.addEventListener("mousemove", (event) => {
            body.classList.toggle("over-mark", Boolean(markAt(body, event.clientX, event.clientY)));
        });
        body.addEventListener("mouseleave", () => body.classList.remove("over-mark"));
    };
    // What a Text row draws: the whole contents once fetched to open, else
    // the opening it keeps. `display` and `cast` are there only when the
    // server marked something.
    const rowContents = (it) => expanded.get(it.id) ?? { text: it.snippet, display: it.display, cast: it.cast };
    const readDisplay = (json) => {
        if (!json) return undefined;
        try {
            const v = JSON.parse(json);
            return Array.isArray(v) ? v : undefined;
        } catch {
            return undefined;
        }
    };
    // The note on a cut row: how much the link holds in all.
    const fullSize = (it) => {
        if (!it.chars) return "More";
        const chars = `${plainPlural(it.chars, "character")} total`;
        return it.kind === "redirect" || !(it.lines > 1) ? chars : `${plainPlural(it.lines, "line")}, ${chars}`;
    };
    // Rows opened to their whole contents, this page view only: the id, and the
    // contents fetched for it, as fetchContents gives them (null until they
    // arrive, or if they cannot).
    const expanded = new Map();
    const toggleExpand = async (it) => {
        const refocus = () => document.querySelector(`.history-well[data-for="${it.id}"] .history-more`)?.focus();
        if (expanded.has(it.id)) {
            expanded.delete(it.id);
            renderHistory();
            refocus();
            return;
        }
        expanded.set(it.id, null);
        renderHistory();
        refocus();
        if (it.kind !== "text" || snippetComplete(it)) return;
        const full = await fetchContents(it);
        if (full != null && expanded.has(it.id)) {
            expanded.set(it.id, full);
            renderHistory();
            refocus();
        }
    };
    // Whether the row holds the whole of what the link holds.
    const snippetComplete = (it) => !!it.snippet && it.chars != null && [...it.snippet].length >= it.chars;
    // The whole contents from the server, with the tree the row draws them
    // by: `{ text, display, cast }`. The creator token goes along when the row
    // has one: it is what lets a limited link's creator read it back without
    // spending its use. Null when the server will not or cannot say.
    const fetchContents = async (it) => {
        if (!it.name) return null;
        try {
            const resp = await fetch(`${API_BASE}/api/v0/links/${encodeURIComponent(it.name)}?display=true`, {
                headers: it.token ? { Authorization: `Bearer ${it.token}` } : {},
            });
            if (!resp.ok) return null;
            const data = await resp.json();
            const text = data.content ?? data.target;
            return typeof text === "string" ? { text, display: data.display, cast: data.cast } : null;
        } catch {
            return null;
        }
    };
    // Copy what the link holds: from the row when it has all of it, else from
    // the server. A row in Removed copies it without its hidden characters.
    // The clipboard write starts inside the click, with the text to follow,
    // since Safari refuses a write begun after the fetch returns.
    const copyContents = async (it, button) => {
        const plain = (c) => (dropFor(it) && Array.isArray(c.display) ? treeText(c.display, dropFor(it)) : c.text);
        const full = expanded.get(it.id);
        if (full) return copyToClipboard(plain(full), button);
        if (snippetComplete(it)) return copyToClipboard(plain(rowContents(it)), button);
        const failed = () => flashClass(button, "failed");
        if (typeof ClipboardItem === "function") {
            const blob = fetchContents(it).then((c) => {
                if (c == null) throw new Error("contents unavailable");
                return new Blob([plain(c)], { type: "text/plain" });
            });
            try {
                await navigator.clipboard.write([new ClipboardItem({ "text/plain": blob })]);
                flashCopied(button);
            } catch {
                failed();
            }
            return;
        }
        const c = await fetchContents(it);
        if (c == null) failed();
        else copyToClipboard(plain(c), button);
    };
    // After the rows are drawn: stack Copy and the eye beside contents of more
    // than one line, and mark a row cut short (fade, note) when its contents run
    // past the ten lines or past what this device kept.
    const fitWells = () => {
        for (const well of document.querySelectorAll(".history-well:not(.covered):not(.none)")) {
            const body = well.querySelector(".history-well-body");
            const more = well.querySelector(".history-more");
            if (!body || !more) continue;
            const it = memHistory.find((e) => e.id === well.dataset.for);
            // Stacked only when the contents take two lines even beside the
            // narrower stacked pair. Measured in that layout every time, so a link
            // that wraps only beside the side-by-side pair stays side by side and
            // wraps, rather than flipping with whichever layout it was last in.
            well.classList.add("stacked");
            well.classList.toggle("stacked", body.getBoundingClientRect().height > 24);
            const open = well.classList.contains("open");
            if (body.classList.contains("ticked")) fitCounts(body);
            const cut = !open && (body.scrollHeight > body.clientHeight + 1 || (it?.chars ?? 0) > SNIPPET_MAX);
            body.classList.toggle("cut", cut);
            more.hidden = !(cut || open);
            if (open && it && it.kind === "redirect" && (it.chars ?? 0) > SNIPPET_MAX) {
                more.lastChild.textContent = `Show Less \u00b7 the first ${SNIPPET_MAX.toLocaleString("en-US")} characters`;
            }
        }
    };
    // Line 2: the link without its scheme, as it can be typed or pasted (Copy
    // still copies it whole). The host comes from the link itself, so a server
    // with its own domain shows that.
    const linkLine = (url) => {
        const line = document.createElement("code");
        line.className = "history-link";
        // An em dash first: the link is the source of the quote above it.
        const dash = document.createElement("span");
        dash.className = "history-link-dash";
        dash.setAttribute("aria-hidden", "true");
        dash.textContent = "\u2014";
        line.append(dash);
        const m = url.match(/^[a-z][a-z0-9+.-]*:\/\/([^/]+)\/([^#]*)/i);
        if (!m) { line.append(url); return line; }
        const host = document.createElement("span");
        host.className = "history-link-host";
        host.textContent = `${m[1]}/`;
        const name = document.createElement("span");
        name.className = "history-link-name";
        name.append(nameSpans(m[2]));
        line.append(host, name);
        return line;
    };
    // The header eye conceals every row that holds something; once nothing
    // shows, it reveals the public ones, which puts every row back as it
    // started. Only an eye, named by its tooltip, like the rows' own. With
    // nothing to do either way it steps aside (kept in the layout, so the head
    // holds still).
    const syncHideAll = (rows) => {
        const btn = document.getElementById("history-hide");
        if (!btn) return;
        const withContents = rows.filter((it) => !it.tombstone && it.snippet);
        const anyShown = withContents.some((it) => !isCovered(it));
        const anyChanged = withContents.some((it) => it.cover !== undefined);
        btn.replaceChildren(eyeIcon(anyShown));
        btn.setAttribute("aria-label", anyShown ? "Conceal All Contents" : "Reveal Public Contents");
        btn.title = anyShown ? "Conceal the contents of every link" : "Reveal the contents of public links";
        btn.dataset.action = anyShown ? "hide" : "public";
        btn.hidden = false;
        btn.classList.toggle("idle", !anyShown && !anyChanged);
        btn.disabled = !anyShown && !anyChanged;
    };
    const hideAllOrShowPublic = () => {
        const hide = document.getElementById("history-hide")?.dataset.action === "hide";
        for (const it of memHistory) {
            if (it.tombstone || !it.snippet) continue;
            setCover(it, hide ? true : isPrivate(it));
        }
        renderHistory();
    };

    const renderHistory = () => {
        dropExpiredSnippets();
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
        // Only the warning belonging to the switch that was flipped is shown; the other
        // stays hidden, so the message never appears in two places at once.
        const warned = warnArmed && !persistEnabled && n > 0;
        const warnPill = document.getElementById("storage-warning-pill");
        if (warnPill) warnPill.hidden = !(warned && warnAt === "pill");
        const warnHistory = document.getElementById("storage-warning-history");
        if (warnHistory) warnHistory.hidden = !(warned && warnAt === "history");

        const listEl = document.getElementById("history-list");
        if (!listEl) return;
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
        // Hand the reserved height its real number, so the space held open since the
        // first paint matches what is about to fill it. Both live on the root element
        // (see PRE_PAINT_JS in views.rs).
        const root = document.documentElement;
        root.style.setProperty("--history-rows", String(n));
        root.style.setProperty("--history-live", String(shown.filter((it) => !it.tombstone).length));
        root.style.setProperty("--history-extra", String(shown.reduce((sum, it) => sum + knownExtraPx(it), 0)));
        root.classList.toggle("has-history", n > 0);
        if (n === 0) return;
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
            li.className = "history-item live";
            if (isExpired(it)) li.classList.add("expired");
            const rowName = it.url.split("#")[0].split("/").pop();

            // Line 3: how long it has, led by a single-use flame or a use limit,
            // on the left; the actions on the right. The kind needs no word: the
            // well's mark and edge carry it.
            const foot = document.createElement("div");
            foot.className = "history-foot";
            const meta = document.createElement("small");
            meta.className = "history-meta";
            if (it.uses === 1) {
                const once = document.createElement("span");
                once.className = "history-once";
                once.append(flameIcon(), "Once");
                meta.append(once);
            } else if (it.uses) {
                const uses = document.createElement("span");
                uses.className = "history-uses";
                uses.textContent = plainPlural(it.uses, "use");
                meta.append(uses);
            }
            // A Text row's hidden characters, counted in what it draws: the
            // chip that cycles how they are drawn, and a gray one for the
            // soft kinds, each on its own line.
            const drawn = it.kind === "text" && it.snippet && !isCovered(it) && !isExpired(it) ? rowContents(it).display : null;
            const marks = Array.isArray(drawn) ? markCount(drawn) : 0;
            const softMarks = Array.isArray(drawn) ? markCount(drawn, true) : 0;
            if (marks || softMarks) {
                const tiers = document.createElement("span");
                tiers.className = "history-tiers";
                if (marks) tiers.append(marksChip(it, marks));
                if (softMarks) tiers.append(softChip(it, softMarks));
                meta.append(tiers);
            }
            const span = document.createElement("span");
            span.className = "countdown";
            span.dataset.phrase = "expires";
            span.dataset.expires = it.expires ?? "";
            if (it.created) span.dataset.created = String(it.created);
            updateCountdown(span);
            meta.append(span);

            const actions = document.createElement("div");
            actions.className = "history-actions";
            // The actions are symbols (styled in app.css), ordered Copy, Preview,
            // Trash: the two link actions first, the destructive one last at the
            // row's edge (a stray tap only opens the confirm). A copy flashes the
            // sheets green and puts a green check after the link itself, the
            // thing copied. Copy and the arrow are real links to the URL, so
            // right-click offers Copy Link / Open in New Tab; a left click on
            // the sheets copies instead.
            //
            // Every label names the link it acts on, title-cased like the visible
            // labels but leaving the name's own alternating case alone. The symbols
            // are identical down the list, so "Copy Link" by itself would give four
            // links one name and four destinations — indistinguishable to a screen
            // reader reading the page's links on their own.
            const link = linkLine(it.url);
            const copy = document.createElement("a");
            copy.className = "history-copy";
            copy.href = it.url;
            copy.append(copyIcon());
            copy.setAttribute("aria-label", `Copy Link ${rowName}`);
            copy.title = "Copy the link";
            copy.addEventListener("click", (event) => {
                event.preventDefault();
                copyToClipboard(it.url, copy, () => flashClass(link, "copied"));
            });
            const show = document.createElement("a");
            show.className = "history-show";
            show.href = it.url;
            show.target = "_blank";
            show.rel = "noopener noreferrer";
            show.append(openIcon());
            show.setAttribute("aria-label", `Preview Link ${rowName}`);
            show.title = "Open this link's preview in a new tab";
            const remove = document.createElement("button");
            remove.className = "history-remove";
            remove.type = "button";
            // No trailing "…": the glyph sat on the text baseline and pulled the eye
            // off the row.
            remove.append(trashIcon());
            remove.setAttribute("aria-label", `Remove Link ${rowName}`);
            remove.title = "Remove this link";
            // Opens the remove menu in this line, in place of these buttons — not a
            // toggle; the menu carries its own Cancel. openConfirm closes any other
            // row's menu first.
            remove.addEventListener("click", () => openConfirm(li, it));
            actions.append(copy, show, remove);
            foot.append(meta, actions);

            const cast = marks || softMarks ? castPanel(it) : null;
            li.append(contentsWell(it, rowName), ...(cast ? [cast] : []), link, foot);
            listEl.append(li);
        }
        fitWells();
        syncHideAll(shown);
        syncClearMenu();
    };

    // --- the window's right edge: drag it to widen the front page ---
    // Nothing is drawn for it: the edge itself is the handle, found by the
    // resize cursor it shows.
    // The window is centered, so it grows from the middle: the edge follows the
    // pointer and the left edge moves out to match. Keys work the same on the
    // focused grip; a double-click, or Home, puts the default width back. Phones
    // keep the full-width sheet, and app.css hides the grip there.
    const setupWindowGrip = () => {
        const win = document.querySelector(".app-window");
        if (!win) return;
        const grip = document.createElement("div");
        grip.className = "window-grip";
        grip.tabIndex = 0;
        grip.setAttribute("role", "separator");
        grip.setAttribute("aria-orientation", "vertical");
        grip.setAttribute("aria-label", "Window Width");
        grip.setAttribute("aria-valuemin", String(WIDTH_MIN));
        grip.setAttribute("aria-valuemax", String(WIDTH_MAX));
        grip.title = "Drag to widen the window; double-click to reset";
        const sync = () => grip.setAttribute("aria-valuenow", String(windowWidth ?? WIDTH_MIN));
        const set = (w) => {
            windowWidth = w && w > WIDTH_MIN ? clampWidth(w) : null;
            applyWindowWidth();
            sync();
        };
        let startX = 0;
        let startW = 0;
        grip.addEventListener("pointerdown", (event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            startX = event.clientX;
            startW = win.getBoundingClientRect().width;
            grip.setPointerCapture(event.pointerId);
            grip.classList.add("dragging");
        });
        grip.addEventListener("pointermove", (event) => {
            if (!grip.hasPointerCapture(event.pointerId)) return;
            set(startW + 2 * (event.clientX - startX));
        });
        const end = (event) => {
            if (!grip.hasPointerCapture(event.pointerId)) return;
            grip.releasePointerCapture(event.pointerId);
            grip.classList.remove("dragging");
            persistWidth();
        };
        grip.addEventListener("pointerup", end);
        grip.addEventListener("pointercancel", end);
        grip.addEventListener("dblclick", () => { set(null); persistWidth(); });
        grip.addEventListener("keydown", (event) => {
            const now = windowWidth ?? WIDTH_MIN;
            const step = event.shiftKey ? 80 : 20;
            let next;
            if (event.key === "ArrowRight") next = now + step;
            else if (event.key === "ArrowLeft") next = now - step;
            else if (event.key === "Home") next = WIDTH_MIN;
            else if (event.key === "End") next = WIDTH_MAX;
            else return;
            event.preventDefault();
            set(next);
            persistWidth();
        });
        sync();
        win.append(grip);
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

    // --- per-item removal: a menu in the row's last line, then break-on-server or forget ---
    // `refocus` hands focus back to the trash that opened the menu (its Cancel was
    // used); a menu closed because another row's opened leaves focus alone.
    const closeConfirm = (li, refocus = false) => {
        li.classList.remove("confirming");
        li.querySelector(".history-confirm")?.remove();
        if (refocus) li.querySelector(".history-remove")?.focus();
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
        const i = memHistory.findIndex((e) => e.id === it.id);
        // A cleared entry is gone: what is left under its id is only the death-certificate
        // other tabs still have to adopt. Writing over it would put the row back on screen
        // — which is exactly what a stale result panel's Delete used to do, so the row could
        // be cleared, resurrected, cleared again, forever.
        return i !== -1 && memHistory[i].tombstone === "cleared" ? -1 : i;
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
        clearResultFor([memHistory[i].id]);
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

    // While the server is contacted, swap the menu to a spinner; on failure offer
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
        actions.append(retry, cancelButton(li));
        overlay.replaceChildren(label, actions);
    };
    // Take the hero's Delete away and shut its prompt. Called once a link is withdrawn:
    // the only thing left for that button to do is ask the server to delete a link that
    // is already gone.
    const dropPanelDelete = () => {
        const del = document.getElementById("delete-result");
        if (del) { del.hidden = true; del.onclick = null; }
        // Close an unanswered prompt (its Cancel is still there) — it now asks about a
        // link that is already gone. A prompt already answered is left alone: the hero's
        // own Delete writes its outcome into this same box just after this runs.
        const box = document.getElementById("result-confirm");
        if (box && (!box.firstChild || box.querySelector(".cancel"))) {
            box.hidden = true;
            box.replaceChildren();
        }
    };
    // The hero shows exactly one link. When that link is withdrawn — from its own
    // Delete, from a history row, or from a forget-grace window — the hero has to say
    // so, and keep saying so. `withdrawn` is the sticky record of that: set once here,
    // never cleared, and read by updateCountdown, which owns `.expired` on the clock.
    // Matched by name off a dataset stamped when the result was rendered, rather than
    // by reading the displayed text, so it does not depend on how the URL is drawn.
    const markPanelWithdrawn = (name) => {
        const panel = document.getElementById("link-panel");
        if (!panel || !name || panel.dataset.name !== name) return;
        panel.classList.add("withdrawn", "expired");
        const copyBtn = panel.querySelector(".result-copy");
        copyBtn?.classList.add("disabled");
        copyBtn?.removeAttribute("href");
        dropPanelDelete();
    };
    // Clearing a history row also clears the hero, when the hero is showing that same
    // link: the row was the last record of it, and a hero left standing over a record
    // that no longer exists is a dead end — it kept offering Delete, and taking it wrote
    // the row back into the list. Matched by the entry id stamped when the result was
    // rendered, since a tombstone has already given up its name and url. The panel goes
    // back to its pre-create state, so the next link fills a clean one.
    const clearResultFor = (ids) => {
        const panel = document.getElementById("link-panel");
        const id = panel?.dataset.entry;
        if (!panel || !id || !ids.includes(id)) return;
        panel.hidden = true;
        panel.classList.remove("withdrawn", "expired", "stale");
        delete panel.dataset.entry;
        delete panel.dataset.name;
        currentResultUrl = null;
        dropPanelDelete();
        const box = document.getElementById("result-confirm");
        if (box) { box.hidden = true; box.replaceChildren(); }
    };
    // Best-effort DELETE on the server; true if the link is gone (204) or already gone
    // (404, expired/reaped). Shared by the confirm-menu Delete and the tombstone out.
    const serverDelete = async (name, token) => {
        try {
            const resp = await fetch(`${API_BASE}/api/v0/links/${encodeURIComponent(name)}`, {
                method: "DELETE",
                headers: { Authorization: `Bearer ${token}` },
            });
            const gone = resp.ok || resp.status === 404;
            // Marked here rather than at each call site, so no delete path added later
            // can forget to: every one of them comes through this function.
            if (gone) markPanelWithdrawn(name);
            return gone;
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
    // " Link" after Forget and Delete, dropped on a narrow row (app.css).
    const pillTail = () => {
        const tail = document.createElement("span");
        tail.className = "history-confirm-tail";
        tail.textContent = " Link";
        return tail;
    };
    // Cancel (✕) is the only way out, since the trash only opens the menu. It sits
    // at the right end, where the trash was, so a second tap there cancels.
    const cancelButton = (li) => {
        const cancel = document.createElement("button");
        cancel.type = "button";
        cancel.className = "history-confirm-cancel";
        cancel.textContent = "✕";
        cancel.title = "Cancel";
        cancel.setAttribute("aria-label", "Cancel");
        cancel.addEventListener("click", () => closeConfirm(li, true));
        return cancel;
    };
    const openConfirm = (li, it) => {
        for (const el of document.querySelectorAll(".history-item.confirming")) closeConfirm(el);
        li.classList.add("confirming");

        // R2: the time stays and the buttons become the answers, so the pills say
        // what is asked and no question is written out.
        const overlay = document.createElement("div");
        overlay.className = "history-confirm";
        const actions = document.createElement("div");
        actions.className = "history-confirm-actions";
        // An expired link is already gone server-side, which changes both buttons below.
        const expired = isExpired(it);

        // Forget first; Delete Link (destructive) second.
        const forget = document.createElement("button");
        forget.type = "button";
        forget.className = "history-confirm-forget";
        forget.append("Forget", pillTail());
        forget.setAttribute("aria-label", "Forget Link");
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
            server.append("Delete", pillTail());
            server.setAttribute("aria-label", "Delete Link");
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

        actions.append(cancelButton(li));

        overlay.append(actions);
        li.querySelector(".history-foot").append(overlay);
        forget.focus();
    };

    // --- hidden characters in the create form ---
    // The field holds plain text, so the marks are drawn over it: a copy of the
    // text laid out by the field's own metrics, transparent, with a tick (a
    // thin bar under a wedge) where each run of hidden characters sits. The
    // marks follow the caret, never the pointer: with the caret beside a run,
    // a flag over it names the run, its bar pulls up into its wedge, clear of
    // the caret, and its entry opens under the field; a selection opens every
    // entry inside it. The pointer only peeks. The caret is always the
    // field's own. Safari draws it a character off beside a hidden character
    // (a WebKit bug); a caret drawn by the page would trade that for trouble
    // with input methods, the phone's loupe and system caret settings.
    const COPY_PROPS = ["fontFamily", "fontSize", "fontWeight", "fontStyle", "fontStretch", "letterSpacing", "wordSpacing",
        "lineHeight", "paddingTop", "paddingLeft", "paddingBottom", "borderTopWidth", "borderRightWidth", "borderBottomWidth",
        "borderLeftWidth", "borderStyle", "boxSizing", "whiteSpace", "overflowWrap", "wordBreak", "textAlign", "textIndent",
        "direction", "tabSize", "alignContent", "borderRadius"];
    // The text without one tier of its hidden characters: with `soft`, the
    // no-break spaces (a plain space each) and soft hyphens; else the rest.
    const withoutHidden = (text, soft) => {
        const drop = new Set(scanHidden(text).filter((m) => m.soft === soft).map((m) => m.at));
        let out = "";
        let at = 0;
        for (const ch of text) {
            if (!drop.has(at)) out += ch;
            else if (ch === "\u2028" || ch === "\u2029") out += "\n";
            else if (soft && ch !== "\u00ad") out += " ";
            at += ch.length;
        }
        return out;
    };
    const setupFieldMarks = (ta, isRedirect) => {
        const field = document.getElementById("hc-field");
        if (!field || !CHARS) return { render: () => {} };
        const overlay = document.createElement("div");
        overlay.className = "hc-overlay ticked";
        overlay.setAttribute("aria-hidden", "true");
        const flag = document.createElement("span");
        flag.className = "hc-flag";
        const peek = document.createElement("span");
        peek.className = "hc-flag peek";
        flag.hidden = peek.hidden = true;
        field.append(overlay, flag, peek);
        const slot = document.createElement("div");
        slot.className = "hc-slot";
        const live = document.createElement("p");
        live.className = "visually-hidden";
        live.setAttribute("aria-live", "polite");
        field.after(slot, live);
        // `flag` is the run the caret stands beside (its `at`), `open` the
        // entries shown; `removed` holds the text around each tier's Remove
        // (`{ before, after }`), so Put Back can undo it until the field
        // changes.
        const st = { flag: null, open: [], removed: { hard: null, soft: null }, announced: 0, down: false, typing: false, ours: false };

        const el = (tag, cls, ...kids) => {
            const e = document.createElement(tag);
            if (cls) e.className = cls;
            e.append(...kids);
            return e;
        };
        const pct = (cp) => encodeURIComponent(String.fromCodePoint(cp)).toUpperCase();
        // A run's tick, its count, and its flag are placed from the first
        // character's box.
        const runCenter = (m) => {
            const r = m.getClientRects()[0];
            if (!r) return null;
            const fs = Number.parseFloat(getComputedStyle(overlay).fontSize);
            return { x: r.left + r.width / 2, top: r.top + 0.06 * fs, bottom: r.bottom };
        };
        const place = (badge, m, text) => {
            const c = m && runCenter(m);
            const box = ta.getBoundingClientRect();
            badge.hidden = !c || c.top < box.top || c.bottom > box.bottom;
            if (badge.hidden) return;
            const fr = field.getBoundingClientRect();
            badge.textContent = text;
            // Centered over the run, but never past the field's edges.
            const half = badge.offsetWidth / 2;
            badge.style.left = `${Math.min(Math.max(c.x - fr.left, half), fr.width - half)}px`;
            badge.style.top = `${c.top - fr.top}px`;
        };
        const placeFlag = () => {
            const m = overlay.querySelector(".hid[data-flag]");
            flag.classList.toggle("soft", Boolean(m?.classList.contains("soft")));
            place(flag, m, m?.dataset.flag);
        };

        // The field's text again, laid out as the field lays it out, with a
        // mark around each hidden character.
        const paintOverlay = (marks) => {
            const cs = getComputedStyle(ta);
            for (const p of COPY_PROPS) overlay.style[p] = cs[p];
            // The field's scrollbar narrows its text; the copy has none.
            const bars = ta.offsetWidth - ta.clientWidth - Number.parseFloat(cs.borderLeftWidth) - Number.parseFloat(cs.borderRightWidth);
            overlay.style.paddingRight = `${Number.parseFloat(cs.paddingRight) + Math.max(0, bars)}px`;
            overlay.replaceChildren();
            const runs = runsOfMarks(marks);
            const v = ta.value;
            let from = 0;
            for (const run of runs) {
                for (const m of run.marks) {
                    overlay.append(v.slice(from, m.at));
                    const s = el("span", "hid", m.ch);
                    s.dataset.run = String(run.at);
                    if (run.marks.every((x) => x.soft)) s.classList.add("soft");
                    if (st.open.includes(m.id)) s.classList.add("open");
                    if (m.i === 0) {
                        s.classList.add("first");
                        s.dataset.n = String(run.marks.length);
                        if (run.at === st.flag) s.dataset.flag = runLabel(run.marks.map((x) => x.cp));
                    }
                    if (run.at === st.flag) s.classList.add("flagged");
                    overlay.append(s);
                    from = m.at + m.ch.length;
                }
            }
            // A zero-width space holds open the empty line a final line break makes.
            overlay.append(v.slice(from), "\u200b");
            overlay.style.height = `${ta.offsetHeight}px`;
            overlay.scrollTop = ta.scrollTop;
            fitCounts(overlay);
            placeFlag();
        };

        // An entry: the character's tiles and name, the three ways to write
        // it (the one this kind of link stores in the mark's reverse red; a
        // click copies one), and what it does.
        // The caption's three words share one cell, so the button keeps its
        // width whichever shows.
        const formButton = (text, where, bad) => {
            const cap = el("small", "", el("span", "now", where), el("span", "", "Copied"), el("span", "", "Copy Failed"));
            const b = el("button", "form", el("code", bad ? "bad" : "", text), cap);
            b.type = "button";
            b.title = `Copy ${text}`;
            b.addEventListener("click", async () => {
                let ok = true;
                try { await navigator.clipboard.writeText(text); } catch { ok = false; }
                const [w, done, failed] = cap.children;
                for (const c of cap.children) c.classList.toggle("now", c === (ok ? done : failed));
                b.classList.toggle("copied", ok);
                setTimeout(() => {
                    for (const c of cap.children) c.classList.toggle("now", c === w);
                    b.classList.remove("copied");
                }, 1300);
            });
            return b;
        };
        const entryEl = (cp, count, url) => {
            const soft = isSoft(cp);
            const [sym, name] = charDress(cp);
            const times = count > 1 ? ` It appears ${count === 2 ? "twice" : `${count} times`}.` : "";
            return el("div", `entry solo ${soft ? "soft" : "warn"}`,
                el("span", "tiles", el("span", "tile sym", sym), el("span", "tile raw", cp ? String.fromCodePoint(cp) : "")),
                el("span", "what", el("b", "", name),
                    el("span", "forms",
                        formButton(`U+${hex4(cp)}`, "in Unicode", false),
                        formButton(`\\u{${cp.toString(16).toUpperCase()}}`, "in code", !url),
                        formButton(pct(cp), "in a URL", url)),
                    el("span", "after", `${charMeaning(cp)}.${times} The empty box is the character.`)));
        };

        const replaceField = (text) => {
            ta.focus();
            ta.select();
            st.ours = true;
            // insertText keeps the change on the field's undo stack.
            if (!document.execCommand("insertText", false, text)) {
                ta.value = text;
                ta.dispatchEvent(new Event("input", { bubbles: true }));
            }
            st.ours = false;
        };
        const removedNow = (tier) => st.removed[tier] != null && ta.value === st.removed[tier].after;
        // A tier's chip: red for the kinds that hide things, gray for the
        // soft ones. Each removes its own kind and puts it back.
        const chip = (tier, n) => {
            const soft = tier === "soft";
            const removed = removedNow(tier);
            const c = el("button", soft ? "history-hidden soft" : "history-hidden");
            c.type = "button";
            c.dataset.mode = removed ? "removed" : "ticks";
            c.dataset.tier = tier;
            const what = plural(n, soft ? "Special Character" : "Hidden Character");
            if (removed) c.append(checkIcon(), `${what} Removed`, el("span", "history-hidden-mode", "· Put Back"));
            else c.append(soft ? infoIcon() : alertIcon(), what, el("span", "history-hidden-mode", "· Remove"));
            c.addEventListener("click", () => {
                const words = soft ? "Special characters" : "Hidden characters";
                if (removed) {
                    const back = st.removed[tier].before;
                    st.removed[tier] = null;
                    replaceField(back);
                    live.textContent = `${words} put back.`;
                } else {
                    const before = ta.value;
                    const after = withoutHidden(before, soft);
                    replaceField(after);
                    st.removed[tier] = { before, after };
                    Object.assign(st, { open: [], flag: null });
                    live.textContent = `${words} removed.`;
                }
                render();
                slot.querySelector(`.history-hidden[data-tier="${tier}"]`)?.focus();
            });
            return c;
        };

        const render = () => {
            const marks = scanHidden(ta.value);
            const runs = runsOfMarks(marks);
            for (const tier of ["hard", "soft"]) if (!removedNow(tier)) st.removed[tier] = null;
            st.open = st.open.filter((id) => marks.some((m) => m.id === id));
            if (!runs.some((r) => r.at === st.flag)) st.flag = null;
            const hard = marks.filter((m) => !m.soft).length;
            const soft = marks.length - hard;
            if (marks.length && !st.announced) live.textContent = `${plural(marks.length, "hidden character")} in the text.`;
            st.announced = marks.length;
            slot.replaceChildren();
            peek.hidden = true;
            const url = isRedirect();
            const shown = st.open.map((id) => marks.filter((m) => m.id === id)).filter((list) => list.length);
            if (shown.length) slot.append(el("div", "history-cast pv-cast", ...shown.map((list) => entryEl(list[0].cp, list.length, url))));
            // A line per tier: its count, or how many were removed.
            const was = (tier) => scanHidden(st.removed[tier].before).filter((m) => m.soft === (tier === "soft")).length;
            for (const [tier, n] of [["hard", hard], ["soft", soft]]) {
                if (removedNow(tier)) slot.append(el("div", "history-meta", chip(tier, was(tier))));
                else if (n) slot.append(el("div", "history-meta", chip(tier, n)));
            }
            // A hidden character in a domain: the server drops it, or refuses
            // the address and the link is Text.
            const t = ta.value.trim();
            const dom = looksLikeUrl(t) && SCHEMES.has(t.slice(0, t.indexOf(":")).toLowerCase()) ? domainHidden(t) : null;
            const one = (n, a, b) => (n === 1 ? a : b);
            if (dom?.refused) {
                slot.append(el("p", "hc-note", alertIcon(), el("span", "",
                    `${one(dom.refused + dom.dropped, "Hidden character", "Hidden characters")} in domain, so this becomes a text link. Remove ${one(dom.refused + dom.dropped, "it", "them")} to redirect to `,
                    el("code", "", dom.clean), ".")));
            } else if (dom?.dropped) {
                slot.append(el("p", "hc-note", alertIcon(), el("span", "",
                    `${one(dom.dropped, "Hidden character", "Hidden characters")} in domain ${one(dom.dropped, "is", "are")} dropped. Goes to `,
                    el("code", "", dom.clean), ".")));
            }
            paintOverlay(marks);
        };

        // The caret decides what is open: after a click or a drag ends, and
        // after a key that moves it. Typing only closes: a run the caret left
        // shuts, but none opens while you type.
        const follow = ({ closeOnly = false } = {}) => {
            const a = ta.selectionStart;
            const b = ta.selectionEnd;
            const marks = scanHidden(ta.value);
            let flagAt = null;
            let ids;
            if (a === b) {
                const run = runsOfMarks(marks).find((r) => a >= r.at && a <= r.end);
                flagAt = run ? run.at : null;
                ids = run ? [...new Set(run.marks.map((m) => m.id))] : [];
            } else ids = [...new Set(marks.filter((m) => m.at >= a && m.at + m.ch.length <= b).map((m) => m.id))];
            if (closeOnly && (flagAt !== st.flag || st.flag == null)) [flagAt, ids] = [null, []];
            if (flagAt === st.flag && ids.join() === st.open.join()) return;
            Object.assign(st, { flag: flagAt, open: ids });
            if (ids.length) live.textContent = ids.map((id) => charDress(Number.parseInt(id.slice(1), 16))[1]).join(", ");
            render();
        };
        ta.addEventListener("pointerdown", () => { st.down = true; });
        document.addEventListener("pointerup", () => {
            if (!st.down) return;
            st.down = false;
            follow();
        });
        ta.addEventListener("keyup", (event) => {
            if (event.key.startsWith("Arrow") || ["Home", "End", "PageUp", "PageDown"].includes(event.key)
                || (event.key === "a" && (event.metaKey || event.ctrlKey))) follow();
        });
        // A held arrow key moves the caret long before keyup: close what it
        // left at once, so a flag never stays behind.
        document.addEventListener("selectionchange", () => {
            if (document.activeElement === ta && !st.down && !st.typing) follow({ closeOnly: true });
        });
        ta.addEventListener("beforeinput", () => { st.typing = true; });
        ta.addEventListener("input", () => {
            render();
            st.typing = false;
            follow({ closeOnly: true });
        });
        // The peek: the flag, quiet, over the run under the pointer, unless
        // that run is the open one.
        ta.addEventListener("mousemove", (event) => {
            const m = markNear(overlay, event.clientX, event.clientY);
            const first = m && overlay.querySelector(`.hid.first[data-run="${m.dataset.run}"]`);
            if (!first || Number(first.dataset.run) === st.flag) { peek.hidden = true; return; }
            const run = runsOfMarks(scanHidden(ta.value)).find((r) => String(r.at) === first.dataset.run);
            peek.classList.toggle("soft", first.classList.contains("soft"));
            place(peek, first, runLabel(run.marks.map((x) => x.cp)));
        });
        ta.addEventListener("mouseleave", () => { peek.hidden = true; });
        ta.addEventListener("scroll", () => {
            overlay.scrollTop = ta.scrollTop;
            placeFlag();
            peek.hidden = true;
        });
        if (typeof ResizeObserver === "function") new ResizeObserver(() => paintOverlay(scanHidden(ta.value))).observe(ta);
        render();
        return { render };
    };
    // The mark near a point: within 6px of where its tick stands, on its line.
    // A tick takes no width, so the pointer is matched to it, never the
    // other way round.
    const markNear = (root, x, y) => {
        for (const m of root.querySelectorAll(".hid")) {
            for (const r of m.getClientRects()) {
                if (x >= r.left - 6 && x <= r.right + 6 && y >= r.top && y <= r.bottom) return m;
            }
        }
        return null;
    };
    // A run's count stands right of its wedge; where a neighbor's tick or
    // count leaves no room it goes, and the bar thickens instead (more than
    // one). The flag and the peek still say how many.
    const fitCounts = (root) => {
        const fs = Number.parseFloat(getComputedStyle(root).fontSize);
        const w = 0.46 * fs;
        const digit = 0.6 * fs * 0.62;
        const gap = 0.1 * fs;
        const marks = [...root.querySelectorAll(".hid.first")].map((m) => {
            m.classList.remove("cnt-none");
            const r = m.getClientRects()[0];
            return r && { m, x: r.left + r.width / 2, line: Math.round(r.top), n: Number(m.dataset.n) };
        }).filter(Boolean);
        const placed = marks.map((k) => ({ a: k.x - w / 2, b: k.x + w / 2, line: k.line }));
        for (const k of marks) {
            if (k.n < 2) continue;
            const a = k.x + w / 2 + gap;
            const b = a + String(k.n).length * digit + 1;
            if (placed.every((p) => p.line !== k.line || b <= p.a || a >= p.b)) placed.push({ a, b, line: k.line });
            else k.m.classList.add("cnt-none");
        }
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
        // Park the thumb on the stop nearest the duration actually in force, so the
        // slider never contradicts the readout while the exact field is driving.
        // Setting .value programmatically fires no "input" event, so this cannot
        // trip the handler that clears the exact field when the slider is dragged.
        // An over-limit value pins the thumb to the last stop: the ceiling is where
        // that duration will land if it is submitted.
        const syncSliderToSeconds = (secs, over) => {
            if (!ttlSlider) return;
            if (over || !Number.isFinite(secs)) {
                ttlSlider.value = String(TTL_STOPS.length - 1);
                return;
            }
            let best = 0;
            for (let i = 1; i < TTL_STOPS.length; i += 1) {
                if (Math.abs(TTL_STOPS[i] - secs) < Math.abs(TTL_STOPS[best] - secs)) best = i;
            }
            ttlSlider.value = String(best);
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
            syncSliderToSeconds(secs, over);
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
            // JS adds the live readout, and CSS has already folded the exact
            // field away behind it (a filled exact field beats the slider).
            syncCustomMax();
            ttlSlider.addEventListener("input", () => {
                ttlCustomValue.value = ""; // the slider takes back over
                updateTtlReadout();
            });
            ttlReadout.addEventListener("click", () => {
                // Closing the exact field on an over-limit value settles it at
                // the ceiling — tapping the number is accepting what it shows.
                const open = ttlCustomField.classList.contains("open");
                if (open && (ttlOverLimit() || ttlCustomValue.validity.badInput)) {
                    ttlCustomValue.value = ttlCustomValue.max;
                    updateTtlReadout();
                }
                if (ttlCustomField.classList.toggle("open")) ttlCustomValue.focus();
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
        // The hidden-character marks over the field (setupFieldMarks).
        let fieldMarks = null;

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
        // One control picks the type: public (reusable, short, guessable), secret
        // (reusable, long unguessable name), or once (single-use, long name).
        const linkType = () => checkedValue("link_type", "public");
        const maxUses = () => (linkType() === "once" ? 1 : null);
        const isSecret = () => linkType() === "secret";

        // Holding Option/Alt forces a Text link even when the input looks like a URL.
        // We track whether it is held so the button label reflects what a submit makes.
        let altHeld = false;

        // The primary button names what it will create (Option flips a URL to Text).
        const formNote = document.getElementById("form-note");
        const updateSubmitLabel = () => {
            const empty = content.value.trim() === "";
            const kind = altHeld ? "text" : detectKind(content.value);
            submitBtn.textContent = empty ? "Create Link" : `Create ${kindLabel(kind)} Link`;
            // Input that tries to be an address but cannot be one becomes Text;
            // say so before it is sent, so a typo is caught rather than shared.
            const t = content.value.trim();
            // A hidden character in the domain is said under the field instead,
            // beside its marks, with the way to a redirect.
            const problem = !altHeld && looksLikeUrl(t) && !domainHidden(t)?.refused ? addressProblem(t) : "";
            if (formNote) {
                formNote.textContent = problem;
                formNote.hidden = !problem;
            }
            // Hint the hidden override only when it would change the outcome (a URL that
            // would otherwise redirect), so the tooltip is never misleading.
            submitBtn.title = (!empty && !altHeld && detectKind(content.value) === "redirect")
                ? "Hold Option to share as a text link instead"
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
            const name = url.split("#")[0].split("/").pop();
            if (linkWordEl) fillHero(linkWordEl, name);
            renderUrlInto(linkEl, url);
            linkEl.dataset.w = fitBucket([...url].length * 6 + 15); // + the copy check
            // Served by this page's own origin (`/{name}/qr.svg`), whatever API_BASE says.
            const qr = document.getElementById("link-qr");
            if (qr) { qr.src = `/${encodeURIComponent(name)}/qr.svg`; qr.hidden = false; }
            // `withdrawn` is sticky by design (see markPanelWithdrawn) and this is a
            // different link, so the hero starts alive again — otherwise the link just
            // created inherited the strike-through of the one deleted before it.
            panel.classList.remove("withdrawn", "expired");
            const copyBtn = document.getElementById("copy-result");
            if (copyBtn) { copyBtn.href = url; copyBtn.classList.remove("disabled"); }
            buildMeta(metaEl, kind, expiresIso, uses);
            if (resultNoteEl) resultNoteEl.hidden = true; // reset; create path re-shows if crowded
            panel.hidden = false;
            // Focus the panel (it precedes the form in the DOM) so the next Tab lands
            // on the input — no visible text selection, which looks tidier. ⌘C works
            // page-wide regardless (the quiet-copy handler listens on the document).
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
            const priv = isSecret();

            const kind = forceText ? "text" : detectKind(raw);
            const payload = kind === "redirect" ? normalizeTarget(raw.trim()) : raw;

            const restore = submitBtn.textContent;
            submitBtn.disabled = true;
            // Only show "Creating…" if the request runs long enough to notice; a fast
            // create would otherwise flash the label pointlessly.
            const creatingLabel = setTimeout(() => { submitBtn.textContent = "Creating…"; }, 150);
            try {
                const resp = await fetch(`${API_BASE}/api/v0/links`, {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({
                        kind,
                        content: payload,
                        ttl_seconds: ttl,
                        max_uses: uses,
                        secret: priv,
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
                // Keep the name + creator token so the history row can offer a real
                // server delete (token is undefined if the backend didn't send one).
                // `terms` is the creation receipt: the version + SHA-256 fingerprint
                // of the terms this link was made under (see /legal#verification).
                // `snippet` is what the link holds, cut short, so the row can say
                // what it was for; `secret` asks the row to keep it covered, and
                // `chars` and `lines` let the cover say how much is under it.
                // `display` is the contents as the row draws them, a tree from
                // the server (see paintDisplay): a redirect's destination in the
                // preview's dress, or Text with hidden characters marked, whose
                // `cast` names them. Such Text keeps the tree's own text as its
                // snippet, so the two never differ.
                const entry = { url, name: data.name, kind, uses, secret: priv, snippet: snippetFor(kind, payload, data.display), display: data.display, cast: data.cast, ...sizeOf(payload), expires: data.expires_at, token: data.creator_token, terms: data.terms, created: Date.now() };
                addHistory(entry); // stamps entry.id, which the result's Delete needs
                renderHistory();
                setupResultActions(entry);
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

        // Clear is hidden by CSS without JS, so here it only needs its handler.
        if (clearBtn) {
            clearBtn.addEventListener("click", () => {
                content.value = "";
                autosize();
                updateSubmitLabel();
                fieldMarks?.render();
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
        const flipPersist = (where) => {
            const turningOff = persistEnabled;
            setPersist(!persistEnabled);
            warnArmed = turningOff; // only warn when actively switching saving off
            warnAt = turningOff ? where : null;
            renderHistory();
        };
        document.getElementById("storage-toggle")?.addEventListener("click", () => flipPersist("pill"));
        document.getElementById("history-persist")?.addEventListener("click", () => flipPersist("history"));

        document.getElementById("history-hide")?.addEventListener("click", hideAllOrShowPublic);
        setupWindowGrip();
        // Rows are fitted to the list's width (fitWells): again whenever it
        // changes, as when the window's edge is dragged or a collapsed list opens
        // (rows drawn while it was collapsed had no width to measure).
        const list = document.getElementById("history-list");
        if (list && typeof ResizeObserver === "function") {
            let width = 0;
            new ResizeObserver(([entry]) => {
                const w = Math.round(entry.contentRect.width);
                if (w !== width) { width = w; fitWells(); }
            }).observe(list);
        }

        document.getElementById("history-clear-open")?.addEventListener("click", () => {
            setClearMenu(true);
        });
        document.getElementById("history-clear")?.addEventListener("click", () => {
            // Tombstone every row (don't just empty the list) so the clear propagates to
            // other tabs instead of being undone by their in-memory copies.
            memHistory = memHistory.map(clearedMarker);
            persistNow();
            clearResultFor(memHistory.map((it) => it.id));
            setClearMenu(false);
            renderHistory();
        });
        document.getElementById("history-toggle")?.addEventListener("click", () => {
            setHistoryOpen(!historyOpen);
        });
        document.getElementById("history-clear-expired")?.addEventListener("click", () => {
            const cleared = [];
            memHistory = memHistory.map((it) => {
                if (!isExpired(it)) return it;
                cleared.push(it.id);
                return clearedMarker(it);
            });
            persistNow();
            clearResultFor(cleared);
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
        fieldMarks = setupFieldMarks(content, () => !altHeld && detectKind(content.value) === "redirect");
        setupTtl();
        renderHistory();
        applyHistoryOpen(); // open by default on load (or the remembered state)
        applyWindowWidth(); // the pre-paint script set it already; this keeps them one
        content.focus();
    };

    // The no-JS result page: record the just-created link and return its fields.
    const recordResultPage = (linkEl) => {
        const url = linkEl.textContent.trim();
        if (!url) return null;
        const metaText = document.getElementById("link-expiry")?.textContent ?? "";
        const when = metaText.match(/expires (\d{4}-\d\d-\d\d \d\d:\d\d:\d\d) UTC/);
        const usesMatch = metaText.match(/max (\d+) uses/);
        // The terms receipt the server stamped on the result panel: version +
        // SHA-256 fingerprint of the terms this link was created under.
        const panel = document.getElementById("link-panel");
        const terms = panel?.dataset.termsVersion
            ? { version: panel.dataset.termsVersion, sha256: panel.dataset.termsSha256 }
            : null;
        const kind = /^Text/.test(metaText) ? "text" : "redirect";
        // What the form posted, as the server echoed it onto the panel (already
        // cut short there); a redirect gets the scheme the server stored it with.
        const raw = panel?.dataset.snippet ?? "";
        const display = readDisplay(panel?.dataset.display);
        const entry = {
            url,
            // The name is the last path segment (minus any #fragment). No token on the
            // no-JS path, so this row can only be forgotten on device, not server-deleted.
            name: url.split("#")[0].split("/").pop(),
            kind,
            uses: /one-time/.test(metaText) ? 1 : (usesMatch ? Number.parseInt(usesMatch[1], 10) : null),
            secret: panel?.dataset.secret === "1",
            snippet: raw ? snippetFor(kind, kind === "redirect" ? normalizeTarget(raw) : raw, display) : undefined,
            // The size of the whole contents, counted by the server, since the
            // snippet it echoed is cut short.
            chars: Number(panel?.dataset.chars) || undefined,
            lines: Number(panel?.dataset.lines) || undefined,
            display,
            cast: readDisplay(panel?.dataset.cast),
            expires: when ? when[1] : null,
            token: null,
            terms,
            created: Date.now(), // the result page loads at ~creation; grace runs from now
        };
        addHistory(entry);
        return entry;
    };

    // A selection in a row copies through the browser, which is exact unless
    // it spans a NUL's empty mark; only then is it rebuilt.
    document.addEventListener("copy", (event) => {
        const sel = window.getSelection();
        if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
        const range = sel.getRangeAt(0);
        const common = range.commonAncestorContainer;
        const el = common.nodeType === Node.ELEMENT_NODE ? common : common.parentElement;
        if (!el?.closest(".history-well-body")) return;
        const part = range.cloneContents();
        if (!part.querySelector("[data-cp]")) return;
        event.clipboardData.setData("text/plain", exactText(part));
        event.preventDefault();
    });

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
                    setupResultActions(entry); // Preview only: the page carries no token
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
