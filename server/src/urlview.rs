//! Decompose a stored redirect URI for trustworthy display on the interstitial.
//!
//! Three jobs: split the URL into parts the view can colour (scheme /
//! delimiters / subdomain / **registrable domain** / path / query); judge
//! whether the host is a deceptive internationalized lookalike; and — the parts
//! model at the bottom of this file — cut the stored string into an ordered list
//! of verbatim [`Slice`]s the card lists, edits, and reassembles.
//!
//! **The invariant.** Concatenating [`UriView::prefix`] with every slice's
//! [`Slice::raw`] in order reproduces the stored string, character for
//! character. Everything the card shows is either one of those characters or a
//! rendering the card openly labels as one ("Exactly as stored" carries the
//! record — inside the http(s) fold, underneath every other rendered headline —
//! precisely so the headline may be formatted for reading). See
//! `slices_reassemble_the_stored_string` in the tests.
//!
//! IDN policy (UTS #46 + UTS #39): decode punycode to Unicode for display, but
//! only when every label is *single-script*. A single-script label — Latin (incl.
//! diacritics, e.g. `münchen`), all-Cyrillic, all-Greek, CJK — is a legitimate
//! international domain and is shown decoded with no warning. A label that mixes
//! scripts (the classic `аpple.com`, Cyrillic `а` + Latin `pple`) is a homograph
//! attack: we show the raw `xn--…` punycode instead and flag it. Note this
//! catches mixed-script labels, not whole-script confusables (e.g. an all-Cyrillic
//! string shaped like Latin); those pass, consistent with "all-Cyrillic = legit".

use idna::domain_to_unicode;
use unicode_security::MixedScript;

/// A host split at the registrable-domain boundary, in display form.
pub struct HostView {
    /// Subdomain labels with no trailing dot (`docs`, `a.b`), or empty.
    pub subdomain: String,
    /// The registrable domain (eTLD+1), e.g. `example.com` — the part to trust.
    pub registrable: String,
    /// Set when the host is a deceptive lookalike; carries both forms to warn.
    pub warning: Option<IdnWarning>,
}

/// The two faces of a deceptive host, for the red warning panel.
pub struct IdnWarning {
    /// What the punycode decodes to — the misleading Unicode (`аpple.com`).
    pub displays_as: String,
    /// The unambiguous real address shown instead (`xn--pple-43d.com`).
    pub real: String,
}

/// Split an ASCII host at the registrable boundary (via the Public Suffix List)
/// and classify it for safe display.
fn build_host(host_ascii: &str) -> HostView {
    let (decoded, decode_result) = domain_to_unicode(host_ascii);
    let deceptive = decode_result.is_err() || has_mixed_script_label(&decoded);

    // The PSL works on the ASCII/punycode form; fall back to the whole host when
    // it has no recognized public suffix (IPs, intranet names, …).
    let registrable_ascii = psl::domain_str(host_ascii).unwrap_or(host_ascii);
    let subdomain_ascii = host_ascii
        .strip_suffix(registrable_ascii)
        .map(|s| s.trim_end_matches('.'))
        .unwrap_or("");

    if deceptive {
        // Show the raw punycode; reveal both forms in the warning.
        HostView {
            subdomain: subdomain_ascii.to_string(),
            registrable: registrable_ascii.to_string(),
            warning: Some(IdnWarning {
                displays_as: decoded,
                real: host_ascii.to_string(),
            }),
        }
    } else {
        // Safe: show the decoded Unicode for each part.
        HostView {
            subdomain: decode_part(subdomain_ascii),
            registrable: decode_part(registrable_ascii),
            warning: None,
        }
    }
}

/// Decode a host fragment (one or more labels, no leading/trailing dot) from
/// punycode to Unicode. Empty in, empty out.
fn decode_part(part: &str) -> String {
    if part.is_empty() {
        String::new()
    } else {
        domain_to_unicode(part).0
    }
}

/// True if any label of the decoded host is not single-script (a homograph risk).
/// ASCII labels (`com`, `de`) are trivially single-script, so a non-Latin SLD
/// under an ASCII TLD (`δοκιμή.gr`) is judged per label and stays legitimate.
fn has_mixed_script_label(decoded_host: &str) -> bool {
    decoded_host
        .split('.')
        .filter(|label| !label.is_empty())
        .any(|label| !label.is_single_script())
}

// --------------------------------------------------------------------------
// The parts model
// --------------------------------------------------------------------------

/// What following a stored string would cost, which is the only thing the card
/// grades. Nothing here is a verdict about the destination — a site can be
/// hostile over https and a `magnet:` can be harmless.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    /// `http` / `https`: a website opens, one press.
    Web,
    /// The rest of the allowlist: it leaves the browser and lands wherever the
    /// device decides, which is why every card in this tier carries the hedge.
    Handoff,
    /// Off the allowlist: printed, never linked, given no control at all.
    Refused,
}

/// Where a slice sits in the URI's grammar. This decides whether the slice is a
/// row at all, and whether that row can be unticked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// `alice@` before a host. Always earns a warning chip.
    Userinfo,
    /// The host. Never a row — it is the headline.
    Host,
    /// An explicit `:8443`. The `url` crate never serialises a default port, so
    /// a visible one is always non-standard.
    Port,
    /// A run of path segments with no parameters of their own.
    Path,
    /// An RFC 3986 path-segment parameter, `;sid=9f2c`. Each carries its own
    /// `;`, so repeated ones never need a shared delimiter.
    PathParam,
    /// One `key=value` of the query, in stored order, repeats kept.
    Query,
    /// The fragment, or one pair of an `&`-shaped one (the OAuth implicit case).
    Fragment,
    /// A comma-joined address or number (mailto, sms).
    Recipient,
    /// A body no standard gives us structure for: a `tel:` number, a
    /// `spotify:` or `matrix:` body. Never a row — there is nothing to remove.
    Opaque,
}

/// One run of a value, ready to be coloured. The card never substitutes a glyph
/// for a character: an escape that stays escaped is *shown* escaped, so what is
/// on screen and what ⌘C yields agree.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Piece {
    /// Ordinary readable text.
    Text(String),
    /// A space that was stored as `%20`. Drawn with a dotted underline — a
    /// receipt saying "this space is an escape in the stored link".
    DecodedSpace,
    /// A space run that is padding: two or more, or one at either edge. Tinted,
    /// never replaced by a visible glyph, so a copy still yields the real
    /// string. Fires [`Hazard::PaddedWithSpaces`].
    Padding(String),
    /// An escape left escaped because decoding it would redraw the URI's own
    /// structure: `&`, `=`, `#`, `%`. Drawn dim, as encoding noise.
    Escape(String),
    /// An escape decoding to something invisible or direction-changing. Left
    /// escaped and drawn in danger colour, always with its chip.
    BadEscape(String),
    /// A structural delimiter inside a value — the `://` and `/` of an address
    /// carried in a query parameter.
    Delim(String),
    /// A registrable domain inside a value. Bold, never washed: the accent wash
    /// is spent once per page, on the headline.
    Domain(String),
    /// The local part of an address. Full colour, regular weight — bold is the
    /// domain's mark site-wide.
    Local(String),
}

impl Piece {
    /// The characters this piece puts on screen. Used for the invariant test
    /// and for anything that needs the reading rather than the storage.
    pub fn text(&self) -> &str {
        match self {
            Piece::Text(s)
            | Piece::Padding(s)
            | Piece::Escape(s)
            | Piece::BadEscape(s)
            | Piece::Delim(s)
            | Piece::Domain(s)
            | Piece::Local(s) => s,
            Piece::DecodedSpace => " ",
        }
    }
}

