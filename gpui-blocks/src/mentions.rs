//! @-mentions: typing `@` lists the people, agents, pages and dates the host offers, and picking
//! one puts an inline mention in the text.
//!
//! The editor knows nothing about who or what can be mentioned: the host gives it a
//! [`Mentions`] with [`Options::mentions`](crate::Options::mentions), and is asked for candidates
//! as the query is typed. Without one, `@` is an ordinary character.
//!
//! A mention is an ordinary link whose destination says what it is, so any Markdown tool shows a
//! link:
//!
//! ```markdown
//! Ask [Finances](agent:finances) about [Jordan Lee](person:jordan) and [Roadmap](doc:4f2a)
//! before [Nov 20, 2026](date:2026-11-20).
//! ```
//!
//! - **People** (`person:`) and **agents** (`agent:`) are drawn as `@Name` chips.
//! - **Pages** are the document mentions of [`crate::docs`] (`doc:`), with the app's title.
//! - **Dates** (`date:` and an ISO date) are drawn as a date chip. The editor offers Today,
//!   Tomorrow, Yesterday, the coming weekdays and typed dates (`2026-12-01`) when the host
//!   tells it today's date ([`Mentions::today`]).
//!
//! Each is one unit, like a picture: the caret steps over it and Backspace deletes it whole. A
//! click sends [`EditorEvent::MentionClicked`](crate::EditorEvent::MentionClicked) (a page's,
//! [`EditorEvent::DocClicked`](crate::EditorEvent::DocClicked)); the editor opens nothing itself.
//!
//! The menu follows the slash menu's rules: `@` opens it at the start of a line or after a space
//! or `(`, the text typed after it narrows the list (it may hold spaces, not lead with one), ↑
//! and ↓ move, Enter or Tab picks, Esc closes it and leaves the text, and it closes on its own
//! once nothing matches. Picking replaces the `@…` with the mention and a space, as one undo step.

use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{ActiveTheme as _, Icon, IconName};
use markdown::mdast::{Link, Node};
use markdown_editor::{BlockId, BlockKind, CustomNode, EditorCommand, Extension, InlineContext};

use crate::{Editor, docs};

/// What a mention is of.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MentionKind {
    /// A person: `[Name](person:id)`.
    Person,
    /// An agent: `[Name](agent:id)`, the form a comment uses to wake one.
    Agent,
    /// A document: `[Title](doc:id)`, as [`crate::docs`] writes it.
    Page,
    /// A day: `[Nov 20, 2026](date:2026-11-20)`.
    Date,
}

impl MentionKind {
    /// The link destination's prefix that marks this kind.
    pub fn scheme(self) -> &'static str {
        match self {
            Self::Person => "person:",
            Self::Agent => "agent:",
            Self::Page => docs::SCHEME,
            Self::Date => "date:",
        }
    }

    /// The heading the menu lists this kind under, unless the candidate names its own.
    pub fn group(self) -> &'static str {
        match self {
            Self::Person => "People",
            Self::Agent => "Agents",
            Self::Page => "Pages",
            Self::Date => "Dates",
        }
    }

    fn of_url(url: &str) -> Option<(Self, &str)> {
        [Self::Person, Self::Agent, Self::Date]
            .into_iter()
            .find_map(|kind| Some((kind, url.strip_prefix(kind.scheme())?)))
            .filter(|(kind, id)| kind.valid_id(id))
    }

    fn valid_id(self, id: &str) -> bool {
        match self {
            Self::Date => Date::parse(id).is_some(),
            Self::Page => docs::doc_id(&format!("{}{id}", docs::SCHEME)).is_some(),
            Self::Person | Self::Agent => {
                (1..=MAX_ID).contains(&id.len())
                    && id.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')
                    })
            }
        }
    }
}

/// The longest person or agent id the editor reads.
const MAX_ID: usize = 128;

