//! `/legal` — the terms page, alone in its own file on purpose: the page links
//! to this file's commit history as the public record of every past version of
//! the terms. Keep the path stable (`server/src/legal.rs`) — renaming or moving
//! the file breaks that link and orphans the history it promises.

use crate::views::{document_full, external_mark, home_chip};
use maud::{html, Markup};

/// The legal page: who provides the service, what it keeps while a link lives,
/// and the terms it is offered under — plain words first, the legal terms where
/// they matter. Every claim in it restates something the code enforces (the
/// seven-day ceiling, the reaper, the in-memory rate buckets, the aggregate-only
/// stats); when one of those changes, this page must change with it. The contact
/// addresses are placeholders (`span.ph`) until publishable ones exist.
pub fn legal_page() -> Markup {
    let body = html! {
        (home_chip("/", "Back to YuioLink"))
        h2.help-title { "Legal" }
        p.help-lead {
            "Who provides this service, what it keeps while a link lives, and "
            "the terms you use it under — in plain words, with the legal terms "
            "where they matter."
        }

        h3.help-h #operator { "Who provides this" }
        p.help-p {
            "YuioLink is a personal, non-commercial project operated by jooize "
            "(\u{201c}the operator\u{201d}). It is offered free of charge, "
            "without accounts and without advertising. A publishable contact "
            "address will appear here: "
            span.ph { "operator contact — to be published" }
            "."
        }

        h3.help-h #service { "The service" }
        p.help-p {
            "YuioLink turns a web address or a piece of text into a short link. "
            "Every link expires: the longest lifetime is seven days, and when a "
            "link's time is up it stops working for everyone. There are no "
            "permanent links, by design. Creating a link means accepting these "
            "terms; if you do not accept them, do not create links here."
        }

        h3.help-h #stored { "What is stored" }
        p.help-p {
            "A link is one record: its name, what it points to — the destination "
            "address, or the text — when it was created, when it expires, an "
            "optional use limit and a count of uses, a deletion secret, and "
            "whether its creator has deleted it. That is the whole record. There "
            "are no accounts, and nothing in the record identifies who created "
            "or opened a link."
        }
        p.help-p {
            "The history panel on the front page lives in your own browser's "
            "storage and is never sent to the server. The public "
            a href="/stats" { "statistics" }
            " are daily tallies — counts of events per day, with nothing in them "
            "that can be traced to a person or a link."
        }

        h3.help-h #retention { "How long it is kept" }
        p.help-p {
            "A link's record is deleted shortly after the link expires — at most "
            "seven days after it was created. Deleting a link yourself stops it "
            "resolving immediately; the record then remains only so the name "
            "cannot be claimed by someone else, is no longer readable by "
            "visitors, and is removed on the same expiry schedule."
        }

        h3.help-h #network { "Network data" }
        p.help-p {
            "To keep link creation from being abused, the server briefly holds "
            "the network address a creation request came from — in working "
            "memory only, never written to disk. Like essentially every service "
            "on the web, the server also keeps short-lived technical logs of "
            "requests for operations and abuse defence; they are routinely "
            "discarded and are not used to build profiles of anyone."
        }

        h3.help-h #use { "Acceptable use" }
        p.help-p {
            "Do not use YuioLink to point at, or to carry, anything unlawful — "
            "including malware, phishing, content that infringes copyright or "
            "trademark, private information published without consent, or "
            "material that exploits or harms children — and do not use it for "
            "spam or harassment. The operator may withdraw any link and refuse "
            "service at any time, without notice, at the operator's sole "
            "discretion."
        }

        h3.help-h #abuse { "Reporting abuse" }
        p.help-p {
            "To report a link, send its name — the word part of the address — "
            "along with what you found and a way to reach you, to "
            span.ph { "abuse contact — to be published" }
            ". Reports are read by a person. A link that violates these terms "
            "is withdrawn; and because every link expires within seven days, "
            "even an unreported one is short-lived."
        }

        h3.help-h #warranty { "No warranty" }
        p.help-p {
            "The service is provided \u{201c}as is\u{201d} and \u{201c}as "
            "available\u{201d}, without warranty of any kind, express or "
            "implied — including the implied warranties of merchantability, "
            "fitness for a particular purpose, and non-infringement. Links "
            "expire, and the service itself may change, pause, or end at any "
            "time. Do not let a YuioLink be the only copy of anything you care "
            "about."
        }

        h3.help-h #liability { "Liability" }
        p.help-p {
            "To the fullest extent permitted by law, the operator is not liable "
            "for any indirect, incidental, special, or consequential damages "
            "arising from using — or being unable to use — this service, nor "
            "for the content of third-party destinations that links point to. "
            "Nothing in these terms limits liability that cannot lawfully be "
            "limited."
        }

        h3.help-h #changes { "Changes" }
        p.help-p {
            "These terms may change as the service does. A change takes effect "
            "when it is published here, and the Updated date in the footer of "
            "the front page moves with it. Because every link expires within "
            "seven days, no link ever outlives the terms it was created under "
            "by more than a week. And every past version of this page stays "
            "public: it is kept in one file of the project's source, so its "
            a.ext href="https://github.com/jooize/YuioLink/commits/main/server/src/legal.rs" {
                "complete history" (external_mark())
            }
            " is readable by anyone, back to the day the page first appeared."
        }

        footer { a href="/" { "Back to YuioLink" } }
    };
    document_full(
        "YuioLink — Legal",
        html! {
            meta name="description" content="Who provides YuioLink, what it stores while a link lives, and the terms it is offered under: every link expires within seven days, no accounts, no tracking.";
        },
        body,
        html! {},
    )
}