/// One verbatim cut of the stored string.
///
/// [`Slice::raw`] rebuilds exactly the characters this slice was cut from,
/// delimiter included; `display` is the same value decoded for reading.
#[derive(Clone, Debug)]
pub struct Slice {
    pub role: Role,
    /// The literal delimiter that introduces this part in the stored string
    /// (`?`, `&`, `;`, `,`, `#`, `:`), or empty for a positional first part.
    /// A key cell either carries this — the URI's own character — or, bare,
    /// a word we chose; the design keeps those two voices apart.
    pub delim: String,
    /// The key, verbatim, for keyed parts (`sid`, `next`, `cc`, `xt`, `tr`).
    pub key: Option<String>,
    /// True when the stored string really has a `=` after the key. `xmpp:?join`
    /// has a key and no `=`, and inventing one would be a lie about the string.
    pub equals: bool,
    /// Everything after the `=`, verbatim (still percent-encoded).
    pub value: String,
    /// `value`, decoded for reading.
    pub display: Vec<Piece>,
    /// Whether the card lets this part be unticked. Removal only, always: a
    /// subset of an allowlisted URI is still allowlisted.
    pub removable: bool,
}

impl Slice {
    /// The stored characters this slice was cut from.
    pub fn raw(&self) -> String {
        let mut s = String::with_capacity(self.delim.len() + self.value.len() + 8);
        s.push_str(&self.delim);
        if let Some(k) = &self.key {
            s.push_str(k);
        }
        if self.equals {
            s.push('=');
        }
        s.push_str(&self.value);
        s
    }

    /// Whether this slice gets a row of its own. The host is the headline and an
    /// opaque body has nothing to remove, so neither is listed twice.
    pub fn is_row(&self) -> bool {
        !matches!(self.role, Role::Host | Role::Opaque)
    }

    /// True when reading this value put different characters on screen from the
    /// ones in storage. Colour is not a difference — a bold domain and a dim
    /// delimiter still spell the stored string — so this is a plain comparison
    /// of what is shown against what is kept. It is what gates the record on
    /// an http(s) card: everyday links stay one quiet line.
    pub fn decoded_differs(&self) -> bool {
        self.display.iter().map(Piece::text).collect::<String>() != self.value
    }
}

/// Something true about the stored string that the reader would want said out
/// loud. Every one of these is provable from the string or from a published
/// table — none of them is a guess about the destination's intentions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hazard {
    /// Plain `http`. "Not Encrypted", not "Not Secure": we are describing the
    /// transport, not grading the site.
    NotEncrypted,
    /// `alice@` before the host — the part of a URL people misread as the site.
    UsernameInTheAddress,
    /// An escape decoding to an invisible or direction-changing character.
    HiddenCharacters,
    /// A run of spaces, or a space at an edge, where one would not be noticed.
    PaddedWithSpaces,
    /// A query value that is itself a complete web address.
    CarriesAnotherAddress,
}

/// A stored URI, cut into verbatim slices and classified.
pub struct UriView {
    /// Lowercase scheme with no colon.
    pub scheme: String,
    pub tier: Tier,
    /// Everything before the first slice: `https://`, `mailto:`, `tel:`.
    pub prefix: String,
    pub slices: Vec<Slice>,
    pub hazards: Vec<Hazard>,
    /// The host, split at the registrable boundary, for the schemes that have
    /// one. `None` for `mailto:`, `tel:`, `magnet:`, and friends.
    pub host: Option<HostView>,
    /// The URI's own word for what it points at, where the scheme publishes
    /// one: `track` for `spotify:track:…`, `r` for `matrix:r/…`. The card reads
    /// its subtitle off this rather than guessing.
    pub type_segment: Option<String>,
}

impl UriView {
    /// Reassemble the stored string from the parts. That this equals the string
    /// it was parsed from is the invariant the whole card rests on.
    pub fn raw(&self) -> String {
        let mut s = self.prefix.clone();
        for slice in &self.slices {
            s.push_str(&slice.raw());
        }
        s
    }

    pub fn has(&self, hazard: Hazard) -> bool {
        self.hazards.contains(&hazard)
    }

    /// The registrable domain for a host-based URI, else the scheme as a
    /// stand-in. This is what the button, the tab title, and the share card
    /// name the destination by.
    pub fn card_domain(&self) -> String {
        match &self.host {
            Some(h) => h.registrable.clone(),
            None => self.scheme.clone(),
        }
    }

    /// Set when the host is a deceptive internationalized lookalike.
    pub fn idn_warning(&self) -> Option<&IdnWarning> {
        self.host.as_ref().and_then(|h| h.warning.as_ref())
    }

    /// The first slice with this role, if any.
    pub fn first(&self, role: Role) -> Option<&Slice> {
        self.slices.iter().find(|s| s.role == role)
    }

    /// True when any warning fired that is about the *string* rather than the
    /// transport. This is what opens the fold: a warning should never need a
    /// click to understand, but "Not Encrypted" says nothing about the parts.
    pub fn warns_about_the_string(&self) -> bool {
        self.hazards.iter().any(|h| *h != Hazard::NotEncrypted)
    }

    /// True when reading changed any value, which is the only thing that makes
    /// an http(s) card grow its "Exactly as stored" record (inside the fold).
    pub fn decoding_changed_anything(&self) -> bool {
        self.slices.iter().any(Slice::decoded_differs)
    }

    pub fn rows(&self) -> impl Iterator<Item = &Slice> {
        self.slices.iter().filter(|s| s.is_row())
    }

    pub fn recipients(&self) -> impl Iterator<Item = &Slice> {
        self.slices.iter().filter(|s| s.role == Role::Recipient)
    }

    /// The value of the first parameter with this key, decoded for reading.
    pub fn param(&self, key: &str) -> Option<&Slice> {
        self.slices
            .iter()
            .find(|s| s.key.as_deref() == Some(key) && s.role == Role::Query)
    }

    /// The fold earns its line: it appears only when it has something to add —
    /// a part that can be unticked, or a value that reads differently from the
    /// way it is stored. A bare path never folds.
    pub fn fold_is_worth_it(&self) -> bool {
        self.rows().any(|s| s.removable) || self.decoding_changed_anything()
    }
}

/// Which tier a scheme falls in.
pub fn tier_of(scheme: &str) -> Tier {
    match scheme {
        "http" | "https" => Tier::Web,
        s if yuiolink_core::DEFAULT_ALLOWED_SCHEMES.contains(&s) => Tier::Handoff,
        _ => Tier::Refused,
    }
}

/// Cut a stored URI into its parts.
///
/// Every branch works on the stored characters directly rather than on a
/// re-serialised `url::Url`, because the invariant is about *these* characters:
/// a round trip through a parser is exactly the place a stray normalisation
/// would slip in and make the card's "Exactly as stored" line a lie.
pub fn parse_uri(stored: &str) -> UriView {
    let scheme = stored
        .split_once(':')
        .map(|(s, _)| s.to_ascii_lowercase())
        .unwrap_or_default();
    let tier = tier_of(&scheme);
    let mut view = match scheme.as_str() {
        "http" | "https" | "ftp" | "ftps" | "irc" | "ircs" => hierarchical(stored, &scheme),
        "mailto" => mailto(stored, &scheme),
        "tel" => tel(stored, &scheme),
        "sms" => sms(stored, &scheme),
        "magnet" => magnet(stored, &scheme),
        _ => opaque(stored, &scheme),
    };
    view.tier = tier;
    view.hazards = detect_hazards(&view);
    view
}

