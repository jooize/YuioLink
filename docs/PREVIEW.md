# The preview page

Status: **implemented**. Route reference: `docs/ROUTES.md`. Pixel reference:
`design/preview-all-schemes.html` (the full-cast gallery) with
`design/preview-flags-and-flashes.html` for multi-number stacks. The decision
log behind every rule here is `.agents-work/20260811-preview-scheme-design/`.

## Why the page exists

YuioLink names are short, case-insensitive words that **recycle** after a link
expires. A blind redirect therefore cannot be trusted: the same name may point
somewhere new next month, and a prefetch bot could burn a one-time link before
its recipient ever sees it. So every link previews first, and nothing is spent
by looking.

The page has one job beyond that: **tell the truth about the stored string,
without pretending to know what the reader's machine will do with it.**
Everything below follows from those two sentences.

---

## 1. Three tiers, by consequence

| Tier | Schemes | What the card offers |
|------|---------|----------------------|
| Web | `http`, `https` | The URL line and one amber **Continue to `<domain>`** anchor. |
| Handoff | `mailto`, `tel`, `sms`, `ftp`, `ftps`, `magnet`, `spotify`, `xmpp`, `irc`, `ircs`, `matrix` | A two-line amber button — a lead verb over the scheme's own definition — and one hedge line beneath it. |
| Refused | anything else (`javascript`, `data`, `vbscript`, …) | The string, printed inert. No control at all — not a disabled one. |

**Tier 3 is a render-time decision.** `views::is_linkable` runs where the markup
is written, on the preview page *and* on the revealed page, so an off-allowlist
scheme can never be emitted as an `href` anywhere. The refusal reads:

> **An Instruction, Not an Address**
> What is stored here would tell your browser to do something rather than go
> somewhere, so YuioLink shows it, gives it no button, and stops there.

**Describe the scheme, never predict the outcome.** We cannot see the reader's
device, so "opens your mail app" is a claim about an unseen machine; "An email
address" is a published fact about the string and stays true whatever happens.
Hence the one hedge, once, under every handoff button:

> What opens it, if anything, is up to your device.

The "if anything" is load-bearing — a `magnet:` with nothing registered does
nothing at all.

---

## 2. Three registers

| Register | Says | Element |
|----------|------|---------|
| Headline | what the link **is**, formatted for reading | `.pv-url`, `.pv-line`, `.pv-value`, `.pv-list`, `.pv-stack2` |
| Slices | what it **carries**, each row a verbatim cut | `.pv-slices` / `.pv-slice` |
| Exact line | what is **stored**, character for character | `.rawline` |

Every character of the stored string appears in at least one of them. The
headline may be formatted precisely *because* "Exactly as stored" sits
underneath — and where the headline already **is** the stored string, character
for character (`spotify:`, `matrix:`, `irc:`, `ftp:`), there is nothing left to
prove and no exact line appears at all. On an http(s) card the URL line is the
stored string too, so the exact line appears only when decoding for reading
changed something.

The exact line never collapses. It is the record.

### The invariant

`urlview::parse_uri` cuts the stored string into an ordered list of
[`Slice`]s. Concatenating `UriView::prefix` with every `Slice::raw()` in order
reproduces the stored string exactly. This is checked over nineteen specimens by
`slices_reassemble_the_stored_string`; if it ever fails, "Exactly as stored" has
stopped being exact.

### Decode for reading

Values decode, **except**:

- the four structure characters `& = # %`, which stay escaped and dim — decoding
  them would redraw the URI's structure on screen, and a `%26` shown as `&`
  reads as another parameter starting;
- anything invisible or direction-changing (bidi controls, zero-width, format
  characters), which stays escaped and red with its chip.

That leaves three appearances of a space, with no overlap:

| On screen | Means |
|-----------|-------|
| space with a dotted underline | it is `%20` in the stored link |
| dim `%NN` | it is still an escape |
| bare space | it really is a space (legal only in `mailto:`, `tel:`, `sms:`) |

---

## 3. Chips

**Facts keep the pill; warnings are bare red icon and words.** The contrast
carries the meaning.

Warnings, all provable from the string or a published table:

| Chip | Fires on |
|------|----------|
| Not Encrypted | plain `http`. Not "Not Secure" — this is the transport, not a verdict on the site. |
| Username in the Address | `alice@` before the host. |
| Hidden Characters | an escape decoding to something invisible or direction-changing. |
| Padded With Spaces | a run of two or more spaces, or one at either edge. A single interior space is ordinary English and stays silent. |
| Carries Another Address | a query value that is itself a complete web address. |
| Premium Rate | libphonenumber says the number bills at a raised rate. Fires for `sms:` as well as `tel:` — reverse-billed messaging is a real subscription trap. |