/// One thing the menu offers. Make one with [`Mention::new`] and its builder methods.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Mention {
    pub kind: MentionKind,
    /// What the link points at: 1 to 128 of `A-Z a-z 0-9 - _ . ~` for a person, an agent or a
    /// page, and `YYYY-MM-DD` for a date.
    pub id: SharedString,
    /// The name shown in the menu and written as the link's text.
    pub title: SharedString,
    /// A glyph or emoji for the menu's tile; the kind's icon when `None`.
    pub icon: Option<SharedString>,
    /// A second line in the menu: a role, a page's path.
    pub hint: SharedString,
    /// The heading it's listed under: the kind's ([`MentionKind::group`]) unless set.
    pub group: SharedString,
}

impl Mention {
    pub fn new(
        kind: MentionKind,
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
    ) -> Self {
        Self {
            kind,
            id: id.into(),
            title: title.into(),
            icon: None,
            hint: SharedString::default(),
            group: kind.group().into(),
        }
    }

    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = group.into();
        self
    }

    /// The Markdown the mention is written as, or `None` for an id its kind doesn't allow.
    pub fn markdown(&self) -> Option<String> {
        if !self.kind.valid_id(&self.id) {
            return None;
        }
        Some(match self.kind {
            MentionKind::Page => docs::mention(&self.id, &self.title),
            kind => format!(
                "[{}]({}{})",
                docs::link_text(&self.title),
                kind.scheme(),
                self.id
            ),
        })
    }
}

/// Who and what can be mentioned: the app's people, agents and pages.
pub trait Mentions: Send + Sync + 'static {
    /// The candidates for `query` (what's typed after the `@`, maybe empty), in the order to
    /// list them. Candidates of one group are listed together, groups in the order they first
    /// appear. The editor keeps those whose title holds the query, whatever its case, so
    /// returning every candidate is fine; it shows at most 50. Called on each keystroke while
    /// the menu is open: keep it cheap, from a cache the app keeps.
    fn candidates(&self, query: &str, cx: &App) -> Vec<Mention>;

    /// Today, where the person is, for the Dates group. `None` (the default) leaves dates out.
    fn today(&self, _cx: &App) -> Option<Date> {
        None
    }
}

/// A day of the proleptic Gregorian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    year: i32,
    month: u8,
    day: u8,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
/// Monday first.
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

