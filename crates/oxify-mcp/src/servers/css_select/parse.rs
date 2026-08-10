//! The selector parser.
//!
//! A single left-to-right pass over the input bytes. The scanner only ever
//! advances one byte at a time from a bounds-checked `peek`, and only ever
//! slices the original `&str` through `str::get`, so no input can trigger an
//! out-of-bounds index or a char-boundary panic.

use super::{
    AttrOp, AttrSelector, Combinator, Complex, Compound, Nth, Qualifier, SelectorError,
    SelectorList, MAX_COMPOUNDS_PER_SELECTOR, MAX_IDENT_LEN, MAX_NTH_MAGNITUDE,
    MAX_QUALIFIERS_PER_COMPOUND, MAX_SELECTORS_IN_LIST, MAX_SELECTOR_LEN,
};

/// Parses a comma-separated CSS selector list.
///
/// # Errors
///
/// Returns a [`SelectorError`] for malformed input, for syntax outside the
/// supported subset, and for input that exceeds one of the module's hard caps.
pub fn parse_selector_list(input: &str) -> Result<SelectorList, SelectorError> {
    if input.len() > MAX_SELECTOR_LEN {
        return Err(SelectorError::new(format!(
            "selector is longer than the {MAX_SELECTOR_LEN}-byte limit"
        )));
    }

    let mut scanner = Scanner::new(input);
    let mut selectors = Vec::new();

    loop {
        selectors.push(scanner.parse_complex()?);
        if selectors.len() > MAX_SELECTORS_IN_LIST {
            return Err(SelectorError::new(format!(
                "selector list holds more than the {MAX_SELECTORS_IN_LIST} permitted selectors"
            )));
        }

        scanner.skip_whitespace();
        if !scanner.eat(b',') {
            break;
        }
    }

    scanner.skip_whitespace();
    if scanner.peek().is_some() {
        return Err(SelectorError::new("unexpected trailing characters"));
    }

    Ok(SelectorList { selectors })
}

/// Whether `byte` may appear in an identifier.
///
/// Every byte of a multi-byte UTF-8 character is `>= 0x80`, so an identifier run
/// never stops in the middle of a character.
const fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' || byte >= 0x80
}

