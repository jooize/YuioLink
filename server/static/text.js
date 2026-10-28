// Text-link viewer. The <pre> is already filled by the server; this wires the
// Copy button (which ships hidden — it is dead without JavaScript) and, when the
// snippet is long, folds it behind a "Show all" control.
//
// The fold lives here rather than in app.css on purpose: with no JavaScript there
// would be no way to open a collapsed box, so a no-JS visitor must get the whole
// snippet in flow. A stylesheet cap would hide text from exactly the people who
// cannot reveal it.
(() => {
    "use strict";

    // Fold only when the text is enough taller than the cap that folding earns the
    // extra control: a box two lines over the limit should just be shown.
    const SLACK = 90;

    const collapse = (wrap, pre) => {
        wrap.classList.add("capped");
        pre.classList.add("capped");

        const lines = pre.textContent.replace(/\n$/, "").split("\n").length;
        const row = document.createElement("div");
        row.className = "text-expand-row";
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "text-expand";
        btn.setAttribute("aria-expanded", "false");
        btn.setAttribute("aria-controls", "text-body");
        const label = document.createElement("span");
        label.textContent = "Show all";
        const count = document.createElement("span");
        count.className = "lines";
        count.textContent = `${lines} lines`;
        const chev = document.createElementNS("http://www.w3.org/2000/svg", "svg");
        chev.setAttribute("width", "10");
        chev.setAttribute("height", "7");
        chev.setAttribute("viewBox", "0 0 10 7");
        chev.setAttribute("fill", "none");
        chev.setAttribute("aria-hidden", "true");
        const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
        path.setAttribute("d", "M1 1.5 5 5.5l4-4");
        path.setAttribute("stroke", "currentColor");
        path.setAttribute("stroke-width", "1.8");
        path.setAttribute("stroke-linecap", "round");
        path.setAttribute("stroke-linejoin", "round");
        chev.append(path);
        btn.append(label, count, chev);
        row.append(btn);
        // Right under the box, straddling its bottom edge (the cast, when
        // there is one, follows it).
        wrap.after(row);

        btn.addEventListener("click", () => {
            const open = pre.classList.toggle("capped") === false;
            wrap.classList.toggle("capped", !open);
            btn.setAttribute("aria-expanded", String(open));
            label.textContent = open ? "Show less" : "Show all";
        });
        return btn;
    };

    // The stored text, read back from the page. A hidden character's mark draws
    // its escape as generated content, which is never part of the text, so the
    // page's text is already exact, with one exception: HTML cannot carry a
    // NUL (the parser turns it into U+FFFD), so its mark is empty and holds the
    // code point in data-cp instead.
    const exactText = (root) => {
        let out = "";
        for (const node of root.childNodes) {
            if (node.nodeType === Node.TEXT_NODE) out += node.data;
            else if (node.nodeType !== Node.ELEMENT_NODE) continue;
            else if (node.dataset.cp !== undefined) out += String.fromCodePoint(Number(node.dataset.cp));
            else out += exactText(node);
        }
        return out;
    };

    // An entry's ways to write its character ship as plain text, to select
    // without the script; each becomes a button that copies it, as in the
    // create form. The caption's three words share one cell, so the button
    // keeps its width whichever shows.
    const copyForms = (cast) => {
        for (const f of cast.querySelectorAll("span.form")) {
            const code = f.querySelector("code");
            const cap = f.querySelector("small");
            if (!code || !cap) continue;
            const text = code.textContent;
            const [where, done, failed] = [cap.textContent, "Copied", "Copy Failed"].map((t, i) => {
                const s = document.createElement("span");
                if (i === 0) s.className = "now";
                s.textContent = t;
                return s;
            });
            cap.replaceChildren(where, done, failed);
            const b = document.createElement("button");
            b.type = "button";
            b.className = "form";
            b.title = `Copy ${text}`;
            b.append(code, cap);
            b.addEventListener("click", async () => {
                let ok = true;
                try { await navigator.clipboard.writeText(text); } catch { ok = false; }
                for (const c of cap.children) c.classList.toggle("now", c === (ok ? done : failed));
                b.classList.toggle("copied", ok);
                setTimeout(() => {
                    for (const c of cap.children) c.classList.toggle("now", c === where);
                    b.classList.remove("copied");
                }, 1300);
            });
            f.replaceWith(b);
        }
    };

    // An open entry's close button, as app.js draws it: a cross. A press
    // keeps the focus where it was.
    const closeButton = (close) => {
        const ns = "http://www.w3.org/2000/svg";
        const svg = document.createElementNS(ns, "svg");
        for (const [k, v] of Object.entries({ width: "14", height: "14", viewBox: "0 0 14 14", fill: "none", "aria-hidden": "true" })) {
            svg.setAttribute(k, v);
        }
        const path = document.createElementNS(ns, "path");
        for (const [k, v] of Object.entries({ d: "M3.8 3.8l6.4 6.4M10.2 3.8l-6.4 6.4", stroke: "currentColor", "stroke-width": "1.6", "stroke-linecap": "round" })) {
            path.setAttribute(k, v);
        }
        svg.append(path);
        const b = document.createElement("button");
        b.type = "button";
        b.className = "entry-close";
        b.title = "Close";
        b.setAttribute("aria-label", "Close");
        b.append(svg);
        b.addEventListener("pointerdown", (event) => event.preventDefault());
        b.addEventListener("click", close);
        return b;
    };

    // Marks are drawn beside the text, never in it: an element boundary
    // inside a word changes how its letters join (WebKit splits shaping at
    // any element, Chromium at a generated box), so the server leaves the
    // joiners spelling puts in words as plain text and lists where they sit
    // (`data-words`: `[[start, end, id], ...]`, string offsets into the
    // text). Each gets an empty box over its characters, placed by measuring
    // them, which draws its wedge and is found by a click like any mark. A
    // character drawn inside another's shape has no box of its own, so then
    // the point where it starts. As app.js placeMarks does it.
    const placeMarks = (host, marks) => {
        const h = host.getBoundingClientRect();
        const ox = h.left + host.clientLeft - host.scrollLeft;
        const oy = h.top + host.clientTop - host.scrollTop;
        const walk = document.createTreeWalker(host, NodeFilter.SHOW_TEXT);
        let node = walk.nextNode();
        let base = 0;
        // A start at the seam of two text nodes takes the later one, an end
        // the earlier; marks come in order, so the walk never goes back.
        const seek = (i, start) => {
            while (node && (start ? i >= base + node.length : i > base + node.length)) {
                const next = walk.nextNode();
                if (!next) break;
                base += node.length;
                node = next;
            }
            return node && i >= base && i <= base + node.length ? [node, i - base] : null;
        };
        const range = document.createRange();
        for (const { at, end, box } of marks) {
            const a = seek(at, true);
            const b = a && seek(end, false);
            box.hidden = !b;
            if (!b) continue;
            range.setStart(...a);
            range.setEnd(...b);
            let r = range.getClientRects()[0];
            if (!r) {
                range.collapse(true);
                r = range.getBoundingClientRect();
            }
            Object.assign(box.style, {
                position: "absolute",
                left: `${r.left - ox}px`,
                top: `${r.top - oy}px`,
                width: `${r.width}px`,
                height: `${r.height}px`,
            });
        }
    };
    const readWords = (pre) => {
        let list;
        try {
            list = JSON.parse(pre.dataset.words ?? "[]");
        } catch {
            return [];
        }
        if (!Array.isArray(list)) return [];
        return list
            .filter((w) => Array.isArray(w) && Number.isInteger(w[0]) && Number.isInteger(w[1])
                && w[0] <= w[1] && /^w[0-9a-f]+$/.test(w[2]))
            .map(([at, end, tell]) => ({ at, end, tell }));
    };

    // A run's count stands right of its wedge; where a neighbor's tick or
    // count leaves no room it goes, and the bar thickens instead. As app.js
    // fitCounts does it.
    const fitCounts = (root) => {
        const fs = Number.parseFloat(getComputedStyle(root).fontSize);
        const w = 0.46 * fs;
        const digit = 0.6 * fs * 0.62;
        const gap = 0.1 * fs;
        const marks = [...root.querySelectorAll(".hid.first")].map((m) => {
            m.classList.remove("cnt-none");
            const r = m.getClientRects()[0];
            const after = m.classList.contains("cluster") ? r?.right + 0.08 * fs : null;
            return r && { m, x: r.left + r.width / 2, after, line: Math.round(r.top), n: Number(m.dataset.n) };
        }).filter(Boolean);
        const placed = marks.map((k) => ({ a: k.x - w / 2, b: k.x + w / 2, line: k.line }));
        for (const k of marks) {
            if (k.n < 2) continue;
            const a = k.after ?? k.x + w / 2 + gap;
            const b = a + String(k.n).length * digit + 1;
            if (placed.every((p) => p.line !== k.line || b <= p.a || a >= p.b)) placed.push({ a, b, line: k.line });
            else k.m.classList.add("cnt-none");
        }
    };

    // The characters that set direction (Unicode's Bidi_Control). Drawn
    // plain, one would reorder the line around it; each keeps an isolate
    // of its own, as in Labels.
    const BIDI_CONTROL = /^[؜‎‏‪-‮⁦-⁩]$/u;
    // A mark's ids ("u200b", or a cluster's "u200d w200d"); the letter is
    // its tier, `w` for a joiner where spelling puts it.
    const idsOf = (m) => (m.dataset.inner ?? m.dataset.tell ?? "").split(" ").filter(Boolean);
    const idCp = (id) => Number.parseInt(id.slice(1), 16);
    // A cluster's text without its counted marks, taken in order, as
    // app.js clusterLeft does it.
    const clusterLeft = (text, ids) => {
        const left = ids.filter((id) => id.startsWith("u")).map(idCp);
        let out = "";
        for (const ch of text) {
            if (ch.codePointAt(0) === left[0]) left.shift();
            else out += ch;
        }
        return out;
    };
    const svgIcon = (parts) => {
        const ns = "http://www.w3.org/2000/svg";
        const svg = document.createElementNS(ns, "svg");
        for (const [k, v] of Object.entries({ width: "12", height: "12", viewBox: "0 0 16 16", fill: "none", "aria-hidden": "true" })) {
            svg.setAttribute(k, v);
        }
        for (const [tag, attrs] of parts) {
            const el = document.createElementNS(ns, tag);
            for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
            svg.append(el);
        }
        return svg;
    };
    // A ringed check: the characters are taken out (app.js checkIcon).
    const checkIcon = () => svgIcon([
        ["circle", { cx: "8", cy: "8", r: "6.6", stroke: "currentColor", "stroke-width": "1.4" }],
        ["path", { d: "M5.2 8.2 7.2 10.1 10.9 6", stroke: "currentColor", "stroke-width": "1.6", "stroke-linecap": "round", "stroke-linejoin": "round" }],
    ]);

    // A ringed "!": the kinds that hide things (app.js alertIcon).
    const alertIcon = () => svgIcon([
        ["circle", { cx: "8", cy: "8", r: "6.6", stroke: "currentColor", "stroke-width": "1.4" }],
        ["path", { d: "M8 4.6v4.2", stroke: "currentColor", "stroke-width": "1.6", "stroke-linecap": "round" }],
        ["circle", { cx: "8", cy: "11.2", r: ".95", fill: "currentColor" }],
    ]);
    const PREV = "M9.8 3.4 5.2 8l4.6 4.6";
    const NEXT = "M6.2 3.4 10.8 8l-4.6 4.6";
    const svgArrow = (d) => svgIcon([["path", { d, stroke: "currentColor", "stroke-width": "1.7", "stroke-linecap": "round", "stroke-linejoin": "round" }]]);

    // The head above the text: a line per kind of character, each a name
    // and a verb, and the Labels | Ticks switch; under them the entry a
    // name or a mark opens. One state draws it all:
    //
    // - `view`: how marks are drawn. Labels names each run in the line, as
    //   the server drew it, so nothing moves when this script arrives;
    //   Ticks puts a tick where each run sits, beside the text.
    // - `out`: a tier taken out of the text, and so of a selection and of
    //   Copy. Red takes out the kinds that hide things; gray takes out the
    //   soft kinds, a no-break space becoming a plain space.
    // - `words`: whether the joiners spelling puts in words show their
    //   wedges. They start hidden; they are spelling, not a trick.
    // - `open`: the entry shown, as its group (`tier`) and its place there.
    //
    // A name opens its group's first entry, and a second click puts it
    // away; the arrows on the entry, and Left and Right, step through the
    // group. A click on a mark opens its entry, and on the open one puts it
    // away. While an entry is open the head stays at the top of the window
    // (app.css), so a long text scrolls under it.
    //
    // Every drawing starts again from the server's text (`src`): taking a
    // tier out can join two runs or split one, so each run is named anew,
    // from the names the entries give.
    //
    // A mark's label ignores the pointer, because in Safari a label that
    // takes it breaks a drag across the text. So a click is matched to a
    // mark by where it landed, and only a plain click counts: one that ends
    // a drag-selection leaves the entry alone.
    const setupHead = (pre, head, fold) => {
        const wrap = document.getElementById("text-wrap");
        const cast = document.querySelector(".text-cast");
        // The deck joins the head; until an entry opens it takes no room,
        // so the move shifts nothing.
        if (cast) {
            head.append(cast);
            copyForms(cast);
        }
        const src = [...pre.childNodes].map((n) => n.cloneNode(true));
        const words = readWords(pre);
        const soft = new Set((head.querySelector('[data-tier="soft"] .name')?.dataset.cast ?? "").split(" ").filter(Boolean));
        const entryFor = (name) => cast?.querySelector(`.entry[data-name="${name}"]`) ?? null;
        const names = new Map([...(cast?.querySelectorAll(".entry[data-name]") ?? [])]
            .map((e) => [e.dataset.name, e.querySelector(".what b")?.textContent ?? ""]));
        // As views.rs run_label names a run.
        const runLabel = (ids) => {
            const cps = ids.map(idCp);
            if (cps.length > 1 && cps.every((c) => c >= 0xe0020 && c <= 0xe007e)) {
                const spelled = String.fromCodePoint(...cps.slice(0, 40).map((c) => c - 0xe0000));
                return `tags spelling "${spelled}${cps.length > 40 ? "…" : ""}"`;
            }
            if (ids.every((id) => id === ids[0])) return `${ids.length > 1 ? `${ids.length} × ` : ""}${names.get(ids[0]) ?? ""}`;
            return `${ids.length} hidden characters of ${new Set(ids).size} kinds`;
        };
        const st = { view: "labels", out: { hard: false, soft: false }, words: false, open: null };
        const openId = () => (st.open ? groups.get(st.open.tier).entries[st.open.i].dataset.name : null);

        let placed = [];
        const place = () => {
            placeMarks(pre, placed);
            if (pre.classList.contains("ticked")) fitCounts(pre);
        };
        const draw = () => {
            const isMark = (n) => n.nodeType === Node.ELEMENT_NODE && n.classList.contains("hid");
            // The server's text with the tiers taken out; `shifts` says where
            // it changed length, to move the listed joiners with it.
            const out = [];
            const shifts = [];
            let at = 0;
            for (const n of src) {
                const len = n.textContent.length;
                let instead = null;
                if (isMark(n)) {
                    const ids = idsOf(n);
                    if (n.dataset.inner) {
                        if (st.out.hard) instead = clusterLeft(n.textContent, ids);
                    } else if (soft.has(ids[0])) {
                        if (st.out.soft) instead = idCp(ids[0]) === 0xad ? "" : " ";
                    } else if (st.out.hard) instead = "";
                }
                at += len;
                if (instead == null) {
                    out.push(n.cloneNode(true));
                    continue;
                }
                if (instead) out.push(document.createTextNode(instead));
                shifts.push([at, instead.length - len]);
            }
            // Runs anew: marks side by side, a cluster always alone.
            const runs = [];
            out.forEach((n, i) => {
                if (!isMark(n)) return;
                const prev = runs.at(-1);
                const alone = (m) => m.classList.contains("cluster");
                if (prev && out[i - 1] === prev.at(-1) && !alone(n) && !alone(prev[0])) prev.push(n);
                else runs.push([n]);
            });
            const shown = openId();
            runs.forEach((run, k) => {
                const ids = run.flatMap(idsOf);
                const open = ids.includes(shown);
                run.forEach((m, i) => {
                    m.dataset.run = String(k);
                    m.classList.toggle("first", i === 0);
                    m.classList.toggle("rest", i > 0);
                    m.classList.toggle("soft", ids.every((id) => soft.has(id)));
                    m.classList.toggle("open", open);
                    if (i > 0) delete m.dataset.label;
                    // A cluster keeps the server's name for its marks.
                    else if (!m.dataset.inner) m.dataset.label = runLabel(ids);
                });
                run[0].dataset.n = String(ids.length);
            });
            // In Ticks each mark gives way to its plain characters, and an
            // empty box over them takes its classes and data. A NUL keeps its
            // empty span, for its data-cp; a direction control an isolate.
            const ticked = st.view === "ticks";
            const flat = [];
            const boxes = [];
            let pos = 0;
            for (const n of out) {
                const len = n.textContent.length;
                if (ticked && isMark(n)) {
                    const box = document.createElement("span");
                    box.className = n.className;
                    for (const k of ["run", "n", "tell", "inner"]) {
                        if (n.dataset[k] !== undefined) box.dataset[k] = n.dataset[k];
                    }
                    let plain = document.createTextNode(n.textContent);
                    if (n.dataset.cp !== undefined) {
                        plain = document.createElement("span");
                        plain.dataset.cp = n.dataset.cp;
                    } else if (BIDI_CONTROL.test(n.textContent)) {
                        plain = document.createElement("span");
                        plain.className = "iso";
                        plain.textContent = n.textContent;
                    }
                    flat.push(plain);
                    boxes.push({ at: pos, end: pos + len, box });
                } else flat.push(n);
                pos += len;
            }
            const moved = (o) => o + shifts.reduce((d, [end, delta]) => d + (end <= o ? delta : 0), 0);
            for (const w of words) {
                const box = document.createElement("span");
                box.className = w.tell === shown ? "hid word open" : "hid word";
                box.dataset.tell = w.tell;
                boxes.push({ at: moved(w.at), end: moved(w.end), box });
            }
            boxes.sort((a, b) => a.at - b.at);
            pre.replaceChildren(...flat);
            // Adjacent text nodes become one, so nothing splits a word.
            pre.normalize();
            pre.classList.toggle("ticked", ticked);
            pre.classList.toggle("words-hidden", !st.words);
            pre.append(...boxes.map((m) => m.box));
            placed = boxes;
            place();
        };

        // The groups an entry steps through: each kind's name, with the
        // entries its data-cast lists, in the order of the text.
        const groups = new Map();
        for (const pair of head.querySelectorAll(".pair[data-tier]")) {
            const name = pair.querySelector(".name");
            const entries = (name?.dataset.cast ?? "").split(" ").map(entryFor).filter(Boolean);
            if (!name || !entries.length) continue;
            groups.set(pair.dataset.tier, {
                name,
                verb: pair.querySelector(".verb"),
                icon: name.querySelector("svg"),
                n: Number.parseInt(name.textContent, 10) || 0,
                entries,
            });
        }

        const live = document.createElement("p");
        live.className = "visually-hidden";
        live.setAttribute("aria-live", "polite");
        head.after(live);

        // Each entry, once: its place in its group after its name, and in
        // its corner the close button with the arrows under it.
        for (const [tier, g] of groups) {
            g.entries.forEach((entry, i) => {
                const side = document.createElement("span");
                side.className = "entry-side";
                side.append(closeButton(() => {
                    const had = entry.contains(document.activeElement);
                    close();
                    if (had) g.name.focus({ preventScroll: true });
                }));
                if (g.entries.length > 1) {
                    const of = document.createElement("span");
                    of.className = "of";
                    of.textContent = ` · ${i + 1} of ${g.entries.length}`;
                    entry.querySelector(".what b")?.after(of);
                    const arrow = (d, title, j) => {
                        const b = document.createElement("button");
                        b.type = "button";
                        b.title = title;
                        b.setAttribute("aria-label", title);
                        b.append(svgArrow(d));
                        b.disabled = j < 0 || j >= g.entries.length;
                        b.addEventListener("click", () => {
                            open(tier, j);
                            // The arrow keeps the focus, on the entry now shown.
                            g.entries[j].querySelector(`.stepper button:nth-child(${d === PREV ? 1 : 2})`)?.focus({ preventScroll: true });
                        });
                        return b;
                    };
                    const stepper = document.createElement("span");
                    stepper.className = "stepper";
                    stepper.append(arrow(PREV, "Previous (Left Arrow)", i - 1), arrow(NEXT, "Next (Right Arrow)", i + 1));
                    side.append(stepper);
                }
                entry.append(side);
            });
        }

        const paintChips = () => {
            for (const [tier, g] of groups) {
                const out = Boolean(st.out[tier]);
                const expanded = st.open?.tier === tier;
                const { name, verb } = g;
                name.classList.toggle("done", out);
                const icon = out ? checkIcon() : g.icon;
                const was = name.querySelector("svg");
                if (icon && was !== icon) {
                    if (was) was.replaceWith(icon);
                    else name.prepend(icon);
                }
                // Taken out, its entries name characters no longer there.
                if (out) {
                    name.removeAttribute("role");
                    name.removeAttribute("tabindex");
                    name.removeAttribute("aria-expanded");
                    name.removeAttribute("title");
                } else {
                    name.setAttribute("role", "button");
                    name.setAttribute("tabindex", "0");
                    name.setAttribute("aria-expanded", String(expanded));
                    name.title = expanded ? "Close" : g.entries.length > 1 ? "Show what each one is" : g.n === 1 ? "Show what it is" : "Show what they are";
                }
                if (!verb) continue;
                if (tier === "word") {
                    verb.textContent = st.words ? "· Hide" : "· Show";
                    verb.title = st.words ? "Hide the marks; the joiners stay in the text" : "Show where the joiners are";
                    continue;
                }
                const kind = tier === "hard" ? "hidden" : "special";
                const them = g.n === 1 ? `the ${kind} character` : `the ${g.n} ${kind} characters`;
                verb.textContent = out ? "· Put Back" : "· Remove";
                verb.title = out ? `Put ${them} back` : `Take ${them} out of the text`;
            }
            for (const b of head.querySelectorAll(".seg button")) {
                b.setAttribute("aria-pressed", String((b.textContent === "Ticks") === (st.view === "ticks")));
            }
        };

        // What Copy takes, under its label: the soft and hidden kinds still
        // in the text. The joiners in words are spelling, never named.
        const copyBtn = document.getElementById("copy-text");
        const paintCopy = () => {
            if (!copyBtn) return;
            const has = (t) => groups.has(t) && !st.out[t];
            const one = (t) => (groups.get(t).n === 1 ? `a ${t === "hard" ? "Hidden" : "Special"} Character` : `${t === "hard" ? "Hidden" : "Special"} Characters`);
            let sub = null;
            if (has("hard") && has("soft")) sub = [alertIcon(), "Includes Hidden and Special Characters"];
            else if (has("hard")) sub = [alertIcon(), `Includes ${one("hard")}`];
            else if (has("soft")) sub = [alertIcon(), `Includes ${one("soft")}`];
            else if (groups.has("hard") || groups.has("soft")) sub = [checkIcon(), "Leaves Out Hidden and Special Characters"];
            copyBtn.querySelector(".copy-sub")?.remove();
            if (!sub) return;
            const small = document.createElement("small");
            small.className = "copy-sub";
            small.append(...sub);
            copyBtn.append(small);
        };

        // The pulse marks an answer just given: only an entry newly shown,
        // never a redraw.
        let shownId = null;
        const paint = () => {
            draw();
            paintChips();
            paintCopy();
            const id = openId();
            // The deck stands as tall as the open group's tallest entry, so
            // stepping through the group never moves the text.
            for (const [tier, g] of groups) g.entries.forEach((e) => e.classList.toggle("in-group", st.open?.tier === tier));
            if (id === shownId) return;
            shownId = id;
            for (const e of cast?.querySelectorAll(".entry.shown") ?? []) e.classList.remove("shown");
            const entry = id && entryFor(id);
            if (!entry) return;
            void entry.offsetWidth; // restart the pulse
            entry.classList.add("shown");
        };

        // The mark an entry names, in the text as drawn now.
        const markOf = (id) => [...pre.querySelectorAll(".hid, .var")].find((m) => idsOf(m).includes(id));
        // Opening an entry: from a name, an arrow or a key, or a click in the
        // text (`from`, the clicked point, which then stays under the pointer).
        const open = (tier, i, from = false) => {
            const g = groups.get(tier);
            if (!g || i < 0 || i >= g.entries.length || st.out[tier]) {
                close();
                return;
            }
            const boxTop = wrap.getBoundingClientRect().top;
            st.open = { tier, i };
            paint();
            live.textContent = g.entries[i].querySelector(".what b")?.textContent ?? "";
            let m = markOf(openId());
            // Under the fold: open it, so the character is on screen.
            if (m && fold.capped() && m.getBoundingClientRect().top > pre.getBoundingClientRect().bottom - 30) {
                fold.open();
                m = markOf(openId());
            }
            if (from) {
                // The entry grew above the text: scroll by as much, at once,
                // so the text never slides under the pointer.
                const moved = wrap.getBoundingClientRect().top - boxTop;
                if (moved) window.scrollBy({ top: moved, behavior: "instant" });
            } else if (m) {
                // In view, below the head that stays at the top, and clear
                // of the window's bottom edge.
                const r = m.getBoundingClientRect();
                const top = head.offsetHeight;
                if (r.top < top + 8 || r.bottom > window.innerHeight - 48) {
                    window.scrollBy(0, r.top - top - Math.max(40, (window.innerHeight - top) / 3));
                }
            }
            stuck();
        };
        const close = () => {
            if (!st.open) return;
            st.open = null;
            paint();
            stuck();
        };

        for (const [tier, g] of groups) {
            const toggle = () => {
                if (st.out[tier]) return;
                if (st.open?.tier === tier) close();
                else open(tier, 0);
            };
            g.name.addEventListener("click", toggle);
            g.name.addEventListener("keydown", (event) => {
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                toggle();
            });
            g.verb?.addEventListener("click", () => {
                if (tier === "word") {
                    st.words = !st.words;
                    live.textContent = st.words ? "Joiners in words shown." : "Joiners in words hidden.";
                } else {
                    st.out[tier] = !st.out[tier];
                    const words = tier === "hard" ? "Hidden characters" : "Special characters";
                    live.textContent = st.out[tier] ? `${words} removed.` : `${words} put back.`;
                    if (st.open?.tier === tier) st.open = null;
                }
                paint();
                stuck();
            });
        }
        for (const b of head.querySelectorAll(".seg button")) {
            b.addEventListener("click", () => {
                st.view = b.textContent === "Ticks" ? "ticks" : "labels";
                live.textContent = `Marks shown as ${st.view}.`;
                paint();
            });
        }

        // A tick, and a joiner where spelling puts it, take no room in the
        // line, so each is matched within 6px of where it stands. A joiner
        // whose wedge is hidden is not there to click, unless it is the one
        // open. The marks are looked up each time: every change draws them
        // anew.
        const markAt = (x, y) =>
            [...pre.querySelectorAll(".hid[data-tell]")].find((m) => {
                const word = m.classList.contains("word");
                if (word && !st.words && !m.classList.contains("open")) return false;
                const pad = word || pre.classList.contains("ticked") ? 6 : 0;
                return [...m.getClientRects()].some(
                    (r) => x >= r.left - pad && x <= r.right + pad && y >= r.top && y <= r.bottom,
                );
            });
        const tierOf = (id) => [...groups].find(([, g]) => g.entries.some((e) => e.dataset.name === id));
        pre.addEventListener("click", (event) => {
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed) return;
            // A registered pair is real text, so it takes the click itself.
            const mark = markAt(event.clientX, event.clientY) || event.target.closest(".var");
            const id = mark?.dataset.tell;
            const found = id && tierOf(id);
            if (!found) return;
            if (id === openId()) {
                close();
                return;
            }
            const [tier, g] = found;
            open(tier, g.entries.findIndex((e) => e.dataset.name === id), true);
        });
        pre.addEventListener("mousemove", (event) => {
            pre.classList.toggle("over-mark", Boolean(markAt(event.clientX, event.clientY)));
        });
        pre.addEventListener("mouseleave", () => pre.classList.remove("over-mark"));

        // Left and Right step the open entry through its group, Escape puts
        // it away; never while typing, and never with a modifier.
        document.addEventListener("keydown", (event) => {
            if (!st.open || event.metaKey || event.ctrlKey || event.altKey) return;
            if (event.target.closest?.("input, textarea, select, [contenteditable]")) return;
            if (event.key === "Escape") {
                const had = cast?.contains(document.activeElement);
                const name = groups.get(st.open.tier).name;
                close();
                if (had) name.focus({ preventScroll: true });
                event.preventDefault();
                return;
            }
            const dir = { ArrowLeft: -1, ArrowRight: 1 }[event.key];
            if (!dir) return;
            event.preventDefault();
            const j = st.open.i + dir;
            if (j >= 0 && j < groups.get(st.open.tier).entries.length) open(st.open.tier, j);
        });

        // Pinned over the text, the head casts a soft edge.
        const stuck = () => {
            const r = head.getBoundingClientRect();
            head.classList.toggle("stuck", Boolean(st.open) && r.top <= 0.5 && wrap.getBoundingClientRect().top < r.bottom - 1);
        };
        window.addEventListener("scroll", stuck, { passive: true });

        paint();
        // Placed again whenever the text wraps anew (the window's width, the
        // fold opening) or a font that arrives late redraws it.
        if (typeof ResizeObserver === "function") new ResizeObserver(place).observe(pre);
        document.fonts?.addEventListener?.("loadingdone", place);
    };

    document.addEventListener("DOMContentLoaded", () => {
        const pre = document.getElementById("text-body");
        const wrap = document.getElementById("text-wrap");
        // The fold, once there is one: opened when an entry steps to a
        // character under it.
        const fold = {
            btn: null,
            capped: () => Boolean(pre?.classList.contains("capped")),
            open: () => { if (fold.capped()) fold.btn?.click(); },
        };
        const head = document.getElementById("text-head");
        if (pre && wrap && head) setupHead(pre, head, fold);
        const copyBtn = document.getElementById("copy-text");
        if (!pre || !wrap || !copyBtn) return;
        const copyLabel = copyBtn.querySelector(".copy-label") ?? copyBtn;

        // The box's own border goes green — the confirmation belongs to the thing
        // that was copied. Held as long as the button's label so the two land and
        // clear together.
        const copyAll = async () => {
            try {
                await navigator.clipboard.writeText(exactText(pre));
            } catch {
                return; // clipboard unavailable (insecure context) or denied
            }
            pre.classList.add("copied");
            copyLabel.textContent = "Copied";
            clearTimeout(copyAll.timer);
            copyAll.timer = setTimeout(() => {
                pre.classList.remove("copied");
                copyLabel.textContent = "Copy";
            }, 1500);
        };

        copyBtn.hidden = false;
        copyBtn.addEventListener("click", copyAll);

        // A selection the visitor made copies through the browser, which is
        // exact unless it spans a NUL's empty mark; only then is it rebuilt.
        document.addEventListener("copy", (event) => {
            const sel = window.getSelection();
            if (!sel || sel.isCollapsed || sel.rangeCount === 0) return;
            const range = sel.getRangeAt(0);
            if (!pre.contains(range.commonAncestorContainer)) return;
            const part = range.cloneContents();
            if (!part.querySelector("[data-cp]")) return;
            event.clipboardData.setData("text/plain", exactText(part));
            event.preventDefault();
        });

        // ⌘C anywhere on the page takes the whole snippet — that is what this page
        // is for. A selection the visitor made themselves wins: copying part of a
        // snippet on purpose is a real thing to want, and the button is right there
        // for the whole of it. Collapsed or expanded makes no difference; the text
        // copied is always the full text.
        document.addEventListener("keydown", (event) => {
            if (!(event.metaKey || event.ctrlKey)) return;
            if (event.key !== "c" && event.key !== "C") return;
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed) return;
            event.preventDefault();
            copyAll();
        });

        // Measure against the cap the stylesheet would apply, then decide.
        pre.classList.add("capped");
        const capped = pre.clientHeight;
        const full = pre.scrollHeight;
        pre.classList.remove("capped");
        if (full > capped + SLACK) fold.btn = collapse(wrap, pre);
    });
})();
