//! `/legal` — the terms page, its own archive, and the hash chain that makes
//! the archive verifiable. Every version of the terms ships inside the binary:
//! the last entry of [`VERSIONS`] is the current terms at `/legal`, and every
//! earlier entry stays readable at `/legal/<id>` — the promise of past versions
//! depends on nothing but the running server (no repository host, no external
//! archive).
//!
//! Each version also has one canonical plain-text form, served at
//! `/legal/<id>.txt`. Its SHA-256 hash is the version's fingerprint, and the
//! text itself names the fingerprint of the version before it (the first
//! chains from a fixed genesis line) — so the versions form a hash chain, and
//! rewriting any published version would break every fingerprint after it.
//! The chain alone proves nothing to a fresh visitor, which is why every
//! link-creation response carries the current head ([`receipt`]): each creator
//! stores independent evidence of what the terms said when they accepted them.
//!
//! To change the terms — any edit at all — append a new entry to [`VERSIONS`]
//! and pin its hash in the freeze test, which pins every entry: published
//! history is only ever added to, never rewritten.

use crate::views::{document_full, home_chip};
use maud::{Markup, html};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Every version of the terms, oldest first. The id is the moment a version
/// took effect — `YYYY-MM-DDTHHMMSSZ`, no colons, so it sorts as a string and
/// is a safe path segment — and its permanent address (`/legal/<id>`). The
/// LAST entry is the current terms; every entry is frozen by the pinned-hash
/// test — see the module doc.
pub const VERSIONS: &[TermsEntry] = &[("2026-09-24T184300Z", terms_2026_09_24)];

