//! OpenUI cards (openui.com): the UI an agent answers with, written in OpenUI Lang, read here
//! into a typed tree of components a native app can draw. OpenUI's own React renderer draws the
//! same programs with @openuidev/react-lang; this reads them the way its parser, lang-core
//! 1.1.0, does (syntax.rs, parse.rs, eval.rs follow it step by step), and the golden tests
//! compare the two on the same inputs (tests/openui/golden.mjs).
//!
//! This module has no GPUI in it: with `default-features = false` it's all the crate builds.
//!
//! Using it: [`parse`] an agent's program once into a [`Program`]; give it a [`Store`] (the
//! card's $variables and form fields, [`Program::store`] at first); [`Program::render`] the
//! [`Card`] to draw; when the person using it presses something, [`Program::act`] runs its
//! [`Action`] against the store and says what to send the agent or open, after which the card
//! renders again from the changed store.
//!
//! Every string in a program comes from an agent, so it's treated as hostile: the work, the
//! nesting, the number of components and the size of every list and text are capped (the MAX_
//! constants below), text is only ever text (never HTML), a link is kept only when it's http or
//! https, an image source is only ever data for the host to judge ([`ImageSrc`]), and anything
//! malformed gives a [`CardError`], never a panic.

mod action;
mod chart;
mod eval;
mod model;
mod parse;
mod schema;
mod syntax;
mod value;

#[cfg(test)]
mod tests;

pub use action::{Action, Data, Effect, Expr, Link, Rule, Rules, Step, Store};
pub use chart::{
    Bars, Chart, ChartKind, Curve, Scatter, ScatterPoint, ScatterSet, Series, Slice, Slices,
    SlicesKind,
};
pub use model::*;
pub use parse::Issue;

use value::V;
#[cfg(feature = "gpui")]
pub(crate) use value::num_text;

/// The most program text a card may have. Real cards are a few kilobytes.
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
/// How deep reading and evaluating may recurse: brackets inside brackets, references followed
/// through references, expressions inside expressions, links in a chain like a + b + c. Real
/// cards reach about 20. The cap is what fits a 512 KB stack (a GCD worker's, where GPUI's
/// background executor runs on macOS) with half of it to spare in a debug build, where frames are biggest.
pub const MAX_DEPTH: usize = 64;
/// How many components deep the drawn tree may go. Real cards reach about 10; a component
/// costs about three levels of MAX_DEPTH, so this is the cap a card nested by references meets
/// first.
pub const MAX_NESTING: usize = 20;
/// The most components one card may draw.
pub const MAX_COMPONENTS: usize = 2_000;
/// The most children, items, options or tags one component keeps; more are left off.
pub const MAX_CHILDREN: usize = 200;
/// The most rows a table keeps.
pub const MAX_ROWS: usize = 500;
/// The most columns a table keeps.
pub const MAX_COLUMNS: usize = 24;
/// The most points a chart keeps per series (and labels), or per scatter dataset.
pub const MAX_POINTS: usize = 1_000;
/// The most keys one object in a program keeps; more are left off.
pub const MAX_KEYS: usize = 1_000;
/// The most series (or scatter datasets) a chart keeps.
pub const MAX_SERIES: usize = 24;
/// The most characters one text keeps; a longer one ends in an ellipsis.
pub const MAX_TEXT: usize = 20_000;
/// The most characters of text one card may draw in all.
pub const MAX_TOTAL_TEXT: usize = 1_000_000;
/// The most steps of work reading and evaluating a card may take.
pub const MAX_WORK: usize = 2_000_000;
/// The most bytes a string computed while evaluating may grow to.
pub(crate) const MAX_EVAL_TEXT: usize = 100_000;
/// The most issues kept about one card.
pub(crate) const MAX_ISSUES: usize = 200;