/// `scheme://[userinfo@]host[:port][/path][;params][?query][#fragment]`, and
/// `irc:` too — it was never standardised, but every draft gives it this shape.
fn hierarchical(stored: &str, scheme: &str) -> UriView {
    let prefix = format!("{scheme}://");
    let rest = stored.get(prefix.len()..).unwrap_or("");
    let mut slices = Vec::new();

    // Authority runs to the first character that starts a later component.
    let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(auth_end);

    // Userinfo is everything up to the LAST `@`: an address may legitimately
    // carry one of its own, and the host is what comes after the final one.
    let (userinfo, hostport) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..=i]), &authority[i + 1..]),
        None => (None, authority),
    };
    if let Some(u) = userinfo {
        slices.push(Slice {
            role: Role::Userinfo,
            delim: String::new(),
            key: None,
            equals: false,
            value: u.to_string(),
            display: vec![Piece::Text(u.to_string())],
            removable: true,
        });
    }

    // A port is a trailing `:digits`, never the `:` inside an IPv6 literal.
    let port_at = hostport
        .rfind(':')
        .filter(|i| *i > hostport.rfind(']').map_or(0, |b| b))
        .filter(|i| {
            let p = &hostport[i + 1..];
            !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())
        });
    let (host_str, port) = match port_at {
        Some(i) => (&hostport[..i], Some(&hostport[i..])),
        None => (hostport, None),
    };

    let host = (!host_str.is_empty()).then(|| build_host(host_str));
    slices.push(Slice {
        role: Role::Host,
        delim: String::new(),
        key: None,
        equals: false,
        value: host_str.to_string(),
        display: host_display(host.as_ref(), host_str),
        removable: false,
    });
    if let Some(p) = port {
        slices.push(Slice {
            role: Role::Port,
            delim: ":".to_string(),
            key: None,
            equals: false,
            value: p[1..].to_string(),
            display: vec![Piece::Text(p[1..].to_string())],
            // Dropping a port changes which server answers, so it identifies
            // the resource as much as the host does.
            removable: false,
        });
    }

    let (path, query, fragment) = split_tail(tail);
    slices.extend(path_slices(path));
    slices.extend(query_slices(query, "?"));
    slices.extend(fragment_slices(fragment));

    UriView {
        scheme: scheme.to_string(),
        tier: Tier::Web,
        prefix,
        slices,
        hazards: Vec::new(),
        host,
        // The IRC target, minus any `,isnick` flag, is the closest thing this
        // scheme has to a type word.
        type_segment: (scheme.starts_with("irc"))
            .then(|| path.trim_start_matches('/').to_string())
            .filter(|s| !s.is_empty()),
    }
}

/// Split what follows the authority into path, query, and fragment, each
/// keeping its introducing character.
fn split_tail(tail: &str) -> (&str, &str, &str) {
    let (before_frag, fragment) = match tail.find('#') {
        Some(i) => (&tail[..i], &tail[i..]),
        None => (tail, ""),
    };
    match before_frag.find('?') {
        Some(i) => (&before_frag[..i], &before_frag[i..], fragment),
        None => (before_frag, "", fragment),
    }
}

/// The path, cut so that each `;key=value` segment parameter is its own
/// removable slice and the segments around it stay whole. In the ordinary case
/// — no parameters anywhere — this is a single fixed slice.
fn path_slices(path: &str) -> Vec<Slice> {
    let mut out = Vec::new();
    let mut plain = String::new();
    for (i, segment) in path.split('/').enumerate() {
        // `split('/')` drops the separators; put each one back.
        if i > 0 {
            plain.push('/');
        }
        let mut bits = segment.split(';');
        plain.push_str(bits.next().unwrap_or(""));
        for param in bits {
            if !plain.is_empty() {
                out.push(plain_path_slice(std::mem::take(&mut plain)));
            }
            let (key, equals, value) = split_pair(param);
            out.push(Slice {
                role: Role::PathParam,
                delim: ";".to_string(),
                key: Some(key.to_string()),
                equals,
                display: decode_for_reading(value),
                value: value.to_string(),
                removable: true,
            });
        }
    }
    if !plain.is_empty() {
        out.push(plain_path_slice(plain));
    }
    out
}

fn plain_path_slice(path: String) -> Slice {
    Slice {
        role: Role::Path,
        delim: String::new(),
        key: None,
        equals: false,
        display: decode_for_reading(&path),
        value: path,
        // The path IS the destination: the editing strips what rides along, not
        // where the link points. And nearly every URL has one, so a removable
        // path would put checkboxes on every everyday card.
        removable: false,
    }
}

/// `?k=v&k2=v2` as one slice per pair, in stored order, repeats kept. `lead` is
/// the character that introduces the first pair (`?` everywhere except an
/// `&`-shaped fragment, which reuses `#`).
fn query_slices(query: &str, lead: &str) -> Vec<Slice> {
    let body = query.strip_prefix(lead).unwrap_or(query);
    if query.is_empty() {
        return Vec::new();
    }
    body.split('&')
        .enumerate()
        .map(|(i, pair)| {
            let (key, equals, value) = split_pair(pair);
            Slice {
                role: Role::Query,
                delim: if i == 0 {
                    lead.to_string()
                } else {
                    "&".to_string()
                },
                key: Some(key.to_string()),
                equals,
                display: structure_value(decode_for_reading(value)),
                value: value.to_string(),
                removable: true,
            }
        })
        .collect()
}

/// At most one fragment per URI — the first `#` wins and everything after it is
/// opaque to the network. An `=`/`&`-shaped one (the OAuth implicit flow, which
/// is how a bearer token ends up in a shared link) unrolls on `&` only, so
/// `#a=b,c=d` stays one row with the comma inside the value.
fn fragment_slices(fragment: &str) -> Vec<Slice> {
    if fragment.is_empty() {
        return Vec::new();
    }
    let body = &fragment[1..];
    if body.contains('=') {
        return body
            .split('&')
            .enumerate()
            .map(|(i, pair)| {
                let (key, equals, value) = split_pair(pair);
                Slice {
                    role: Role::Fragment,
                    delim: if i == 0 {
                        "#".to_string()
                    } else {
                        "&".to_string()
                    },
                    key: Some(key.to_string()),
                    equals,
                    display: decode_for_reading(value),
                    value: value.to_string(),
                    removable: true,
                }
            })
            .collect();
    }
    vec![Slice {
        role: Role::Fragment,
        delim: "#".to_string(),
        key: None,
        equals: false,
        display: decode_for_reading(body),
        value: body.to_string(),
        removable: true,
    }]
}

/// `mailto:` per RFC 6068: comma-joined recipients, then `?`/`&` header fields.
///
/// Every address is a removable slice, including a sole one — the Draft rule.
/// RFC 6068 is happy with a to-less, cc-only, even address-less mailto (a bare
/// compose window opens), so nothing here has a floor and nothing locks.
fn mailto(stored: &str, scheme: &str) -> UriView {
    let prefix = format!("{scheme}:");
    let rest = stored.get(prefix.len()..).unwrap_or("");
    let (body, query) = match rest.find('?') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let mut slices = recipient_slices(body, Role::Recipient);
    slices.extend(query_slices(query, "?"));
    // An address is an address wherever it appears, so `cc` and `bcc` read the
    // way the recipients beside them do.
    for slice in &mut slices {
        if matches!(slice.key.as_deref(), Some("to" | "cc" | "bcc")) {
            slice.display = address_list_pieces(&slice.value);
        }
    }
    UriView {
        scheme: scheme.to_string(),
        tier: Tier::Handoff,
        prefix,
        slices,
        hazards: Vec::new(),
        host: None,
        type_segment: None,
    }
}

/// `tel:` per RFC 3966 — a single number and its `;`-parameters. The grammar
/// has no list form, so a `tel:` never gets the recipient treatment.
fn tel(stored: &str, scheme: &str) -> UriView {
    let prefix = format!("{scheme}:");
    let rest = stored.get(prefix.len()..).unwrap_or("");
    let mut bits = rest.split(';');
    let number = bits.next().unwrap_or("");
    let mut slices = vec![Slice {
        role: Role::Opaque,
        delim: String::new(),
        key: None,
        equals: false,
        display: decode_for_reading(number),
        value: number.to_string(),
        removable: false,
    }];
    for param in bits {
        let (key, equals, value) = split_pair(param);
        slices.push(Slice {
            role: Role::PathParam,
            delim: ";".to_string(),
            key: Some(key.to_string()),
            equals,
            display: decode_for_reading(value),
            value: value.to_string(),
            // `;ext=` is part of dialling the number, not an extra riding along.
            removable: false,
        });
    }
    UriView {
        scheme: scheme.to_string(),
        tier: Tier::Handoff,
        prefix,
        slices,
        hazards: Vec::new(),
        host: None,
        type_segment: None,
    }
}