Facts: the region (flag + English name) and the line type (Mobile, Toll Free,
Fixed Line), both from libphonenumber's tables. On a card with several numbers
the facts move **beside their own number**, because a pooled row cannot say
which number a Premium Rate warning is about.

**There is no green.** Encryption is the norm; its absence is the signal. A
padlock on a phishing site is still a padlock, so "Encrypted" would be a verdict
we cannot stand behind.

---

## 4. The fold

http(s) keeps its quiet single-line page and offers the parts behind
`<details class="pv-parts">`. Every other scheme lists them outright.

- The fold — and, on the other schemes, the slice list itself — appears **only
  when it has something to add**: a part that can be unticked, or a value that
  reads differently from the way it is stored. A bare path never folds; a plain
  `ftp://` card and a `tel:` card show no rows at all.
- It arrives **open** when a warning about the *string* fired. "Not Encrypted"
  does not open it: that one is about the transport and says nothing about the
  parts.
- Warn chips always sit **outside** the fold. A warning that needs a click is a
  warning that was not made.
- The chevron is `::before` generated content, so a copy of the label never
  arrives in the clipboard as "Show".

---

## 5. Editing (`server/static/preview.js`)

Without the script the page is complete: every part listed, every value
readable, the destination a real link, the raw lines selectable text. What the
script adds is the ability to strip what rides along.

**Removal only.** Nothing in `preview.js` can add a character, so every rebuilt
string is a subset of the stored one and an allowlisted link cannot be edited
into one that would not have passed.

**Nothing is revealed or hidden on load.** Every element the script needs is one
it creates. The served markup has no checkboxes, no Copy buttons, and no split
segment — a dead control is worse than no control, and un-hiding one after load
is how this site earned its layout-shift history.

| Fixed (checked + disabled) | Removable |
|----------------------------|-----------|
| host, port, path, magnet `xt` | userinfo, path parameters, query parameters, fragment, every mailto address, sms body, magnet `dn`/`tr`, irc `?key`, ftp `;type` |

The path is fixed because the editing strips what rides *along* — trackers,
session ids, prefills — while the path **is** the destination. And nearly every
URL has one, so a removable path would put checkboxes on every everyday card.

**Floors, by grammar.** RFC 5724 requires at least one sms recipient, so the
last number standing locks. RFC 6068 requires none, so `mailto:` never locks:
empty the list and the button offers **"Draft a Message"** over a legal
`mailto:?subject=…`.

**Delimiter promotion.** Drop the first query pair and the next one's `&`
becomes the `?`; drop the first recipient and its comma goes with it. Path
parameters need no promotion — each brings its own `;`. The rule is stated twice
on purpose: `preview.js` runs it, and `urlview::rebuild` (in the tests) holds it
still.

The button follows the edits — the count, the surviving address, the xmpp
subtitle when `?join` goes — and all of it is still read off the URI's
structure.

**No HTML crosses the boundary.** The site's CSP carries
`require-trusted-types-for 'script'` with no policy allowed, so an `innerHTML`
assignment throws outright in browsers that enforce Trusted Types. The parts
model on `.pv-slices[data-card]` therefore ships `(class, text)` runs, and
`preview.js` builds elements and sets `textContent`.

**Copy is explicit, always.** There is no selection or clipboard interception
anywhere: selecting text does exactly what it looks like. The raw lines get Copy
pills; the split's blue segment copies what the button would open (edits
included) and then answers "copied what?" by *pointing* — the line it took from
lights the site's own green check.

---

## 6. One-time links are blind

A one-time card discloses nothing before its use is spent — not the path, not
the domain, not the scheme:

> The destination is shown when revealed.

Showing the domain would let anyone holding the link learn where it points
without burning the use, invisibly, when that burn is the whole tamper-evidence
a one-time link offers. `POST /:name/reveal` spends the use and the next page is
the **full card**, with the amber button waiting: you spend the use to *look*,
never to be thrown somewhere.

The **"Opens Once"** badge sits below the blue Reveal button in the create
picker's own purple, so the badge and the type segment that made the link are
visibly the same fact.

---

## 7. Flows

