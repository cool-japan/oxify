//! The selector matcher.
//!
//! One arena walk per call. Every element in the subtree is visited once, in
//! document order, and tested against the already-parsed selector list.
//!
//! A selector's ancestor and sibling constraints are evaluated right-to-left
//! over a *set* of candidate nodes rather than by exploring each candidate path
//! independently, and a per-step visited filter stops overlapping ancestor and
//! sibling chains from being walked twice. Matching is therefore linear in the
//! nodes each step can reach, and no arrangement of combinators can provoke the
//! exponential backtracking a naive recursive matcher would suffer.
//!
//! The left-most constraint only has to hold for *one* candidate, so that step
//! stops at its first match instead of enumerating every one. Two-compound
//! selectors — `div.post p`, `li + li`, the shapes callers actually write —
//! therefore cost a short walk per element rather than a full axis scan.

use super::{AttrOp, AttrSelector, Combinator, Complex, Compound, Nth, Qualifier, SelectorList};
use oxixml_dom::{Document, NodeId, NodeKind};
use std::collections::HashSet;

/// Buffers reused across every element of one walk, so matching allocates a
/// bounded amount regardless of document size.
#[derive(Default)]
struct Scratch {
    /// Nodes that satisfied the constraints considered so far.
    current: Vec<NodeId>,
    /// Nodes reachable from `current` through the constraint being applied.
    next: Vec<NodeId>,
    /// Nodes already expanded during the current step.
    visited: HashSet<NodeId>,
}

/// Returns every element under `root` matching `list`, in document order.
///
/// An element matching several selectors of the list is reported once.
pub fn match_elements(list: &SelectorList, document: &Document, root: NodeId) -> Vec<NodeId> {
    let mut matches = Vec::new();
    let mut scratch = Scratch::default();

    for node in document.traverse(root) {
        if !is_element(document, node) {
            continue;
        }
        if list
            .selectors
            .iter()
            .any(|selector| matches_complex(document, node, selector, &mut scratch))
        {
            matches.push(node);
        }
    }

    matches
}

/// Whether `node` matches `selector`, subject compound first.
fn matches_complex(
    document: &Document,
    node: NodeId,
    selector: &Complex,
    scratch: &mut Scratch,
) -> bool {
    if !matches_compound(document, node, &selector.subject) {
        return false;
    }
    // Constraints run right-to-left; the last one is the selector's left-most
    // compound, which only needs a single witness.
    let Some((leftmost, inner)) = selector.ancestry.split_last() else {
        return true;
    };

    let Scratch {
        current,
        next,
        visited,
    } = scratch;
    current.clear();
    current.push(node);

    for (combinator, compound) in inner {
        next.clear();
        visited.clear();
        if !expand(
            document,
            *combinator,
            compound,
            current,
            next,
            visited,
            false,
        ) {
            return false;
        }
        std::mem::swap(current, next);
    }

    next.clear();
    visited.clear();
    expand(
        document,
        leftmost.0,
        &leftmost.1,
        current,
        next,
        visited,
        true,
    )
}

/// Steps every node of `current` across `combinator` and keeps those matching
/// `compound`, reporting whether any did.
///
/// Matches are collected into `next` unless `existence_only` is set, in which
/// case the walk returns as soon as one is found. `visited` is cleared by the
/// caller and stops the overlapping tails of two candidates' ancestor or
/// sibling chains from being walked twice.
fn expand(
    document: &Document,
    combinator: Combinator,
    compound: &Compound,
    current: &[NodeId],
    next: &mut Vec<NodeId>,
    visited: &mut HashSet<NodeId>,
    existence_only: bool,
) -> bool {
    let mut found = false;

    match combinator {
        Combinator::Descendant => {
            for &candidate in current {
                for ancestor in document.ancestors(candidate) {
                    // Ancestor chains share a suffix: reaching an already-seen
                    // node means the rest of this chain is already done.
                    if !visited.insert(ancestor) {
                        break;
                    }
                    if is_element(document, ancestor)
                        && matches_compound(document, ancestor, compound)
                        && accept(ancestor, next, &mut found, existence_only)
                    {
                        return true;
                    }
                }
            }
        }
        Combinator::Child => {
            for &candidate in current {
                if let Some(parent) = document.parent(candidate) {
                    if visited.insert(parent)
                        && is_element(document, parent)
                        && matches_compound(document, parent, compound)
                        && accept(parent, next, &mut found, existence_only)
                    {
                        return true;
                    }
                }
            }
        }
        Combinator::NextSibling => {
            for &candidate in current {
                if let Some(sibling) = previous_element_sibling(document, candidate) {
                    if visited.insert(sibling)
                        && matches_compound(document, sibling, compound)
                        && accept(sibling, next, &mut found, existence_only)
                    {
                        return true;
                    }
                }
            }
        }
        Combinator::SubsequentSibling => {
            for &candidate in current {
                let mut cursor = previous_element_sibling(document, candidate);
                while let Some(sibling) = cursor {
                    // As with ancestors, sibling chains share a suffix.
                    if !visited.insert(sibling) {
                        break;
                    }
                    if matches_compound(document, sibling, compound)
                        && accept(sibling, next, &mut found, existence_only)
                    {
                        return true;
                    }
                    cursor = previous_element_sibling(document, sibling);
                }
            }
        }
    }

    found
}