/// Why a card can't be shown. A host shows "This card couldn't be shown" (or the message's
/// text) in its place; the reason is for logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardError {
    /// More than MAX_SOURCE_BYTES of program.
    TooLarge,
    /// Nested deeper than MAX_DEPTH or MAX_NESTING.
    TooDeep,
    /// More than MAX_WORK steps, MAX_COMPONENTS components or MAX_TOTAL_TEXT characters.
    TooMuch,
    /// No root component: nothing parsed, or the root isn't a component (where the web shows
    /// the message's text instead).
    NoRoot,
}

impl CardError {
    /// What a host says in the card's place.
    pub const SHOWN: &'static str = "This card couldn't be shown.";
}

impl std::fmt::Display for CardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CardError::TooLarge => "the card is too large",
            CardError::TooDeep => "the card nests too deeply",
            CardError::TooMuch => "the card asks for too much work",
            CardError::NoRoot => "the card has no root component",
        })
    }
}

impl std::error::Error for CardError {}

/// The work and depth a card has used; once over either limit everything stops and the card
/// fails with the reason.
pub(crate) struct Budget {
    work: usize,
    depth: usize,
    #[cfg(test)]
    peak: usize,
    pub failed: Option<CardError>,
}

impl Budget {
    pub fn new() -> Budget {
        Budget {
            work: 0,
            depth: 0,
            #[cfg(test)]
            peak: 0,
            failed: None,
        }
    }

    /// One step of work, one level deeper; false once over budget (and then nothing is entered,
    /// so the caller doesn't leave).
    pub fn enter(&mut self) -> bool {
        if self.failed.is_some() {
            return false;
        }
        self.work += 1;
        if self.work > MAX_WORK {
            self.failed = Some(CardError::TooMuch);
            return false;
        }
        if self.depth >= MAX_DEPTH {
            self.failed = Some(CardError::TooDeep);
            return false;
        }
        self.depth += 1;
        #[cfg(test)]
        {
            self.peak = self.peak.max(self.depth);
        }
        true
    }

    pub fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Work done in a loop, n steps at once.
    pub fn charge(&mut self, n: usize) {
        self.work = self.work.saturating_add(n);
        if self.work > MAX_WORK && self.failed.is_none() {
            self.failed = Some(CardError::TooMuch);
        }
    }
}

/// A card's program, read and checked: what [`Program::render`] draws from.
pub struct Program {
    parsed: parse::Parsed,
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program")
            .field("issues", &self.parsed.issues)
            .finish_non_exhaustive()
    }
}

/// Reads an agent's OpenUI Lang program (a message's `ui`).
pub fn parse(source: &str) -> Result<Program, CardError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(CardError::TooLarge);
    }
    let mut budget = Budget::new();
    let parsed = parse::parse(source, &mut budget)?;
    if parsed.root.is_none() {
        return Err(CardError::NoRoot);
    }
    Ok(Program { parsed })
}

impl Program {
    /// The card's state before anyone has done anything: each $variable at its default.
    pub fn store(&self) -> Store {
        Store::new(&self.parsed.state)
    }

    /// What was dropped or read differently from how it was written; the card shows the rest.
    pub fn issues(&self) -> &[Issue] {
        &self.parsed.issues
    }

    /// The card as it stands with this state.
    pub fn render(&self, store: &Store) -> Result<Card, CardError> {
        let root = self.evaluate(store)?;
        model::build(&root)
    }

    fn evaluate(&self, store: &Store) -> Result<V, CardError> {
        let mut budget = Budget::new();
        let state = |name: &str| store.state(name);
        let root = eval::Eval::new(&mut budget, &state).root(self.parsed.root.as_ref(), None);
        if let Some(e) = budget.failed {
            return Err(e);
        }
        root.ok_or(CardError::NoRoot)
    }

    /// Runs what was pressed: `words` are what the press says (a button's label, a card's
    /// title), `form` the form it was pressed in. $variables change in the store; what's left
    /// for the host (a message to send the agent as the user, a link to open) comes back in order.
    pub fn act(
        &self,
        action: &Action,
        words: &str,
        form: Option<&str>,
        store: &mut Store,
    ) -> Vec<Effect> {
        action::run(action, words, form, store, &self.parsed.state)
    }
}
