//! A small, deliberately bounded CSS selector engine over [`oxixml_dom`] trees.
//!
//! # Why this exists
//!
//! The `web_scrape` MCP tool accepts a `selector` argument straight out of a
//! client's JSON request, so the selector string is fully caller-controlled.
//! This module implements exactly the subset of CSS Selectors that tool needs,
//! and rejects everything else — an unknown or malformed selector is reported
//! through the caller-facing error path rather than being partially honoured.
//!
//! # Supported syntax
//!
//! * type selectors (`div`), matched ASCII-case-insensitively and without
//!   regard to namespace, and the universal selector `*`
//! * `#id` and `.class`
//! * attribute selectors `[attr]`, `[attr=v]`, `[attr~=v]`, `[attr^=v]`,
//!   `[attr$=v]` and `[attr*=v]`, with optionally quoted values
//! * the structural pseudo-classes `:first-child`, `:last-child` and
//!   `:nth-child(An+B | odd | even)`, plus `:not(...)` over a comma-separated
//!   list of compound selectors
//! * the combinators descendant (whitespace), child `>`, next-sibling `+` and
//!   subsequent-sibling `~`
//! * comma-separated selector lists
//!
//! Anything else — pseudo-elements, `:has()`, `:is()`, `:where()`, `*-of-type`,
//! `[attr|=v]`, case-sensitivity flags, escape sequences, nested `:not()` — is
//! an error, not a silent no-match.
//!
//! # Robustness
//!
//! Parsing is byte-oriented but never slices at an unchecked offset, never
//! indexes without a bounds check and never panics: every input, however
//! malformed, yields either a selector or a [`SelectorError`]. Hard caps on
//! length, list size, compound count and identifier length (see the constants
//! below) bound the work an adversarial selector can buy, and `:not()` may not
//! be nested, so the grammar has no unbounded recursion.
//!
//! Matching is a single arena walk per call: the selector is parsed once, then
//! every element in document order is tested against the parsed form. Ancestor
//! and sibling constraints are evaluated right-to-left over a *set* of
//! candidate nodes with a visited filter, so no combination of combinators can
//! trigger exponential backtracking.

mod matcher;
mod parse;

use oxixml_dom::{Document, NodeId};
use std::fmt;

/// Maximum accepted length of a selector string, in bytes.
pub const MAX_SELECTOR_LEN: usize = 1024;

/// Maximum number of comma-separated selectors in one list.
pub const MAX_SELECTORS_IN_LIST: usize = 16;

/// Maximum number of compound selectors joined by combinators in one selector.
pub const MAX_COMPOUNDS_PER_SELECTOR: usize = 8;

/// Maximum number of qualifiers (`#id`, `.class`, `[attr]`, `:pseudo`) that may
/// follow one type selector.
pub const MAX_QUALIFIERS_PER_COMPOUND: usize = 16;

/// Maximum length of a single identifier or attribute value, in bytes.
pub const MAX_IDENT_LEN: usize = 256;

/// Maximum magnitude accepted for either coefficient of an `An+B` expression.
pub const MAX_NTH_MAGNITUDE: i32 = 1_000_000;

/// A parsed, comma-separated list of CSS selectors.
#[derive(Debug)]
pub struct SelectorList {
    /// The individual selectors; an element matches the list if it matches any.
    selectors: Vec<Complex>,
}

impl SelectorList {
    /// Parses `input` as a CSS selector list.
    ///
    /// # Errors
    ///
    /// Returns a [`SelectorError`] if `input` is malformed, exceeds one of this
    /// module's hard caps, or uses syntax outside the supported subset.
    pub fn parse(input: &str) -> Result<Self, SelectorError> {
        parse::parse_selector_list(input)
    }

    /// Returns every element in the subtree rooted at `root` that matches this
    /// list, in document order, with no element reported twice.
    pub fn match_elements(&self, document: &Document, root: NodeId) -> Vec<NodeId> {
        matcher::match_elements(self, document, root)
    }
}

