// The preview page's progressive enhancement.
//
// Without this file the page is complete and honest: every part is listed,
// every value is readable, the destination is a real link, and the raw lines
// are selectable text. What the script adds is the ability to *edit* — to untick
// the parts of a link you would rather not carry — plus explicit Copy controls
// and the ⌘C shortcut.
//
// Two rules govern everything here.
//
// **Removal only.** Nothing in this file can add a character. Every rebuilt
// string is a subset of the stored one, so a link that passed the allowlist on
// the way in cannot be edited into one that would not.
//
// **Nothing is revealed or hidden on load.** Every element the script needs is
// one it creates itself; it never un-hides something the server left in the
// page. That is what keeps the first paint the final paint (the site has a
// layout-shift history), and it is why the no-JS page has no empty checkboxes,
// no dead Copy buttons, and no collapsed rows waiting for a script. The one
// reveal here happens on a *click*, never on load: a cast entry naming a
// marked character. The stylesheet hides the cast only when this script is
// present (html.js); the no-JS page shows the list in full.
//
// There is also no selection or clipboard interception, deliberately. Copying
// is what the Copy buttons do; selecting text does exactly what it looks like.
//
// And no HTML is ever assembled as a string here. The site's CSP carries
// `require-trusted-types-for 'script'` with no policy allowed, so an innerHTML
// assignment throws; the server hands over `(class, text)` runs and this file
// builds elements and sets textContent.
(() => {
    "use strict";

    /** How long a copy confirmation stays up. */
    const FEEDBACK_MS = 1300;
    /** A single click waits this long, so a double-click can cancel it. */
    const TOGGLE_DELAY_MS = 180;

    document.addEventListener("DOMContentLoaded", () => {
        const card = new Card();
        card.wire();
    });

    /**
     * One preview page's worth of behaviour. Every piece is optional: a card
     * with no editable parts still gets its Copy controls, and a card with no
     * destination at all (a blind one-time link) gets nothing and says nothing.
     */
    class Card {
        constructor() {
            this.action = document.querySelector(".pv-btn.go");
            this.slices = document.querySelector(".pv-slices[data-card]");
            this.model = readModel(this.slices);
            this.rows = [];
            this.edited = null;
        }

        wire() {
            copyShortcut();
            if (this.model) this.buildRows();
            this.buildCast();
            this.buildSplit();
            this.buildRawCopyButtons();
            if (this.model) {
                this.buildEditedLine();
                this.refresh();
            }
        }

        // ------------------------------------------------------------------
        // The cast
        // ------------------------------------------------------------------

        /**
         * The cast answers when asked. Every marked character the server
         * tagged with `data-tell` becomes a control: clicking it opens the
         * fold if it was closed and shows the one cast entry that names the
         * character, pulsing once. One answer at a time; asking another
         * swaps it. A click only ever opens — asking the same character
         * again re-pulses its entry rather than blanking it, and putting the
         * answers away is what closing the fold does (the user's call:
         * toggling read as confusion). Without this script the stylesheet
         * shows the whole cast instead, so nothing here is the only way to
         * the information.
         */
        buildCast() {
            const cast = document.querySelector(".pv-cast");
            if (!cast) return;
            const wired = [];
            document.querySelectorAll("[data-tell]").forEach((mark) => {
                const name = mark.getAttribute("data-tell");
                const entry = cast.querySelector('.entry[data-name="' + name + '"]');
                if (!entry) return;
                wired.push(mark);
                mark.classList.add("ask");
                mark.setAttribute("role", "button");
                mark.setAttribute("tabindex", "0");
                mark.setAttribute("aria-expanded", "false");
                const tell = (event) => {
                    // A mark can sit inside a togglable row; naming a
                    // character must not also untick the part it lives in.
                    event.stopPropagation();
                    const selection = window.getSelection();
                    if (selection && !selection.isCollapsed) return;
                    cast.querySelectorAll(".entry.shown").forEach((shown) =>
                        shown.classList.remove("shown"),
                    );
                    wired.forEach((m) => m.setAttribute("aria-expanded", "false"));
                    const fold = cast.closest("details");
                    if (fold) fold.open = true;
                    void entry.offsetWidth; // restart the pulse
                    entry.classList.add("shown");
                    mark.setAttribute("aria-expanded", "true");
                };
                mark.addEventListener("click", tell);
                mark.addEventListener("keydown", (event) => {
                    if (event.key !== "Enter" && event.key !== " ") return;
                    event.preventDefault();
                    tell(event);
                });
            });
        }

        // ------------------------------------------------------------------
        // The checkboxes
        // ------------------------------------------------------------------

        /**
         * Give every listed part a native checkbox.
         *
         * Fixed parts — the host, the port, the path, a magnet's `xt` — get a
         * checked, disabled box rather than no box at all: same geometry, and
         * the browser's own disabled look says "not yours to change" better
         * than an absence would. The boxes are re-inked grey with
         * `accent-color` rather than replaced, so focus, keyboard, and screen
         * reader behaviour are the platform's.
         */
        buildRows() {
            this.slices.querySelectorAll(".pv-slice").forEach((row) => {
                const index = Number(row.getAttribute("data-slice"));
                const part = this.model.byIndex.get(index);
                if (!part) return;
                const box = document.createElement("input");
                box.type = "checkbox";
                box.checked = true;
                box.disabled = part.fixed;
                box.setAttribute("aria-label", keepLabel(part));
                row.insertBefore(box, row.firstChild);
                const entry = { part, row, box };
                this.rows.push(entry);
                box.addEventListener("change", () => this.refresh());
                if (!part.fixed) this.makeRowClickable(entry);
            });
        }

        /**
         * Clicking anywhere on a removable row flips its box — but only when
         * the click left no selection behind, because reading a value often
         * means selecting it. A single click waits out `TOGGLE_DELAY_MS` so a
         * double-click word-select cancels it, and a click on the box itself is
         * left entirely to the native control.
         */
        makeRowClickable(entry) {
            const { row, box } = entry;
            row.classList.add("togglable");
            let pending = null;
            row.addEventListener("click", (event) => {
                if (event.target === box) return;
                if (box.disabled) return;
                if (event.detail > 1) return;
                clearTimeout(pending);
                pending = setTimeout(() => {
                    const selection = window.getSelection();
                    if (selection && !selection.isCollapsed) return;
                    if (box.disabled) return;
                    box.checked = !box.checked;
                    this.refresh();
                }, TOGGLE_DELAY_MS);
            });
            row.addEventListener("dblclick", () => clearTimeout(pending));
        }

        // ------------------------------------------------------------------
        // Rebuilding
        // ------------------------------------------------------------------

        /** Re-read the boxes and bring the whole card back into agreement. */
        refresh() {
            this.applyFloor();
            const kept = new Set();
            this.rows.forEach(({ part, row, box }) => {
                if (box.checked) kept.add(part.i);
                row.classList.toggle("off", !box.checked);
            });
            const built = build(this.model, kept);
            if (this.action) this.action.setAttribute("href", built.raw);
            this.dimStackRows(kept);
            this.relabelAction(kept);
            this.showEdited(built);
            return built.raw;
        }

        /**
         * RFC 5724's grammar needs at least one sms recipient, so the last
         * number standing locks — checked and disabled, the same vocabulary as
         * a fixed part. Re-tick another and it unlocks.
         *
         * `mailto:` has no floor at all: RFC 6068 is happy with a to-less,
         * cc-only, even address-less link, and a bare compose window is a
         * perfectly good thing for a button to offer.
         */
        applyFloor() {
            if (!this.model.floor) return;
            const recipients = this.rows.filter((r) => r.part.role === "recipient");
            const kept = recipients.filter((r) => r.box.checked);
            recipients.forEach((r) => {
                r.box.disabled = false;
            });
            if (kept.length === 1) kept[0].box.disabled = true;
            recipients.forEach((r) => {
                r.row.classList.toggle("locked", r.box.disabled);
            });
        }

        /** A number the reader dropped dims in the stack, and its facts with it. */
        dimStackRows(kept) {
            document.querySelectorAll(".pv-stack2 [data-slice]").forEach((element) => {
                const index = Number(element.getAttribute("data-slice"));
                element.classList.toggle("off", !kept.has(index));
            });
        }

        /**
         * The button says what it would do now, not what it would have done
         * before the edits. Everything it says is still read off the URI's own
         * structure — the count, the surviving address, the type word — so it
         * stays a description of the string rather than a prediction.
         */
        relabelAction(kept) {
            if (!this.action) return;
            const lead = this.action.querySelector(".lead");
            const what = this.action.querySelector(".what");
            if (!lead || !what) return;
            const live = (role, key) =>
                this.model.parts.filter(
                    (p) =>
                        (p.fixed || kept.has(p.i)) &&
                        (role ? p.role === role : true) &&
                        (key ? key.includes(p.k) : true),
                );

            if (this.model.scheme === "mailto") {
                // `cc` and `bcc` are addresses too, so they count -- but the
                // recipients are the ones that carry a name for the button.
                const to = live("recipient");
                const copies = live("query", ["cc", "bcc"]);
                const total = to.length + copies.length;
                if (total === 0) {
                    setText(lead, "Draft a Message");
                    setText(what, "An email with no recipient");
                } else if (total === 1) {
                    const only = to[0] || copies[0];
                    setText(lead, "Write to " + (only.label || only.v));
                    setText(what, "An email address");
                } else {
                    setText(lead, "Write to " + total + " addresses");
                    setText(what, "Email addresses");
                }
                return;
            }
            if (this.model.scheme === "sms") {
                const to = live("recipient");
                if (to.length === 1) {
                    setText(lead, "Message " + (to[0].label || to[0].v));
                    setText(what, "A phone number, for a message");
                } else {
                    setText(lead, "Message " + to.length + " numbers");
                    setText(what, "Phone numbers, for one message");
                }
                return;
            }
            if (this.model.scheme === "xmpp") {
                // Drop `?join` and the link genuinely stops being a room.
                const room = live("query", ["join"]).length > 0;
                setText(what, room ? "A chat room on XMPP" : "An XMPP chat address");
            }
        }

        // ------------------------------------------------------------------
        // The lines
        // ------------------------------------------------------------------

        /**
         * "After your edits" — the tool, next to the record. It exists only
         * while the rebuilt string differs from the stored one; an untouched
         * card never grows a second line saying the same thing twice.
         */
        buildEditedLine() {
            const line = document.createElement("code");
            line.className = "rawline edited";
            const label = document.createElement("span");
            label.className = "lbl";
            label.textContent = "After Your Edits";
            const copy = document.createElement("button");
            copy.type = "button";
            copy.className = "copybtn";
            copy.textContent = "Copy";
            copy.addEventListener("click", () => copyText(this.refresh(), copy));
            label.appendChild(copy);
            const body = document.createElement("span");
            body.className = "str";
            line.append(label, body);

            const anchor = document.querySelector(".pv-split") || this.action;
            if (!anchor || !anchor.parentNode) return;
            anchor.parentNode.insertBefore(line, anchor);
            this.edited = { line, body };
        }

        showEdited(built) {
            if (!this.edited) return;
            const changed = built.raw !== this.model.stored;
            this.edited.line.classList.toggle("show", changed);
            if (!changed) return;
            this.edited.body.replaceChildren(...built.runs.map(runNode));
        }

        // ------------------------------------------------------------------
        // Copying
        // ------------------------------------------------------------------

        /**
         * The split: the action, and a blue segment that takes with you exactly
         * what the action would open — edits included. Blue because copying
         * leaves you on YuioLink.
         */
        buildSplit() {
            const action = this.action;
            if (!action || !action.parentNode) return;
            const split = document.createElement("div");
            split.className = "pv-split pv-open";
            action.parentNode.insertBefore(split, action);
            split.appendChild(action);
            action.classList.remove("btn-block");

            // Two labels in one grid cell: the segment is as wide as the wider
            // of them, so confirming a copy cannot shove the button it is
            // attached to. A button-sized control just checks; the word belongs
            // on the small raw-line pills, where there is room for it.
            const copy = document.createElement("button");
            copy.type = "button";
            copy.className = "btn splitcopy";
            copy.append(labelSpan("word", "Copy"), labelSpan("tick", "\u2713"));
            copy.setAttribute("aria-label", "Copy the destination address");
            copy.addEventListener("click", () => {
                copyText(action.getAttribute("href") || "", copy);
                // Answer "copied what?" by pointing rather than explaining: the
                // line the string came from lights the site's own green check.
                flashSource();
            });
            split.appendChild(copy);
        }

        /** A Copy pill on each raw line. The stored line always copies the record. */
        buildRawCopyButtons() {
            document.querySelectorAll(".rawline:not(.edited)").forEach((line) => {
                const label = line.querySelector(".lbl");
                const body = line.querySelector(".str");
                if (!label || !body) return;
                const copy = document.createElement("button");
                copy.type = "button";
                copy.className = "copybtn";
                copy.textContent = "Copy";
                copy.addEventListener("click", () => copyText(body.textContent, copy));
                label.appendChild(copy);
            });
        }
    }

    // ----------------------------------------------------------------------
    // The parts model
    // ----------------------------------------------------------------------

    function readModel(container) {
        if (!container) return null;
        try {
            const model = JSON.parse(container.getAttribute("data-card"));
            model.byIndex = new Map(model.parts.map((p) => [p.i, p]));
            return model;
        } catch (e) {
            return null;
        }
    }

    /**
     * Reassemble the stored string from the parts that survive, promoting the
     * delimiters as it goes: drop the first query pair and the next one's `&`
     * has to become the `?`; drop the first recipient and its comma goes with
     * it. Path parameters need no promotion — each brings its own `;`.
     *
     * Returns the raw string and the same thing as `(class, text)` runs, so the
     * edited line wears exactly the dress the stored line does.
     */
    function build(model, kept) {
        let raw = model.prefix;
        const runs = model.prefixRuns.slice();
        let recipient = false;
        let query = false;
        let fragment = false;
        model.parts.forEach((part) => {
            if (!part.fixed && !kept.has(part.i)) return;
            let delim;
            if (part.role === "recipient") {
                delim = recipient ? "," : "";
                recipient = true;
            } else if (part.role === "query") {
                delim = query ? "&" : "?";
                query = true;
            } else if (part.role === "fragment") {
                delim = fragment ? "&" : "#";
                fragment = true;
            } else {
                delim = part.d;
            }
            raw += delim + (part.k || "") + (part.e ? "=" : "") + part.v;
            if (delim) runs.push(["dl", delim]);
            part.p.forEach((run) => runs.push(run));
        });
        return { raw, runs };
    }

    /** One `(class, text)` run as a node. An empty class means bare text. */
    function runNode(run) {
        const [className, text] = run;
        if (!className) return document.createTextNode(text);
        const span = document.createElement("span");
        span.className = className;
        span.textContent = text;
        return span;
    }

    function keepLabel(part) {
        if (part.fixed) return (part.role || "part") + ", fixed";
        return "Keep " + (part.label || part.k || part.v || part.role);
    }

    // ----------------------------------------------------------------------
    // Small shared pieces
    // ----------------------------------------------------------------------

    function setText(element, text) {
        if (element.textContent !== text) element.textContent = text;
    }

    /**
     * Light the line the copied string came from: "After your edits" while
     * edits are live, otherwise "Exactly as stored", otherwise the headline
     * that already is the stored string. No tooltip — touch and screen readers
     * lose those — and no extra caption.
     */
    function flashSource() {
        const pill =
            document.querySelector(".rawline.edited.show .copybtn") ||
            document.querySelector(".rawline:not(.edited) .copybtn");
        // The stored line can sit inside a closed fold now; a confirmation on
        // a pill nobody can see is no confirmation, so fall through to the
        // headline instead of flashing into the void.
        if (pill && pill.offsetParent !== null) {
            confirmOn(pill);
            return;
        }
        const line = document.querySelector(".pv-url, .pv-line");
        if (!line) return;
        line.classList.add("copied");
        setTimeout(() => line.classList.remove("copied"), FEEDBACK_MS);
    }

    function labelSpan(className, text) {
        const span = document.createElement("span");
        span.className = className;
        span.textContent = text;
        return span;
    }

    /**
     * Confirm a copy on the control that was pressed.
     *
     * The split's segment already holds both of its labels and only swaps which
     * one is visible, so nothing about the page moves. A raw-line pill is small
     * and free-standing, so it says the word.
     */
    function confirmOn(button) {
        button.classList.add("done");
        const pill = !button.classList.contains("splitcopy");
        if (pill) button.textContent = "Copied ✓";
        setTimeout(() => {
            button.classList.remove("done");
            if (pill) button.textContent = "Copy";
        }, FEEDBACK_MS);
    }

    function copyText(text, button) {
        if (!navigator.clipboard || !navigator.clipboard.writeText) return;
        navigator.clipboard.writeText(text).then(
            () => confirmOn(button),
            () => {
                // Clipboard unavailable (insecure context) or permission
                // denied. Say nothing rather than claim a copy that never
                // happened -- the text is on the page and selectable.
            },
        );
    }

    /**
     * ⌘C / Ctrl-C copies the destination when nothing is selected. A selection
     * the visitor made always wins: copying it is what they asked for, and this
     * page has text worth selecting.
     */
    function copyShortcut() {
        const destination = document.getElementById("destination");
        if (!destination) return;
        document.addEventListener("keydown", (event) => {
            if (!(event.metaKey || event.ctrlKey)) return;
            if (event.key !== "c" && event.key !== "C") return;
            const selection = window.getSelection();
            if (selection && !selection.isCollapsed) return;
            const text = destination.textContent.trim();
            if (!text) return;
            if (!navigator.clipboard || !navigator.clipboard.writeText) return;
            event.preventDefault();
            navigator.clipboard.writeText(text).then(() => {
                destination.classList.add("copied");
                setTimeout(() => destination.classList.remove("copied"), 1500);
            }, () => {});
        });
    }
})();
