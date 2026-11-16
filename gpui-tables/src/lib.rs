//! Notion-style database views for GPUI apps: tables, boards and lists over rows the host owns.
//!
//! A database is a doc with properties (typed columns) and saved views; its rows are docs too,
//! each a page the host opens. This crate reads them as JSON ([`model`]), works out what a view
//! shows ([`engine`]: filter, sort, group, relations), places things with fractional indexes
//! ([`order`]), and emits every change as an [`Edit`] for the host to store. It never stores,
//! fetches or navigates itself.
//!
//! - [`model`], [`engine`] and [`order`] have no GPUI in them: with `default-features = false`
//!   they're all that builds, for a server or the non-UI part of an app.
//! - [`view`] (the default `gpui` feature) is the table and board views, drawn with GPUI and
//!   gpui-core. Lists come next; the README says what they'll be.
//!
//! The README is the design note, with the JSON and an example.

pub mod engine;
pub mod model;
pub mod order;
#[cfg(feature = "gpui")]
pub mod view;

pub use model::*;

// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