/// Records `candidate` as a match, reporting whether the walk may stop here.
fn accept(
    candidate: NodeId,
    next: &mut Vec<NodeId>,
    found: &mut bool,
    existence_only: bool,
) -> bool {
    *found = true;
    if existence_only {
        return true;
    }
    next.push(candidate);
    false
}

/// Whether `node` is an element satisfying every part of `compound`.
fn matches_compound(document: &Document, node: NodeId, compound: &Compound) -> bool {
    let Some(element) = document.element(node) else {
        return false;
    };

    if let Some(name) = &compound.type_name {
        // HTML type selectors are ASCII case-insensitive; namespaces are ignored,
        // matching the behaviour of a selector carrying no namespace prefix.
        if !element.local_name().eq_ignore_ascii_case(name) {
            return false;
        }
    }

    compound
        .qualifiers
        .iter()
        .all(|qualifier| matches_qualifier(document, node, qualifier))
}

/// Whether `node` satisfies one qualifier.
fn matches_qualifier(document: &Document, node: NodeId, qualifier: &Qualifier) -> bool {
    match qualifier {
        Qualifier::Id(id) => {
            attribute_value(document, node, "id").is_some_and(|value| value == id.as_str())
        }
        Qualifier::Class(class) => attribute_value(document, node, "class").is_some_and(|value| {
            value
                .split_ascii_whitespace()
                .any(|token| token == class.as_str())
        }),
        Qualifier::Attribute(selector) => matches_attribute(document, node, selector),
        Qualifier::FirstChild => previous_element_sibling(document, node).is_none(),
        Qualifier::LastChild => next_element_sibling(document, node).is_none(),
        Qualifier::NthChild(nth) => matches_nth(*nth, element_index(document, node)),
        // The parser forbids `:not()` inside `:not()`, so this recurses once.
        Qualifier::Not(compounds) => !compounds
            .iter()
            .any(|compound| matches_compound(document, node, compound)),
    }
}

/// Whether `node`'s attribute satisfies `selector`.
fn matches_attribute(document: &Document, node: NodeId, selector: &AttrSelector) -> bool {
    let Some(value) = attribute_value(document, node, &selector.name) else {
        return false;
    };
    let Some((operator, expected)) = &selector.test else {
        return true;
    };

    match operator {
        AttrOp::Exact => value == expected.as_str(),
        AttrOp::Includes => value
            .split_ascii_whitespace()
            .any(|token| token == expected.as_str()),
        // A substring test against the empty string never matches, per
        // CSS Selectors Level 3 §6.3.2.
        AttrOp::Prefix => !expected.is_empty() && value.starts_with(expected.as_str()),
        AttrOp::Suffix => !expected.is_empty() && value.ends_with(expected.as_str()),
        AttrOp::Substring => !expected.is_empty() && value.contains(expected.as_str()),
    }
}

/// Whether the 1-based `index` satisfies the `An+B` expression `nth`.
fn matches_nth(nth: Nth, index: i64) -> bool {
    let a = i64::from(nth.a);
    let b = i64::from(nth.b);

    if a == 0 {
        return index == b;
    }
    let offset = index - b;
    offset % a == 0 && offset / a >= 0
}

/// The 1-based position of `node` among its element siblings.
fn element_index(document: &Document, node: NodeId) -> i64 {
    let mut index: i64 = 1;
    let mut cursor = previous_element_sibling(document, node);
    while let Some(sibling) = cursor {
        index = index.saturating_add(1);
        cursor = previous_element_sibling(document, sibling);
    }
    index
}