/// Whether `byte` is CSS whitespace.
const fn is_whitespace_byte(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

/// A bounds-checked cursor over the selector text.
struct Scanner<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }

    /// The byte at the cursor, if any.
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Advances one byte. Only called after a successful [`Self::peek`].
    fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    /// Consumes `byte` if it is at the cursor.
    fn eat(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consumes `byte`, or reports what was expected.
    fn expect(&mut self, byte: u8) -> Result<(), SelectorError> {
        if self.eat(byte) {
            Ok(())
        } else {
            Err(SelectorError::new(format!(
                "expected `{}`",
                char::from(byte)
            )))
        }
    }

    /// Consumes any run of whitespace, reporting whether there was one.
    fn skip_whitespace(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(is_whitespace_byte) {
            self.bump();
        }
        self.pos != start
    }

    /// Consumes `keyword` if it appears at the cursor as a whole identifier.
    fn eat_keyword_ci(&mut self, keyword: &str) -> bool {
        let end = self.pos.saturating_add(keyword.len());
        let Some(slice) = self.input.get(self.pos..end) else {
            return false;
        };
        if !slice.eq_ignore_ascii_case(keyword) {
            return false;
        }
        if self.bytes.get(end).copied().is_some_and(is_ident_byte) {
            return false;
        }
        self.pos = end;
        true
    }

    /// Reads an identifier.
    ///
    /// Backslash escapes are deliberately unsupported: `\` is not an identifier
    /// byte, so it terminates the run and is then reported as unexpected.
    fn read_ident(&mut self) -> Result<&'a str, SelectorError> {
        let start = self.pos;
        while self.peek().is_some_and(is_ident_byte) {
            self.bump();
        }
        if self.pos == start {
            return Err(SelectorError::new("expected an identifier"));
        }
        if self.pos.saturating_sub(start) > MAX_IDENT_LEN {
            return Err(SelectorError::new(format!(
                "identifier is longer than the {MAX_IDENT_LEN}-byte limit"
            )));
        }
        self.input
            .get(start..self.pos)
            .ok_or_else(|| SelectorError::new("identifier is not valid text"))
    }

    /// Reads an attribute selector's value: a quoted string or a bare identifier.
    fn read_attr_value(&mut self) -> Result<String, SelectorError> {
        let quote = match self.peek() {
            Some(byte @ (b'"' | b'\'')) => byte,
            _ => return self.read_ident().map(str::to_owned),
        };
        self.bump();

        let start = self.pos;
        loop {
            match self.peek() {
                None => return Err(SelectorError::new("unterminated quoted attribute value")),
                Some(b'\\') => {
                    return Err(SelectorError::new("escape sequences are not supported"))
                }
                Some(byte) if byte == quote => {
                    let end = self.pos;
                    self.bump();
                    if end.saturating_sub(start) > MAX_IDENT_LEN {
                        return Err(SelectorError::new(format!(
                            "attribute value is longer than the {MAX_IDENT_LEN}-byte limit"
                        )));
                    }
                    return self
                        .input
                        .get(start..end)
                        .map(str::to_owned)
                        .ok_or_else(|| SelectorError::new("attribute value is not valid text"));
                }
                Some(_) => self.bump(),
            }
        }
    }

    /// Parses one selector: a chain of compounds joined by combinators.
    fn parse_complex(&mut self) -> Result<Complex, SelectorError> {
        self.skip_whitespace();

        let mut subject = self.parse_compound(true)?;
        let mut ancestry = Vec::new();

        loop {
            let had_whitespace = self.skip_whitespace();
            let combinator = match self.peek() {
                None | Some(b',') => break,
                Some(b'>') => {
                    self.bump();
                    Combinator::Child
                }
                Some(b'+') => {
                    self.bump();
                    Combinator::NextSibling
                }
                Some(b'~') => {
                    self.bump();
                    Combinator::SubsequentSibling
                }
                // Whitespace between two compounds is the descendant combinator;
                // anything else here is trailing garbage the caller reports.
                Some(_) if had_whitespace => Combinator::Descendant,
                Some(_) => break,
            };

            self.skip_whitespace();
            let next = self.parse_compound(true)?;
            ancestry.push((combinator, subject));
            subject = next;

            if ancestry.len().saturating_add(1) > MAX_COMPOUNDS_PER_SELECTOR {
                return Err(SelectorError::new(format!(
                    "selector joins more than the {MAX_COMPOUNDS_PER_SELECTOR} permitted compound selectors"
                )));
            }
        }

        // Stored right-to-left so matching can walk outwards from the subject.
        ancestry.reverse();
        Ok(Complex { subject, ancestry })
    }

    /// Parses one compound selector.
    ///
    /// `allow_negation` is false inside `:not()`, which is what forbids nested
    /// negation and keeps the grammar's recursion depth at one.
    fn parse_compound(&mut self, allow_negation: bool) -> Result<Compound, SelectorError> {
        let mut compound = Compound::default();
        let mut saw_anything = false;

        match self.peek() {
            Some(b'*') => {
                self.bump();
                saw_anything = true;
            }
            Some(byte) if is_ident_byte(byte) => {
                compound.type_name = Some(self.read_ident()?.to_ascii_lowercase());
                saw_anything = true;
            }
            _ => {}
        }

        loop {
            let qualifier = match self.peek() {
                Some(b'#') => {
                    self.bump();
                    Qualifier::Id(self.read_ident()?.to_owned())
                }
                Some(b'.') => {
                    self.bump();
                    Qualifier::Class(self.read_ident()?.to_owned())
                }
                Some(b'[') => {
                    self.bump();
                    Qualifier::Attribute(self.parse_attribute()?)
                }
                Some(b':') => {
                    self.bump();
                    self.parse_pseudo_class(allow_negation)?
                }
                _ => break,
            };

            compound.qualifiers.push(qualifier);
            saw_anything = true;
            if compound.qualifiers.len() > MAX_QUALIFIERS_PER_COMPOUND {
                return Err(SelectorError::new(format!(
                    "compound selector holds more than the {MAX_QUALIFIERS_PER_COMPOUND} permitted qualifiers"
                )));
            }
        }

        if saw_anything {
            Ok(compound)
        } else {
            Err(SelectorError::new("expected a selector"))
        }
    }

    /// Parses the body of an attribute selector, up to and including the `]`.
    fn parse_attribute(&mut self) -> Result<AttrSelector, SelectorError> {
        self.skip_whitespace();
        let name = self.read_ident()?.to_ascii_lowercase();
        self.skip_whitespace();

        let operator = match self.peek() {
            Some(b']') => None,
            Some(b'=') => {
                self.bump();
                Some(AttrOp::Exact)
            }
            Some(b'~') => {
                self.bump();
                self.expect(b'=')?;
                Some(AttrOp::Includes)
            }
            Some(b'^') => {
                self.bump();
                self.expect(b'=')?;
                Some(AttrOp::Prefix)
            }
            Some(b'$') => {
                self.bump();
                self.expect(b'=')?;
                Some(AttrOp::Suffix)
            }
            Some(b'*') => {
                self.bump();
                self.expect(b'=')?;
                Some(AttrOp::Substring)
            }
            Some(_) => return Err(SelectorError::new("unsupported attribute operator")),
            None => return Err(SelectorError::new("unterminated attribute selector")),
        };

        let test = match operator {
            None => None,
            Some(operator) => {
                self.skip_whitespace();
                let value = self.read_attr_value()?;
                self.skip_whitespace();
                Some((operator, value))
            }
        };

        // A case-sensitivity flag (`i` / `s`) would sit here; it is unsupported,
        // so it surfaces as a missing `]`.
        self.expect(b']')?;
        Ok(AttrSelector { name, test })
    }

    /// Parses a pseudo-class, the leading `:` already consumed.
    fn parse_pseudo_class(&mut self, allow_negation: bool) -> Result<Qualifier, SelectorError> {
        let name = self.read_ident()?.to_ascii_lowercase();

        match name.as_str() {
            "first-child" => Ok(Qualifier::FirstChild),
            "last-child" => Ok(Qualifier::LastChild),
            "nth-child" => {
                self.expect(b'(')?;
                let nth = self.parse_nth()?;
                self.expect(b')')?;
                Ok(Qualifier::NthChild(nth))
            }
            "not" => {
                if !allow_negation {
                    return Err(SelectorError::new(
                        "`:not()` may not be nested inside another `:not()`",
                    ));
                }
                self.expect(b'(')?;

                let mut compounds = Vec::new();
                loop {
                    self.skip_whitespace();
                    compounds.push(self.parse_compound(false)?);
                    if compounds.len() > MAX_SELECTORS_IN_LIST {
                        return Err(SelectorError::new(format!(
                            "`:not()` holds more than the {MAX_SELECTORS_IN_LIST} permitted selectors"
                        )));
                    }
                    self.skip_whitespace();
                    if !self.eat(b',') {
                        break;
                    }
                }

                // Combinators inside `:not()` are unsupported and land here as a
                // missing `)`.
                self.expect(b')')?;
                Ok(Qualifier::Not(compounds))
            }
            other => Err(SelectorError::new(format!(
                "unsupported pseudo-class `:{other}`"
            ))),
        }
    }

    /// Parses an `An+B` expression, or the `odd` / `even` keywords.
    fn parse_nth(&mut self) -> Result<Nth, SelectorError> {
        self.skip_whitespace();

        if self.eat_keyword_ci("odd") {
            self.skip_whitespace();
            return Ok(Nth { a: 2, b: 1 });
        }
        if self.eat_keyword_ci("even") {
            self.skip_whitespace();
            return Ok(Nth { a: 2, b: 0 });
        }

        let sign = self.read_sign();
        let magnitude = self.read_optional_number()?;

        let nth = if matches!(self.peek(), Some(b'n' | b'N')) {
            self.bump();
            let a = signed(sign, magnitude.unwrap_or(1))?;

            self.skip_whitespace();
            let b = match self.peek() {
                Some(byte @ (b'+' | b'-')) => {
                    self.bump();
                    self.skip_whitespace();
                    let offset = self.read_number()?;
                    if byte == b'-' {
                        signed(-1, offset)?
                    } else {
                        offset
                    }
                }
                _ => 0,
            };
            Nth { a, b }
        } else {
            let offset = magnitude
                .ok_or_else(|| SelectorError::new("expected an `An+B` expression"))?;
            Nth {
                a: 0,
                b: signed(sign, offset)?,
            }
        };

        self.skip_whitespace();
        Ok(nth)
    }

    /// Consumes a leading `+` or `-`, returning the sign it denotes.
    fn read_sign(&mut self) -> i32 {
        if self.eat(b'-') {
            -1
        } else {
            let _ = self.eat(b'+');
            1
        }
    }

    /// Reads a run of ASCII digits, capped at [`MAX_NTH_MAGNITUDE`].
    fn read_number(&mut self) -> Result<i32, SelectorError> {
        let start = self.pos;
        let mut value: i32 = 0;

        while let Some(byte) = self.peek() {
            if !byte.is_ascii_digit() {
                break;
            }
            value = value
                .checked_mul(10)
                .and_then(|scaled| scaled.checked_add(i32::from(byte - b'0')))
                .filter(|candidate| *candidate <= MAX_NTH_MAGNITUDE)
                .ok_or_else(|| {
                    SelectorError::new(format!("number exceeds the {MAX_NTH_MAGNITUDE} limit"))
                })?;
            self.bump();
        }

        if self.pos == start {
            return Err(SelectorError::new("expected a number"));
        }
        Ok(value)
    }

    /// Reads a number if one is at the cursor.
    fn read_optional_number(&mut self) -> Result<Option<i32>, SelectorError> {
        if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.read_number().map(Some)
        } else {
            Ok(None)
        }
    }
}