/// A selector: one subject compound plus its ancestor/sibling constraints.
#[derive(Debug)]
struct Complex {
    /// The right-most compound — the element the selector actually selects.
    subject: Compound,
    /// Constraints ordered right-to-left. Entry `i` says how to step away from
    /// the node matched by the previous entry (or from `subject`, for entry 0)
    /// and which compound the node reached that way must match.
    ancestry: Vec<(Combinator, Compound)>,
}

/// How two adjacent compound selectors are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Combinator {
    /// `a b` — `a` is any ancestor of `b`.
    Descendant,
    /// `a > b` — `a` is the parent of `b`.
    Child,
    /// `a + b` — `a` is the element sibling immediately before `b`.
    NextSibling,
    /// `a ~ b` — `a` is any element sibling before `b`.
    SubsequentSibling,
}

/// A compound selector: an optional type selector plus zero or more qualifiers,
/// all of which must match the same element.
#[derive(Debug, Default)]
struct Compound {
    /// The lowercased type name, or `None` for `*` and for a bare qualifier
    /// sequence such as `.post`.
    type_name: Option<String>,
    /// Qualifiers applied on top of the type selector.
    qualifiers: Vec<Qualifier>,
}

/// A single condition applied to one element.
#[derive(Debug)]
enum Qualifier {
    /// `#id`, compared case-sensitively.
    Id(String),
    /// `.class`, matched against the whitespace-separated `class` attribute.
    Class(String),
    /// `[attr]` and its comparison forms.
    Attribute(AttrSelector),
    /// `:first-child`.
    FirstChild,
    /// `:last-child`.
    LastChild,
    /// `:nth-child(An+B)`.
    NthChild(Nth),
    /// `:not(...)` over a list of compounds. The parser rejects `:not()` inside
    /// `:not()`, so the compounds here never carry a nested [`Qualifier::Not`]
    /// and matching recurses exactly one level.
    Not(Vec<Compound>),
}

/// An attribute selector: a lowercased attribute name and an optional test.
#[derive(Debug)]
struct AttrSelector {
    /// The attribute's local name, lowercased.
    name: String,
    /// The comparison to apply, or `None` for a bare presence test.
    test: Option<(AttrOp, String)>,
}

/// The comparison an attribute selector performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrOp {
    /// `[attr=v]` — the whole value equals `v`.
    Exact,
    /// `[attr~=v]` — one whitespace-separated word of the value equals `v`.
    Includes,
    /// `[attr^=v]` — the value starts with a non-empty `v`.
    Prefix,
    /// `[attr$=v]` — the value ends with a non-empty `v`.
    Suffix,
    /// `[attr*=v]` — the value contains a non-empty `v`.
    Substring,
}

/// The `An+B` coefficients of an `:nth-child()` expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Nth {
    /// The step `A`.
    a: i32,
    /// The offset `B`.
    b: i32,
}

/// A selector that could not be parsed, or that uses unsupported syntax.
#[derive(Debug)]
pub struct SelectorError {
    message: String,
}

impl SelectorError {
    /// Builds an error carrying `message`.
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SelectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SelectorError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `html`, applies `selector`, and returns the lowercased tag name of
    /// every matched element in document order.
    fn matched_tags(html: &str, selector: &str) -> Vec<String> {
        let selectors = SelectorList::parse(selector).expect("selector should parse");
        let parsed = oxixml_html::parse_document(html);
        let document = parsed.document();
        selectors
            .match_elements(document, parsed.root())
            .into_iter()
            .filter_map(|id| document.element(id))
            .map(|element| element.local_name().to_ascii_lowercase())
            .collect()
    }

