//! Convenience queries for [`ungrammar`].
//!
//! `ungrammar` is deliberately small and does not expose an API for extracting higher-level
//! structural information.
//!
//! This crate provides [`Analysis`], a companion view for name lookup ([`Node`]
//! and [`Token`]) and rule characterisation ([`Symbol`] to [`Cardinality`]).
//!
//! ```
//! use std::collections::HashMap;
//! use ungrammar_analysis::{Analysis, Cardinality, Symbol};
//!
//! let grammar = "
//!     A = B 'c' | 'c'
//!     B = 'b'
//! "
//! .parse()
//! .unwrap();
//!
//! let analysis = Analysis::from(&grammar);
//!
//! let node_a = analysis.node("A").unwrap();
//! let node_b = analysis.node("B").unwrap();
//! let token_c = analysis.token("c").unwrap();
//!
//! let symbols = analysis
//!     .rule_analysis(node_a)
//!     .unwrap()
//!     .symbols()
//!     .collect::<HashMap<_, _>>();
//!
//! assert_eq!(symbols[&node_b.into()], Cardinality::Optional);
//! assert_eq!(symbols[&token_c.into()], Cardinality::One);
//! ```
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(rustdoc::broken_intra_doc_links)]

use std::collections::HashMap;
use ungrammar::{Grammar, Node, Rule, Token};

/// A precomputed view of a [`Grammar`].
#[derive(Debug)]
pub struct Analysis<'g> {
    // NB: Node and Token are u16 handles
    nodes_by_name: HashMap<&'g str, Node>,
    tokens_by_name: HashMap<&'g str, Token>,
    rules: HashMap<Node, RuleAnalysis>,
}

impl Analysis<'_> {
    /// Looks up a node by name.
    pub fn node(&self, name: &str) -> Option<Node> {
        self.nodes_by_name.get(name).copied()
    }

    /// Looks up a token by name (without the surrounding quotes.)
    pub fn token(&self, name: &str) -> Option<Token> {
        self.tokens_by_name.get(name).copied()
    }

    /// Returns the precomputed analysis for a node's rule.
    pub fn rule_analysis(&self, node: Node) -> Option<&RuleAnalysis> {
        self.rules.get(&node)
    }
}

impl<'g> From<&'g Grammar> for Analysis<'g> {
    fn from(grammar: &'g Grammar) -> Self {
        let nodes_by_name = grammar
            .iter()
            .map(|node| (grammar[node].name.as_str(), node))
            .collect();

        let tokens_by_name = grammar
            .tokens()
            .map(|token| (grammar[token].name.as_str(), token))
            .collect();

        let rules = grammar
            .iter()
            .map(|node| (node, RuleAnalysis::from(&grammar[node].rule)))
            .collect();

        Self {
            nodes_by_name,
            tokens_by_name,
            rules,
        }
    }
}

/// A node or token mentioned by a [`Rule`].
///
/// In `A = B 'c'`, the symbols are `B` ([`Symbol::NonTerminal`]) and
/// `'c'` ([`Symbol::Terminal`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol {
    /// A node.
    NonTerminal(Node),
    /// A token.
    Terminal(Token),
}

impl From<Node> for Symbol {
    fn from(node: Node) -> Self {
        Self::NonTerminal(node)
    }
}

impl From<Token> for Symbol {
    fn from(token: Token) -> Self {
        Self::Terminal(token)
    }
}

/// How often a [`Symbol`] may appear inside a [`Rule`].
///
/// This is a deliberately coarse summary. For example, symbols from `B*` and
/// `B B` are both reported as [`Cardinality::Many`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cardinality {
    /// The symbol is required exactly once.
    ///
    /// In `A = B 'c'`, both symbols have [`Cardinality::One`].
    One,
    /// The symbol may be absent, but appears at most once.
    ///
    /// In `A = B? | C`, both symbols have [`Cardinality::Optional`].
    Optional,
    /// The symbol may be absent or appear any number of times.
    ///
    /// In `A = B* | C C`, both symbols have [`Cardinality::Many`].
    Many,
}

impl Cardinality {
    /// What happens to the current cardinality if we apply `_?` to it.
    fn optional(self) -> Self {
        match self {
            Self::Many => Self::Many,
            _ => Self::Optional,
        }
    }

    /// What happens to the current cardinality if we apply `_*` to it.
    fn repeated(self) -> Self {
        Self::Many
    }

    /// What happens to the current cardinality if we sequence (`_ _`) it with another.
    fn sequence(self, _right: Self) -> Self {
        // NB: it is this simple because we don't distinguish between 1+ and 0+ (Many)
        Self::Many
    }

    /// What happens to the current cardinality if we apply `_ | _` to it.
    fn alternate(self, right: Self) -> Self {
        match (self, right) {
            (Self::One, Self::One) => Self::One,
            (Self::Many, _) | (_, Self::Many) => Self::Many,
            (Self::Optional, _) | (_, Self::Optional) => Self::Optional,
        }
    }
}

/// Precomputed facts about one [`Rule`].
#[derive(Debug)]
pub struct RuleAnalysis {
    symbols: HashMap<Symbol, Cardinality>,
}