/// One entry of [`VERSIONS`]: the id and the function that renders that text.
pub type TermsEntry = (&'static str, fn() -> Markup);

/// The line the chain starts from: the first version's `Previous:` fingerprint
/// is the SHA-256 of this string, so even the head of the chain is anchored to
/// something published rather than to an empty field.
const GENESIS: &str = "YuioLink terms hash chain genesis\n";

/// The terms as they took effect 2026-09-24 — the first version. Frozen by the
/// pinned-hash test: once published, ANY edit belongs in a new entry, because
/// creation receipts in the wild hold this version's fingerprint.
///
/// Every factual claim in here restates something the code enforces (the
/// seven-day ceiling, the reaper, the in-memory rate buckets, the
/// aggregate-only stats, the opt-in local history); when one of those changes,
/// a new version of this page must change with it. The legal citations are the
/// operator's situation as of writing — Swedish operator, EU hosting: GDPR +
/// Sweden's Data Protection Act (2018:218), the browser-storage consent rule
/// in the Electronic Communications Act (2022:482) 9 kap. 28 §, the DSA's
/// notice-and-action shape without conceding it applies. The contact addresses
/// are placeholders (`span.ph`) until publishable ones exist.
fn terms_2026_09_24() -> Markup {
    html! {
        h3.help-h #operator { "Who provides this" }
        p.help-p {
            "YuioLink is a personal, non-commercial project operated by jooize "
            "(\u{201c}the operator\u{201d}). It is offered free of charge, "
            "without accounts and without advertising, and it is operated from "
            "Sweden. For the little personal data the service touches, the "
            "operator is the data controller (GDPR, Article 4(7)). A "
            "publishable contact address will appear here: "
            span.ph { "operator contact — to be published" }
            "."
        }

        h3.help-h #service { "The service" }
        p.help-p {
            "YuioLink turns a web address or a piece of text into a short link. "
            strong { "Every link expires" }
            ": the longest lifetime is "
            strong { "seven days" }
            ", and when a link's time is up it stops working for everyone. "
            "There are no permanent links, by design. Creating a link means "
            "accepting these terms; if you do not accept them, do not create "
            "links here."
        }

        h3.help-h #stored { "What is stored" }
        p.help-p {
            "A link is one record: its name, what it points to (the destination "
            "address, or the text), when it was created, when it expires, an "
            "optional use limit and a count of uses, a deletion secret, and "
            "whether its creator has deleted it. That is the whole record. "
            "There are "
            strong { "no accounts" }
            ", and nothing in the record identifies who created or opened a "
            "link."
        }
        p.help-p {
            "The history panel on the front page lives in your own browser's "
            "storage and is "
            strong { "never sent to the server" }
            ". It is kept beyond the visit only after you switch it on; that "
            "switch is the consent EU rules ask for before a site stores "
            "things in your browser (in Sweden, the Electronic Communications "
            "Act (2022:482), Chapter 9, Section 28). There are no cookies and "
            "no other stored state. The public "
            a href="/stats" { "statistics" }
            " are daily tallies: counts of events per day, with nothing in "
            "them that can be traced to a person or a link."
        }

        h3.help-h #retention { "How long it is kept" }
        p.help-p {
            "A link's record is deleted shortly after the link expires, at most "
            "seven days after it was created. Deleting a link yourself stops it "
            "resolving immediately; the record then remains only so the name "
            "cannot be claimed by someone else, is no longer readable by "
            "visitors, and is removed on the same expiry schedule."
        }

        h3.help-h #network { "Network data" }
        p.help-p {
            "To keep link creation from being abused, the server briefly holds "
            "the network address a creation request came from, "
            strong { "in working memory only" }
            ", never written to disk. Like essentially every service "
            "on the web, the server also keeps short-lived technical logs of "
            "requests for operations and abuse defence; they are routinely "
            "discarded and are not used to build profiles of anyone. In legal "
            "terms, both rest on the operator's legitimate interest in keeping "
            "the service working and unabused (GDPR, Article 6(1)(f))."
        }

        h3.help-h #rights { "Your rights" }
        p.help-p {
            "European data protection law applies to this service: the GDPR "
            "(Regulation (EU) 2016/679), supplemented in Sweden by the Data "
            "Protection Act (2018:218). This page is the information those "
            "rules call for, in plain words. Most of the GDPR's rights are "
            "honored here by not collecting things: a link's record identifies "
            "no one, and where a service cannot tell which records are whose, "
            "the rights to access, correct, or export them do not attach "
            "(Article 11). There is nothing to look a person up by. What a "
            "creator can always do is delete their own link, with the deletion "
            "secret issued at creation."
        }
        p.help-p {
            "If you believe this service mishandles personal data, you can "
            "complain to a data protection authority (GDPR, Article 77). The "
            "operator's is Sweden's Integritetsskyddsmyndigheten (IMY), and "
            "the authority of the country where you live works too."
        }

        h3.help-h #use { "Acceptable use" }
        p.help-p {
            "Do not use YuioLink to point at, or to carry, anything unlawful, "
            "and do not use it for spam or harassment. That covers, among "
            "other things, malware, phishing, content that infringes copyright "
            "or trademark, private information published without consent, and "
            "material that exploits or harms children. The operator may "
            "withdraw any link and refuse service at any time, without notice, "
            "at the operator's sole discretion."
        }

        h3.help-h #abuse { "Reporting abuse" }
        p.help-p {
            "To report a link, send its name (the word part of the address) "
            "along with what you found and a way to reach you, to "
            span.ph { "abuse contact — to be published" }
            ". Reports are read by a person. A link that violates these terms "
            "is withdrawn, and because every link expires within seven days, "
            "even an unreported one is short-lived. Reports are handled "
            "notice-and-action style, the shape the EU Digital Services Act "
            "(Regulation (EU) 2022/2065) gives such procedures, whether or "
            "not that act strictly binds a personal, non-commercial service."
        }

        h3.help-h #warranty { "No warranty" }
        p.help-p {
            "The service is provided \u{201c}as is\u{201d} and \u{201c}as "
            "available\u{201d}, without warranty of any kind, express or "
            "implied, including the implied warranties of merchantability, "
            "fitness for a particular purpose, and non-infringement. Links "
            "expire, and the service itself may change, pause, or end at any "
            "time. Do not let a YuioLink be "
            strong { "the only copy" }
            " of anything you care about."
        }

        h3.help-h #liability { "Liability" }
        p.help-p {
            "To the fullest extent permitted by law, the operator is not liable "
            "for any indirect, incidental, special, or consequential damages "
            "arising from using, or being unable to use, this service, nor "
            "for the content of third-party destinations that links point to. "
            "Nothing in these terms limits liability that cannot lawfully be "
            "limited."
        }

        h3.help-h #law { "Governing law" }
        p.help-p {
            "These terms are governed by Swedish law. Where you use this "
            "service as a consumer, mandatory protections of the country "
            "where you live stand regardless of this clause."
        }

        h3.help-h #changes { "Changes" }
        p.help-p {
            "These terms may change as the service does. A change takes effect "
            "when it is published, as a new dated version of this page. Every "
            "version, this one and each one before it, keeps its own "
            "permanent address on this site, listed at the end of the page. "
            "Because every link expires within seven days, no link ever "
            "outlives the terms it was created under by more than a week."
        }
    }
}

// --------------------------------------------------------------------------
// The hash chain
// --------------------------------------------------------------------------

/// One version with its canonical form and place in the chain.
pub struct Version {
    pub id: &'static str,
    /// The canonical plain text, served at `/legal/<id>.txt`. Derived from the
    /// rendered markup by [`text_of`], with a fixed header naming the version
    /// and the previous fingerprint — so `sha256sum` of this exact file is the
    /// whole verification.
    pub txt: String,
    /// Lowercase-hex SHA-256 of [`Self::txt`] — this version's fingerprint.
    pub hash: String,
    /// The fingerprint this version chains from: the previous version's
    /// [`Self::hash`], or the genesis hash for the first version.
    pub prev: String,
}

/// Every version with its canonical text and fingerprint, oldest first.
/// Computed once; everything after startup reads the same bytes the freeze
/// test pinned.
pub fn chain() -> &'static [Version] {
    static CHAIN: OnceLock<Vec<Version>> = OnceLock::new();
    CHAIN.get_or_init(|| {
        let mut prev = hex(&Sha256::digest(GENESIS));
        VERSIONS
            .iter()
            .map(|(id, terms)| {
                let txt = format!(
                    "# YuioLink Terms\n\nVersion: {id}\nPrevious: {prev}\n\n{}\n",
                    text_of(&terms().into_string())
                );
                let hash = hex(&Sha256::digest(&txt));
                Version {
                    id,
                    txt,
                    hash: hash.clone(),
                    prev: std::mem::replace(&mut prev, hash),
                }
            })
            .collect()
    })
}