    #[test]
    fn end_to_end_selection_over_parsed_html() {
        let html = r#"<html><body>
            <div class="post" id="a"><p>one</p></div>
            <div class="post"><span>two</span></div>
        </body></html>"#;

        assert_eq!(matched_tags(html, "div.post p"), vec!["p".to_string()]);
        assert_eq!(matched_tags(html, "#a"), vec!["div".to_string()]);
        assert_eq!(
            matched_tags(html, "p, span"),
            vec!["p".to_string(), "span".to_string()]
        );
    }

    #[test]
    fn an_element_matching_several_selectors_is_reported_once() {
        let html = "<html><body><p class=\"x\">t</p></body></html>";

        // `p`, `.x` and `p.x` all select the same element; the walk must still
        // yield it exactly once.
        assert_eq!(matched_tags(html, "p, .x, p.x"), vec!["p".to_string()]);
    }

    #[test]
    fn an_oversized_selector_is_rejected_without_being_parsed() {
        let selector = "a".repeat(100 * 1024);

        let error = SelectorList::parse(&selector).expect_err("100KB selector must be rejected");
        assert!(
            error.to_string().contains("longer than"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn an_oversized_selector_list_is_rejected() {
        let selector = std::iter::repeat_n("div", MAX_SELECTORS_IN_LIST + 4)
            .collect::<Vec<_>>()
            .join(",");

        let error = SelectorList::parse(&selector).expect_err("over-long list must be rejected");
        assert!(
            error.to_string().contains("selectors"),
            "unexpected message: {error}"
        );
    }

    #[test]
    fn deeply_nested_input_is_rejected_without_recursing() {
        // 10k levels of `:not(` nesting. The length cap sheds this before the
        // parser looks at a single character; even shortened to fit the cap the
        // nested-`:not()` rule rejects it at the second level, so no input can
        // drive the parser into deep recursion.
        let deep = format!("div{}{}", ":not(".repeat(10_000), ")".repeat(10_000));
        assert!(SelectorList::parse(&deep).is_err());

        assert!(SelectorList::parse("div:not(:not(p))").is_err());
    }

    #[test]
    fn deeply_nested_brackets_and_parens_are_rejected() {
        assert!(SelectorList::parse(&"(".repeat(500)).is_err());
        assert!(SelectorList::parse(&"[".repeat(500)).is_err());
        assert!(SelectorList::parse(&"[a=[a=[a=b]]]".repeat(30)).is_err());
    }

    #[test]
    fn adversarial_bytes_never_panic() {
        let cases = [
            "",
            " ",
            "\u{0}",
            "\u{feff}",
            "\\",
            "\\41",
            "*|*",
            "[",
            "]",
            "[]",
            "[=]",
            "[a=]",
            "[a=\"unterminated",
            "[a='mixed\"]",
            "::",
            ":::",
            ":nth-child(",
            ":nth-child()",
            ":nth-child(n",
            ":nth-child(--)",
            ":nth-child(999999999999999999999999)",
            ":not(",
            ":not()",
            "div:not(p q)",
            "🦀",
            ".🦀",
            "[🦀=🦀]",
            "a\u{300}b",
            ">>> not a valid selector <<<",
            "a,,b",
            "a >",
            "> a",
            "a ~",
            "~",
            "+",
            "a b c d e f g h i j k",
        ];

        for case in cases {
            // The only requirement is total absence of panics; both outcomes are
            // legitimate for a hostile input.
            let _ = SelectorList::parse(case);
        }
    }

    #[test]
    fn a_pathological_attribute_value_in_the_document_is_handled() {
        let value = "x".repeat(200_000);
        let html = format!("<html><body><div data-x=\"{value}\">t</div></body></html>");

        assert_eq!(matched_tags(&html, "[data-x^=xxx]"), vec!["div".to_string()]);
        assert_eq!(matched_tags(&html, "[data-x=short]"), Vec::<String>::new());
    }

    #[test]
    fn error_display_is_not_empty() {
        let error = SelectorError::new("boom");
        assert_eq!(error.to_string(), "boom");
    }
}
