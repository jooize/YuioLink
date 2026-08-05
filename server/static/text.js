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
        // Between the box and the Copy button, straddling the box's bottom edge.
        copyBtn.before(row);

        btn.addEventListener("click", () => {
            const open = pre.classList.toggle("capped") === false;
            wrap.classList.toggle("capped", !open);
            btn.setAttribute("aria-expanded", String(open));
            label.textContent = open ? "Show less" : "Show all";
        });
    };

    document.addEventListener("DOMContentLoaded", () => {
        const pre = document.getElementById("text-body");
        const wrap = document.getElementById("text-wrap");
        const copyBtn = document.getElementById("copy-text");
        if (!pre || !wrap || !copyBtn) return;

        // The box's own border goes green — the confirmation belongs to the thing
        // that was copied. Held as long as the button's label so the two land and
        // clear together.
        const copyAll = async () => {
            try {
                await navigator.clipboard.writeText(pre.textContent);
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
