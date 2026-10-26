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
        const marks = [...pre.querySelectorAll(".hid[data-tell]")];
        const chips = [...document.querySelectorAll(".pv-fact[data-cast]")];
        const settle = () => chips.forEach((c) => c.setAttribute("aria-expanded", "false"));
        const entryFor = (name) => cast.querySelector(`.entry[data-name="${name}"]`);
        const show = (entry) => {
            cast.querySelectorAll(".entry.shown").forEach((e) => e.classList.remove("shown"));
            if (!entry) return;
            void entry.offsetWidth; // restart the pulse
            entry.classList.add("shown");
        };
        // A joiner where spelling puts it draws only a tick and takes no
        // width, so it is matched within 6px of where it stands.
        const markAt = (x, y) =>
            marks.find((m) => {
                const pad = m.classList.contains("word") ? 6 : 0;
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
