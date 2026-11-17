# gpui-core

Components for [GPUI](https://www.gpui.rs) apps on macOS and iOS, drawn in the host's theme.
It replaced GPUI Kit in this workspace's crates, covering what they use. See
[PLAN.md](PLAN.md) for the inventory and the build order.

![gpui-core's gallery in the dark theme: buttons in four sizes and five variants, a segmented
control, toggles, a slider and an open menu with a check, a submenu, a shortcut and a disabled
item](docs/gallery.png)

- **Theme.** `Theme` holds the colours, fonts and sizes. Its colour names are the web app's
  CSS variables in snake case (`background`, `muted_foreground`, `ring`, …). The host sets it
  with `Theme::set`, and components read it with `cx.theme()`.
- **Root.** It makes the theme's `font_size` the window's rem, so every size in rems follows
  the host's zoom. It draws the window's surface in the theme and moves the focus with Tab.
- **Setup.** `gpui_core::init(cx)` binds the components' keys and sets the default theme. It
  also runs by itself the first time a `Root` or an `InputState` is made, so a crate that embeds
  gpui-core's components in a host that never calls it still works; calling it twice does
  nothing.
- **A palette of your own.** The look comes from the global `Theme`. An element drawn on a
  palette that differs from it (a light card in a dark app) takes its colours per element:
  `Press::focus_ring_color`, `Input::caret_color` / `selection_color` / `placeholder_color`,
  `Slider::track_color` / `fill_color` / `thumb_color`, `Tooltip::background` / `text_color` /
  `border_color`, `Popover::colors`, `DatePicker::colors` / `MonthGrid::colors`.
- **Popover.** `Popover::new(id).open(open).on_dismiss(…).trigger(element).child(…)` hangs a
  panel from its trigger, or from a box or point in the window with `anchor`. It goes below, or
  above where there's more room, stays inside the window and scrolls past `max_height`. A click
  outside, Escape, or the trigger scrolling away calls `on_dismiss` with the reason; the host
  owns `open`. While open it has the keyboard, and on closing it hands the focus back.
- **Breadcrumb.** `Breadcrumb::new(id).item(BreadcrumbItem::new(id, label))….on_navigate(…)`:
  the path to the current page, its last item, separated by chevrons. The other items are links
  (a press, Enter or Space calls `on_navigate` with the item's id); the current page is no link
  and no Tab stop, and tells assistive tech it's the current page. When the path doesn't fit
  its width, or has more than `max_items(n)`, its middle folds into an ellipsis that opens a
  menu of the folded items; whatever's left is cut short rather than spilling. `touch(true)`
  makes its links 44 points tall.
- **Sheet.** `Sheet::new(id).open(open).on_dismiss(…).child(…)` is Popover's touch form: a
  panel across the bottom of the window over a scrim, centred and capped at `max_width` on a
  wide window, scrolling past nine tenths of its height. A press on the scrim or Escape calls
  `on_dismiss`; it takes the keyboard while open and hands it back on closing.
- **Pagination.** `Pagination::new(id).total(n).page(p).on_page(…)`: Previous, the page numbers
  round the current one (`siblings(n)` either side, one unless given) with ellipses for the
  rest, and Next. Each is a link in the Tab order, pressed with a click, Enter or Space; the
  current page tells assistive tech it's the current one, and Previous and Next are disabled at
  the ends. The row keeps its length as the page moves. When it doesn't fit its width it drops
  Previous's and Next's words, then shows only "Page 3 of 10" between the arrows (`compact(true)`
  always). `touch(true)` makes its links 44 points.
- **HoverCard.** `HoverCard::new(id).trigger(link).child(…)` shows a card of more about its
  trigger, hung from it like a Popover, once the pointer has rested on it for `open_delay`
  (700 ms) and at once while the trigger has the focus, since nothing works by hover alone. It
  hides `close_delay` (300 ms) after the pointer leaves the trigger and the card, when the focus
  leaves, on Escape and on a press outside. It never takes the focus, and keeps whether it's
  shown itself.
- **Dialog.** `Dialog::new(id).open(open).title(…).description(…).on_dismiss(…).child(…)
  .footer(button)` is a modal panel in the middle of the window over a scrim, at most
  `max_width` wide and scrolling when taller than the window, with a close button (an ×,
  `close_button(false)` hides it). Escape, a press on the scrim or the close button calls
  `on_dismiss` with which it was. On opening, the keyboard goes to `initial_focus` or the first
  thing inside, Tab and Shift-Tab go round inside it and never out, and on closing the focus
  goes back. It's drawn over popovers, a popover or menu opened from inside it over it, and
  toasts over it. `AlertDialog::new(id)….cancel(button).action(button)` asks for a decision:
  no close button, a press on the scrim doesn't close it, and it opens with the keyboard on
  the cancel.
- **Code editor.** `InputState::code(window, cx)` with `Input::new(&state).size_full()` in a box
  the host sizes: lines that don't wrap (it scrolls sideways to the caret), the theme's
  monospace font, line numbers (`Input::line_numbers(false)` hides them), Tab and Shift-Tab
  indenting a line or every selected line, Enter keeping the indent, and Ctrl-Tab moving the
  keyboard on. Only the rows in view are drawn, so a file of thousands of lines types as fast as
  a short one. Undo, the input method and the rest are a field's. `set_error_line(Some((line,
  message)))` marks the line a server said is wrong (gone when the text changes),
  `highlight(f, "yaml")` colours it (the `highlight` feature's `Highlighter`, or the host's), and
  ⌘F opens a find bar: matches found case aside as you type, Enter and Shift-Enter (⌘G and ⌘⇧G
  from the text) going round them, Escape closing it.
- **Badge.** `Badge::new(label)` with `.secondary()`, `.destructive()` or `.outline()` (or
  `variant(BadgeVariant::…)`) and an optional `.icon(name)` before the label: shadcn's pill,
  on one line, its label cut short with an ellipsis in a narrow space. It's text until it's
  given `.on_click(…)`; then it's a button that Tab reaches, with the focus ring, and
  `touch(true)` has it take presses 44 points each way around it.
- **Kbd.** `Kbd::new("⌘")` is a key's cap as shadcn draws it: muted, 20 pixels tall and at
  least as wide, in the theme's own font (shadcn's isn't monospace), with an optional
  `.icon(name)` (or `Kbd::icon_only(name)`). `KbdGroup::keys(["⌘", "K"])` or
  `KbdGroup::new().child(Kbd…)` sets a shortcut's keys four pixels apart. In a narrow space a
  long key name ends in an ellipsis and a group is clipped at its edge.
- **Separator.** `Separator::horizontal()` or `Separator::vertical()`: shadcn's one-pixel
  line in the border colour, across its parent or down the flex row it's in. It's decorative
  (nothing to assistive tech) unless `.decorative(false)`, which makes it a separator with its
  orientation (AccessKit's `Role::Splitter`, its name for ARIA's separator); give those an
  `.id(…)` of their own when there are several side by side.
- **Spinner.** `Spinner::new(id)`: shadcn's loader-circle turning once a second in the text's
  colour (or `.color(…)`), 16 pixels, or 12, 14 or 20 with `Sizable`, or any size like a `div`.
  To assistive tech it's a busy indicator (a progress indicator with no value) labelled
  "Loading", or `.accessibility_label(…)`. It turns only while drawn, and not at all under
  GPUI's `cx.set_reduce_motion(true)`.
- **Skeleton.** `Skeleton::new()` sized like a `div` (`.h_4().w(px(250.))`, `.size_12()
  .rounded_full()`): shadcn's muted rounded block standing in for content on its way, never
  wider than its parent. It pulses (half opacity and back every two seconds, every skeleton
  as one) only while it's drawn; `animate(false)` or GPUI's `cx.set_reduce_motion(true)`
  keeps it still.
- **Avatar.** `Avatar::new(name)` with `.image(…)`: shadcn's circle at 32 pixels (24 or 40
  with `.small()` or `.large()`), drawing a picture the host already has (an
  `Arc<RenderImage>`, or an `Arc<Image>`'s bytes, decoded off the main thread; never a URL),
  and the name's initials on the muted colour until then or without one. To assistive tech
  it's an image named for the person. `AvatarGroup::new(id).children(…).max(3)` overlaps
  them by eight pixels, each ringed in the page's colour, with a "+N" circle for the rest,
  clipped at its edge in a narrow space.
- **EmptyState.** `EmptyState::new().icon(name).title(…).description(…).child(action)`:
  shadcn's `Empty` (renamed, as GPUI has an `Empty` of its own), an icon in a 40-pixel muted
  circle over a title, a muted description and an action area (its children), centred in the
  space it fills, with its text held to shadcn's 384 pixels and wrapping in a narrow space.
- **ScrollArea.** `ScrollArea::new(id).h(…).child(…)` scrolls its children inside a box
  with shadcn's thin overlay scrollbars, up and down unless `horizontal()` or `both()`. The bars
  show where the content overflows, while it scrolls or the pointer is over it, then fade (never
  for a finger, which scrolls with GPUI's momentum); a thumb drags and a press on a track pages.
  With the focus, the arrows, Page Up and Page Down, Space, Home and End scroll it.
  `track_scroll(&ScrollAreaHandle)` lets the host read the position and scroll to an offset, a
  child (`scroll_to_child`) or an end.
- **Carousel.** `Carousel::new(id).w(…).children(slides).on_slide(…)` shows slides in a row,
  one in view or `slides_per_view(n)`, with previous and next buttons outside its sides
  (disabled at the ends unless `looping(true)`) and optional `dots(true)` below. Left and Right
  move it while the focus is inside, a sideways swipe on a touch screen or a trackpad drags it
  and snaps to a slide, and a move slides briefly into place. `index(i)` moves it from the host.
  It's a region described as a carousel, its slides groups named "1 of 5" and so on.
- **Progress.** `Progress::new(id).value(40.)`: shadcn's eight-pixel bar across its parent,
  the primary colour filling a 20% track as far as the value (clamped to 0–100, and 0 for
  NaN). Without a value it's indeterminate: a stretch of fill sweeping across, only while
  drawn and not under GPUI's `cx.set_reduce_motion(true)`. To assistive tech it's a progress
  indicator with its value (none when indeterminate) and an optional
  `.accessibility_label(…)`.
- **AspectRatio.** `AspectRatio::new(16. / 9.).child(…)`: a box as wide as its parent (or
  the width it's given) and as tall as the ratio makes that, its children filling it and
  clipped at its edges, never stretching it. A ratio that isn't a positive number is a square.
- **InputOtp.** `InputOtpState::new(window, cx)` (six slots; `with_length(n, …)` for others,
  `digits_only(false)` for letters too) drawn with `InputOtp::new(&state).groups([3, 3])
  .on_complete(…)`: shadcn's one-time code field, single-character slots joined edge to edge,
  a dash between groups. It's one hidden InputState with `max_chars`, so the platform's text
  input works as in any field. Typing fills the slots in turn and rings the next one, Backspace
  empties the last, a paste fills them all with what it takes of the clipboard (digits only by
  default), and when every slot is full `on_complete` and `InputOtpEvent::Complete` give the
  code, once per code. The keyboard is told no corrections, capitals or suggestions (GPUI has
  no one-time-code content type to ask for yet). `touch()` makes the slots 44 points.
- **SegmentedControl.** `SegmentedControl::new(id).segment(id, label)….selected(id)
  .on_change(…)`: a few options side by side, one on (a theme's System, Light and Dark). It's a
  radio group to assistive tech, with one Tab stop (the option that's on); the arrows, Home and
  End move along and turn on what they reach, passing over a disabled option and stopping at
  the ends. `touch(true)` makes its options 44 points tall; in a narrow space their labels are
  cut short.
- **Toast.** `Toast::success("Saved.").show(window, cx)` (or `new`, `error`) shows a note at the
  bottom of a window whose root is a `Root`, as the web's are: newest at the bottom, at most
  three, gone by themselves after four seconds (eight for an error or one with an `action`),
  each with a close button. `id(…)` replaces one shown with the same id; `description(…)` adds a
  muted line. An error is an alert to assistive tech, the rest a status. Nothing redraws while
  one shows but its one timer.
- **Card.** `Card::new().title(…).description(…).action(element).child(…).footer(element)`:
  a raised panel in the theme's `card` colours with a border and the large radius. The title
  and description wrap beside the action at the top right; the children are its content; the
  footer is a row along the bottom on a quieter fill. `small()` tightens its spacing. It draws
  only the parts it's given and takes no input of its own.
- **Alert.** `Alert::new(id).icon(IconName::Info).title(…).description(…)`, `.destructive()`
  for the variant in the destructive colour: a bordered box on the card colour with the icon
  level with the title and the text beside it, wrapping. Children go under the description. A
  destructive alert is an alert to assistive technology, the default one a status, named by
  its title; `role(Role::Note)` (or another) says otherwise.
- **Collapsible.** `Collapsible::new(id).label(text).child(…)`: a trigger that shows or
  hides the content under it (hidden content isn't drawn). `label` draws a trigger with a
  chevron; `trigger(press)` or `trigger_with(|open| press)` takes the host's own `Press`. The
  host keeps the state with `open(bool)` and `on_toggle(…)`, or leaves it to the collapsible
  under its id (`default_open`). Enter, Space, a click or a tap toggles it, the trigger says
  whether it's expanded, `touch(true)` makes the label trigger 44 points tall, and it doesn't
  animate.
- **SplitView.** `SplitView::new(id).pane(Pane::new(child).width(w).min(w).max(w))…` puts two
  or three panes side by side (a sidebar, a list, a detail) with a handle between each two. A
  drag on a handle resizes the pane before it within its minimum and maximum, the last pane
  taking the rest; a focused handle moves with Left and Right (Shift for bigger steps), Home and
  End, and a double press puts its pane back at its default width. `on_resize` tells the host
  the widths to keep and `widths(…)` passes them back. `Pane::collapsed(true)` hides a pane and
  gives its room to the others. Below `narrow_below(w)` it shows one pane, the one
  `narrow_shows(i)` says, and `on_narrow_change` tells the host so it can draw a back button.
  Panes clip their content; handles are splitters to assistive tech, and `touch(true)` makes
  them take presses 44 points wide.
- **Tabs.** `Tabs::new(id).tab(Tab::new(id, label).child(panel))….selected(id).on_change(…)`:
  a tab list over the selected tab's panel, the only one drawn. The host owns the selection, or
  leaves it to the tabs with `default_selected(id)`. The list has one Tab stop (the selected
  tab); Left and Right move along it, wrapping, and Home and End go to its ends, passing over a
  disabled tab and selecting what they reach (`manual(true)` leaves that to Enter or Space).
  It's drawn on a muted track with the selected tab raised, or underlined with `line()`; when
  the tabs don't fit, their labels are cut short, or with `overflow(TabsOverflow::Scroll)` the
  list scrolls, keeping the focused tab in view. `stretch(true)` fills the width; `touch(true)`
  makes tabs 44 points.
- **Item.** `Item::new(id).icon(IconName::User).title(…).description(…).action(element)`: a
  row with media at the start (`icon`, or the host's own with `media`), a title cut short on one
  line, a description cut short after two, and actions at the end; `outline()` and `muted()`
  variants, and `small()`. With `on_click` the whole row is a `Press` (a button named by its
  title, lit under the pointer, at least 44 points tall with `touch()`), and `disabled(true)`
  dims it. `ItemGroup::new(id)` stacks items as a list, with `ItemSeparator::new()` rules
  between them.
- **Accordion.** `Accordion::new(id).item(AccordionItem::new(id).title(…).child(…))…`:
  items with a rule between them, each a header that shows or hides its content (hidden
  content isn't drawn). One opens at a time, or several with `multiple(true)`. The host keeps
  which are open with `open(ids)` and `on_change(…)`, or leaves it to the accordion under its id
  (`default_open(ids)`). Every header is a Tab stop; Enter, Space, a click or a tap toggles it,
  Up and Down move between headers (going round), Home and End go to the ends, and disabled
  items are passed over. The chevron points down or up; nothing animates. `touch(true)` makes
  headers at least 44 points tall.
- **Table.** `Table::new(id).column(TableColumn::new("Amount").fixed(px(100.)))…
  .rows(count, |range, window, cx| rows).footer(row).caption(…)`: a simple display table (the
  data grid is gpui-tables'). Columns are fixed, a `fraction` of what's left, or `Auto` (an
  equal share), aligned `Start`, `Center` or `End`, and line up in every row; cells cut their
  text short. Give it a height and its body scrolls by itself, asking the host only for the
  rows in view, so a thousand rows cost what a screenful does; without one it draws them all.
  The host says which row is `selected`; with `on_select` a click asks to select a row and the
  body is a Tab stop where Up, Down, Home, End, Page Up and Page Down ask for another and
  scroll it into view. Rows light up under the pointer. It's a table of rows and cells to
  assistive technology, named by its caption.
- **Checkbox.** `Checkbox::new(id).label(text).checked(on).on_click(…)`: a small rounded box,
  filled with the primary colour and a check when on, or a dash with `indeterminate(true)`. A
  press on the box or its label, or Space, calls `on_click` with the state to take (on, from
  indeterminate); the host keeps it. It's a check box to assistive tech (mixed when
  indeterminate), named by its label; `invalid(true)` gives it the destructive border,
  `disabled(true)` dims it, `touch()` takes presses 44 points tall, and a long label wraps.
- **RadioGroup.** `RadioGroup::new(id).option(id, label)….selected(id).on_change(…)`: options
  one above the other (`horizontal(true)` puts them side by side, wrapping), each a circle and
  a label, one on. It's a radio group to assistive tech with one Tab stop (the option that's
  on); the arrows move to the next option and turn it on, going round from the last to the
  first and passing over `disable(id)`'d ones, and a press on a circle or its label turns it
  on. `disabled(true)` dims them all, `invalid(true)` gives them the destructive border,
  `touch()` makes each option 44 points tall, and a long label wraps.
- **Toggle and ToggleGroup.** `Toggle::new(id).icon(icon).label(text).pressed(on).on_click(…)`
  is a button that stays pressed in (shadcn's toggle), plain or `outline()`, in the button
  sizes; `on_click` is told the state to take, and assistive tech hears it as pressed or not.
  `ToggleGroup::new(id).item(id, label)` (or `item_with_icon`, `icon_item`)`….selected(ids)
  .on_change(…)` joins toggles edge to edge: one pressed at most (a radio group to assistive
  tech; pressing it again lets it out), or any number with `multiple(true)`. `on_change` is told
  everything that should be pressed. It's one Tab stop; the arrows, Home and End move the
  keyboard along, going round, and Space or Enter presses. `touch()` makes them 44 points
  tall; in a narrow space their labels are cut short.
- **Label and Field.** `Label::new(id, text).for_focus(&handle)` names a control; a press on
  it focuses the control, or does what `on_click` says (toggles a check box). `Field::new(id)
  .label(text).for_focus(&handle).description(text).error(Some(text)).child(control)` lays out
  shadcn's field: label, control, description and error one above the other, or side by side
  with `horizontal(true)` (`control_first(true)` for a check box's order). An error is drawn in
  the destructive colour, the label too, and assistive tech hears the field as a group named by
  its label and described by its error or description. `FieldSet::new(id).legend(text)
  .description(text)` groups fields under a `FieldLegend`, and `FieldGroup` stacks them. Every
  text wraps.
- **PopupMenu.** `PopupMenu::new(id).item(PopupMenuItem::new(id, label)…).separator()
  .on_select(…)`, usually as a Popover's content. Entries take an icon, a check, a shortcut's
  text, a host-drawn chip, and can be disabled. Up and Down move a highlight over what can be
  picked (the pointer moves it too), Enter or a press picks, and Escape closes the popover.
  `PopupMenuItem::submenu(menu)` opens a menu beside its entry, on the right unless there's
  no room: on a press, Enter, Space or Right, or when the pointer rests on it; Left or Escape
  closes only it, and its picks go to the outer menu's `on_select` unless it has its own. It's
  drawn in the colours of the popover it's in, stays its entry's while entries come and go,
  can be made only when it opens (`submenu_with(|| …)`), and a menu can open with one open
  and the keyboard in it (`open_submenu(id)`).
  `touch(true)` makes rows 44 points tall. (`Menu` and `MenuItem` are GPUI's app menus.)
- **ContextMenu.** `ContextMenu::new(id, element).menu(|window, cx| PopupMenu::new(…)…)` opens
  the menu over everything: at the pointer on a right-click, where a finger rests on the
  element for half a second (GPUI's long press, with rows 44 points tall), and under the
  element on Shift-F10 or the Menu key while it or something in it has the focus. The menu is
  made only while open. A pick runs its `on_select` (its submenus' picks too) and closes it;
  Escape, a press outside and the element scrolling out of view close it as well, and the
  focus goes back to what had it. A plain click, a tap and selecting text stay the element's.
- **Menubar.** `Menubar::new(id).menu(MenubarMenu::new(id, title, PopupMenu::new(…)…))…
  .on_select(…)`: a row of menus, as shadcn's Menubar, each a PopupMenu (submenus and all) in a
  Popover under its title, its picks going to `on_select` with the menu's id. The bar is one Tab
  stop; Left and Right move along it, and Down, Enter or Space opens a menu. While one is open,
  Left and Right open the menu beside it (Right opens a highlighted submenu first), the pointer
  moving onto another title opens that one, and Escape or a press outside closes it, giving the
  keyboard back to its title. `touch(true)` makes titles and entries 44 points.
- **Select.** `Select::new(id).placeholder(text).heading(title).option(id, label)…
  .separator().selected(Some(id)).on_change(…)`: shadcn's select, a field-like button showing
  the chosen option (or the placeholder, muted) that opens a PopupMenu of the options in a
  Popover, the chosen one checked and highlighted. A press, Enter, Space, Down or Up opens it;
  in it the menu's keys work, typing a letter moves to the next option starting with it, and
  Escape closes it, the keyboard going back to the select. On the closed select a letter
  chooses the next option starting with it. `disable(id)` dims an option, `invalid(true)` gives
  the destructive border, `small()` and `touch()` size it, and a long choice is cut short. It
  isn't searchable: that's Picker.
- **Picker.** `Picker::new(id).items(…).on_select(…)` is a PopupMenu with a field at the top:
  typing filters the entries (every word, case aside) and highlights the first match, Up and
  Down move through the matches while the field keeps the keyboard, Enter picks. A
  single-choice host closes its popover on a pick; a several-choice one keeps it open.
- **NavigationMenu.** `NavigationMenu::new(id).item(NavigationMenuItem::panel(id, label)
  .columns(2).with_link(NavigationMenuLink::new(id, title).description(…)))
  .item(NavigationMenuItem::link(id, label)).on_navigate(…)`: a site's top navigation, as
  shadcn's. A trigger opens a Popover of links, each a title over a muted description cut short
  after two lines, in as many of its columns as the window has room for. A press, Enter, Space
  or Down opens it, from the keyboard with its first link focused; the arrows, Home and End move
  between the links, Left and Right along the row (switching panels while one is open), and the
  pointer moving onto another trigger switches too. Escape or a press outside closes it,
  following a link calls `on_navigate`, and `touch(true)` makes items 44 points.
- **SearchList.** `SearchList::new(id).items([(id, search text), …]).render_row(…)
  .on_open(…).selected(id)` is a page's list beside a detail pane (a vault's logins): a search
  field above rows the host draws. Typing filters the rows (every word, case aside), Up and
  Down move a highlight while the field keeps the keyboard, and Enter or a press opens a row.
  The open row is drawn selected and the highlight goes to it; the highlight stays on its row
  when the host re-sorts. Escape clears the search. Rows are one height (`row_height`, 44
  points at least with `touch(true)`), so a list of thousands draws only the rows in view.
- **TextView.** `TextView::markdown(id, source)` draws GitHub Markdown natively: selectable
  with `selectable(true)`, links as `on_link_click` events, and `[text][label]` links
  resolved to their definitions. `TextView::blocks(id, [(key, source), …])` draws several
  keyed sources as one document (a streamed reply's settled blocks and its live tail): spaced,
  selected and copied as one view, with each source read again only when its text changes and
  each top-level block laid out again only when it changes. A selection in unchanged sources
  stays while later ones change. `trailing_caret(Some(colour))` draws a caret after the last
  source's text, painted only, so it never moves a line.
- **Sidebar.** `Sidebar::new(id, &state).header(…).group(SidebarGroup::new(id).label(…)
  .item(SidebarItem::new(id, label).icon(…).badge(…).action(icon, label, …)))….footer(…)
  .active(id).on_select(…).content(page)`: shadcn's app sidebar beside the page, with the host's
  `SidebarState` entity saying whether it's collapsed. `SidebarTrigger::new(id, &state)` or ⌘B
  (Ctrl-B off Apple, while the keyboard is in it or the page) collapses it to a rail of icons,
  each item's label then a tooltip, and expands it. Under its `breakpoint` (768 pixels unless
  given) it leaves the page the whole width and the trigger opens it over the page instead, on a
  scrim, with the keyboard in it; Escape, the scrim or a pick closes it. Items are links to
  assistive tech (the active one the current page), Up, Down, Home and End move between them,
  `collapsible(true)` groups fold under their label, long labels are cut short, and
  `touch(true)` makes items 44 points.
- **Syntax highlighting** (the `highlight` feature, off by default for the grammars' size).
  `Highlighter::new()` knows Rust, Swift, Go, TypeScript/TSX, JavaScript/JSX, Python, JSON,
  TOML, YAML, Bash, SQL, Markdown, HTML, CSS, C and C++ by the names code fences use (case
  aside, first word, with aliases such as `rs`, `py`, `yml`, `c++`), and hosts can `register`
  more tree-sitter grammars. `highlight(lang, code, &SyntaxTheme)` gives ranges and styles;
  `into_fn(theme)` makes the `HighlightFn` (`text::HighlightFn`, there without the feature too)
  that hosts hand their code views. Colours are the theme's `syntax_*` tokens, each at least
  4.5:1 on the page, a card and `muted`.
- **Rules.** Nothing redraws while nothing changes, text stays in its box at any width,
  everything works from the keyboard with a focus ring, nothing works by hover alone, and links
  and actions are events for the host.

Depend on GPUI under its own name, at the same version as this crate:

```toml
[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.12" }
gpui-core = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
# The app's window. Without `font-kit`, GPUI draws no text on macOS (GPUI Kit turned it on
# for you).
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.12", features = ["font-kit"] }
```

```rust
use gpui::*;
use gpui_core::{ActiveTheme as _, Theme};

struct Hello;

impl Render for Hello {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().text_color(cx.theme().muted_foreground).child("Hello")
    }
}

fn run(cx: &mut App) {
    gpui_core::init(cx);
    Theme::set(Theme::dark(), cx);
    gpui_core::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Hello)).unwrap();
}
```

Components draw Lucide icons from gpui-core's assets, so the app serves them:
`gpui_platform::application().with_assets(gpui_core::Icons)`.

## The gallery

Every component in one window, from the repository's root:

```sh
cargo run -p gpui-core --example gallery              # dark; --light for the light theme
cargo run -p gpui-core --features shots --example gallery -- --shot out.png
```

`--shot` draws the window offscreen into a PNG without showing it, as the picture above was made.

## Licence

Apache-2.0.