/// The value of `node`'s attribute whose local name matches `name`
/// case-insensitively.
fn attribute_value<'a>(document: &'a Document, node: NodeId, name: &str) -> Option<&'a str> {
    document.attributes(node).find_map(|id| {
        let attribute = document.attr(id)?;
        attribute
            .local_name()
            .eq_ignore_ascii_case(name)
            .then(|| attribute.value())
    })
}

/// Whether `node` is an element node.
fn is_element(document: &Document, node: NodeId) -> bool {
    document.kind(node) == Some(NodeKind::Element)
}

/// The nearest element sibling before `node`.
fn previous_element_sibling(document: &Document, node: NodeId) -> Option<NodeId> {
    let mut cursor = document.previous_sibling(node);
    while let Some(candidate) = cursor {
        if is_element(document, candidate) {
            return Some(candidate);
        }
        cursor = document.previous_sibling(candidate);
    }
    None
}

/// The nearest element sibling after `node`.
fn next_element_sibling(document: &Document, node: NodeId) -> Option<NodeId> {
    let mut cursor = document.next_sibling(node);
    while let Some(candidate) = cursor {
        if is_element(document, candidate) {
            return Some(candidate);
        }
        cursor = document.next_sibling(candidate);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `html`, applies `selector`, and returns each matched element as
    /// `tag#id` (the `#id` part omitted when the element carries no `id`).
    fn select(html: &str, selector: &str) -> Vec<String> {
        let selectors = SelectorList::parse(selector)
            .unwrap_or_else(|error| panic!("`{selector}` should parse, got: {error}"));
        let parsed = oxixml_html::parse_document(html);
        let document = parsed.document();

        selectors
            .match_elements(document, parsed.root())
            .into_iter()
            .map(|id| {
                let tag = document
                    .element(id)
                    .map(|element| element.local_name().to_ascii_lowercase())
                    .unwrap_or_default();
                match attribute_value(document, id, "id") {
                    Some(value) => format!("{tag}#{value}"),
                    None => tag,
                }
            })
            .collect()
    }

    /// A document exercising ids, classes, attributes and sibling order.
    const FIXTURE: &str = r#"<html><body>
        <div class="post first" id="p1" data-kind="article x">
            <h2 id="h1">First</h2>
            <p id="t1">alpha</p>
            <p id="t2">beta</p>
        </div>
        <div class="post" id="p2" data-kind="article">
            <p id="t3">gamma</p>
        </div>
        <section id="s1"><p id="t4">delta</p></section>
    </body></html>"#;

    #[test]
    fn type_selector_matches_in_document_order() {
        assert_eq!(select(FIXTURE, "p"), ["p#t1", "p#t2", "p#t3", "p#t4"]);
    }

    #[test]
    fn type_selector_is_case_insensitive_on_both_sides() {
        assert_eq!(select("<html><body><P>x</P></body></html>", "p"), ["p"]);
        assert_eq!(select("<html><body><p>x</p></body></html>", "P"), ["p"]);
        assert_eq!(
            select("<html><body><DiV>x</DiV></body></html>", "dIv"),
            ["div"]
        );
    }

    #[test]
    fn universal_selector_matches_every_element() {
        let tags = select("<html><body><p>x</p></body></html>", "*");
        assert_eq!(tags, ["html", "head", "body", "p"]);
    }

    #[test]
    fn id_selector_matches_one_element() {
        assert_eq!(select(FIXTURE, "#t2"), ["p#t2"]);
        assert_eq!(select(FIXTURE, "#missing"), Vec::<String>::new());
    }

    #[test]
    fn id_matching_is_case_sensitive() {
        assert_eq!(select(FIXTURE, "#T2"), Vec::<String>::new());
    }

    #[test]
    fn class_selector_matches_one_word_of_the_class_attribute() {
        assert_eq!(select(FIXTURE, ".post"), ["div#p1", "div#p2"]);
        assert_eq!(select(FIXTURE, ".first"), ["div#p1"]);
        assert_eq!(select(FIXTURE, ".pos"), Vec::<String>::new());
    }

    #[test]
    fn class_matching_is_case_sensitive() {
        assert_eq!(select(FIXTURE, ".POST"), Vec::<String>::new());
    }

    #[test]
    fn compound_selectors_require_every_part() {
        assert_eq!(select(FIXTURE, "div.post.first"), ["div#p1"]);
        assert_eq!(select(FIXTURE, "section.post"), Vec::<String>::new());
        assert_eq!(select(FIXTURE, "div.post#p2"), ["div#p2"]);
    }

    #[test]
    fn attribute_operators_behave_as_specified() {
        assert_eq!(select(FIXTURE, "[data-kind]"), ["div#p1", "div#p2"]);
        assert_eq!(select(FIXTURE, "[data-kind=article]"), ["div#p2"]);
        assert_eq!(
            select(FIXTURE, "[data-kind~=article]"),
            ["div#p1", "div#p2"]
        );
        assert_eq!(select(FIXTURE, "[data-kind~=x]"), ["div#p1"]);
        assert_eq!(select(FIXTURE, "[data-kind^=art]"), ["div#p1", "div#p2"]);
        assert_eq!(select(FIXTURE, "[data-kind$=cle]"), ["div#p2"]);
        assert_eq!(select(FIXTURE, "[data-kind*=ticl]"), ["div#p1", "div#p2"]);
    }

    #[test]
    fn substring_operators_never_match_an_empty_value() {
        let html = "<html><body><p data-x=\"v\">t</p></body></html>";
        assert_eq!(select(html, "[data-x^='']"), Vec::<String>::new());
        assert_eq!(select(html, "[data-x$='']"), Vec::<String>::new());
        assert_eq!(select(html, "[data-x*='']"), Vec::<String>::new());
        // An exact test against the empty string is a real comparison, though.
        assert_eq!(
            select("<html><body><p data-x>t</p></body></html>", "[data-x='']"),
            ["p"]
        );
    }

    #[test]
    fn attribute_names_match_case_insensitively_but_values_do_not() {
        let html = "<html><body><p DATA-X=\"Val\">t</p></body></html>";
        assert_eq!(select(html, "[data-x]"), ["p"]);
        assert_eq!(select(html, "[DATA-X=Val]"), ["p"]);
        assert_eq!(select(html, "[data-x=val]"), Vec::<String>::new());
    }

    #[test]
    fn descendant_combinator_crosses_any_depth() {
        assert_eq!(select(FIXTURE, "div.post p"), ["p#t1", "p#t2", "p#t3"]);
        assert_eq!(select(FIXTURE, "body p"), ["p#t1", "p#t2", "p#t3", "p#t4"]);
    }

    #[test]
    fn child_combinator_requires_the_direct_parent() {
        assert_eq!(select(FIXTURE, "div.post > p"), ["p#t1", "p#t2", "p#t3"]);
        assert_eq!(select(FIXTURE, "body > p"), Vec::<String>::new());
        assert_eq!(select(FIXTURE, "body > div > p"), ["p#t1", "p#t2", "p#t3"]);
    }

    #[test]
    fn sibling_combinators_look_backwards_only() {
        assert_eq!(select(FIXTURE, "h2 + p"), ["p#t1"]);
        assert_eq!(select(FIXTURE, "h2 ~ p"), ["p#t1", "p#t2"]);
        assert_eq!(select(FIXTURE, "p + h2"), Vec::<String>::new());
        assert_eq!(select(FIXTURE, "div + section"), ["section#s1"]);
        assert_eq!(select(FIXTURE, "div ~ section"), ["section#s1"]);
    }

    #[test]
    fn combinators_chain_in_the_right_order() {
        assert_eq!(select(FIXTURE, "body > div.post h2 + p"), ["p#t1"]);
        assert_eq!(select(FIXTURE, "body div.post > h2 ~ p"), ["p#t1", "p#t2"]);
        assert_eq!(select(FIXTURE, "section > div p"), Vec::<String>::new());
    }

    #[test]
    fn structural_pseudo_classes_count_element_siblings_only() {
        assert_eq!(
            select(FIXTURE, "div.post > :first-child"),
            ["h2#h1", "p#t3"]
        );
        assert_eq!(select(FIXTURE, "div.post > :last-child"), ["p#t2", "p#t3"]);
        assert_eq!(select(FIXTURE, "div.post > p:first-child"), ["p#t3"]);
    }

    #[test]
    fn nth_child_selects_by_position() {
        let html = "<html><body><ul>\
            <li id=\"a\">1</li><li id=\"b\">2</li><li id=\"c\">3</li>\
            <li id=\"d\">4</li><li id=\"e\">5</li></ul></body></html>";

        assert_eq!(select(html, "li:nth-child(1)"), ["li#a"]);
        assert_eq!(select(html, "li:nth-child(odd)"), ["li#a", "li#c", "li#e"]);
        assert_eq!(select(html, "li:nth-child(even)"), ["li#b", "li#d"]);
        assert_eq!(select(html, "li:nth-child(2n+1)"), ["li#a", "li#c", "li#e"]);
        assert_eq!(
            select(html, "li:nth-child(n)"),
            ["li#a", "li#b", "li#c", "li#d", "li#e"]
        );
        assert_eq!(select(html, "li:nth-child(-n+2)"), ["li#a", "li#b"]);
        assert_eq!(select(html, "li:nth-child(3n)"), ["li#c"]);
        assert_eq!(select(html, "li:nth-child(0)"), Vec::<String>::new());
        assert_eq!(select(html, "li:nth-child(-1)"), Vec::<String>::new());
    }

    #[test]
    fn negation_excludes_matching_compounds() {
        assert_eq!(select(FIXTURE, "div.post > p:not(#t1)"), ["p#t2", "p#t3"]);
        assert_eq!(select(FIXTURE, "div:not(.first)"), ["div#p2"]);
        assert_eq!(
            select(FIXTURE, "div.post > *:not(h2, #t2)"),
            ["p#t1", "p#t3"]
        );
        assert_eq!(select(FIXTURE, "p:not(p)"), Vec::<String>::new());
    }

    #[test]
    fn selector_lists_union_their_matches_in_document_order() {
        assert_eq!(select(FIXTURE, "h2, section"), ["h2#h1", "section#s1"]);
        assert_eq!(select(FIXTURE, "#t4, #t1"), ["p#t1", "p#t4"]);
    }

    #[test]
    fn quirky_html_still_yields_a_usable_tree() {
        // Unclosed tags, implied `<tbody>`, and mixed-case markup.
        let html = "<DIV CLASS=Post><P>one<P>two</DIV>\
            <table><tr><td>cell</td></tr></table>";

        assert_eq!(select(html, "div.Post p"), ["p", "p"]);
        assert_eq!(select(html, "table tbody tr td"), ["td"]);
        assert_eq!(select(html, "p + p"), ["p"]);
    }

    #[test]
    fn a_document_without_html_or_body_tags_is_still_matched() {
        assert_eq!(select("<p>bare</p>", "html > body > p"), ["p"]);
    }

    #[test]
    fn deeply_nested_documents_do_not_blow_up() {
        // The parser caps nesting at its own depth limit; a selector chaining
        // the maximum number of descendant combinators over it must still
        // terminate quickly.
        let depth = 200;
        let html = format!(
            "<html><body>{}<i id=\"leaf\">x</i>{}</body></html>",
            "<div>".repeat(depth),
            "</div>".repeat(depth)
        );

        assert_eq!(select(&html, "div div div div div div div i"), ["i#leaf"]);
        assert_eq!(select(&html, "i:not(div)"), ["i#leaf"]);
    }

    #[test]
    fn wide_sibling_lists_do_not_blow_up() {
        let width = 1_000;
        let mut html = String::from("<html><body><ul>");
        for index in 0..width {
            html.push_str(&format!("<li id=\"n{index}\">{index}</li>"));
        }
        html.push_str("</ul></body></html>");

        // Every `~` step re-walks the sibling chain; the visited filter keeps
        // that linear rather than quadratic in the number of candidates.
        let matched = select(&html, "li ~ li ~ li ~ li ~ li ~ li");
        assert_eq!(matched.len(), width - 5);
        assert_eq!(matched.first().map(String::as_str), Some("li#n5"));
    }

    #[test]
    fn nth_arithmetic_handles_the_edges() {
        assert!(matches_nth(Nth { a: 0, b: 1 }, 1));
        assert!(!matches_nth(Nth { a: 0, b: 1 }, 2));
        assert!(matches_nth(Nth { a: 1, b: 0 }, 7));
        assert!(matches_nth(Nth { a: -1, b: 3 }, 3));
        assert!(!matches_nth(Nth { a: -1, b: 3 }, 4));
        assert!(matches_nth(Nth { a: 2, b: -1 }, 1));
        assert!(!matches_nth(Nth { a: 2, b: -1 }, 2));
        // Extreme coefficients must not overflow or panic.
        assert!(!matches_nth(
            Nth {
                a: i32::MAX,
                b: i32::MIN
            },
            1
        ));
    }
}
