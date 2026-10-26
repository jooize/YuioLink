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

    const collapse = (wrap, pre, copyBtn) => {
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

    // The text draws its hidden characters as a History row does, in a
    // mode the red chip steps through: Labels (each run named in the line,
    // as the server drew it, so nothing moves when this script arrives),
    // Ticks (a tick where each run sits, beside the text) and Removed (left
    // out of the text, and so of a selection and of Copy). The gray chip
    // takes the soft kinds out, a no-break space becoming a plain space, and
    // puts them back. Both hold for this page view.
    //
    // Every drawing starts again from the server's text (`src`): removing
    // a tier can join two runs or split one, so each run is named anew,
    // from the names the cast entries give.
    const setupMarks = (pre) => {
        const src = [...pre.childNodes].map((n) => n.cloneNode(true));
        const words = readWords(pre);
        const soft = new Set((document.querySelector(".pv-fact.soft")?.dataset.cast ?? "").split(" ").filter(Boolean));
        const names = new Map([...document.querySelectorAll(".pv-cast .entry[data-name]")]
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
        const st = { mode: "labels", softOut: false };
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
                        if (st.mode === "removed") instead = clusterLeft(n.textContent, ids);
                    } else if (soft.has(ids[0])) {
                        if (st.softOut) instead = idCp(ids[0]) === 0xad ? "" : " ";
                    } else if (st.mode === "removed") instead = "";
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
            runs.forEach((run, k) => {
                const ids = run.flatMap(idsOf);
                run.forEach((m, i) => {
                    m.dataset.run = String(k);
                    m.classList.toggle("first", i === 0);
                    m.classList.toggle("rest", i > 0);
                    m.classList.toggle("soft", ids.every((id) => soft.has(id)));
                    if (i > 0) delete m.dataset.label;
                    // A cluster keeps the server's name for its marks.
                    else if (!m.dataset.inner) m.dataset.label = runLabel(ids);
                });
                run[0].dataset.n = String(ids.length);
            });
            // In Ticks each mark gives way to its plain characters, and an
            // empty box over them takes its classes and data. A NUL keeps its
            // empty span, for its data-cp; a direction control an isolate.
            const ticked = st.mode === "ticks";
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
                box.className = "hid word";
                box.dataset.tell = w.tell;
                boxes.push({ at: moved(w.at), end: moved(w.end), box });
            }
            boxes.sort((a, b) => a.at - b.at);
            pre.replaceChildren(...flat);
            // Adjacent text nodes become one, so nothing splits a word.
            pre.normalize();
            pre.classList.toggle("ticked", ticked);
            pre.append(...boxes.map((m) => m.box));
            placed = boxes;
            place();
        };
        draw();
        // Placed again whenever the text wraps anew (the window's width, the
        // fold opening) or a font that arrives late redraws it.
        if (typeof ResizeObserver === "function") new ResizeObserver(place).observe(pre);
        document.fonts?.addEventListener?.("loadingdone", place);

        const tiers = document.getElementById("text-tiers");
        if (!tiers) return;
        const live = document.createElement("p");
        live.className = "visually-hidden";
        live.setAttribute("aria-live", "polite");
        tiers.after(live);
        // Each chip keeps the server's count and icon; only its state is
        // drawn here.
        const wire = (chip, paint, step) => {
            if (!chip) return;
            const what = [...chip.childNodes].find((n) => n.nodeType === Node.TEXT_NODE)?.data ?? "";
            const icon = chip.querySelector("svg");
            const mode = (text) => {
                const s = document.createElement("span");
                s.className = "hidden-chip-mode";
                s.textContent = `· ${text}`;
                return s;
            };
            const render = () => paint(chip, { what, icon, mode });
            chip.addEventListener("click", () => {
                live.textContent = step();
                render();
                draw();
            });
            render();
        };
        const MODES = ["labels", "ticks", "removed"];
        wire(tiers.querySelector('[data-tier="hard"]'), (chip, { what, icon, mode }) => {
            chip.dataset.mode = st.mode;
            if (st.mode === "removed") chip.replaceChildren(checkIcon(), `${what} Removed`);
            else chip.replaceChildren(icon, what, mode(st.mode === "ticks" ? "Ticks" : "Labels"));
            chip.title = { labels: "Show them as ticks", ticks: "Take them out of the text", removed: "Show them as labels" }[st.mode];
        }, () => {
            st.mode = MODES[(MODES.indexOf(st.mode) + 1) % MODES.length];
            return { labels: "Hidden characters shown as labels.", ticks: "Hidden characters shown as ticks.", removed: "Hidden characters removed." }[st.mode];
        });
        wire(tiers.querySelector('[data-tier="soft"]'), (chip, { what, icon, mode }) => {
            chip.dataset.mode = st.softOut ? "removed" : "kept";
            if (st.softOut) chip.replaceChildren(checkIcon(), `${what} Removed`, mode("Put Back"));
            else chip.replaceChildren(icon, what, mode("Remove"));
            chip.title = st.softOut ? "Put them back in the text" : "Take them out of the text";
        }, () => {
            st.softOut = !st.softOut;
            return st.softOut ? "Special characters removed." : "Special characters put back.";
        });
    };

    // The cast answers when asked, the same deck the preview card uses: one
    // entry shown at a time, a click on a mark shows the entry naming its
    // character, and each chip with a data-cast (Hidden Characters, Variant
    // Forms) steps through its own entries (one past the last puts the deck
    // away). Without this script the stylesheet shows the whole list instead.
    //
    // A registered variation sequence answers the same way: its pair is
    // wrapped whole and opens the entry naming the form it picks.
    //
    // A mark's label ignores the pointer, because in Safari a label that
    // takes it breaks a drag across the text. So a click is matched to a
    // mark by where it landed, and only a plain click counts: one that ends
    // a drag-selection leaves the deck alone.
    const wireCast = (pre) => {
        const cast = document.querySelector(".pv-cast");
        if (!cast) return;
        copyForms(cast);
        const chips = [...document.querySelectorAll(".pv-fact[data-cast]")];
        const settle = () => chips.forEach((c) => c.setAttribute("aria-expanded", "false"));
        const entryFor = (name) => cast.querySelector(`.entry[data-name="${name}"]`);
        const show = (entry) => {
            cast.querySelectorAll(".entry.shown").forEach((e) => e.classList.remove("shown"));
            if (!entry) return;
            void entry.offsetWidth; // restart the pulse
            entry.classList.add("shown");
        };
        // A tick, and a joiner where spelling puts it, take no room in the
        // line, so each is matched within 6px of where it stands. The marks
        // are looked up each time: a change of mode draws them anew.
        const markAt = (x, y) =>
            [...pre.querySelectorAll(".hid[data-tell]")].find((m) => {
                const pad = m.classList.contains("word") || pre.classList.contains("ticked") ? 6 : 0;
                return [...m.getClientRects()].some(
                    (r) => x >= r.left - pad && x <= r.right + pad && y >= r.top && y <= r.bottom,
                );
            });
        // Each entry closes with a button: a shown entry is otherwise put
        // away only by stepping a chip past the last. From the keyboard the
        // focus goes back to the first chip, since the button goes with its
        // entry. A whole list or a sole entry stays open, so it has none.
        const closable = cast.classList.contains("whole") ? [] : cast.querySelectorAll(".entry:not(.solo)");
        for (const entry of closable) entry.append(closeButton(() => {
            const had = document.activeElement?.closest(".entry") === entry;
            show(null);
            settle();
            if (had) chips[0]?.focus({ preventScroll: true });
        }));

        pre.addEventListener("click", (event) => {
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed) return;
            // A registered pair is real text, so it takes the click itself.
            const mark = markAt(event.clientX, event.clientY) || event.target.closest(".var");
            if (!mark) return;
            show(entryFor(mark.dataset.tell));
            settle();
        });
        pre.addEventListener("mousemove", (event) => {
            pre.classList.toggle("over-mark", Boolean(markAt(event.clientX, event.clientY)));
        });
        pre.addEventListener("mouseleave", () => pre.classList.remove("over-mark"));

        for (const chip of chips) {
            const entries = chip.dataset.cast.split(" ").map(entryFor).filter(Boolean);
            chip.setAttribute("role", "button");
            chip.setAttribute("tabindex", "0");
            chip.setAttribute("aria-expanded", "false");
            const step = () => {
                // An entry another chip or a mark showed restarts this deck.
                const at = entries.findIndex((e) => e.classList.contains("shown"));
                const next = at + 1;
                show(entries[next]);
                settle();
                chip.setAttribute("aria-expanded", String(next < entries.length));
            };
            chip.addEventListener("click", step);
            chip.addEventListener("keydown", (event) => {
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                step();
            });
        }
    };

    document.addEventListener("DOMContentLoaded", () => {
        const pre = document.getElementById("text-body");
        if (pre) setupMarks(pre);
        if (pre) wireCast(pre);
        const wrap = document.getElementById("text-wrap");
        const copyBtn = document.getElementById("copy-text");
        if (!pre || !wrap || !copyBtn) return;

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
            copyBtn.textContent = "Copied";
            clearTimeout(copyAll.timer);
            copyAll.timer = setTimeout(() => {
                pre.classList.remove("copied");
                copyBtn.textContent = "Copy";
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
        if (full > capped + SLACK) collapse(wrap, pre, copyBtn);
    });
})();