Unlimited redirect — one step, no POST at all:
```
GET /:name    -> the card, with Continue as a real <a href> to the destination
```
Following it never touches this server. That is also the CSP fix: `form-action
'self'` is applied to every redirect hop of a form submission in Chrome and
Safari, so the old `POST /:name/go` -> 303 was refused and the press went
nowhere. A link has nothing for `form-action` to block.

One-time redirect or text — two steps:
```
GET  /:name          -> blind card: "shown when revealed" + blue Reveal + Opens Once
POST /:name/reveal   -> spend the use, mint yl_reveal, 303 back to GET /:name
GET  /:name          -> with the cookie: the full card; the row is redacted and
                        the cookie expired on this same response
```

Unlimited text — no interstitial, nothing to vet:
```
GET /:name    -> the text, in an inert <pre>
```

### The reveal cookie

`POST /:name/reveal` mints a stateless HMAC token (`server/src/token.rs`, TTL 10
minutes) and sets `yl_reveal=<token>; Path=/<name>; HttpOnly; SameSite=Lax`
(+ `Secure` over HTTPS), then 303s back to `/:name`. It exists to survive
exactly one redirect hop — proving that this GET is the request that spent the
use — and `revealed_view` expires it (`Max-Age=0`) on the same response that
redacts the row, so its lifetime is one request.

The 10-minute TTL stays: a stalled radio or a sleeping phone can stretch the
hop, and the failure mode (use spent, 410, content never seen) is the worst on
the site.

---

## 8. Trust model: tombstone + immutability

A link's destination is immutable and its name is reserved until expiry. What
the preview shows is what the link is, for its whole stated life. It can degrade
to "gone", but never silently become a different live destination.

| State | Condition | Response |
|-------|-----------|----------|
| Live | not past `expires_at`, uses left, not withdrawn | 200 card (or immediate text) |
| Used up | `uses >= max_uses` | **410 Gone** |
| Withdrawn | creator deleted it, not yet expired | **410 Gone** |
| Expired / unknown / recycled | reaped or never existed | **404 Not Found** |

410 and 404 are not interchangeable: 410 says "this was a real link, now spent
or withdrawn", which is the *someone already opened it* signal for a one-time
link. Only the clock frees a name (`reap_expired`).

### Counting

`uses` (renamed from `hits` in migration `0006`) exists to gate a one-time link
and nothing else. **There is no per-link view counter anywhere**, so there is no
dedup question to answer and nothing about one link to leak. Previews and
reveals go to the aggregate `stats` table as the day-granular `previewed` and
`revealed` metrics, which is where the old counter's job now lives.

---

## 9. Share cards

The card `<head>` emits `og:site_name`, `og:title`, `og:description`,
`og:image` (-> `GET /:name/card.png`, rendered with `resvg` on
`spawn_blocking`), `twitter:card`, and `theme-color`. The card always shows the
destination domain, and fetching it spends no use — crawlers fetch it.

---

## 10. Tests

- Render-time refusal: `javascript:`/`data:` are printed and never linked, on
  the preview page **and** the revealed page (`server/src/web.rs`).
- Tier assignment per scheme; slice reassembly == stored; repeated query keys;
  per-segment path parameters; OAuth-shaped fragment unrolling; mailto/sms
  recipient splitting; delimiter promotion (`server/src/urlview.rs`).
- Space-run and edge-space padding; escapes decoding to invisibles; the
  structure characters staying escaped; decode-gating of the exact line.
- Phone classification fixtures: SE mobile, NO premium 820, US toll-free, FR
  fixed line, and a number that does not parse (`server/src/phone.rs`).
- Card composition per tier and per scheme, the hedge appearing once, notes
  appearing only where a standard opens a gap, and the served markup carrying
  none of the JS-injected controls (`server/src/views.rs`).
- Resolve classification, PRG, reveal-token forgery, cookie expiry, and a
  crawler simulation that spends nothing (`server/src/web.rs`).

---

## 11. Rejected, on purpose

Multi-use links; per-link view counters; consent or dedup cookies; a lid or
cover in the common case; key-column parameter tables; uppercase row tags;
italic key labels; green `https`; red action buttons; "Potentially Risky"
wording; selection or copy interception; a domain preview on one-time links;
removable path, port, or host; pooled chips on multi-number cards.

## 12. Known gap

`phonenumber` 0.3.10 compiles only `PhoneNumberMetadata.xml` into its database,
so `Type::ShortCode` never matches and premium-rate **SMS short codes** are not
detectable through it. Premium-rate ranges in ordinary numbers are, and the chip
fires for `sms:` as well as `tel:`.