/// `sms:` per RFC 5724: `sms-recipient *("," sms-recipient) [ "?" hfields ]`.
/// The grammar requires at least one recipient, which is the one asymmetry with
/// `mailto:` — the last number standing locks instead of leaving a draft.
fn sms(stored: &str, scheme: &str) -> UriView {
    let prefix = format!("{scheme}:");
    let rest = stored.get(prefix.len()..).unwrap_or("");
    let (body, query) = match rest.find('?') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let mut slices = recipient_slices(body, Role::Recipient);
    slices.extend(query_slices(query, "?"));
    UriView {
        scheme: scheme.to_string(),
        tier: Tier::Handoff,
        prefix,
        slices,
        hazards: Vec::new(),
        host: None,
        type_segment: None,
    }
}

/// A comma-joined list, each entry keeping the comma that introduced it.
fn recipient_slices(body: &str, role: Role) -> Vec<Slice> {
    if body.is_empty() {
        return Vec::new();
    }
    body.split(',')
        .enumerate()
        .map(|(i, one)| Slice {
            role,
            delim: if i == 0 {
                String::new()
            } else {
                ",".to_string()
            },
            key: None,
            equals: false,
            display: address_pieces(one),
            value: one.to_string(),
            removable: true,
        })
        .collect()
}

/// `magnet:?xt=…&dn=…&tr=…`. Every tracker is listed; none is ever counted —
/// fifteen trackers are fifteen rows.
fn magnet(stored: &str, scheme: &str) -> UriView {
    let prefix = format!("{scheme}:");
    let rest = stored.get(prefix.len()..).unwrap_or("");
    let mut slices = query_slices(rest, "?");
    for slice in &mut slices {
        // Only `xt` identifies the data. `dn` is a name the link's creator
        // chose and `tr` is a tracker you would announce to; strip every `tr`
        // and the link is DHT-only, which still works.
        slice.removable = slice.key.as_deref() != Some("xt");
    }
    UriView {
        scheme: scheme.to_string(),
        tier: Tier::Handoff,
        prefix,
        slices,
        hazards: Vec::new(),
        host: None,
        type_segment: None,
    }
}

/// Everything else on the allowlist — `spotify:`, `xmpp:`, `matrix:` — plus any
/// scheme that is off it. The body is kept whole and only a query is split off,
/// which is what `xmpp:…?join` needs.
fn opaque(stored: &str, scheme: &str) -> UriView {
    let prefix = format!("{scheme}:");
    let rest = stored.get(prefix.len()..).unwrap_or("");
    let (body, query) = match rest.find('?') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    let mut slices = vec![Slice {
        role: Role::Opaque,
        delim: String::new(),
        key: None,
        equals: false,
        display: opaque_pieces(scheme, body),
        value: body.to_string(),
        removable: false,
    }];
    slices.extend(query_slices(query, "?"));
    UriView {
        scheme: scheme.to_string(),
        tier: Tier::Handoff,
        prefix,
        slices,
        hazards: Vec::new(),
        host: None,
        type_segment: type_segment_of(scheme, body),
    }
}

/// The URI's own word for what it points at, where the scheme publishes one.
///
/// `matrix:` follows MSC2312 — `u/`, `r/`, `roomid/`, `e/`. The deprecated
/// `user/`, `room/`, `event/` forms are still in the wild, so they are accepted
/// when classifying and never generated.
fn type_segment_of(scheme: &str, body: &str) -> Option<String> {
    match scheme {
        "spotify" => body.split(':').next().map(str::to_string),
        "matrix" => body.split('/').next().map(|s| match s {
            "user" => "u".to_string(),
            "room" => "r".to_string(),
            "event" => "e".to_string(),
            other => other.to_string(),
        }),
        _ => None,
    }
    .filter(|s| !s.is_empty())
}

// --------------------------------------------------------------------------
// Reading: decoding, addresses, hazards
// --------------------------------------------------------------------------

/// Split `key=value`, reporting whether the `=` was really there. A bare `join`
/// keeps `equals: false` so nothing invents a value it does not have.
fn split_pair(pair: &str) -> (&str, bool, &str) {
    match pair.split_once('=') {
        Some((k, v)) => (k, true, v),
        None => (pair, false, ""),
    }
}

/// A host, split so the registrable domain can carry the weight.
pub(crate) fn host_pieces(host: &str) -> Vec<Piece> {
    if host.is_empty() {
        return Vec::new();
    }
    let registrable = psl::domain_str(host).unwrap_or(host);
    let sub = host.strip_suffix(registrable).unwrap_or("");
    let mut out = Vec::new();
    if !sub.is_empty() {
        out.push(Piece::Text(sub.to_string()));
    }
    out.push(Piece::Domain(registrable.to_string()));
    out
}

/// The characters the http(s) headline actually shows for the host.
/// [`build_host`] decides between the decoded Unicode and the raw punycode;
/// mirroring its choice here keeps [`Slice::decoded_differs`] honest about the
/// one part of the card that is decoded somewhere other than
/// [`decode_for_reading`] — a stored `xn--…` host that reads as Unicode grows
/// the record like any other rendering.
fn host_display(host: Option<&HostView>, stored: &str) -> Vec<Piece> {
    match host {
        Some(h) => {
            let mut out = Vec::new();
            if !h.subdomain.is_empty() {
                out.push(Piece::Text(format!("{}.", h.subdomain)));
            }
            out.push(Piece::Domain(h.registrable.clone()));
            out
        }
        None => host_pieces(stored),
    }
}

/// An address or a bare number, read for what it is. On freemail the local part
/// *is* the identity, so it takes full colour — but regular weight, because
/// bold means one thing site-wide.
fn address_pieces(value: &str) -> Vec<Piece> {
    match value.rfind('@') {
        Some(i) if i > 0 && i + 1 < value.len() => {
            let mut out = vec![
                Piece::Local(value[..i].to_string()),
                Piece::Delim("@".to_string()),
            ];
            out.extend(host_pieces(&value[i + 1..]));
            out
        }
        _ => decode_for_reading(value),
    }
}

/// Addresses in a header field (`cc`, `bcc`), read the way the recipient list
/// is. An address is an address wherever it appears, so `?cc=archive@…` wears
/// the same dress as the `to` beside it rather than reading as opaque text.
fn address_list_pieces(value: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    for (n, one) in value.split(',').enumerate() {
        if n > 0 {
            out.push(Piece::Delim(",".to_string()));
        }
        out.extend(address_pieces(one));
    }
    out
}

/// Re-read a value that is itself a URI, so one palette really does cover the
/// URL line, the slices, and the exact line: the scheme's punctuation recedes
/// and the registrable domain takes the site's bold.
///
/// No accent wash — that is spent once per page, on the headline — and no
/// warning of any kind. A tracker in a magnet is the structure that scheme is
/// made of, not a hazard; whether an address inside a value is worth a chip is
/// decided separately, and only for an http(s) query value.
///
/// Returns `None` unless the whole thing really is `scheme://host…`, and the
/// pieces always spell the input back exactly.
fn uri_value_pieces(text: &str) -> Option<Vec<Piece>> {
    let (scheme, rest) = text.split_once("://")?;
    let scheme_ok = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    if !scheme_ok {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => (h, Some(p)),
        _ => (authority, None),
    };
    if host.is_empty() {
        return None;
    }
    let mut out = vec![
        Piece::Text(scheme.to_string()),
        Piece::Delim("://".to_string()),
    ];
    out.extend(host_pieces(host));
    if let Some(p) = port {
        out.push(Piece::Delim(":".to_string()));
        out.push(Piece::Text(p.to_string()));
    }
    for (n, segment) in tail.split('/').enumerate() {
        if n > 0 {
            out.push(Piece::Delim("/".to_string()));
        }
        if !segment.is_empty() {
            out.push(Piece::Text(segment.to_string()));
        }
    }
    Some(out)
}