impl Date {
    /// The day, or `None` when there's no such day (February 30th, month 13).
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        (1..=days).contains(&day).then_some(Self {
            year,
            month: month as u8,
            day: day as u8,
        })
    }

    /// A date written `YYYY-MM-DD`, years 1000 to 9999.
    pub fn parse(iso: &str) -> Option<Self> {
        let b = iso.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let num = |s: &str| -> Option<u32> {
            s.bytes()
                .all(|c| c.is_ascii_digit())
                .then(|| s.parse().ok())?
        };
        let year = num(&iso[..4])?;
        (year >= 1000).then_some(())?;
        Self::new(year as i32, num(&iso[5..7])?, num(&iso[8..])?)
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u32 {
        self.month.into()
    }

    pub fn day(self) -> u32 {
        self.day.into()
    }

    /// `YYYY-MM-DD`.
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// As a mention shows it: `Nov 20, 2026`.
    pub fn label(self) -> String {
        format!(
            "{} {}, {}",
            MONTHS[usize::from(self.month) - 1],
            self.day,
            self.year
        )
    }

    /// 0 for Monday to 6 for Sunday.
    pub fn weekday(self) -> u32 {
        // 1970-01-01 was a Thursday.
        (self.days() + 3).rem_euclid(7) as u32
    }

    /// The day `n` days later (earlier for a negative `n`).
    pub fn add_days(self, n: i64) -> Self {
        Self::from_days(self.days() + n)
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    fn days(self) -> i64 {
        let (m, d) = (i64::from(self.month), i64::from(self.day));
        let y = i64::from(self.year) - i64::from(m <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    fn from_days(z: i64) -> Self {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self { year, month, day }
    }
}

/// The dates the menu offers for `query`, given `today`: Today, Tomorrow and Yesterday while
/// nothing is typed; then also the coming weekdays ("Next Friday", the first Friday after today)
/// and a typed `YYYY-MM-DD`.
pub fn dates(query: &str, today: Date) -> Vec<Mention> {
    let q = query.trim().to_lowercase();
    let date = |title: String, day: Date| {
        Mention::new(MentionKind::Date, day.iso(), day.label()).hint(title)
    };
    let mut out = Vec::new();
    if let Some(day) = Date::parse(&q) {
        out.push(date(day.label(), day));
    }
    let near = [("Today", 0), ("Tomorrow", 1), ("Yesterday", -1)];
    for (title, n) in near {
        if title.to_lowercase().starts_with(&q) {
            out.push(date(title.to_string(), today.add_days(n)));
        }
    }
    if !q.is_empty() {
        for (ix, name) in WEEKDAYS.iter().enumerate() {
            let title = format!("Next {name}");
            let lower = title.to_lowercase();
            if lower.starts_with(&q) || name.to_lowercase().starts_with(&q) {
                let ahead = (ix as i64 - i64::from(today.weekday())).rem_euclid(7);
                let ahead = if ahead == 0 { 7 } else { ahead };
                out.push(date(title, today.add_days(ahead)));
            }
        }
    }
    out
}

/// Where a mention is being typed: the byte offset of its `@` in the caret's line and the query
/// after it. `prefix` is the line up to the caret. An `@` counts at the start, after a space or
/// after `(`, so an address's (`a@b.com`) doesn't open the menu; the query may hold spaces but
/// doesn't start with one, and is at most 32 characters.
pub fn query_at(prefix: &str) -> Option<(usize, &str)> {
    let at = prefix.rfind('@')?;
    let before = &prefix[..at];
    if !(before.is_empty() || before.ends_with(char::is_whitespace) || before.ends_with('(')) {
        return None;
    }
    let query = &prefix[at + 1..];
    if query.starts_with(char::is_whitespace)
        || query.contains(['\n', '\u{FFFC}'])
        || query.chars().count() > 32
    {
        return None;
    }
    Some((at, query))
}

/// The most the menu lists.
const MAX_SHOWN: usize = 50;

/// What the menu lists for `query`: the host's candidates whose title holds it (titles that
/// start with it first within each group), grouped in the order groups first appear, then the
/// dates.
pub(crate) fn matches(mentions: &dyn Mentions, query: &str, cx: &App) -> Vec<Mention> {
    let q = query.to_lowercase();
    let candidates: Vec<Mention> = mentions
        .candidates(query, cx)
        .into_iter()
        .filter(|m| m.kind != MentionKind::Date && m.kind.valid_id(&m.id))
        .collect();
    // Groups keep the order the host gives them, whatever is typed, so they don't jump.
    let mut groups: Vec<&SharedString> = Vec::new();
    for m in &candidates {
        if !groups.contains(&&m.group) {
            groups.push(&m.group);
        }
    }
    let mut found: Vec<(usize, bool, usize, &Mention)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(n, m)| {
            let title = m.title.to_lowercase();
            let group = groups.iter().position(|g| **g == m.group)?;
            title
                .contains(&q)
                .then(|| (group, !title.starts_with(&q), n, m))
        })
        .collect();
    found.sort_by_key(|(group, later, n, _)| (*group, *later, *n));
    let mut out: Vec<Mention> = found.into_iter().map(|(.., m)| m.clone()).collect();
    if let Some(today) = mentions.today(cx) {
        out.extend(dates(query, today));
    }
    out.truncate(MAX_SHOWN);
    out
}

/// A person, agent or date mention as the editor keeps it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MentionRef {
    pub(crate) kind: MentionKind,
    pub(crate) id: SharedString,
    pub(crate) title: SharedString,
}

impl MentionRef {
    fn from_link(link: &Link) -> Option<Self> {
        let (kind, id) = MentionKind::of_url(&link.url)?;
        let title = docs::plain_text(&link.children);
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let title = match (title.is_empty(), kind) {
            (false, _) => title,
            (true, MentionKind::Date) => Date::parse(id).map(Date::label).unwrap_or_default(),
            (true, _) => id.to_string(),
        };
        Some(Self {
            kind,
            id: id.to_string().into(),
            title: title.into(),
        })
    }
}

/// Called with a mention's kind and id when it's clicked.
pub(crate) type OnMentionClick =
    Arc<dyn Fn(MentionKind, &str, &mut Window, &mut App) + Send + Sync>;

/// What the chips do: the editor to tell about clicks; empty for a document read without one.
#[derive(Clone, Default)]
pub(crate) struct MentionHooks {
    pub(crate) on_click: Option<OnMentionClick>,
}

/// The custom node's name for a person, agent or date mention.
pub(crate) const CHIP: &str = "mention-chip";

/// Inline person, agent and date mentions: `[Name](person:id)`, `[Name](agent:id)` and
/// `[Nov 20, 2026](date:2026-11-20)` inside text.
pub(crate) struct ChipPlugin(pub(crate) MentionHooks);

impl Extension for ChipPlugin {
    fn name(&self) -> &str {
        CHIP
    }

    fn parse(&self, node: &Node) -> Option<CustomNode> {
        let Node::Link(link) = node else {
            return None;
        };
        let mention = MentionRef::from_link(link)?;
        let label = match mention.kind {
            MentionKind::Date => format!("Date {}", mention.title),
            _ => format!("Mention of {}", mention.title),
        };
        let markdown =
            Mention::new(mention.kind, mention.id.clone(), mention.title.clone()).markdown()?;
        Some(
            CustomNode::new(CHIP, mention.clone())
                .text(mention.title.clone())
                .accessibility_label(label)
                .markdown(markdown),
        )
    }

    fn render_inline(
        &self,
        node: &CustomNode,
        line_ctx: InlineContext,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, Pixels)> {
        let mention = node.data::<MentionRef>()?;
        let theme = cx.theme();
        let accent = theme.accent;
        let muted = theme.muted_foreground;
        let size = line_ctx.font_size;
        let line = line_ctx.line_height;
        let (kind, id) = (mention.kind, mention.id.clone());
        let on_click = self.0.on_click.clone();
        let lead = match mention.kind {
            MentionKind::Date => Icon::new(IconName::Calendar)
                .size(size * 0.85)
                .color(muted)
                .into_any_element(),
            _ => div().text_color(muted).child("@").into_any_element(),
        };
        let chip = div()
            .debug_selector({
                let id = mention.id.clone();
                move || format!("blocks-chip-{id}")
            })
            .id(SharedString::from(format!("blocks-chip-{}", mention.id)))
            .h(line)
            .max_w(px(480.))
            .flex()
            .items_center()
            .gap(size * 0.15)
            .px(px(2.))
            .rounded(px(4.))
            .cursor_pointer()
            .hover(move |s| s.bg(accent))
            .text_size(size)
            .line_height(line)
            .text_color(theme.foreground)
            .font_weight(FontWeight::MEDIUM)
            .child(div().flex_none().flex().items_center().child(lead))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(mention.title.clone()),
            )
            // A press here is the chip's, not the editor's caret.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                if let Some(on_click) = &on_click {
                    on_click(kind, &id, window, cx);
                }
            });
        Some((chip.into_any_element(), (line + size * 0.7) / 2.))
    }
}

