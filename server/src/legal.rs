//! `/legal` — the terms page and its own archive. Every version of the terms
//! ships inside the binary: the last entry of [`VERSIONS`] is the current terms
//! at `/legal`, and every earlier entry stays readable at `/legal/<date>` — the
//! promise of past versions depends on nothing but the running server (no
//! repository host, no external archive). To change the terms, append a new
//! dated entry to [`VERSIONS`] and edit only it: the freeze test pins the
//! rendered bytes of every non-final entry, so published history cannot be
//! rewritten, only added to.

use crate::views::{document_full, home_chip};
use maud::{html, Markup};

/// Every version of the terms, oldest first. The date is the day a version took
/// effect, and its permanent address (`/legal/<date>`). The LAST entry is the
/// current terms; every earlier entry is frozen — see the module doc.
pub const VERSIONS: &[(&str, fn() -> Markup)] = &[("2026-08-25", terms_2026_08_25)];

/// The terms as they took effect 2026-08-25 — the first version. Frozen the
/// moment a later entry exists: after that, edits belong in the new entry.
///
/// Every claim in here restates something the code enforces (the seven-day
/// ceiling, the reaper, the in-memory rate buckets, the aggregate-only stats);
/// when one of those changes, a new version of this page must change with it.
/// The contact addresses are placeholders (`span.ph`) until publishable ones
/// exist.
fn terms_2026_08_25() -> Markup {
    html! {
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
            "when it is published, as a new dated version of this page — and "
            "every version, this one and each one before it, keeps its own "
            "permanent address on this site, listed at the end of the page. "
            "Because every link expires within seven days, no link ever "
            "outlives the terms it was created under by more than a week."
        }
    }
}

/// `GET /legal` — the current terms, with the version list under them.
pub fn legal_page() -> Markup {
    let (date, terms) = *VERSIONS.last().expect("at least one terms version");
    page(date, terms(), true)
}

/// `GET /legal/<date>` — one version by its date, current or archived; `None`
/// for a date that never was.
pub fn legal_version_page(date: &str) -> Option<Markup> {
    let idx = VERSIONS.iter().position(|(d, _)| *d == date)?;
    let (d, terms) = VERSIONS[idx];
    Some(page(d, terms(), idx == VERSIONS.len() - 1))
}

/// The chrome every version shares: title, lead, the archived-version notice
/// where one is due, the version list, the footer. The notice and the list live
/// out here rather than in the versioned bodies, so freezing a version never
/// freezes its own successor list.
fn page(date: &str, terms: Markup, current: bool) -> Markup {
    let last = VERSIONS.len() - 1;
    let body = html! {
        (home_chip("/", "Back to YuioLink"))
        h2.help-title { "Legal" }
        p.help-lead {
            "Who provides this service, what it keeps while a link lives, and "
            "the terms you use it under — in plain words, with the legal terms "
            "where they matter."
        }

        @if !current {
            p.legal-past {
                "This is a past version of the terms, in effect from "
                (date)
                " until it was replaced. The "
                a href="/legal" { "current terms" }
                " apply today."
            }
        }

        (terms)

        h3.help-h #versions { "Versions" }
        ul.legal-versions {
            @for (i, (d, _)) in VERSIONS.iter().enumerate().rev() {
                li {
                    a href=(format!("/legal/{d}")) { (d) }
                    @if i == last { " — current, in effect since this date" }
                    @else { " — replaced " (VERSIONS[i + 1].0) }
                }
            }
        }

        footer { a href="/" { "Back to YuioLink" } }
    };
    document_full(
        "YuioLink — Legal",
        html! {
            meta name="description" content="Who provides YuioLink, what it stores while a link lives, and the terms it is offered under: every link expires within seven days, no accounts, no tracking.";
            // Archived versions stay readable but point search engines at the
            // current page instead of themselves.
            @if !current {
                meta name="robots" content="noindex";
            }
        },
        body,
        html! {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rendered bytes of every ARCHIVED version, pinned by FNV-1a hash.
    /// Appending a new version to [`VERSIONS`] freezes the previously-last
    /// entry: add its (date, hash) here — a failing run prints the hash.
    /// Editing a frozen version fails this test; that is the point.
    const FROZEN: &[(&str, u64)] = &[];

    fn fnv1a(s: &str) -> u64 {
        s.bytes()
            .fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
                (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }

    #[test]
    fn archived_terms_never_change() {
        for (i, (date, terms)) in VERSIONS.iter().enumerate() {
            if i == VERSIONS.len() - 1 {
                continue; // the current version is the one allowed to change
            }
            let hash = fnv1a(&terms().into_string());
            let pinned = FROZEN.iter().find(|(d, _)| d == date).map(|(_, h)| *h);
            assert_eq!(
                pinned,
                Some(hash),
                "archived terms {date} changed or is unpinned; its hash is {hash:#018x}"
            );
        }
    }

    #[test]
    fn versions_are_dated_in_order() {
        // Oldest first, strictly — the chrome derives "current" and each
        // version's replacement date from this order.
        assert!(VERSIONS.windows(2).all(|w| w[0].0 < w[1].0));
        // ISO dates only: they sort as strings and are safe path segments.
        for (d, _) in VERSIONS {
            assert!(
                d.len() == 10
                    && d.bytes()
                        .enumerate()
                        .all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() }),
                "{d} is not YYYY-MM-DD"
            );
        }
    }
}