/// Give a decoded value its structure back, where it turns out to have some.
/// Only when nothing needed marking: an escape that stayed escaped, or a space
/// worth pointing at, is the more important thing to say about the value.
fn structure_value(pieces: Vec<Piece>) -> Vec<Piece> {
    if !pieces.iter().all(|p| matches!(p, Piece::Text(_))) {
        return pieces;
    }
    let text: String = pieces.iter().map(Piece::text).collect();
    uri_value_pieces(&text).unwrap_or(pieces)
}

/// A body no standard structures for us, given the little shape its scheme
/// does publish: `spotify:track:ID` and `matrix:r/room:server` both wear their
/// separators, and those separators are worth dimming.
fn opaque_pieces(scheme: &str, body: &str) -> Vec<Piece> {
    match scheme {
        "xmpp" => address_pieces(body),
        "spotify" | "matrix" => {
            let mut out = Vec::new();
            let mut token = String::new();
            for c in body.chars() {
                if matches!(c, ':' | '/') {
                    if !token.is_empty() {
                        out.push(Piece::Text(std::mem::take(&mut token)));
                    }
                    out.push(Piece::Delim(c.to_string()));
                } else {
                    token.push(c);
                }
            }
            if !token.is_empty() {
                out.push(Piece::Text(token));
            }
            out
        }
        _ => decode_for_reading(body),
    }
}

/// The four characters an escape is never decoded through. Decoding them would
/// redraw the URI's structure on screen — a `%26` shown as `&` reads as another
/// parameter starting, which is exactly the confusion the escape exists to
/// prevent.
const STRUCTURE: [char; 4] = ['&', '=', '#', '%'];

/// Decode a stored value for reading, and mark what could not be decoded
/// honestly.
///
/// The rule in one line: values decode, except the four structure characters
/// (which stay escaped and dim) and anything invisible or direction-changing
/// (which stays escaped and red, with its chip). A space that came from `%20`
/// is drawn with a dotted underline; a literal stored space — legal only in the
/// cannot-be-a-base schemes, since the URL spec percent-encodes the rest — is
/// drawn bare. Three appearances, three meanings, no overlap.
pub fn decode_for_reading(value: &str) -> Vec<Piece> {
    // (character, the escape it came from if it was one)
    let mut chars: Vec<(char, Option<String>)> = Vec::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Gather the whole run of escapes so a multi-byte UTF-8 character
            // is decoded as one character rather than three replacement marks.
            let start = i;
            let mut buf: Vec<u8> = Vec::new();
            while i + 2 < bytes.len() + 1 && bytes.get(i) == Some(&b'%') {
                match (hex(bytes.get(i + 1)), hex(bytes.get(i + 2))) {
                    (Some(h), Some(l)) => {
                        buf.push(h * 16 + l);
                        i += 3;
                    }
                    _ => break,
                }
            }
            if buf.is_empty() {
                chars.push(('%', None));
                i += 1;
                continue;
            }
            let raw = &value[start..i];
            match std::str::from_utf8(&buf) {
                Ok(decoded) => {
                    // One escape per decoded character, so each keeps its own
                    // provenance and the structure rule applies per character.
                    let mut consumed = 0;
                    for c in decoded.chars() {
                        let n = c.len_utf8() * 3;
                        chars.push((c, Some(raw[consumed..consumed + n].to_string())));
                        consumed += n;
                    }
                }
                // Not valid UTF-8: nothing honest to show, so it stays escaped.
                Err(_) => chars.push((char::REPLACEMENT_CHARACTER, Some(raw.to_string()))),
            }
        } else {
            let c = value[i..].chars().next().unwrap_or('\u{fffd}');
            chars.push((c, None));
            i += c.len_utf8();
        }
    }

    let padded = padding_mask(&chars);
    let mut out: Vec<Piece> = Vec::new();
    for (idx, (c, escape)) in chars.iter().enumerate() {
        let piece = match escape {
            Some(raw) if STRUCTURE.contains(c) => Piece::Escape(raw.clone()),
            // The replacement character stands for bytes that were not valid
            // UTF-8 (or really was U+FFFD in storage — same dress either way):
            // nothing honest to show decoded, so the escape stays on screen.
            Some(raw) if *c == char::REPLACEMENT_CHARACTER => Piece::Escape(raw.clone()),
            Some(raw) if is_invisible(*c) => Piece::BadEscape(raw.clone()),
            _ if is_invisible(*c) && *c != ' ' => Piece::BadEscape(escaped(*c)),
            _ if padded[idx] => Piece::Padding(c.to_string()),
            Some(_) if *c == ' ' => Piece::DecodedSpace,
            _ => Piece::Text(c.to_string()),
        };
        // Merge with the previous piece where merging keeps the meaning.
        match (out.last_mut(), &piece) {
            (Some(Piece::Text(prev)), Piece::Text(s)) => prev.push_str(s),
            (Some(Piece::Padding(prev)), Piece::Padding(s)) => prev.push_str(s),
            (Some(Piece::Escape(prev)), Piece::Escape(s)) => prev.push_str(s),
            (Some(Piece::BadEscape(prev)), Piece::BadEscape(s)) => prev.push_str(s),
            _ => out.push(piece),
        }
    }
    out
}

/// Which characters are padding: a space in a run of two or more, or a space at
/// either end. A single space in the middle of a subject is ordinary English
/// and stays silent.
fn padding_mask(chars: &[(char, Option<String>)]) -> Vec<bool> {
    let mut mask = vec![false; chars.len()];
    let mut i = 0;
    while i < chars.len() {
        if chars[i].0 != ' ' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].0 == ' ' {
            i += 1;
        }
        let run = i - start;
        if run >= 2 || start == 0 || i == chars.len() {
            mask[start..i].fill(true);
        }
    }
    mask
}

fn hex(b: Option<&u8>) -> Option<u8> {
    match b? {
        c @ b'0'..=b'9' => Some(c - b'0'),
        c @ b'a'..=b'f' => Some(c - b'a' + 10),
        c @ b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn escaped(c: char) -> String {
    let mut buf = [0u8; 4];
    c.encode_utf8(&mut buf)
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect()
}

/// Invisible, zero-width, or direction-changing. These are the characters that
/// let a stored string read as one thing and act as another, so they are never
/// decoded for display — the reader sees the escape and the chip.
fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{0000}'..='\u{001f}'
            | '\u{007f}'..='\u{009f}'
            | '\u{00a0}'
            | '\u{00ad}'
            | '\u{034f}'
            | '\u{061c}'
            | '\u{115f}'..='\u{1160}'
            | '\u{17b4}'..='\u{17b5}'
            | '\u{180b}'..='\u{180e}'
            | '\u{2000}'..='\u{200f}'
            | '\u{2028}'..='\u{202f}'
            | '\u{205f}'..='\u{206f}'
            | '\u{3000}'
            | '\u{3164}'
            | '\u{feff}'
            | '\u{ffa0}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{e0000}'..='\u{e007f}')
}