/// The open @ menu: where its `@` is, what's typed after it, and what it lists.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MentionMenu {
    block: BlockId,
    /// Byte offset of the `@` in the caret's line.
    at: usize,
    query: String,
    pub(crate) matches: Vec<Mention>,
    pub(crate) selected: usize,
}

impl Editor {
    /// Opens, narrows or closes the @ menu from the caret's line, as `refresh_slash` does the
    /// slash menu: only typing opens it, and only when the host gave the editor its mentions.
    pub(crate) fn refresh_mention(&mut self, may_open: bool, cx: &mut Context<Self>) {
        let Some(mentions) = self.mentions.clone() else {
            return;
        };
        let next = {
            let state = self.state.read(cx);
            let model = state.model();
            let caret = model.caret().filter(|_| model.selection().is_collapsed());
            let in_text = matches!(
                model.block_kind_at_caret(),
                Some(BlockKind::Paragraph | BlockKind::Heading { .. })
            );
            match (caret, state.caret_prefix().and_then(query_at)) {
                (Some(caret), Some((at, query)))
                    if in_text && self.slash.is_none() && !state.is_readonly() =>
                {
                    let open = self
                        .mention
                        .as_ref()
                        .is_some_and(|m| m.block == caret.block && m.at == at);
                    if !(may_open || open) || self.mention_dismissed == Some((caret.block, at)) {
                        None
                    } else {
                        let found = matches(&*mentions, query, cx);
                        let selected = match &self.mention {
                            Some(m) if m.block == caret.block && m.at == at && m.query == query => {
                                m.selected
                            }
                            _ => 0,
                        };
                        (!found.is_empty()).then(|| MentionMenu {
                            block: caret.block,
                            at,
                            query: query.to_string(),
                            selected: selected.min(found.len() - 1),
                            matches: found,
                        })
                    }
                }
                _ => None,
            }
        };
        if next.is_none() {
            let here = self.mention_here(cx);
            self.mention_dismissed = self.mention_dismissed.filter(|d| here == Some(*d));
        }
        if next != self.mention {
            self.mention = next;
            cx.notify();
        }
    }

