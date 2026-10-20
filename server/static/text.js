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

    // The cast answers when asked, the same deck the preview card uses: one
    // entry shown at a time, a click on a mark shows the entry naming its
    // character, and each chip with a data-cast (Hidden Characters, Variant
    // Forms) steps through its own entries (one past the last puts the deck
    // away). Without this script the stylesheet shows the whole list instead.
    //
    // A mark's label ignores the pointer, because in Safari a label that
    // takes it breaks a drag across the text. So a click is matched to a
    // mark by where it landed, and only a plain click counts: one that ends
    // a drag-selection leaves the deck alone.
    const wireCast = (pre) => {
        const cast = document.querySelector(".pv-cast");
        if (!cast) return;
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
        const markAt = (x, y) =>
            marks.find((m) =>
                [...m.getClientRects()].some(
                    (r) => x >= r.left && x <= r.right && y >= r.top && y <= r.bottom,
                ),
            );

        pre.addEventListener("click", (event) => {
            const sel = window.getSelection();
            if (sel && !sel.isCollapsed) return;
            const mark = markAt(event.clientX, event.clientY);
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
