//! Classify a `tel:` or `sms:` number against Google's published numbering
//! plans, via the Rust port of libphonenumber.
//!
//! The crate is here for [`Class`], not for prettiness. `820` means nothing to
//! a reader outside Norway and everything to the tables: it is a premium-rate
//! range, the one real hazard a phone URI can carry, and nothing else in this
//! stack can see it. Formatting comes along for free, and the card takes it
//! verbatim -- every region's separators (Sweden's hyphen, France's pairs,
//! Germany's long run) are metadata Google maintains against each country's
//! numbering plan, so there is not a line of per-country code here.
//!
//! What the card says about a number is therefore always a published fact or
//! the stored characters. A number that does not parse is shown exactly as it
//! was stored, with no chips at all -- a guess would be worse than silence.

use phonenumber::{Mode, Type, metadata::DATABASE};

/// A number, read for display.
pub struct Number {
    /// libphonenumber's international format, verbatim, or the stored
    /// characters when the number does not parse.
    pub headline: String,
    /// The `+47`, when the number parsed. Split out because a stack of numbers
    /// aligns at the country-code seam.
    pub country_code: Option<String>,
    /// Everything after the country code, dressed in its region's own
    /// punctuation. The whole headline when there is no country code.
    pub national: String,
    /// The region the number belongs to, for the flag-and-name chip.
    pub region: Option<Region>,
    /// What kind of line it is, where the tables can say.
    pub class: Option<Class>,
}

/// A region, named and flagged.
pub struct Region {
    /// The flag as regional-indicator characters. Degrades to the letter pair
    /// on platforms with no flag glyphs, which is exactly the code again -- and
    /// the name beside it carries the content either way.
    pub flag: String,
    pub name: &'static str,
}

/// What the numbering plan says this number costs or is. Only the classes a
/// reader can act on are named; the rest read as an ordinary number and get no
/// chip, because a chip that says nothing is noise.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// Calling or texting it bills the caller at a raised rate. The one real
    /// hazard in a phone URI, and the reason this dependency exists.
    PremiumRate,
    TollFree,
    Mobile,
    FixedLine,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::PremiumRate => "Premium Rate",
            Class::TollFree => "Toll Free",
            Class::Mobile => "Mobile",
            Class::FixedLine => "Fixed Line",
        }
    }

    /// True for the one class that is a warning rather than a fact. Warnings
    /// wear bare red words; facts keep the pill.
    pub fn is_warning(self) -> bool {
        self == Class::PremiumRate
    }
}

/// One run of a formatted number, so the card can recede what routes and bold
/// what identifies.
pub enum Part {
    /// The digits themselves.
    Digits(String),
    /// Whatever the region's format pattern put between them.
    Separator(String),
}

/// Read a stored number.
///
/// `stored` is the URI's own characters -- `+47-820-12-345`, or a national
/// number as the creator typed it. Parsing is attempted with no default region:
/// a link is read by strangers in other countries, so a number that does not
/// say which country it belongs to cannot be assigned one on their behalf.
pub fn read(stored: &str) -> Number {
    let Ok(parsed) = phonenumber::parse(None, stored) else {
        return Number {
            headline: stored.to_string(),
            country_code: None,
            national: stored.to_string(),
            region: None,
            class: None,
        };
    };
    let headline = parsed.format().mode(Mode::International).to_string();
    // The international format is always `+CC ` followed by the national part.
    let (country_code, national) = match headline.split_once(' ') {
        Some((cc, rest)) if cc.starts_with('+') => (Some(cc.to_string()), rest.to_string()),
        _ => (None, headline.clone()),
    };
    let region = parsed.country().id().and_then(|id| region_for(id.as_ref()));
    // Classification needs the number to actually be valid for its plan --
    // otherwise every unparseable prefix would pick up whatever range it
    // happens to fall in.
    let class = parsed
        .is_valid()
        .then(|| class_of(parsed.number_type(&DATABASE)))
        .flatten();
    Number {
        headline,
        country_code,
        national,
        region,
        class,
    }
}

/// Cut a formatted national number into digit runs and the separators its
/// region's tables chose. Nothing is inserted or removed -- concatenating the
/// parts gives the string back.
pub fn parts(national: &str) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    for c in national.chars() {
        let digit = c.is_ascii_digit();
        match out.last_mut() {
            Some(Part::Digits(s)) if digit => s.push(c),
            Some(Part::Separator(s)) if !digit => s.push(c),
            _ => out.push(if digit {
                Part::Digits(c.to_string())
            } else {
                Part::Separator(c.to_string())
            }),
        }
    }
    out
}