/// Applies `sign` to `magnitude`, reporting an overflow rather than wrapping.
fn signed(sign: i32, magnitude: i32) -> Result<i32, SelectorError> {
    sign.checked_mul(magnitude)
        .ok_or_else(|| SelectorError::new("number is out of range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `input`, panicking with the parse error if it is rejected.
    fn parse(input: &str) -> SelectorList {
        match parse_selector_list(input) {
            Ok(list) => list,
            Err(error) => panic!("`{input}` should parse, got: {error}"),
        }
    }

    /// Parses `input` and returns its single selector.
    fn parse_one(input: &str) -> Complex {
        let mut list = parse(input);
        assert_eq!(list.selectors.len(), 1, "`{input}` should hold 1 selector");
        list.selectors.remove(0)
    }

    fn reject(input: &str) -> String {
        match parse_selector_list(input) {
            Ok(_) => panic!("`{input}` should have been rejected"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn type_selector_is_lowercased() {
        let selector = parse_one("DiV");
        assert_eq!(selector.subject.type_name.as_deref(), Some("div"));
        assert!(selector.subject.qualifiers.is_empty());
    }

    #[test]
    fn universal_selector_carries_no_type() {
        let selector = parse_one("*");
        assert!(selector.subject.type_name.is_none());
        assert!(selector.subject.qualifiers.is_empty());
    }

    #[test]
    fn id_and_class_keep_their_case() {
        let selector = parse_one("#MyId.MyClass");
        assert!(selector.subject.type_name.is_none());
        assert!(matches!(
            selector.subject.qualifiers.as_slice(),
            [Qualifier::Id(id), Qualifier::Class(class)] if id == "MyId" && class == "MyClass"
        ));
    }

    #[test]
    fn compound_mixes_type_and_qualifiers() {
        let selector = parse_one("div.post#main[data-x]");
        assert_eq!(selector.subject.type_name.as_deref(), Some("div"));
        assert_eq!(selector.subject.qualifiers.len(), 3);
    }

    #[test]
    fn every_attribute_operator_parses() {
        for (input, expected) in [
            ("[a=b]", AttrOp::Exact),
            ("[a~=b]", AttrOp::Includes),
            ("[a^=b]", AttrOp::Prefix),
            ("[a$=b]", AttrOp::Suffix),
            ("[a*=b]", AttrOp::Substring),
        ] {
            let selector = parse_one(input);
            let [Qualifier::Attribute(attribute)] = selector.subject.qualifiers.as_slice() else {
                panic!("`{input}` should hold one attribute selector");
            };
            assert_eq!(attribute.name, "a");
            assert_eq!(
                attribute.test.as_ref().map(|(operator, _)| *operator),
                Some(expected)
            );
        }
    }

    #[test]
    fn bare_attribute_presence_has_no_test() {
        let selector = parse_one("[data-x]");
        let [Qualifier::Attribute(attribute)] = selector.subject.qualifiers.as_slice() else {
            panic!("expected one attribute selector");
        };
        assert_eq!(attribute.name, "data-x");
        assert!(attribute.test.is_none());
    }

    #[test]
    fn attribute_names_lowercase_but_values_do_not() {
        let selector = parse_one("[DaTa-X=MixedCase]");
        let [Qualifier::Attribute(attribute)] = selector.subject.qualifiers.as_slice() else {
            panic!("expected one attribute selector");
        };
        assert_eq!(attribute.name, "data-x");
        assert_eq!(
            attribute.test.as_ref().map(|(_, value)| value.as_str()),
            Some("MixedCase")
        );
    }

    #[test]
    fn quoted_attribute_values_may_hold_punctuation_and_whitespace() {
        for input in ["[a=\"x y>z,\"]", "[a='x y>z,']"] {
            let selector = parse_one(input);
            let [Qualifier::Attribute(attribute)] = selector.subject.qualifiers.as_slice() else {
                panic!("expected one attribute selector");
            };
            assert_eq!(
                attribute.test.as_ref().map(|(_, value)| value.as_str()),
                Some("x y>z,")
            );
        }
    }

    #[test]
    fn whitespace_inside_attribute_brackets_is_ignored() {
        let selector = parse_one("[ a = 'b' ]");
        let [Qualifier::Attribute(attribute)] = selector.subject.qualifiers.as_slice() else {
            panic!("expected one attribute selector");
        };
        assert_eq!(attribute.name, "a");
        assert_eq!(
            attribute.test.as_ref().map(|(_, value)| value.as_str()),
            Some("b")
        );
    }

    #[test]
    fn every_combinator_parses_and_is_stored_right_to_left() {
        let selector = parse_one("a > b c + d ~ e");
        assert_eq!(selector.subject.type_name.as_deref(), Some("e"));

        let combinators: Vec<Combinator> = selector
            .ancestry
            .iter()
            .map(|(combinator, _)| *combinator)
            .collect();
        assert_eq!(
            combinators,
            vec![
                Combinator::SubsequentSibling,
                Combinator::NextSibling,
                Combinator::Descendant,
                Combinator::Child,
            ]
        );

        let names: Vec<Option<&str>> = selector
            .ancestry
            .iter()
            .map(|(_, compound)| compound.type_name.as_deref())
            .collect();
        assert_eq!(
            names,
            vec![Some("d"), Some("c"), Some("b"), Some("a")],
            "constraints must run outwards from the subject"
        );
    }

    #[test]
    fn combinators_need_no_surrounding_whitespace() {
        let selector = parse_one("a>b");
        assert_eq!(selector.subject.type_name.as_deref(), Some("b"));
        assert_eq!(selector.ancestry.len(), 1);
        assert_eq!(selector.ancestry[0].0, Combinator::Child);
    }

    #[test]
    fn selector_lists_split_on_commas() {
        let list = parse(" p , div.post > span , #id ");
        assert_eq!(list.selectors.len(), 3);
    }

    #[test]
    fn structural_pseudo_classes_parse() {
        assert!(matches!(
            parse_one("p:first-child").subject.qualifiers.as_slice(),
            [Qualifier::FirstChild]
        ));
        assert!(matches!(
            parse_one("p:LAST-CHILD").subject.qualifiers.as_slice(),
            [Qualifier::LastChild]
        ));
    }

    #[test]
    fn nth_child_covers_the_an_plus_b_microsyntax() {
        for (input, a, b) in [
            (":nth-child(odd)", 2, 1),
            (":nth-child(EVEN)", 2, 0),
            (":nth-child(3)", 0, 3),
            (":nth-child(+3)", 0, 3),
            (":nth-child(-3)", 0, -3),
            (":nth-child(n)", 1, 0),
            (":nth-child(N)", 1, 0),
            (":nth-child(+n)", 1, 0),
            (":nth-child(-n)", -1, 0),
            (":nth-child(2n)", 2, 0),
            (":nth-child(2n+1)", 2, 1),
            (":nth-child(2n-1)", 2, -1),
            (":nth-child( 2n + 1 )", 2, 1),
            (":nth-child(-n+3)", -1, 3),
            (":nth-child(0n+5)", 0, 5),
        ] {
            let selector = parse_one(input);
            let [Qualifier::NthChild(nth)] = selector.subject.qualifiers.as_slice() else {
                panic!("`{input}` should hold one `:nth-child()`");
            };
            assert_eq!(*nth, Nth { a, b }, "for `{input}`");
        }
    }

    #[test]
    fn negation_takes_a_compound_list() {
        let selector = parse_one("p:not(.a, #b, [c], :first-child)");
        let [Qualifier::Not(compounds)] = selector.subject.qualifiers.as_slice() else {
            panic!("expected one `:not()`");
        };
        assert_eq!(compounds.len(), 4);
    }

    #[test]
    fn identifiers_may_hold_non_ascii_characters() {
        let selector = parse_one(".見出し");
        assert!(matches!(
            selector.subject.qualifiers.as_slice(),
            [Qualifier::Class(class)] if class == "見出し"
        ));
    }

    #[test]
    fn the_pinned_selectors_still_parse() {
        assert_eq!(parse("div.post p").selectors.len(), 1);
        assert_eq!(parse("table.does-not-exist").selectors.len(), 1);
        assert!(parse_selector_list(">>> not a valid selector <<<").is_err());
    }

    #[test]
    fn empty_and_whitespace_only_input_is_rejected() {
        assert_eq!(reject(""), "expected a selector");
        assert_eq!(reject("   \t\n"), "expected a selector");
    }

    #[test]
    fn dangling_combinators_and_commas_are_rejected() {
        for input in ["> p", "+ p", "~ p", "p >", "p ~", "p,", ",p", "p,,q", "p >> q"] {
            let _ = reject(input);
        }
    }

    #[test]
    fn unsupported_syntax_is_rejected_with_a_reason() {
        assert!(reject("p::before").contains("identifier"));
        assert!(reject("p:has(a)").contains("unsupported pseudo-class"));
        assert!(reject("p:is(a)").contains("unsupported pseudo-class"));
        assert!(reject("p:where(a)").contains("unsupported pseudo-class"));
        assert!(reject("p:nth-of-type(1)").contains("unsupported pseudo-class"));
        assert!(reject("p:root").contains("unsupported pseudo-class"));
        assert!(reject("p:empty").contains("unsupported pseudo-class"));
        assert!(reject("p:only-child").contains("unsupported pseudo-class"));
        assert!(reject("[a|=b]").contains("unsupported attribute operator"));
        assert!(reject("[a=b i]").contains(']'));
        assert!(reject("a\\.b").contains("trailing"));
        assert!(reject("[a=\"x\\ty\"]").contains("escape"));
        assert!(reject("p:not(:not(a))").contains("nested"));
        assert!(reject("p:not(a b)").contains(')'));
        assert!(reject("p:not(a > b)").contains(')'));
    }

    #[test]
    fn malformed_brackets_and_parens_are_rejected() {
        for input in [
            "[",
            "[a",
            "[a=",
            "[a=b",
            "[]",
            "[=b]",
            "[a=\"unterminated]",
            "p:nth-child",
            "p:nth-child(",
            "p:nth-child()",
            "p:nth-child(n",
            "p:nth-child(2n+)",
            "p:nth-child(+)",
            "p:nth-child(--1)",
            "p:not",
            "p:not(",
            "p:not()",
            "p:",
        ] {
            let _ = reject(input);
        }
    }

    #[test]
    fn hard_caps_are_enforced() {
        assert!(reject(&"a".repeat(MAX_SELECTOR_LEN + 1)).contains("longer than"));

        let long_ident = format!(".{}", "a".repeat(MAX_IDENT_LEN + 1));
        assert!(reject(&long_ident).contains("longer than"));

        let long_value = format!("[a=\"{}\"]", "b".repeat(MAX_IDENT_LEN + 1));
        assert!(reject(&long_value).contains("longer than"));

        let deep_chain = ["a"; MAX_COMPOUNDS_PER_SELECTOR + 1].join(" ");
        assert!(reject(&deep_chain).contains("compound"));

        let long_list = ["a"; MAX_SELECTORS_IN_LIST + 1].join(",");
        assert!(reject(&long_list).contains("selectors"));

        let many_qualifiers = format!("a{}", ".x".repeat(MAX_QUALIFIERS_PER_COMPOUND + 1));
        assert!(reject(&many_qualifiers).contains("qualifiers"));

        assert!(reject(":nth-child(99999999999)").contains("limit"));
    }

    #[test]
    fn a_chain_exactly_at_the_compound_cap_is_accepted() {
        let chain = ["a"; MAX_COMPOUNDS_PER_SELECTOR].join(" ");
        let selector = parse_one(&chain);
        assert_eq!(selector.ancestry.len(), MAX_COMPOUNDS_PER_SELECTOR - 1);
    }

    #[test]
    fn input_exactly_at_the_caps_is_still_accepted() {
        let at_ident_cap = format!(".{}", "a".repeat(MAX_IDENT_LEN));
        assert_eq!(parse(&at_ident_cap).selectors.len(), 1);

        let at_list_cap = ["a"; MAX_SELECTORS_IN_LIST].join(",");
        assert!(at_list_cap.len() <= MAX_SELECTOR_LEN);
        assert_eq!(parse(&at_list_cap).selectors.len(), MAX_SELECTORS_IN_LIST);

        let at_qualifier_cap = format!("a{}", ".x".repeat(MAX_QUALIFIERS_PER_COMPOUND));
        assert_eq!(
            parse_one(&at_qualifier_cap).subject.qualifiers.len(),
            MAX_QUALIFIERS_PER_COMPOUND
        );
    }
}