    /// The caret's block and the offset of the `@…` it's at the end of.
    fn mention_here(&self, cx: &App) -> Option<(BlockId, usize)> {
        let state = self.state.read(cx);
        let caret = state.model().caret()?;
        let (at, _) = state.caret_prefix().and_then(query_at)?;
        Some((caret.block, at))
    }

    pub(crate) fn close_mention(&mut self, cx: &mut Context<Self>) {
        if self.mention.take().is_some() {
            cx.notify();
        }
    }

    /// Esc: closed, and it stays closed until the caret leaves this `@`.
    pub(crate) fn dismiss_mention(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.mention.take() else {
            return false;
        };
        self.mention_dismissed = Some((menu.block, menu.at));
        cx.notify();
        true
    }

    pub(crate) fn step_mention(&mut self, by: isize, cx: &mut Context<Self>) -> bool {
        let Some(menu) = &mut self.mention else {
            return false;
        };
        let len = menu.matches.len() as isize;
        menu.selected = (menu.selected as isize + by).rem_euclid(len) as usize;
        self.menu_scroll.scroll_to_item(menu.selected);
        cx.notify();
        true
    }

    /// Puts the `ix`th listed mention in place of the typed `@…`, then a space, as one undo
    /// step. Returns false, and changes nothing, when the menu no longer matches the document.
    pub(crate) fn pick_mention(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = self.mention.take() else {
            return false;
        };
        cx.notify();
        let current = {
            let state = self.state.read(cx);
            !state.is_composing()
                && !state.is_readonly()
                && state.model().selection().is_collapsed()
                && state.model().caret().is_some_and(|c| c.block == menu.block)
                && state.caret_prefix().and_then(query_at) == Some((menu.at, menu.query.as_str()))
        };
        let Some(markdown) = menu
            .matches
            .get(ix)
            .and_then(Mention::markdown)
            .filter(|_| current)
        else {
            return false;
        };
        let typed = menu.query.len() + 1;
        self.state.update(cx, |state, cx| {
            let Ok(mut range) = state.capture_suffix(typed) else {
                return;
            };
            state.group_edits(cx, |state, cx| {
                if state.replace_captured(&mut range, "", cx).is_err() {
                    return;
                }
                let _ = state.run(EditorCommand::InsertMarkdown(markdown), cx);
                if let Ok(mut caret) = state.capture_caret() {
                    let _ = state.replace_captured(&mut caret, " ", cx);
                }
            });
        });
        self.focus(window, cx);
        true
    }