fn class_of(t: Type) -> Option<Class> {
    match t {
        Type::PremiumRate => Some(Class::PremiumRate),
        Type::TollFree => Some(Class::TollFree),
        Type::Mobile => Some(Class::Mobile),
        Type::FixedLine => Some(Class::FixedLine),
        // FixedLineOrMobile, VoiceMail, Voip, Pager, SharedCost, PersonalNumber,
        // Uan, ShortCode, Unknown: nothing a reader would act on differently.
        _ => None,
    }
}

fn region_for(code: &str) -> Option<Region> {
    let i = REGION_NAMES.binary_search_by_key(&code, |(c, _)| c).ok()?;
    let (code, name) = REGION_NAMES[i];
    Some(Region {
        flag: flag_of(code),
        name,
    })
}

/// A flag from a region code: each ASCII letter maps to its regional indicator
/// symbol, which is how every platform composes flag emoji. The user's explicit
/// exception to the site's no-emoji rule -- a flag is a region's own mark, and
/// where the glyphs are missing it falls back to reading as the letters.
fn flag_of(code: &str) -> String {
    code.bytes()
        .filter(|b| b.is_ascii_uppercase())
        .filter_map(|b| char::from_u32(0x1f1e6 + u32::from(b - b'A')))
        .collect()
}

