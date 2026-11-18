//! Page ranges for Split PDF (spec §4.5 item 2): "1-3, 5, 8-".
//! Mirrored in src/shared/pageRanges.ts; keep both rule sets identical.

/// One range; `end: None` means "to the last page".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    pub start: u32,
    pub end: Option<u32>,
}

pub const MAX_RANGES: usize = 200;

impl PageRange {
    /// qpdf page-range syntax: "1-3", "5", "8-z".
    pub fn qpdf(&self) -> String {
        match self.end {
            Some(e) if e == self.start => self.start.to_string(),
            Some(e) => format!("{}-{e}", self.start),
            None => format!("{}-z", self.start),
        }
    }

    /// Output file stem: "page-5", "pages-1-3", "pages-8-end".
    pub fn file_stem(&self) -> String {
        match self.end {
            Some(e) if e == self.start => format!("page-{}", self.start),
            Some(e) => format!("pages-{}-{e}", self.start),
            None => format!("pages-{}-end", self.start),
        }
    }
}

fn num(s: &str, whole: &str) -> Result<u32, String> {
    let bad = || format!("\"{whole}\" isn't a page number or range.");
    // Digits only: u32::from_str would also accept "+5".
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let n: u32 = s.parse().map_err(|_| bad())?;
    if n == 0 {
        return Err("Pages start at 1.".into());
    }
    Ok(n)
}

/// Parse "1-3, 5, 8-". `pages`: the document's page count when known.
/// Identical ranges are kept once (they would write the same file).
pub fn parse(s: &str, pages: Option<u32>) -> Result<Vec<PageRange>, String> {
    let mut out: Vec<PageRange> = Vec::new();
    for part in s.split(',') {
        let p: String = part.chars().filter(|c| !c.is_whitespace()).collect();
        if p.is_empty() {
            continue;
        }
        let r = match p.split_once('-') {
            None => {
                let n = num(&p, &p)?;
                PageRange { start: n, end: Some(n) }
            }
            Some((a, "")) => PageRange { start: num(a, &p)?, end: None },
            Some((a, b)) => {
                let (a, b) = (num(a, &p)?, num(b, &p)?);
                if a > b {
                    return Err(format!("\"{p}\" runs backwards."));
                }
                PageRange { start: a, end: Some(b) }
            }
        };
        if let Some(total) = pages {
            let last = r.end.unwrap_or(r.start);
            if r.start > total || last > total {
                return Err(format!("This PDF has {total} pages; \"{p}\" goes past the end."));
            }
        }
        if !out.contains(&r) {
            out.push(r);
        }
    }
    if out.is_empty() {
        return Err("Enter pages, e.g. 1-3, 5, 8-".into());
    }
    if out.len() > MAX_RANGES {
        return Err(format!("At most {MAX_RANGES} ranges at a time."));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start: u32, end: Option<u32>) -> PageRange {
        PageRange { start, end }
    }

    #[test]
    fn parses_singles_spans_and_open_ends() {
        assert_eq!(
            parse(" 1-3, 5 ,8- ", None).unwrap(),
            [r(1, Some(3)), r(5, Some(5)), r(8, None)]
        );
        assert_eq!(parse("2,,4", None).unwrap(), [r(2, Some(2)), r(4, Some(4))]);
        assert_eq!(parse("1-3,1-3", None).unwrap(), [r(1, Some(3))]);
    }

    #[test]
    fn rejects_bad_input_with_plain_messages() {
        assert_eq!(parse("", None).unwrap_err(), "Enter pages, e.g. 1-3, 5, 8-");
        assert_eq!(parse(" , ", None).unwrap_err(), "Enter pages, e.g. 1-3, 5, 8-");
        assert_eq!(parse("0", None).unwrap_err(), "Pages start at 1.");
        assert_eq!(parse("3-1", None).unwrap_err(), "\"3-1\" runs backwards.");
        assert_eq!(parse("a", None).unwrap_err(), "\"a\" isn't a page number or range.");
        assert_eq!(parse("-4", None).unwrap_err(), "\"-4\" isn't a page number or range.");
        assert_eq!(parse("1-2-3", None).unwrap_err(), "\"1-2-3\" isn't a page number or range.");
        assert_eq!(parse("+5", None).unwrap_err(), "\"+5\" isn't a page number or range.");
        assert_eq!(parse("99999999999", None).unwrap_err(), "\"99999999999\" isn't a page number or range.");
    }

    #[test]
    fn checks_against_the_page_count() {
        assert!(parse("1-10", Some(10)).is_ok());
        assert_eq!(
            parse("9-11", Some(10)).unwrap_err(),
            "This PDF has 10 pages; \"9-11\" goes past the end."
        );
        assert!(parse("11-", Some(10)).is_err());
        assert!(parse("10-", Some(10)).is_ok());
    }

    #[test]
    fn caps_the_number_of_ranges() {
        let many: Vec<String> = (1..=201).map(|n| n.to_string()).collect();
        assert_eq!(parse(&many.join(","), None).unwrap_err(), "At most 200 ranges at a time.");
    }

    #[test]
    fn qpdf_syntax_and_file_stems() {
        assert_eq!(r(1, Some(3)).qpdf(), "1-3");
        assert_eq!(r(5, Some(5)).qpdf(), "5");
        assert_eq!(r(8, None).qpdf(), "8-z");
        assert_eq!(r(1, Some(3)).file_stem(), "pages-1-3");
        assert_eq!(r(5, Some(5)).file_stem(), "page-5");
        assert_eq!(r(8, None).file_stem(), "pages-8-end");
    }
}