    /// The @ menu under the caret, when it's open: grouped rows, 44 points tall for touch.
    pub(crate) fn mention_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.mention.as_ref()?;
        let caret = self.state.read(cx).caret_bounds()?;
        let theme = cx.theme();
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut group = SharedString::default();
        for (ix, mention) in menu.matches.iter().enumerate() {
            if mention.group != group {
                group = mention.group.clone();
                rows.push(
                    div()
                        .px_3()
                        .pt_2()
                        .pb_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(group.clone())
                        .into_any_element(),
                );
            }
            let tile = match (&mention.icon, mention.kind) {
                (Some(icon), _) => div().child(icon.clone()).into_any_element(),
                (None, MentionKind::Page) => div().child("📄").into_any_element(),
                (None, kind) => Icon::new(match kind {
                    MentionKind::Agent => IconName::Bot,
                    MentionKind::Date => IconName::Calendar,
                    _ => IconName::User,
                })
                .size(px(16.))
                .color(theme.muted_foreground)
                .into_any_element(),
            };
            let id = mention.id.clone();
            rows.push(
                div()
                    .id(("blocks-mention-row", ix))
                    .debug_selector(move || format!("blocks-mention-option-{id}"))
                    .mx_1()
                    .px_2()
                    .min_h(px(44.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(ix == menu.selected, |d| d.bg(theme.accent))
                    .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                        // Listeners come from the last frame drawn: the row may hold another
                        // candidate now, or be gone.
                        if let Some(m) = &mut this.mention
                            && m.selected != ix
                            && ix < m.matches.len()
                        {
                            m.selected = ix;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.pick_mention(ix, window, cx);
                        }),
                    )
                    .child(
                        div()
                            .size(px(28.))
                            .flex_none()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_sm()
                            .child(tile),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_sm().truncate().child(mention.title.clone()))
                            .when(!mention.hint.is_empty(), |d| {
                                d.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .truncate()
                                        .child(mention.hint.clone()),
                                )
                            }),
                    )
                    .into_any_element(),
            );
        }
        Some(
            deferred(
                anchored()
                    .position(point(caret.left() - px(4.), caret.bottom() + px(6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("blocks-mention-menu")
                            .debug_selector(|| "blocks-mention-menu".into())
                            .w(px(300.))
                            .max_h(px(340.))
                            .overflow_y_scroll()
                            .track_scroll(&self.menu_scroll)
                            .py_1()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.popover)
                            .text_color(theme.popover_foreground)
                            .shadow_lg()
                            .children(rows),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::rust_2024::test;

    #[test]
    fn an_at_opens_at_line_start_after_a_space_or_paren_only() {
        assert_eq!(query_at("@"), Some((0, "")));
        assert_eq!(query_at("ask @fin"), Some((4, "fin")));
        assert_eq!(query_at("(@jo"), Some((1, "jo")));
        assert_eq!(query_at("ask @jordan le"), Some((4, "jordan le")));
        assert_eq!(query_at("mail a@b.com"), None, "an address");
        assert_eq!(query_at("at @ noon"), None, "a space after the @");
        assert_eq!(query_at("no at here"), None);
        assert_eq!(query_at(&format!("@{}", "x".repeat(33))), None);
    }

    #[test]
    fn dates_count_days_and_weekdays() {
        let d = Date::new(2026, 11, 18).unwrap();
        assert_eq!(d.weekday(), 2, "a Wednesday");
        assert_eq!(d.add_days(14).iso(), "2026-12-02");
        assert_eq!(d.add_days(-322).iso(), "2025-12-31");
        assert_eq!(
            Date::new(2024, 2, 29).unwrap().add_days(365).iso(),
            "2025-02-28"
        );
        assert_eq!(Date::new(1970, 1, 1).unwrap().weekday(), 3, "a Thursday");
        assert_eq!(Date::new(2026, 2, 29), None);
        assert_eq!(Date::parse("2026-11-20"), Date::new(2026, 11, 20));
        assert_eq!(Date::parse("2026-1-20"), None);
        assert_eq!(Date::parse("2026-13-01"), None);
        assert_eq!(Date::parse("+026-11-20"), None);
        assert_eq!(d.label(), "Nov 18, 2026");
    }

    #[test]
    fn the_dates_offered_follow_the_query() {
        let today = Date::new(2026, 11, 18).unwrap();
        let hints = |q: &str| -> Vec<(String, String)> {
            dates(q, today)
                .into_iter()
                .map(|m| (m.hint.to_string(), m.id.to_string()))
                .collect()
        };
        assert_eq!(
            hints(""),
            [
                ("Today".into(), "2026-11-18".into()),
                ("Tomorrow".into(), "2026-11-19".into()),
                ("Yesterday".into(), "2026-11-17".into())
            ]
        );
        assert_eq!(
            hints("next fri"),
            [("Next Friday".into(), "2026-11-20".into())]
        );
        assert_eq!(
            hints("wed"),
            [("Next Wednesday".into(), "2026-11-25".into())],
            "a week on, not today"
        );
        assert_eq!(
            hints("2026-12-01"),
            [("Dec 1, 2026".into(), "2026-12-01".into())]
        );
        assert!(hints("xyz").is_empty());
    }

    #[test]
    fn mentions_are_written_as_links() {
        let agent = Mention::new(MentionKind::Agent, "finances", "Finances");
        assert_eq!(agent.markdown().unwrap(), "[Finances](agent:finances)");
        let person = Mention::new(MentionKind::Person, "j-lee", "Jordan [Lee]");
        assert_eq!(
            person.markdown().unwrap(),
            r"[Jordan \[Lee\]](person:j-lee)"
        );
        let page = Mention::new(MentionKind::Page, "4f2a", "Roadmap");
        assert_eq!(page.markdown().unwrap(), "[Roadmap](doc:4f2a)");
        let date = Mention::new(MentionKind::Date, "2026-11-20", "Nov 20, 2026");
        assert_eq!(date.markdown().unwrap(), "[Nov 20, 2026](date:2026-11-20)");
        assert_eq!(
            Mention::new(MentionKind::Agent, "a b", "x").markdown(),
            None
        );
        assert_eq!(
            Mention::new(MentionKind::Date, "soon", "x").markdown(),
            None
        );
    }

    struct Team;
    impl Mentions for Team {
        fn candidates(&self, _: &str, _: &App) -> Vec<Mention> {
            vec![
                Mention::new(MentionKind::Person, "jordan", "Jordan Lee"),
                Mention::new(MentionKind::Agent, "finances", "Finances"),
                Mention::new(MentionKind::Person, "fin", "Finn Adler"),
                Mention::new(MentionKind::Page, "4f2a", "Fine print"),
                Mention::new(MentionKind::Agent, "bad id", "Finder"),
            ]
        }
        fn today(&self, _: &App) -> Option<Date> {
            Date::new(2026, 11, 18)
        }
    }

    #[gpui::test]
    fn the_menu_groups_filters_and_adds_dates(cx: &mut gpui::TestAppContext) {
        let titles = |q: &str, cx: &mut gpui::TestAppContext| -> Vec<String> {
            cx.update(|cx| matches(&Team, q, cx))
                .into_iter()
                .map(|m| format!("{}:{}", m.group, m.title))
                .collect()
        };
        assert_eq!(
            titles("fin", cx),
            ["People:Finn Adler", "Agents:Finances", "Pages:Fine print",],
            "groups in order of first appearance; an invalid id is left out"
        );
        assert_eq!(
            titles("", cx).last().map(String::as_str),
            Some("Dates:Nov 17, 2026"),
            "the dates come last"
        );
        assert_eq!(titles("tom", cx), ["Dates:Nov 19, 2026"]);
    }

    use crate::{EditorEvent, Options};
    use gpui::{Entity, TestAppContext, VisualTestContext};

    fn open<'a>(
        cx: &'a mut TestAppContext,
        source: &str,
        with_mentions: bool,
    ) -> (Entity<Editor>, &'a mut VisualTestContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let source = source.to_string();
        let (editor, cx) = cx.add_window_view(move |window, cx| {
            let options = if with_mentions {
                Options::default().mentions(Team)
            } else {
                Options::default()
            };
            Editor::with_options(&source, options, window, cx)
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        (editor, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    fn listed(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
        editor.update(cx, |e, _| {
            e.mention
                .as_ref()
                .map(|m| m.matches.iter().map(|m| m.title.to_string()).collect())
        })
    }

    fn markdown(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
        editor.update(cx, |e, cx| e.markdown(cx))
    }

    /// `@` opens the menu, typing narrows it, ↓ and Enter pick: the `@…` becomes the agent's
    /// mention and a space, as one undo step.
    #[gpui::test]
    fn at_opens_narrows_and_picks_with_the_keyboard(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "", true);
        cx.simulate_input("Ask @");
        draw(cx);
        let all = listed(&editor, cx).expect("the menu opens");
        assert_eq!(
            all[..4],
            ["Jordan Lee", "Finn Adler", "Finances", "Fine print"]
        );
        cx.simulate_input("fin");
        draw(cx);
        assert_eq!(
            listed(&editor, cx).unwrap(),
            ["Finn Adler", "Finances", "Fine print"]
        );
        cx.simulate_keystrokes("down enter");
        draw(cx);
        assert_eq!(listed(&editor, cx), None);
        cx.simulate_input("now");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "Ask [Finances](agent:finances) now");
        // The chip is one unit in the text.
        let text = editor.update(cx, |e, cx| {
            e.state()
                .read(cx)
                .model()
                .document()
                .leaves()
                .next()
                .and_then(|l| l.text().map(str::to_string))
        });
        assert_eq!(text.as_deref(), Some("Ask \u{FFFC} now"));
        // Undo takes back the typing, then the mention whole, leaving the `@fin` typed.
        let undo = if cfg!(target_os = "macos") {
            "cmd-z"
        } else {
            "ctrl-z"
        };
        cx.simulate_keystrokes(undo);
        draw(cx);
        cx.simulate_keystrokes(undo);
        draw(cx);
        assert_eq!(markdown(&editor, cx), "Ask @fin");
    }

    /// Pages and dates are picked the same way, and a click on the menu picks too.
    #[gpui::test]
    fn pages_and_dates_go_in_as_their_links(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "", true);
        cx.simulate_input("See @print");
        draw(cx);
        cx.simulate_keystrokes("enter");
        draw(cx);
        cx.simulate_input("by @tom");
        draw(cx);
        assert_eq!(listed(&editor, cx).unwrap(), ["Nov 19, 2026"]);
        let row = cx
            .debug_bounds("blocks-mention-option-2026-11-19")
            .expect("the row is drawn")
            .center();
        cx.simulate_click(row, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(
            markdown(&editor, cx),
            "See [Fine print](doc:4f2a) by [Nov 19, 2026](date:2026-11-19) "
        );
    }

    /// Esc closes the menu and leaves the text; it stays closed for that `@`, and Enter is the
    /// editor's again. An address's `@` never opens it.
    #[gpui::test]
    fn escape_closes_it_and_an_address_never_opens_it(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "", true);
        cx.simulate_input("mail a@b");
        draw(cx);
        assert_eq!(listed(&editor, cx), None);
        cx.simulate_input(" @jo");
        draw(cx);
        assert!(listed(&editor, cx).is_some());
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert_eq!(listed(&editor, cx), None);
        cx.simulate_input("r");
        draw(cx);
        assert_eq!(listed(&editor, cx), None, "still closed for this @");
        cx.simulate_keystrokes("enter");
        draw(cx);
        assert_eq!(markdown(&editor, cx), "mail a@b @jor\n\n");
    }

    /// Without the app's mentions `@` is an ordinary character; a mention already in the text
    /// is still drawn as a chip, and a click on it is told to the app.
    #[gpui::test]
    fn without_mentions_at_is_plain_and_chips_still_click(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "Ask [Finances](agent:finances)", false);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&editor, move |_, event: &EditorEvent, _| {
                if let EditorEvent::MentionClicked { kind, id } = event {
                    seen.borrow_mut().push((*kind, id.to_string()));
                }
            })
            .detach()
        });
        let chip = cx
            .debug_bounds("blocks-chip-finances")
            .expect("the chip is drawn")
            .center();
        cx.simulate_click(chip, gpui::Modifiers::none());
        draw(cx);
        assert_eq!(
            events.borrow().as_slice(),
            [(MentionKind::Agent, "finances".to_string())]
        );
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-down"
        } else {
            "ctrl-end"
        });
        cx.simulate_input(" @");
        draw(cx);
        assert_eq!(listed(&editor, cx), None);
        assert_eq!(markdown(&editor, cx), "Ask [Finances](agent:finances) @");
    }

    /// A tap on a row picks it, as on a phone.
    #[gpui::test]
    fn a_tap_picks_a_row(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "", true);
        cx.simulate_input("@jor");
        draw(cx);
        let row = cx
            .debug_bounds("blocks-mention-option-jordan")
            .expect("the row is drawn")
            .center();
        for phase in [gpui::TouchPhase::Started, gpui::TouchPhase::Ended] {
            cx.simulate_event(gpui::TouchEvent {
                id: gpui::TouchId(3),
                phase,
                position: row,
                predicted_position: None,
                force: None,
            });
            draw(cx);
        }
        assert_eq!(markdown(&editor, cx), "[Jordan Lee](person:jordan) ");
    }
}