/// ISO 3166-1 alpha-2 to an English name, for every region libphonenumber
/// knows. Sorted, so the lookup is a binary search.
///
/// Names come from the time zone database's public-domain `iso3166.tab`, with a
/// handful re-read for a reader rather than a sorter ("Korea (South)" ->
/// "South Korea") and the non-ASCII ones transliterated, since this is code.
const REGION_NAMES: &[(&str, &str)] = &[
    ("AC", "Ascension Island"),
    ("AD", "Andorra"),
    ("AE", "United Arab Emirates"),
    ("AF", "Afghanistan"),
    ("AG", "Antigua & Barbuda"),
    ("AI", "Anguilla"),
    ("AL", "Albania"),
    ("AM", "Armenia"),
    ("AO", "Angola"),
    ("AR", "Argentina"),
    ("AS", "American Samoa"),
    ("AT", "Austria"),
    ("AU", "Australia"),
    ("AW", "Aruba"),
    ("AX", "Aland Islands"),
    ("AZ", "Azerbaijan"),
    ("BA", "Bosnia & Herzegovina"),
    ("BB", "Barbados"),
    ("BD", "Bangladesh"),
    ("BE", "Belgium"),
    ("BF", "Burkina Faso"),
    ("BG", "Bulgaria"),
    ("BH", "Bahrain"),
    ("BI", "Burundi"),
    ("BJ", "Benin"),
    ("BL", "Saint Barthelemy"),
    ("BM", "Bermuda"),
    ("BN", "Brunei"),
    ("BO", "Bolivia"),
    ("BQ", "Caribbean NL"),
    ("BR", "Brazil"),
    ("BS", "Bahamas"),
    ("BT", "Bhutan"),
    ("BW", "Botswana"),
    ("BY", "Belarus"),
    ("BZ", "Belize"),
    ("CA", "Canada"),
    ("CC", "Cocos (Keeling) Islands"),
    ("CD", "DR Congo"),
    ("CF", "Central African Rep."),
    ("CG", "Republic of the Congo"),
    ("CH", "Switzerland"),
    ("CI", "Ivory Coast"),
    ("CK", "Cook Islands"),
    ("CL", "Chile"),
    ("CM", "Cameroon"),
    ("CN", "China"),
    ("CO", "Colombia"),
    ("CR", "Costa Rica"),
    ("CU", "Cuba"),
    ("CV", "Cape Verde"),
    ("CW", "Curacao"),
    ("CX", "Christmas Island"),
    ("CY", "Cyprus"),
    ("CZ", "Czech Republic"),
    ("DE", "Germany"),
    ("DJ", "Djibouti"),
    ("DK", "Denmark"),
    ("DM", "Dominica"),
    ("DO", "Dominican Republic"),
    ("DZ", "Algeria"),
    ("EC", "Ecuador"),
    ("EE", "Estonia"),
    ("EG", "Egypt"),
    ("EH", "Western Sahara"),
    ("ER", "Eritrea"),
    ("ES", "Spain"),
    ("ET", "Ethiopia"),
    ("FI", "Finland"),
    ("FJ", "Fiji"),
    ("FK", "Falkland Islands"),
    ("FM", "Micronesia"),
    ("FO", "Faroe Islands"),
    ("FR", "France"),
    ("GA", "Gabon"),
    ("GB", "United Kingdom"),
    ("GD", "Grenada"),
    ("GE", "Georgia"),
    ("GF", "French Guiana"),
    ("GG", "Guernsey"),
    ("GH", "Ghana"),
    ("GI", "Gibraltar"),
    ("GL", "Greenland"),
    ("GM", "Gambia"),
    ("GN", "Guinea"),
    ("GP", "Guadeloupe"),
    ("GQ", "Equatorial Guinea"),
    ("GR", "Greece"),
    ("GT", "Guatemala"),
    ("GU", "Guam"),
    ("GW", "Guinea-Bissau"),
    ("GY", "Guyana"),
    ("HK", "Hong Kong"),
    ("HN", "Honduras"),
    ("HR", "Croatia"),
    ("HT", "Haiti"),
    ("HU", "Hungary"),
    ("ID", "Indonesia"),
    ("IE", "Ireland"),
    ("IL", "Israel"),
    ("IM", "Isle of Man"),
    ("IN", "India"),
    ("IO", "British Indian Ocean Territory"),
    ("IQ", "Iraq"),
    ("IR", "Iran"),
    ("IS", "Iceland"),
    ("IT", "Italy"),
    ("JE", "Jersey"),
    ("JM", "Jamaica"),
    ("JO", "Jordan"),
    ("JP", "Japan"),
    ("KE", "Kenya"),
    ("KG", "Kyrgyzstan"),
    ("KH", "Cambodia"),
    ("KI", "Kiribati"),
    ("KM", "Comoros"),
    ("KN", "Saint Kitts and Nevis"),
    ("KP", "North Korea"),
    ("KR", "South Korea"),
    ("KW", "Kuwait"),
    ("KY", "Cayman Islands"),
    ("KZ", "Kazakhstan"),
    ("LA", "Laos"),
    ("LB", "Lebanon"),
    ("LC", "Saint Lucia"),
    ("LI", "Liechtenstein"),
    ("LK", "Sri Lanka"),
    ("LR", "Liberia"),
    ("LS", "Lesotho"),
    ("LT", "Lithuania"),
    ("LU", "Luxembourg"),
    ("LV", "Latvia"),
    ("LY", "Libya"),
    ("MA", "Morocco"),
    ("MC", "Monaco"),
    ("MD", "Moldova"),
    ("ME", "Montenegro"),
    ("MF", "Saint Martin"),
    ("MG", "Madagascar"),
    ("MH", "Marshall Islands"),
    ("MK", "North Macedonia"),
    ("ML", "Mali"),
    ("MM", "Myanmar"),
    ("MN", "Mongolia"),
    ("MO", "Macau"),
    ("MP", "Northern Mariana Islands"),
    ("MQ", "Martinique"),
    ("MR", "Mauritania"),
    ("MS", "Montserrat"),
    ("MT", "Malta"),
    ("MU", "Mauritius"),
    ("MV", "Maldives"),
    ("MW", "Malawi"),
    ("MX", "Mexico"),
    ("MY", "Malaysia"),
    ("MZ", "Mozambique"),
    ("NA", "Namibia"),
    ("NC", "New Caledonia"),
    ("NE", "Niger"),
    ("NF", "Norfolk Island"),
    ("NG", "Nigeria"),
    ("NI", "Nicaragua"),
    ("NL", "Netherlands"),
    ("NO", "Norway"),
    ("NP", "Nepal"),
    ("NR", "Nauru"),
    ("NU", "Niue"),
    ("NZ", "New Zealand"),
    ("OM", "Oman"),
    ("PA", "Panama"),
    ("PE", "Peru"),
    ("PF", "French Polynesia"),
    ("PG", "Papua New Guinea"),
    ("PH", "Philippines"),
    ("PK", "Pakistan"),
    ("PL", "Poland"),
    ("PM", "Saint Pierre and Miquelon"),
    ("PR", "Puerto Rico"),
    ("PS", "Palestine"),
    ("PT", "Portugal"),
    ("PW", "Palau"),
    ("PY", "Paraguay"),
    ("QA", "Qatar"),
    ("RE", "Reunion"),
    ("RO", "Romania"),
    ("RS", "Serbia"),
    ("RU", "Russia"),
    ("RW", "Rwanda"),
    ("SA", "Saudi Arabia"),
    ("SB", "Solomon Islands"),
    ("SC", "Seychelles"),
    ("SD", "Sudan"),
    ("SE", "Sweden"),
    ("SG", "Singapore"),
    ("SH", "Saint Helena"),
    ("SI", "Slovenia"),
    ("SJ", "Svalbard & Jan Mayen"),
    ("SK", "Slovakia"),
    ("SL", "Sierra Leone"),
    ("SM", "San Marino"),
    ("SN", "Senegal"),
    ("SO", "Somalia"),
    ("SR", "Suriname"),
    ("SS", "South Sudan"),
    ("ST", "Sao Tome & Principe"),
    ("SV", "El Salvador"),
    ("SX", "Sint Maarten"),
    ("SY", "Syria"),
    ("SZ", "Eswatini"),
    ("TA", "Tristan da Cunha"),
    ("TC", "Turks & Caicos Is"),
    ("TD", "Chad"),
    ("TG", "Togo"),
    ("TH", "Thailand"),
    ("TJ", "Tajikistan"),
    ("TK", "Tokelau"),
    ("TL", "East Timor"),
    ("TM", "Turkmenistan"),
    ("TN", "Tunisia"),
    ("TO", "Tonga"),
    ("TR", "Turkey"),
    ("TT", "Trinidad & Tobago"),
    ("TV", "Tuvalu"),
    ("TW", "Taiwan"),
    ("TZ", "Tanzania"),
    ("UA", "Ukraine"),
    ("UG", "Uganda"),
    ("US", "United States"),
    ("UY", "Uruguay"),
    ("UZ", "Uzbekistan"),
    ("VA", "Vatican City"),
    ("VC", "Saint Vincent and the Grenadines"),
    ("VE", "Venezuela"),
    ("VG", "British Virgin Islands"),
    ("VI", "US Virgin Islands"),
    ("VN", "Vietnam"),
    ("VU", "Vanuatu"),
    ("WF", "Wallis & Futuna"),
    ("WS", "Samoa"),
    ("XK", "Kosovo"),
    ("YE", "Yemen"),
    ("YT", "Mayotte"),
    ("ZA", "South Africa"),
    ("ZM", "Zambia"),
    ("ZW", "Zimbabwe"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_region_table_is_sorted_and_ascii() {
        assert!(
            REGION_NAMES.windows(2).all(|w| w[0].0 < w[1].0),
            "the table is binary-searched, so it has to stay sorted"
        );
        for (code, name) in REGION_NAMES {
            assert!(code.is_ascii() && name.is_ascii(), "{code} {name}");
            assert_eq!(code.len(), 2, "{code}");
        }
    }

    #[test]
    fn a_premium_rate_number_is_named_as_one() {
        // Norwegian 820 is a premium range. This is the whole case for the
        // dependency: nothing in the string says so, and the table does.
        let n = read("+47-820-12-345");
        assert_eq!(n.class, Some(Class::PremiumRate));
        assert!(n.class.unwrap().is_warning());
        assert_eq!(n.region.as_ref().unwrap().name, "Norway");
    }

    #[test]
    fn mobile_toll_free_and_fixed_line_are_facts_not_warnings() {
        let mobile = read("+46701234567");
        assert_eq!(mobile.class, Some(Class::Mobile));
        assert_eq!(mobile.region.unwrap().name, "Sweden");
        assert!(!Class::Mobile.is_warning());

        let toll_free = read("+18002223333");
        assert_eq!(toll_free.class, Some(Class::TollFree));

        let fixed = read("+33142685300");
        assert_eq!(fixed.class, Some(Class::FixedLine));
        assert_eq!(fixed.region.unwrap().name, "France");
    }

    #[test]
    fn every_region_dresses_its_own_number() {
        // No per-country code here: the separators are libphonenumber's, and
        // the card prints what it is handed.
        assert_eq!(read("+46701234567").headline, "+46 70 123 45 67");
        // Norway groups a mobile number in pairs and an eight-digit service
        // number in threes -- both from the same tables, neither from us.
        assert_eq!(read("+4799123456").headline, "+47 99 12 34 56");
        assert_eq!(read("+4782012345").headline, "+47 820 12 345");
        assert_eq!(read("+14155552671").headline, "+1 415-555-2671");
        assert_eq!(read("+33142685300").headline, "+33 1 42 68 53 00");
    }

    #[test]
    fn the_country_code_splits_off_for_the_stack() {
        let n = read("+47-820-12-345");
        assert_eq!(n.country_code.as_deref(), Some("+47"));
        assert_eq!(n.national, "820 12 345");
        // The parts put it back together unchanged.
        let rebuilt: String = parts(&n.national)
            .iter()
            .map(|p| match p {
                Part::Digits(s) | Part::Separator(s) => s.as_str(),
            })
            .collect();
        assert_eq!(rebuilt, n.national);
    }

    #[test]
    fn a_number_that_does_not_parse_is_shown_exactly_as_stored() {
        let n = read("not-a-number");
        assert_eq!(n.headline, "not-a-number");
        assert!(n.region.is_none() && n.class.is_none());
        // A national number with no country in it cannot be assigned one on a
        // stranger's behalf, so it stays as typed and says nothing.
        let n = read("5551234");
        assert!(n.class.is_none());
    }

    #[test]
    fn a_flag_is_the_region_code_in_regional_indicators() {
        assert_eq!(flag_of("NO"), "\u{1f1f3}\u{1f1f4}");
        assert_eq!(flag_of("SE"), "\u{1f1f8}\u{1f1ea}");
    }
}