impl RuleAnalysis {
    /// Iterates over the symbols mentioned by the rule and their cardinalities.
    ///
    /// In `A = B C | C`, the symbols are `B` and `C` with
    /// cardinalities [`Cardinality::Optional`] and [`Cardinality::One`] respectively.
    pub fn symbols(&self) -> impl Iterator<Item = (Symbol, Cardinality)> + '_ {
        self.symbols
            .iter()
            .map(|(&symbol, &cardinality)| (symbol, cardinality))
    }
}

impl From<&Rule> for RuleAnalysis {
    fn from(rule: &Rule) -> Self {
        let symbols = analyze_rule(rule).0;
        Self { symbols }
    }
}

/// A builder for [`RuleAnalysis`].
// NB: currently only tracking symbols and their cardinalities.
// If we later extend [`RuleAnalysis`] with more information,
// it's probably best to rename this.
#[derive(Debug, Default)]
struct Builder(HashMap<Symbol, Cardinality>);

impl Builder {
    fn symbol(mut self, symbol: Symbol) -> Self {
        self.0.insert(symbol, Cardinality::One);
        self
    }

    fn optional(mut self) -> Self {
        self.0.values_mut().for_each(|c| *c = c.optional());
        self
    }

    fn repeated(mut self) -> Self {
        self.0.values_mut().for_each(|c| *c = c.repeated());
        self
    }

    fn sequence(mut self, right: Self) -> Self {
        for (symbol, cardinality) in right.0 {
            self.0
                .entry(symbol)
                .and_modify(|c| *c = c.sequence(cardinality))
                .or_insert(cardinality);
        }
        self
    }

    fn alternate(mut self, right: Self) -> Self {
        let mut right = right;

        for (symbol, c) in &mut self.0 {
            *c = match right.0.remove(symbol) {
                None => c.optional(),
                Some(cardinality) => c.alternate(cardinality),
            };
        }

        for (symbol, cardinality) in right.0 {
            self.0.insert(symbol, cardinality.optional());
        }

        self
    }
}

fn analyze_rule(rule: &Rule) -> Builder {
    match rule {
        Rule::Token(token) => Builder::default().symbol(Symbol::Terminal(*token)),
        Rule::Node(node) => Builder::default().symbol(Symbol::NonTerminal(*node)),
        Rule::Labeled { rule, .. } => analyze_rule(rule),
        Rule::Opt(rule) => analyze_rule(rule).optional(),
        Rule::Rep(rule) => analyze_rule(rule).repeated(),
        Rule::Seq(rules) => rules
            .iter()
            .map(analyze_rule)
            .fold(Builder::default(), Builder::sequence),
        Rule::Alt(rules) => {
            let mut rules = rules.iter().map(analyze_rule);
            if let Some(head) = rules.next() {
                rules.fold(head, Builder::alternate)
            } else {
                Builder::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_finds_nodes_and_tokens() {
        let grammar = r"
            A = B ','
            B = 'b'
            "
        .parse()
        .unwrap();
        let analysis = Analysis::from(&grammar);

        assert!(analysis.node("A").is_some());
        assert!(analysis.node("B").is_some());
        assert!(analysis.token(",").is_some());
        assert!(analysis.token("b").is_some());
        assert!(analysis.node("c").is_none());
    }

    #[test]
    fn rule_analysis_for_alternation_with_optional_and_duplicate_symbols() {
        let grammar = "A = 'b' 'c' | 'c'".parse().unwrap();
        let analysis = Analysis::from(&grammar);
        let node_a = analysis.node("A").unwrap();
        let token_b = analysis.token("b").unwrap();
        let token_c = analysis.token("c").unwrap();
        let rule_a = analysis.rule_analysis(node_a).unwrap();

        assert_eq!(
            rule_a.symbols().collect::<HashMap<_, _>>(),
            HashMap::from([
                (token_b.into(), Cardinality::Optional),
                (token_c.into(), Cardinality::One)
            ])
        )
    }

    #[test]
    fn rule_analysis_for_optional_symbol() {
        let grammar = "A = 'b'?".parse().unwrap();
        let analysis = Analysis::from(&grammar);
        let node_a = analysis.node("A").unwrap();
        let token_b = analysis.token("b").unwrap();
        let rule_a = analysis.rule_analysis(node_a).unwrap();

        assert_eq!(
            rule_a.symbols().collect::<HashMap<_, _>>(),
            HashMap::from([(token_b.into(), Cardinality::Optional)])
        )
    }

    #[test]
    fn rule_analysis_for_repeated_over_optional() {
        let grammar = "A = ('b'?)*".parse().unwrap();
        let analysis = Analysis::from(&grammar);
        let node_a = analysis.node("A").unwrap();
        let token_b = analysis.token("b").unwrap();
        let rule_a = analysis.rule_analysis(node_a).unwrap();

        assert_eq!(
            rule_a.symbols().collect::<HashMap<_, _>>(),
            HashMap::from([(token_b.into(), Cardinality::Many)])
        )
    }

    #[test]
    fn rule_analysis_for_optional_over_repeated() {
        let grammar = "A = ('b'*)?".parse().unwrap();
        let analysis = Analysis::from(&grammar);
        let node_a = analysis.node("A").unwrap();
        let token_b = analysis.token("b").unwrap();
        let rule_a = analysis.rule_analysis(node_a).unwrap();

        assert_eq!(
            rule_a.symbols().collect::<HashMap<_, _>>(),
            HashMap::from([(token_b.into(), Cardinality::Many)])
        )
    }
}