/// Everything the card can say about the stored string that the reader deserves
/// to hear, gathered in one pass over the parts.
fn detect_hazards(view: &UriView) -> Vec<Hazard> {
    let mut out = Vec::new();
    if view.scheme == "http" {
        out.push(Hazard::NotEncrypted);
    }
    if view.slices.iter().any(|s| s.role == Role::Userinfo) {
        out.push(Hazard::UsernameInTheAddress);
    }
    if view
        .slices
        .iter()
        .any(|s| s.display.iter().any(|p| matches!(p, Piece::BadEscape(_))))
    {
        out.push(Hazard::HiddenCharacters);
    }
    if view
        .slices
        .iter()
        .any(|s| s.display.iter().any(|p| matches!(p, Piece::Padding(_))))
    {
        out.push(Hazard::PaddedWithSpaces);
    }
    if view
        .slices
        .iter()
        .filter(|s| s.role == Role::Query)
        .any(|s| carries_an_address(&s.display))
    {
        out.push(Hazard::CarriesAnotherAddress);
    }
    out
}

/// True when a parameter's value is itself a complete web address — the shape
/// an open redirect wears, and one worth saying out loud whether or not it is
/// being used as one.
fn carries_an_address(display: &[Piece]) -> bool {
    let text: String = display.iter().map(Piece::text).collect();
    let lower = text.trim().to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && url::Url::parse(text.trim()).is_ok_and(|u| u.host_str().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rebuild a stored string from the slices that survive `keep`, with the
    /// delimiters promoted.
    ///
    /// This is the model `preview.js` implements in the browser; it lives here as
    /// well because the rule is easy to state and easy to get wrong, and a test can
    /// hold it still. Dropping the first query pair promotes the next one's `&` to
    /// a `?`; dropping the first recipient promotes the next one's `,` away. Path
    /// parameters need no promotion at all — each one brings its own `;`.
    ///
    /// Removal only, always. Every rebuilt string is a subset of an allowlisted
    /// URI, so it is still allowlisted; nothing here can add a character.
    pub fn rebuild(view: &UriView, keep: impl Fn(usize) -> bool) -> String {
        let mut out = view.prefix.clone();
        let mut seen_recipient = false;
        let mut seen_query = false;
        let mut seen_fragment = false;
        for (i, slice) in view.slices.iter().enumerate() {
            if slice.removable && !keep(i) {
                continue;
            }
            let delim: &str = match slice.role {
                Role::Recipient => {
                    let d = if seen_recipient { "," } else { "" };
                    seen_recipient = true;
                    d
                }
                Role::Query => {
                    let d = if seen_query { "&" } else { "?" };
                    seen_query = true;
                    d
                }
                Role::Fragment => {
                    let d = if seen_fragment { "&" } else { "#" };
                    seen_fragment = true;
                    d
                }
                _ => &slice.delim,
            };
            out.push_str(delim);
            if let Some(k) = &slice.key {
                out.push_str(k);
            }
            if slice.equals {
                out.push('=');
            }
            out.push_str(&slice.value);
        }
        out
    }

    fn host_of(url: &str) -> HostView {
        parse_uri(url).host.expect("expected a host")
    }

    /// Punycode-encode a Unicode host so tests can express the readable form.
    fn to_ascii(host: &str) -> String {
        idna::domain_to_ascii(host).expect("encodable host")
    }

    #[test]
    fn plain_ascii_url_decomposes() {
        let v = parse_uri("https://example.com/blog/2026/the-post?ref=share");
        assert_eq!(v.scheme, "https");
        let h = v.host.as_ref().unwrap();
        assert_eq!(h.subdomain, "");
        assert_eq!(h.registrable, "example.com");
        assert!(h.warning.is_none());
        assert_eq!(v.first(Role::Path).unwrap().value, "/blog/2026/the-post");
        assert_eq!(v.param("ref").unwrap().value, "share");
    }

    #[test]
    fn subdomain_splits_from_registrable() {
        let h = host_of("https://docs.acme.co/q3");
        assert_eq!(h.subdomain, "docs");
        assert_eq!(h.registrable, "acme.co");
    }

    #[test]
    fn multi_label_subdomain_under_compound_suffix() {
        let h = host_of("https://a.b.example.co.uk/");
        assert_eq!(h.registrable, "example.co.uk");
        assert_eq!(h.subdomain, "a.b");
    }

    #[test]
    fn ascii_host_has_no_warning() {
        assert!(host_of("https://example.com/").warning.is_none());
    }

    #[test]
    fn single_script_latin_idn_is_shown_decoded() {
        // münchen.de — Latin with a diacritic: legitimate, decoded, no warning.
        let url = format!("https://{}/tickets", to_ascii("münchen.de"));
        let h = host_of(&url);
        assert_eq!(h.registrable, "münchen.de");
        assert!(h.warning.is_none());
    }

    #[test]
    fn single_script_non_latin_idn_is_legit() {
        // All-Greek SLD under an ASCII TLD must not be flagged (per-label check).
        let url = format!("https://{}/", to_ascii("δοκιμή.gr"));
        let h = host_of(&url);
        assert!(h.warning.is_none(), "all-Greek label should be legit");
        assert_eq!(h.registrable, "δοκιμή.gr");
    }

    #[test]
    fn all_cyrillic_idn_is_legit() {
        // All-Cyrillic label + Cyrillic TLD (.рф): legit, no warning.
        let url = format!("https://{}/", to_ascii("почта.рф"));
        let h = host_of(&url);
        assert!(h.warning.is_none(), "all-Cyrillic should be legit");
    }

    #[test]
    fn mixed_script_lookalike_is_flagged() {
        // аpple.com with a Cyrillic 'а' — a homograph attack.
        let punycode = to_ascii("аpple.com");
        let url = format!("https://{punycode}/login");
        let h = host_of(&url);
        let w = h.warning.expect("mixed-script host must warn");
        assert_eq!(w.displays_as, "аpple.com");
        assert_eq!(w.real, punycode);
        // The URL shows the punycode, not the deceptive Unicode.
        assert_eq!(h.registrable, punycode);
    }

    #[test]
    fn a_hostless_scheme_names_itself_on_the_card() {
        let v = parse_uri("mailto:hi@example.com");
        assert_eq!(v.scheme, "mailto");
        assert!(v.host.is_none());
        assert_eq!(v.recipients().next().unwrap().value, "hi@example.com");
        // With no host there is no domain to name the link by, so the scheme
        // stands in on the button, the tab title, and the share card.
        assert_eq!(v.card_domain(), "mailto");
    }

    // ----------------------------------------------------------------------
    // The parts model
    // ----------------------------------------------------------------------

    /// Every shape the card can be handed, in one list, so the reassembly
    /// invariant is checked against all of them at once.
    const SPECIMENS: &[&str] = &[
        "https://blog.example.com/articles/2026/preview-design?ref=newsletter",
        "https://alice@login.example.co.uk:8443/reset;sid=9f2c?next=https%3A%2F%2Fexample-mail.com%2F&q=hello%20world#step-2",
        "http://example.com/pay",
        "https://example.com/",
        "https://example.com/cb#access_token=abc123&expires_in=3600",
        "https://example.com/a;x=1/b;lang=en/c",
        "mailto:ella.storli@gmail.com?subject=Order%204192&body=call me",
        "mailto:sales@example.com,support@example.com?cc=archive@records.example&subject=Order%204192",
        "mailto:?subject=Hello",
        "tel:+47-820-12-345;ext=4021",
        "sms:+4799123456?body=JOIN%20LIST",
        "sms:+46701234567,+4782012345?body=JOIN%20LIST",
        "magnet:?xt=urn:btih:c12fe3a94b81d7e05f2c6a9048bb3e1d7f4a2c60&dn=ubuntu.iso&tr=udp%3A%2F%2Ftracker.example.org%3A6969",
        "spotify:track:6rqhFgbbKwnb9MLmUQDhG6",
        "xmpp:lobby@rooms.example.org?join",
        "matrix:r/keebs:example.org",
        "ircs://libera.chat/yuiolink",
        "ftp://files.example.org/pub/notes.txt",
        "javascript:alert(document.cookie)",
    ];

    /// The invariant the whole card rests on: prefix plus every slice's raw
    /// characters, in order, IS the stored string. If this ever fails, the
    /// "Exactly as stored" line is no longer exact.
    #[test]
    fn slices_reassemble_the_stored_string() {
        for stored in SPECIMENS {
            let v = parse_uri(stored);
            assert_eq!(&v.raw(), stored, "reassembly differs for {stored}");
        }
    }

    /// Keeping everything must also be a no-op through the rebuilder, which is
    /// the path the browser takes on every checkbox change.
    #[test]
    fn rebuilding_with_nothing_removed_is_the_stored_string() {
        for stored in SPECIMENS {
            let v = parse_uri(stored);
            assert_eq!(
                &rebuild(&v, |_| true),
                stored,
                "rebuild differs for {stored}"
            );
        }
    }

    #[test]
    fn tiers_follow_the_allowlist() {
        assert_eq!(tier_of("https"), Tier::Web);
        assert_eq!(tier_of("http"), Tier::Web);
        for s in [
            "mailto", "tel", "sms", "magnet", "spotify", "xmpp", "matrix", "irc", "ircs", "ftp",
            "ftps",
        ] {
            assert_eq!(tier_of(s), Tier::Handoff, "{s}");
        }
        for s in ["javascript", "data", "vbscript", "file"] {
            assert_eq!(tier_of(s), Tier::Refused, "{s}");
        }
    }

    #[test]
    fn web_url_splits_into_the_parts_the_card_lists() {
        let v = parse_uri(SPECIMENS[1]);
        let roles: Vec<Role> = v.slices.iter().map(|s| s.role).collect();
        assert_eq!(
            roles,
            vec![
                Role::Userinfo,
                Role::Host,
                Role::Port,
                Role::Path,
                Role::PathParam,
                Role::Query,
                Role::Query,
                Role::Fragment,
            ]
        );
        // Today's render_url drops userinfo and port entirely; the parts model
        // is where both come back.
        assert_eq!(v.slices[0].raw(), "alice@");
        assert_eq!(v.slices[2].raw(), ":8443");
        // Host and port identify the resource; the tails ride along.
        assert!(!v.slices[1].removable && !v.slices[2].removable);
        assert!(!v.slices[3].removable, "the path is the destination");
        assert!(v.slices[4].removable && v.slices[7].removable);
        // The host never gets a row of its own -- it is the headline, and it
        // arrives already split at the registrable boundary.
        assert!(!v.slices[1].is_row());
        let h = v.host.as_ref().expect("a hierarchical URL has a host");
        assert_eq!(h.registrable, "example.co.uk");
        assert_eq!(h.subdomain, "login");
        // A hostless scheme has none, and says so.
        assert!(parse_uri("mailto:a@b.example").host.is_none());
    }

    #[test]
    fn repeated_query_keys_are_kept_in_order() {
        let v = parse_uri("https://example.com/?tag=a&tag=b&tag=c");
        let keys: Vec<&str> = v
            .slices
            .iter()
            .filter(|s| s.role == Role::Query)
            .map(|s| s.key.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(keys, vec!["tag", "tag", "tag"]);
        let values: Vec<&str> = v
            .slices
            .iter()
            .filter(|s| s.role == Role::Query)
            .map(|s| s.value.as_str())
            .collect();
        assert_eq!(values, vec!["a", "b", "c"]);
    }

    #[test]
    fn each_path_parameter_brings_its_own_semicolon() {
        let v = parse_uri("https://example.com/a;x=1/b;lang=en/c");
        let params: Vec<String> = v
            .slices
            .iter()
            .filter(|s| s.role == Role::PathParam)
            .map(Slice::raw)
            .collect();
        assert_eq!(params, vec![";x=1", ";lang=en"]);
    }

    #[test]
    fn an_oauth_shaped_fragment_unrolls_but_a_plain_one_does_not() {
        let v = parse_uri("https://example.com/cb#access_token=abc123&expires_in=3600");
        let frags: Vec<String> = v
            .slices
            .iter()
            .filter(|s| s.role == Role::Fragment)
            .map(Slice::raw)
            .collect();
        assert_eq!(frags, vec!["#access_token=abc123", "&expires_in=3600"]);

        let plain = parse_uri("https://example.com/x#step-2");
        let one: Vec<String> = plain
            .slices
            .iter()
            .filter(|s| s.role == Role::Fragment)
            .map(Slice::raw)
            .collect();
        assert_eq!(one, vec!["#step-2"]);
        // Commas get no special treatment: `#a=b,c=d` is one pair.
        let commas = parse_uri("https://example.com/x#a=b,c=d");
        assert_eq!(commas.slices.last().unwrap().value, "b,c=d");
    }

    #[test]
    fn mailto_splits_recipients_and_hfields_and_never_locks() {
        let v = parse_uri(SPECIMENS[7]);
        let to: Vec<&str> = v.recipients().map(|s| s.value.as_str()).collect();
        assert_eq!(to, vec!["sales@example.com", "support@example.com"]);
        // The second recipient wears its true comma.
        assert_eq!(v.recipients().nth(1).unwrap().raw(), ",support@example.com");
        // The Draft rule: every address is removable, sole ones included.
        assert!(v.recipients().all(|s| s.removable));
        let single = parse_uri(SPECIMENS[6]);
        assert!(single.recipients().all(|s| s.removable));
        // Dropping every recipient still leaves a legal mailto.
        let drafted = rebuild(&single, |i| single.slices[i].role != Role::Recipient);
        assert_eq!(drafted, "mailto:?subject=Order%204192&body=call me");
        // An address-less mailto is legal on the way in, too.
        assert_eq!(parse_uri(SPECIMENS[8]).recipients().count(), 0);
    }

    #[test]
    fn sms_splits_recipients_but_tel_never_does() {
        let v = parse_uri(SPECIMENS[11]);
        let to: Vec<&str> = v.recipients().map(|s| s.value.as_str()).collect();
        assert_eq!(to, vec!["+46701234567", "+4782012345"]);
        assert_eq!(v.param("body").unwrap().value, "JOIN%20LIST");
        // RFC 3966 has no list form, so a tel: number is one opaque body with
        // its dialling parameters, and none of it is a removable row.
        let t = parse_uri(SPECIMENS[9]);
        assert_eq!(t.recipients().count(), 0);
        assert_eq!(t.slices[0].value, "+47-820-12-345");
        assert_eq!(t.slices[1].raw(), ";ext=4021");
        assert!(t.slices.iter().all(|s| !s.removable));
    }

    #[test]
    fn magnet_fixes_xt_and_lists_every_tracker() {
        let v = parse_uri(SPECIMENS[12]);
        let xt = v.param("xt").unwrap();
        assert!(!xt.removable, "only xt identifies the data");
        assert!(v.param("dn").unwrap().removable);
        let trackers: Vec<&Slice> = v
            .slices
            .iter()
            .filter(|s| s.key.as_deref() == Some("tr"))
            .collect();
        assert!(!trackers.is_empty() && trackers.iter().all(|s| s.removable));
    }

    #[test]
    fn type_segments_come_from_the_uri_itself() {
        assert_eq!(
            parse_uri(SPECIMENS[13]).type_segment.as_deref(),
            Some("track")
        );
        assert_eq!(parse_uri(SPECIMENS[15]).type_segment.as_deref(), Some("r"));
        // MSC2312 deprecated the long forms but they are still in the wild:
        // accepted when classifying, never generated.
        assert_eq!(
            parse_uri("matrix:room/keebs:example.org")
                .type_segment
                .as_deref(),
            Some("r")
        );
        assert_eq!(
            parse_uri("matrix:user/ada:example.org")
                .type_segment
                .as_deref(),
            Some("u")
        );
        assert_eq!(
            parse_uri("matrix:event/x:example.org")
                .type_segment
                .as_deref(),
            Some("e")
        );
        // xmpp says "room" with a bare ?join key and no value at all.
        let x = parse_uri(SPECIMENS[14]);
        let join = x.param("join").unwrap();
        assert!(!join.equals, "?join has no '=' and we must not invent one");
        assert!(join.removable);
    }

    #[test]
    fn a_value_that_is_itself_a_uri_gets_its_structure_back() {
        // One palette across the URL line, the slices, and the exact line
        // (round 18), so a tracker in a magnet reads as the address it is.
        let v = parse_uri("magnet:?xt=urn:btih:abc&tr=udp%3A%2F%2Ftracker.example.org%3A6969");
        let tr = v
            .slices
            .iter()
            .find(|s| s.key.as_deref() == Some("tr"))
            .unwrap();
        assert!(tr.display.contains(&Piece::Delim("://".to_string())));
        // Bold marks the registrable domain, not the whole host, exactly as it
        // does on the URL line above.
        assert!(tr.display.contains(&Piece::Text("tracker.".to_string())));
        assert!(
            tr.display
                .contains(&Piece::Domain("example.org".to_string()))
        );
        assert!(tr.display.contains(&Piece::Delim(":".to_string())));
        // A tracker is the structure a magnet is made of, not a hazard: no chip.
        assert!(!v.has(Hazard::CarriesAnotherAddress));
        // Reading it changed nothing but the escapes, so the pieces still spell
        // the decoded value exactly.
        let read: String = tr.display.iter().map(Piece::text).collect();
        assert_eq!(read, "udp://tracker.example.org:6969");

        // The same treatment on the web tier, where it DOES earn the chip.
        let w = parse_uri("https://example.com/r?next=https%3A%2F%2Fother.example%2Fx");
        let next = w.param("next").unwrap();
        assert!(
            next.display
                .contains(&Piece::Domain("other.example".to_string()))
        );
        assert!(w.has(Hazard::CarriesAnotherAddress));

        // A value that only looks a bit like one is left alone.
        let plain = parse_uri("https://example.com/?q=not://a-uri");
        let q = plain.param("q").unwrap();
        assert_eq!(
            q.display.iter().map(Piece::text).collect::<String>(),
            "not://a-uri"
        );
    }

    #[test]
    fn mailto_header_addresses_read_like_the_recipients_beside_them() {
        let v = parse_uri("mailto:a@b.example?cc=archive@records.example,two@b.example&subject=Hi");
        let cc = v.param("cc").unwrap();
        assert!(cc.display.contains(&Piece::Local("archive".to_string())));
        assert!(
            cc.display
                .contains(&Piece::Domain("records.example".to_string()))
        );
        // A comma-joined cc keeps its comma as a delimiter, like the to-list.
        assert!(cc.display.contains(&Piece::Delim(",".to_string())));
        assert_eq!(
            cc.display.iter().map(Piece::text).collect::<String>(),
            "archive@records.example,two@b.example"
        );
        // An ordinary hfield is still ordinary text.
        let subject = v.param("subject").unwrap();
        assert_eq!(subject.display, vec![Piece::Text("Hi".to_string())]);
    }

    #[test]
    fn structure_characters_stay_escaped_but_everything_else_reads() {
        let pieces = decode_for_reading("a%20b%2Fc%26d%3De");
        let read: String = pieces.iter().map(Piece::text).collect();
        // `/` decodes; `&` and `=` do not, because decoding them would redraw
        // the URI's structure on screen.
        assert_eq!(read, "a b/c%26d%3De");
        assert!(pieces.contains(&Piece::DecodedSpace));
        assert!(pieces.iter().any(|p| matches!(p, Piece::Escape(_))));
    }

    #[test]
    fn a_stored_space_and_an_escaped_one_read_differently() {
        // %20 leaves a receipt; a real stored space does not, because nothing
        // was translated. Only the cannot-be-a-base schemes can store one.
        assert!(decode_for_reading("call%20me").contains(&Piece::DecodedSpace));
        assert_eq!(
            decode_for_reading("call me"),
            vec![Piece::Text("call me".to_string())]
        );
    }

    #[test]
    fn space_runs_and_edge_spaces_are_padding_but_one_interior_space_is_not() {
        assert!(parse_uri("mailto:a@b.example?subject=Order  4192").has(Hazard::PaddedWithSpaces));
        assert!(parse_uri("mailto:a@b.example?subject=%20Order").has(Hazard::PaddedWithSpaces));
        assert!(parse_uri("mailto:a@b.example?subject=Order%20").has(Hazard::PaddedWithSpaces));
        assert!(
            !parse_uri("mailto:a@b.example?subject=Order%204192").has(Hazard::PaddedWithSpaces),
            "one interior space is ordinary English"
        );
    }

    #[test]
    fn escapes_that_decode_to_invisibles_stay_escaped_and_warn() {
        // A right-to-left override, the classic filename-spoofing character.
        let v = parse_uri("mailto:a@b.example?subject=safe%E2%80%AEevil");
        assert!(v.has(Hazard::HiddenCharacters));
        let s = v.param("subject").unwrap();
        assert!(s.display.iter().any(|p| matches!(p, Piece::BadEscape(_))));
        // The escape is still on screen as an escape, so a copy is honest.
        let read: String = s.display.iter().map(Piece::text).collect();
        assert!(read.contains("%E2%80%AE"), "{read}");
        // Ordinary escapes are not hidden characters.
        assert!(!parse_uri("https://example.com/a%20b%2Fc").has(Hazard::HiddenCharacters));
    }

    #[test]
    fn the_string_hazards_open_the_fold_but_the_transport_one_does_not() {
        let plain_http = parse_uri("http://example.com/pay");
        assert!(plain_http.has(Hazard::NotEncrypted));
        assert!(
            !plain_http.warns_about_the_string(),
            "http is about the transport, not the parts"
        );
        assert!(
            !plain_http.fold_is_worth_it(),
            "a bare path has nothing to add"
        );

        let busy = parse_uri(SPECIMENS[1]);
        assert!(busy.has(Hazard::UsernameInTheAddress));
        assert!(busy.has(Hazard::CarriesAnotherAddress));
        assert!(busy.warns_about_the_string());
        assert!(busy.fold_is_worth_it());
    }

    #[test]
    fn the_exact_line_appears_only_when_reading_changed_something() {
        // Nothing decoded: the URL line already IS the stored string.
        assert!(!parse_uri(SPECIMENS[0]).decoding_changed_anything());
        // A %20 in a value reads as a space, so the record needs saying.
        assert!(parse_uri("https://example.com/s?q=hello%20world").decoding_changed_anything());
    }

    #[test]
    fn dropping_a_part_promotes_the_delimiter_that_follows_it() {
        let v = parse_uri(SPECIMENS[1]);
        // Drop `?next=...` and the next pair's `&` has to become the `?`.
        let out = rebuild(&v, |i| v.slices[i].key.as_deref() != Some("next"));
        assert_eq!(
            out,
            "https://alice@login.example.co.uk:8443/reset;sid=9f2c?q=hello%20world#step-2"
        );
        // Drop the first recipient and the comma promotes away.
        let m = parse_uri(SPECIMENS[7]);
        let out = rebuild(&m, |i| m.slices[i].value != "sales@example.com");
        assert_eq!(
            out,
            "mailto:support@example.com?cc=archive@records.example&subject=Order%204192"
        );
        // Fixed parts survive being asked to go.
        let out = rebuild(&v, |_| false);
        assert_eq!(out, "https://login.example.co.uk:8443/reset");
    }

    #[test]
    fn a_refused_scheme_still_parses_into_something_printable() {
        let v = parse_uri("javascript:alert(document.cookie)");
        assert_eq!(v.tier, Tier::Refused);
        assert_eq!(v.raw(), "javascript:alert(document.cookie)");
        // Nothing about it is removable, so there is nothing to offer either.
        assert!(v.slices.iter().all(|s| !s.removable));
    }
}