/// The chain head — the current terms.
pub fn head() -> &'static Version {
    chain().last().expect("at least one terms version")
}

/// The canonical text of one version (`GET /legal/<id>.txt`); `None` for an id
/// that never was.
pub fn canonical_txt(id: &str) -> Option<&'static str> {
    chain().iter().find(|v| v.id == id).map(|v| v.txt.as_str())
}

/// The terms head, as every creation response carries it. Stored client-side
/// (API responses, the browser's local history), this is the creator's
/// receipt: evidence of which terms — byte for byte — were in effect when
/// their link was made, independent of what the server later claims.
#[derive(Serialize, Clone, Copy)]
pub struct TermsReceipt {
    pub version: &'static str,
    pub sha256: &'static str,
}

pub fn receipt() -> TermsReceipt {
    let head = head();
    TermsReceipt {
        version: head.id,
        sha256: &head.hash,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The text content of one version's rendered markup, in the shape the
/// canonical form freezes: one line per block, a blank line between blocks, a
/// heading (`h3`) prefixed `## ` in the Markdown way, inline tags stripped,
/// maud's escapes decoded back to characters. The result is plain text that
/// also happens to be valid Markdown — same bytes, two readable forms.
///
/// This function is part of the canonical definition — its output is what gets
/// hashed — so it must stay byte-stable for the markup the versions actually
/// use; the pinned hashes and the canonical-text test hold it to that.
fn text_of(html_str: &str) -> String {
    let mut blocks: Vec<String> = Vec::new();
    let mut block: Option<String> = None;
    let mut rest = html_str;
    while let Some(lt) = rest.find('<') {
        if let Some(text) = block.as_mut() {
            text.push_str(&rest[..lt]);
        }
        let Some(gt) = rest[lt..].find('>') else {
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        match tag.split(' ').next().unwrap_or(tag) {
            "h3" => block = Some(String::from("## ")),
            "p" => block = Some(String::new()),
            "/h3" | "/p" => {
                if let Some(text) = block.take() {
                    blocks.push(text);
                }
            }
            _ => {} // inline tags (a, span) strip; their text stays
        }
        rest = &rest[lt + gt + 1..];
    }
    let mut out = blocks.join("\n\n");
    // maud's escape set, in an order where `&amp;` cannot double-decode.
    for (entity, ch) in [
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&amp;", "&"),
    ] {
        if out.contains(entity) {
            out = out.replace(entity, ch);
        }
    }
    out
}

/// "2026-08-25 13:22 UTC" from "2026-08-25T132200Z" — the human face of a
/// version id, everywhere one is displayed rather than addressed.
fn human(id: &str) -> String {
    format!("{} {}:{} UTC", &id[..10], &id[11..13], &id[13..15])
}

/// The chain, drawn once: the genesis line hashes into this version's text,
/// that text hashes to the fingerprint, and the fingerprint flows two ways —
/// into every creation receipt and into the next version's `Previous:` line.
/// Chrome, not part of any canonical text, so it can carry the live values.
fn chain_diagram(version: &Version) -> Markup {
    let fp = format!(
        "{}\u{2026}{}",
        &version.hash[..8],
        &version.hash[version.hash.len() - 4..]
    );
    let prev = format!("{}\u{2026}", &version.prev[..8]);
    html! {
        svg.legal-chain viewBox="0 0 340 292" role="img"
            aria-label="Diagram of the hash chain: a fixed genesis line is hashed into this version's canonical text; hashing that text gives the version's fingerprint, which is stored in every creation receipt and named inside the next version's text." {
            defs {
                marker #chain-arrow markerWidth="8" markerHeight="8" refX="6" refY="4" orient="auto" {
                    path.wire d="M0,0 L6,4 L0,8" {}
                }
            }
            rect.box x="115" y="2" width="110" height="24" rx="12" {}
            text.sub x="170" y="18" text-anchor="middle" { "genesis line" }
            line.wire x1="170" y1="26" x2="170" y2="52" marker-end="url(#chain-arrow)" {}
            text.sub x="178" y="44" { "SHA-256" }

            rect.box x="50" y="56" width="240" height="76" rx="10" {}
            text.lbl x="170" y="76" text-anchor="middle" { "Version " (human(version.id)) }
            text.mono x="170" y="96" text-anchor="middle" { "Previous: " (prev) }
            text.sub x="170" y="118" text-anchor="middle" { "\u{2026}the whole terms text\u{2026}" }
            line.wire x1="170" y1="132" x2="170" y2="158" marker-end="url(#chain-arrow)" {}
            text.sub x="178" y="150" { "SHA-256" }

            rect.fp x="95" y="162" width="150" height="26" rx="13" {}
            text.fp-text x="170" y="179" text-anchor="middle" { (fp) }
            path.wire d="M170,188 C170,206 95,206 95,222" marker-end="url(#chain-arrow)" {}
            path.wire.ghost d="M170,188 C170,206 245,206 245,222" marker-end="url(#chain-arrow)" {}

            rect.box x="15" y="226" width="160" height="52" rx="10" {}
            text.lbl x="95" y="248" text-anchor="middle" { "every creation receipt" }
            text.sub x="95" y="266" text-anchor="middle" { "kept in creators' browsers" }

            rect.box.ghost x="185" y="226" width="140" height="52" rx="10" {}
            text.lbl x="255" y="248" text-anchor="middle" { "the next version" }
            text.mono x="255" y="266" text-anchor="middle" { "Previous: " (&version.hash[..8]) "\u{2026}" }
        }
    }
}

// --------------------------------------------------------------------------
// Pages
// --------------------------------------------------------------------------

/// `GET /legal` — the current terms, with the version list under them.
pub fn legal_page(base_url: &str) -> Markup {
    page(base_url, VERSIONS.len() - 1)
}

/// `GET /legal/<id>` — one version by its id, current or archived; `None` for
/// an id that never was.
pub fn legal_version_page(base_url: &str, id: &str) -> Option<Markup> {
    let idx = VERSIONS.iter().position(|(d, _)| *d == id)?;
    Some(page(base_url, idx))
}

/// The chrome every version shares: title, lead, the archived-version notice
/// where one is due, the version list, the verification section, the footer.
/// All of it lives out here rather than in the versioned bodies, so freezing a
/// version never freezes its successor list — and so a version's own
/// fingerprint can be displayed without being part of what it hashes.
fn page(base_url: &str, idx: usize) -> Markup {
    let last = VERSIONS.len() - 1;
    let current = idx == last;
    let version = &chain()[idx];
    let terms = VERSIONS[idx].1;
    let body = html! {
        (home_chip("/", "Back to YuioLink"))
        h2.help-title { "Legal" }
        p.help-lead {
            "Who provides this service, what it keeps while a link lives, and "
            "the terms you use it under, in plain words, with the legal terms "
            "where they matter."
        }

        @if !current {
            p.legal-past {
                "This is a past version of the terms, in effect from "
                (human(version.id))
                " until it was replaced. The "
                a href="/legal" { "current terms" }
                " apply today."
            }
        }

        (terms())

        h3.help-h #versions { "Versions" }
        ul.legal-versions {
            @for (i, (d, _)) in VERSIONS.iter().enumerate().rev() {
                li {
                    a href=(format!("/legal/{d}")) { (human(d)) }
                    @if i == last { " — current, in effect since this moment" }
                    @else { " — replaced " (human(VERSIONS[i + 1].0)) }
                }
            }
        }

        h3.help-h #verification { "Verification" }
        p.help-p {
            "Each version of these terms has one canonical plain-text form; "
            "this version's is at "
            a href=(format!("/legal/{}.txt", version.id)) {
                "/legal/" (version.id) ".txt"
            }
            ". The SHA-256 hash of that file is this version's "
            strong { "fingerprint" }
            ":"
        }
        p.legal-hash { code { (version.hash) } }
        (chain_diagram(version))
        p.help-p {
            "The canonical text names the fingerprint of the version before "
            "it (this one follows "
            code.legal-prev { (version.prev) }
            @if idx == 0 {
                ", the hash of a fixed genesis line, since nothing came "
                "before it"
            }
            "), so the versions form a chain back to the first. Rewriting any "
            "published version would change its fingerprint and break every "
            "version after it. Check this version against its fingerprint "
            "yourself:"
        }
        pre.help-code {
            "curl -s " (base_url) "legal/" (version.id) ".txt | sha256sum"
        }
        p.help-p {
            "Every link-creation response carries the version and fingerprint "
            "of the terms in effect at that moment; the API returns them, and "
            "the local history in your browser stores them with each link. "
            "Creators hold "
            strong { "their own receipts" }
            " of what the terms said, independent of this site."
        }

        footer { a href="/" { "Back to YuioLink" } }
    };
    document_full(
        "YuioLink — Legal",
        html! {
            meta name="description" content="Who provides YuioLink, what it stores while a link lives, and the terms it is offered under: every link expires within seven days, no accounts, no tracking.";
            // Archived versions stay readable — and archivable — but name the
            // current page as the one for search results to carry.
            @if !current {
                link rel="canonical" href=(format!("{base_url}legal"));
            }
        },
        body,
        html! {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every version's chain fingerprint, pinned — including the current one.
    /// Freeze-on-publish: any edit to any entry changes its hash and fails
    /// this test, so a change to the terms means APPENDING a new entry to
    /// [`VERSIONS`] and pinning its hash here (a failing run prints it).
    /// Re-pinning an entry is only ever legitimate before that entry has been
    /// published on yuio.link — after that, receipts in the wild hold the old
    /// fingerprint, and history is append-only.
    const PINNED: &[(&str, &str)] = &[(
        "2026-09-24T184300Z",
        "d2ae7bea711e5d61d5acb552e34f81275d20faa3ac00b60143c46814bf284dc0",
    )];

    #[test]
    fn every_version_fingerprint_is_pinned_and_unchanged() {
        let chain = chain();
        assert_eq!(
            chain.len(),
            PINNED.len(),
            "every version must be pinned; the newest hash is {}",
            chain.last().unwrap().hash
        );
        for (v, (id, hash)) in chain.iter().zip(PINNED) {
            assert_eq!(v.id, *id, "chain and PINNED disagree on version order");
            assert_eq!(
                v.hash, *hash,
                "terms {id} changed after being pinned; its hash is now {}",
                v.hash
            );
        }
    }

    #[test]
    fn the_chain_links_and_starts_at_genesis() {
        let chain = chain();
        assert_eq!(chain[0].prev, hex(&Sha256::digest(GENESIS)));
        for w in chain.windows(2) {
            assert_eq!(w[1].prev, w[0].hash);
        }
        for v in chain {
            // The verification promise: sha256sum of the served file IS the
            // fingerprint, and the file names its predecessor.
            assert_eq!(v.hash, hex(&Sha256::digest(&v.txt)));
            assert!(
                v.txt.contains(&format!("Previous: {}\n", v.prev)),
                "{}",
                v.txt
            );
        }
    }

    #[test]
    fn canonical_text_is_the_page_text_in_plain_form() {
        for (v, (_, terms)) in chain().iter().zip(VERSIONS) {
            // The txt is derived from the rendered markup, so equality is by
            // construction; what can rot is the extraction — a tag or escape
            // it does not handle would leave markup in the "plain" text.
            assert!(!v.txt.contains('<') && !v.txt.contains('>'), "{}", v.txt);
            assert!(
                !v.txt.contains("&amp;") && !v.txt.contains("&#"),
                "{}",
                v.txt
            );
            assert!(
                v.txt
                    .starts_with(&format!("# YuioLink Terms\n\nVersion: {}\n", v.id))
            );
            // Every heading survives extraction as a Markdown `##` line, and
            // typographic characters arrive as themselves.
            let html = terms().into_string();
            for heading in ["Who provides this", "Acceptable use", "Changes"] {
                assert_eq!(
                    html.contains(heading),
                    v.txt.contains(&format!("\n## {heading}\n")),
                    "{heading}"
                );
            }
            assert!(v.txt.contains("\u{201c}the operator\u{201d}"), "{}", v.txt);
        }
    }

    /// Not a check — a viewer. `cargo test -p yuiolink-server print_canonical
    /// -- --ignored --nocapture` prints every version's canonical text for
    /// eyeballing before its hash gets pinned.
    #[test]
    #[ignore]
    fn print_canonical_text() {
        for v in chain() {
            println!("----- {} ({}) -----\n{}", v.id, v.hash, v.txt);
        }
    }

    #[test]
    fn versions_have_timestamp_ids_in_order() {
        // Oldest first, strictly — the chrome derives "current" and each
        // version's replacement moment from this order, and the chain hashes
        // in it.
        assert!(VERSIONS.windows(2).all(|w| w[0].0 < w[1].0));
        // Full timestamps, `YYYY-MM-DDTHHMMSSZ`: they sort as strings, are
        // safe path segments, and two versions can land on the same day.
        for (d, _) in VERSIONS {
            assert!(
                d.len() == 18
                    && d.bytes().enumerate().all(|(i, b)| match i {
                        4 | 7 => b == b'-',
                        10 => b == b'T',
                        17 => b == b'Z',
                        _ => b.is_ascii_digit(),
                    }),
                "{d} is not YYYY-MM-DDTHHMMSSZ"
            );
        }
    }
}
