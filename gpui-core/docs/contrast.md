# Colour contrast: gpui-core's theme against WCAG 2.2 AA

An audit of every foreground and background pair gpui-core's components draw, in the dark and light themes. It covers the theme's own colours (`src/theme.rs`, from `design/tokens.json`) composited as drawn: a colour with alpha is blended over the fill under it, then the contrast ratio is worked out with WCAG's relative luminance.

- **Text** needs 4.5:1 (1.4.3). All of gpui-core's text is under the large-text size (the largest is the 22.5 px title), so 4.5:1 applies throughout.
- **Non-text**, meaning the focus ring, the boundaries that show a control and chart series, needs 3:1 (1.4.11, 2.4.13).
- **Disabled controls** are exempt (1.4.3). They're listed for reference.

Each pair was read from the component that draws it; the last column says where. Some colours are used only by the apps, not by gpui-core's components (the status colours). They're in their own section, because the apps draw them with these tokens.

This is a report only. The colours come from `design/tokens.json`, the apps' shared design file, so any change goes through that file and the apps' design review. This report proposes changes; it doesn't make them.

Note: `tokens.json` already promises 4.5:1 for the syntax colours, and this audit confirms it for every one of them.

## Dark theme

67 pairs; 12 below the WCAG 2.2 AA level they need.

### Text

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Text on the page (foreground on background) | `#ededef` on `#0f0f11` | 16.38:1 | 4.5:1 | pass | everywhere; text links too (text/view.rs:1675, underlined) |
| Text on a card (card_foreground on card) | `#ededef` on `#18181b` | 15.15:1 | 4.5:1 | pass | card.rs, alert.rs |
| Text in a popover or menu (popover_foreground on popover) | `#ededef` on `#18181b` | 15.15:1 | 4.5:1 | pass | toast.rs, menus |
| Text on a secondary fill (secondary_foreground on secondary) | `#ededef` on `#1f1f23` | 14.05:1 | 4.5:1 | pass | button.rs Secondary, badge.rs Secondary |
| Selected tab or segment (foreground on background, in a muted list) | `#ededef` on `#0f0f11` | 16.38:1 | 4.5:1 | pass | tabs.rs:341, segmented.rs:291 |
| Primary button and badge (primary_foreground on primary) | `#0f0f11` on `#ededef` | 16.38:1 | 4.5:1 | pass | button.rs Primary, badge.rs Default, checkbox.rs tick |
| Ghost button or row hover (accent_foreground on accent over background) | `#ededef` on `#1a1a1c` | 14.86:1 | 4.5:1 | pass | button.rs Ghost, sidebar.rs:433 |
| Menu row hover (accent_foreground on accent over popover) | `#ededef` on `#232326` | 13.41:1 | 4.5:1 | pass | menus over popover |
| Outline button (foreground on its fill over background) | `#ededef` on `#1c1c1f` | 14.54:1 | 4.5:1 | pass | button.rs Outline (dark) |
| Selected outline or ghost button (foreground on input / muted) | `#ededef` on `#3a3a41` | 9.65:1 | 4.5:1 | pass | button.rs selected (dark: input) |
| Tooltip (background on foreground) | `#0f0f11` on `#ededef` | 16.38:1 | 4.5:1 | pass | tooltip.rs:90 |
| Sidebar text (sidebar_foreground on sidebar) | `#ededef` on `#0b0b0d` | 16.82:1 | 4.5:1 | pass | sidebar.rs:769 |
| Sidebar selected row (sidebar_accent_foreground on sidebar_accent over sidebar) | `#ededef` on `#171719` | 15.31:1 | 4.5:1 | pass | sidebar.rs:433 |
| Sidebar group label (sidebar_foreground at 70% on sidebar) | `#a9a9ab` on `#0b0b0d` | 8.38:1 | 4.5:1 | pass | sidebar.rs:501 |

### Secondary text

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Muted text on the page (muted_foreground on background) | `#8b8b93` on `#0f0f11` | 5.66:1 | 4.5:1 | pass | 23 uses, e.g. alert.rs, toast.rs, input.rs |
| Muted text on a card (muted_foreground on card) | `#8b8b93` on `#18181b` | 5.24:1 | 4.5:1 | pass | alert.rs description, card.rs |
| Muted text in a popover (muted_foreground on popover) | `#8b8b93` on `#18181b` | 5.24:1 | 4.5:1 | pass | toast.rs description |
| Muted text on a muted fill (muted_foreground on muted) | `#8b8b93` on `#1f1f23` | 4.86:1 | 4.5:1 | pass | kbd.rs:84, tabs.rs (inactive tab), segmented.rs (unselected) |
| Placeholder in a field on the page (muted_foreground on input at 30% over background) | `#8b8b93` on `#1c1c1f` | 5.03:1 | 4.5:1 | pass | input.rs:2361 (placeholder), input.rs fill |
| Placeholder in a field on a card (muted_foreground on field_on_card) | `#8b8b93` on `#222226` | 4.69:1 | 4.5:1 | pass | input.rs:2361, tokens field-on-card |
| Sidebar row time (muted_70 on sidebar) | `#65656b` on `#0b0b0d` | 3.40:1 | 4.5:1 | **fail** | tokens muted-70 (the apps' sidebar) |

### Status text

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Destructive text on the page (destructive on background) | `#e5665b` on `#0f0f11` | 5.82:1 | 4.5:1 | pass | input.rs error, toast.rs |
| Destructive text on a card (destructive on card) | `#e5665b` on `#18181b` | 5.39:1 | 4.5:1 | pass | alert.rs title |
| Destructive description on a card (destructive at 90% on card) | `#d05e55` on `#18181b` | 4.57:1 | 4.5:1 | pass | alert.rs description |
| Destructive button and badge (destructive on its fill over background) | `#e5665b` on `#3a2020` | 4.54:1 | 4.5:1 | pass | button.rs Destructive (dark 20%), badge.rs |
| Destructive in a toast (destructive on popover) | `#e5665b` on `#18181b` | 5.39:1 | 4.5:1 | pass | toast.rs |

### Callouts

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Note callout (callout_note on its background over the page) | `#4493f8` on `#131a23` | 5.65:1 | 4.5:1 | pass | callout title and icon (gpui-blocks) |
| Tip callout | `#3fb950` on `#131d16` | 6.80:1 | 4.5:1 | pass | callout title |
| Important callout | `#ab7df8` on `#1b1823` | 5.84:1 | 4.5:1 | pass | callout title |
| Warning callout | `#d29922` on `#1f1a12` | 6.85:1 | 4.5:1 | pass | callout title |
| Caution callout | `#f85149` on `#221415` | 5.31:1 | 4.5:1 | pass | callout title |

### Code

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Syntax keyword on background | `#c4a7f5` on `#0f0f11` | 9.29:1 | 4.5:1 | pass | code blocks |
| Syntax keyword on secondary | `#c4a7f5` on `#1f1f23` | 7.97:1 | 4.5:1 | pass | code blocks on a fill |
| Syntax string on secondary | `#9fd38a` on `#1f1f23` | 9.51:1 | 4.5:1 | pass | code blocks |
| Syntax comment on secondary | `#8b8b93` on `#1f1f23` | 4.86:1 | 4.5:1 | pass | code blocks |
| Syntax function on secondary | `#8ab4f8` on `#1f1f23` | 7.79:1 | 4.5:1 | pass | code blocks |
| Syntax type and attribute on secondary | `#e9c46a` on `#1f1f23` | 9.83:1 | 4.5:1 | pass | code blocks |
| Syntax constant on secondary | `#f4a072` on `#1f1f23` | 7.91:1 | 4.5:1 | pass | code blocks |
| Syntax property on secondary | `#7fd3e6` on `#1f1f23` | 9.67:1 | 4.5:1 | pass | code blocks |
| Syntax tag on secondary | `#f28b82` on `#1f1f23` | 6.88:1 | 4.5:1 | pass | code blocks |
| Syntax punctuation on secondary | `#a1a1aa` on `#1f1f23` | 6.41:1 | 4.5:1 | pass | code blocks |

### App status colours (used by the apps, not gpui-core's components)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| success_text on background | `#34d399` on `#0f0f11` | 9.96:1 | 4.5:1 | pass | tokens success-text |
| success_text on card | `#34d399` on `#18181b` | 9.22:1 | 4.5:1 | pass | tokens success-text |
| warning_text on warning_fill over background | `#fbbf24` on `#261e10` | 9.86:1 | 4.5:1 | pass | tokens warning-text, warning-fill |
| attention_foreground on background | `#e3b45a` on `#0f0f11` | 9.98:1 | 4.5:1 | pass | tokens attention |
| live (as text) on background | `#4cc38a` on `#0f0f11` | 8.64:1 | 4.5:1 | pass | tokens live |
| live status dot against the page | `#4cc38a` on `#0f0f11` | 8.64:1 | 3:1 | pass | tokens live |
| attention status dot against the page | `#e3b45a` on `#0f0f11` | 9.98:1 | 3:1 | pass | tokens attention |

### Disabled (exempt)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Disabled text (foreground at 50% on background) | `#7e7e80` on `#0f0f11` | 4.73:1 | exempt | exempt | disabled controls: opacity 0.5 (button.rs:417, switch, toggle, tabs, segmented, checkbox) |
| Disabled muted text (muted_foreground at 50% on background) | `#4d4d52` on `#0f0f11` | 2.28:1 | exempt | exempt | as above |

### Focus and boundaries (3:1)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Focus ring (ring at 50%) against the page | `#57575d` on `#0f0f11` | 2.67:1 | 3:1 | **fail** | theme.rs focus_ring(), press.rs:24 |
| Focus ring (ring at 50%) against a card | `#5c5c62` on `#18181b` | 2.67:1 | 3:1 | **fail** | theme.rs focus_ring() |
| Focus ring (ring at 50%) against a muted list (tabs, segments) | `#5f5f66` on `#1f1f23` | 2.59:1 | 3:1 | **fail** | tabs.rs, segmented.rs |
| Focused field border (ring) against the page | `#9f9fa9` on `#0f0f11` | 7.30:1 | 3:1 | pass | input.rs (focused border) |
| Field border (input) against the page | `#3a3a41` on `#0f0f11` | 1.70:1 | 3:1 | **fail** | input.rs, checkbox.rs outline, toggle.rs outline |
| Field border (input) against a card | `#3a3a41` on `#18181b` | 1.57:1 | 3:1 | **fail** | input.rs on a card |
| Switch track off (input at 80%) against the page | `#313137` on `#0f0f11` | 1.48:1 | 3:1 | **fail** | switch.rs:125 (dark) |
| Outline button border against the page | `#3a3a41` on `#0f0f11` | 1.70:1 | 3:1 | **fail** | button.rs Outline (dark: input) |
| Border against the page (dividers, card edges) | `#28282d` on `#0f0f11` | 1.31:1 | 3:1 | **fail** | card.rs, separator.rs, alert.rs |
| Menu and popover edge (overlay_line over popover) against the page | `#2e2e31` on `#0f0f11` | 1.41:1 | 3:1 | **fail** | tokens overlay-line |

### Charts (3:1)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| chart_1 against the page | `#1447e6` on `#0f0f11` | 2.80:1 | 3:1 | **fail** | chart.rs (series colours) |
| chart_2 against the page | `#00bc7d` on `#0f0f11` | 7.74:1 | 3:1 | pass | chart.rs |
| chart_3 against the page | `#fe9a00` on `#0f0f11` | 8.97:1 | 3:1 | pass | chart.rs |
| chart_4 against the page | `#ad46ff` on `#0f0f11` | 4.65:1 | 3:1 | pass | chart.rs |
| chart_5 against the page | `#ff2056` on `#0f0f11` | 5.10:1 | 3:1 | pass | chart.rs |
| chart_1 against a card | `#1447e6` on `#18181b` | 2.59:1 | 3:1 | **fail** | chart.rs in a card |
| chart_3 against a card | `#fe9a00` on `#18181b` | 8.30:1 | 3:1 | pass | chart.rs in a card |

## Light theme

66 pairs; 14 below the WCAG 2.2 AA level they need.

### Text

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Text on the page (foreground on background) | `#0f0f11` on `#fafafa` | 18.34:1 | 4.5:1 | pass | everywhere; text links too (text/view.rs:1675, underlined) |
| Text on a card (card_foreground on card) | `#0f0f11` on `#ffffff` | 19.15:1 | 4.5:1 | pass | card.rs, alert.rs |
| Text in a popover or menu (popover_foreground on popover) | `#0f0f11` on `#ffffff` | 19.15:1 | 4.5:1 | pass | toast.rs, menus |
| Text on a secondary fill (secondary_foreground on secondary) | `#0f0f11` on `#f1f1f3` | 16.97:1 | 4.5:1 | pass | button.rs Secondary, badge.rs Secondary |
| Selected tab or segment (foreground on background, in a muted list) | `#0f0f11` on `#fafafa` | 18.34:1 | 4.5:1 | pass | tabs.rs:341, segmented.rs:291 |
| Primary button and badge (primary_foreground on primary) | `#ffffff` on `#0f0f11` | 19.15:1 | 4.5:1 | pass | button.rs Primary, badge.rs Default, checkbox.rs tick |
| Ghost button or row hover (accent_foreground on accent over background) | `#0f0f11` on `#ededed` | 16.36:1 | 4.5:1 | pass | button.rs Ghost, sidebar.rs:433 |
| Menu row hover (accent_foreground on accent over popover) | `#0f0f11` on `#f2f2f2` | 17.10:1 | 4.5:1 | pass | menus over popover |
| Selected outline or ghost button (foreground on muted) | `#0f0f11` on `#f1f1f3` | 16.97:1 | 4.5:1 | pass | button.rs selected (light: muted) |
| Tooltip (background on foreground) | `#fafafa` on `#0f0f11` | 18.34:1 | 4.5:1 | pass | tooltip.rs:90 |
| Sidebar text (sidebar_foreground on sidebar) | `#0f0f11` on `#f1f1f3` | 16.97:1 | 4.5:1 | pass | sidebar.rs:769 |
| Sidebar selected row (sidebar_accent_foreground on sidebar_accent over sidebar) | `#0f0f11` on `#e5e5e7` | 15.22:1 | 4.5:1 | pass | sidebar.rs:433 |
| Sidebar group label (sidebar_foreground at 70% on sidebar) | `#535355` on `#f1f1f3` | 6.80:1 | 4.5:1 | pass | sidebar.rs:501 |

### Secondary text

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Muted text on the page (muted_foreground on background) | `#6b6b73` on `#fafafa` | 5.06:1 | 4.5:1 | pass | 23 uses, e.g. alert.rs, toast.rs, input.rs |
| Muted text on a card (muted_foreground on card) | `#6b6b73` on `#ffffff` | 5.28:1 | 4.5:1 | pass | alert.rs description, card.rs |
| Muted text in a popover (muted_foreground on popover) | `#6b6b73` on `#ffffff` | 5.28:1 | 4.5:1 | pass | toast.rs description |
| Muted text on a muted fill (muted_foreground on muted) | `#6b6b73` on `#f1f1f3` | 4.68:1 | 4.5:1 | pass | kbd.rs:84, tabs.rs (inactive tab), segmented.rs (unselected) |
| Placeholder in a field on the page (muted_foreground on input at 30% over background) | `#6b6b73` on `#f0f0f2` | 4.64:1 | 4.5:1 | pass | input.rs:2361 (placeholder), input.rs fill |
| Placeholder in a field on a card (muted_foreground on field_on_card) | `#6b6b73` on `#f4f4f5` | 4.81:1 | 4.5:1 | pass | input.rs:2361, tokens field-on-card |
| Sidebar row time (muted_70 on sidebar) | `#939399` on `#f1f1f3` | 2.71:1 | 4.5:1 | **fail** | tokens muted-70 (the apps' sidebar) |

### Status text

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Destructive text on the page (destructive on background) | `#c7362b` on `#fafafa` | 5.04:1 | 4.5:1 | pass | input.rs error, toast.rs |
| Destructive text on a card (destructive on card) | `#c7362b` on `#ffffff` | 5.26:1 | 4.5:1 | pass | alert.rs title |
| Destructive description on a card (destructive at 90% on card) | `#cd4a40` on `#ffffff` | 4.52:1 | 4.5:1 | pass | alert.rs description |
| Destructive button and badge (destructive on its fill over background) | `#c7362b` on `#f5e6e5` | 4.34:1 | 4.5:1 | **fail** | button.rs Destructive (light 10%), badge.rs |
| Destructive in a toast (destructive on popover) | `#c7362b` on `#ffffff` | 5.26:1 | 4.5:1 | pass | toast.rs |

### Callouts

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Note callout (callout_note on its background over the page) | `#0968d8` on `#e7eef7` | 4.51:1 | 4.5:1 | pass | callout title and icon (gpui-blocks) |
| Tip callout | `#197c36` on `#e8f0ea` | 4.55:1 | 4.5:1 | pass | callout title |
| Important callout | `#7e4ed8` on `#f0ecf7` | 4.55:1 | 4.5:1 | pass | callout title |
| Warning callout | `#926200` on `#f2eee6` | 4.57:1 | 4.5:1 | pass | callout title |
| Caution callout | `#cf242f` on `#f7e9ea` | 4.51:1 | 4.5:1 | pass | callout title |

### Code

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Syntax keyword on background | `#7c3aed` on `#fafafa` | 5.46:1 | 4.5:1 | pass | code blocks |
| Syntax keyword on secondary | `#7c3aed` on `#f1f1f3` | 5.05:1 | 4.5:1 | pass | code blocks on a fill |
| Syntax string on secondary | `#237a35` on `#f1f1f3` | 4.77:1 | 4.5:1 | pass | code blocks |
| Syntax comment on secondary | `#63636b` on `#f1f1f3` | 5.28:1 | 4.5:1 | pass | code blocks |
| Syntax function on secondary | `#1d5fd1` on `#f1f1f3` | 5.16:1 | 4.5:1 | pass | code blocks |
| Syntax type and attribute on secondary | `#8a5a00` on `#f1f1f3` | 5.25:1 | 4.5:1 | pass | code blocks |
| Syntax constant on secondary | `#b2430c` on `#f1f1f3` | 5.03:1 | 4.5:1 | pass | code blocks |
| Syntax property on secondary | `#0b6e85` on `#f1f1f3` | 5.20:1 | 4.5:1 | pass | code blocks |
| Syntax tag on secondary | `#b3261e` on `#f1f1f3` | 5.79:1 | 4.5:1 | pass | code blocks |
| Syntax punctuation on secondary | `#52525b` on `#f1f1f3` | 6.85:1 | 4.5:1 | pass | code blocks |

### App status colours (used by the apps, not gpui-core's components)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| success_text on background | `#047857` on `#fafafa` | 5.25:1 | 4.5:1 | pass | tokens success-text |
| success_text on card | `#047857` on `#ffffff` | 5.48:1 | 4.5:1 | pass | tokens success-text |
| warning_text on warning_fill over background | `#b45309` on `#f9f1e2` | 4.47:1 | 4.5:1 | **fail** | tokens warning-text, warning-fill |
| attention_foreground on background | `#a8701a` on `#fafafa` | 4.03:1 | 4.5:1 | **fail** | tokens attention |
| live (as text) on background | `#1f8f5c` on `#fafafa` | 3.91:1 | 4.5:1 | **fail** | tokens live |
| live status dot against the page | `#1f8f5c` on `#fafafa` | 3.91:1 | 3:1 | pass | tokens live |
| attention status dot against the page | `#d9a441` on `#fafafa` | 2.15:1 | 3:1 | **fail** | tokens attention |

### Disabled (exempt)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Disabled text (foreground at 50% on background) | `#848486` on `#fafafa` | 3.58:1 | exempt | exempt | disabled controls: opacity 0.5 (button.rs:417, switch, toggle, tabs, segmented, checkbox) |
| Disabled muted text (muted_foreground at 50% on background) | `#b2b2b6` on `#fafafa` | 2.02:1 | exempt | exempt | as above |

### Focus and boundaries (3:1)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| Focus ring (ring at 50%) against the page | `#848486` on `#fafafa` | 3.58:1 | 3:1 | pass | theme.rs focus_ring(), press.rs:24 |
| Focus ring (ring at 50%) against a card | `#878788` on `#ffffff` | 3.59:1 | 3:1 | pass | theme.rs focus_ring() |
| Focus ring (ring at 50%) against a muted list (tabs, segments) | `#808082` on `#f1f1f3` | 3.49:1 | 3:1 | pass | tabs.rs, segmented.rs |
| Focused field border (ring) against the page | `#0f0f11` on `#fafafa` | 18.34:1 | 3:1 | pass | input.rs (focused border) |
| Field border (input) against the page | `#d9d9de` on `#fafafa` | 1.35:1 | 3:1 | **fail** | input.rs, checkbox.rs outline, toggle.rs outline |
| Field border (input) against a card | `#d9d9de` on `#ffffff` | 1.41:1 | 3:1 | **fail** | input.rs on a card |
| Switch track off (input) against the page | `#d9d9de` on `#fafafa` | 1.35:1 | 3:1 | **fail** | switch.rs:125 (light) |
| Outline button border against the page | `#e4e4e7` on `#fafafa` | 1.22:1 | 3:1 | **fail** | button.rs Outline (light: border) |
| Border against the page (dividers, card edges) | `#e4e4e7` on `#fafafa` | 1.22:1 | 3:1 | **fail** | card.rs, separator.rs, alert.rs |
| Menu and popover edge (overlay_line over popover) against the page | `#e7e7e7` on `#fafafa` | 1.18:1 | 3:1 | **fail** | tokens overlay-line |

### Charts (3:1)

| Pair | Colours (as drawn) | Ratio | Needed | Result | Where |
| --- | --- | --- | --- | --- | --- |
| chart_1 against the page | `#f54900` on `#fafafa` | 3.45:1 | 3:1 | pass | chart.rs (series colours) |
| chart_2 against the page | `#009689` on `#fafafa` | 3.52:1 | 3:1 | pass | chart.rs |
| chart_3 against the page | `#104e64` on `#fafafa` | 8.76:1 | 3:1 | pass | chart.rs |
| chart_4 against the page | `#ffb900` on `#fafafa` | 1.65:1 | 3:1 | **fail** | chart.rs |
| chart_5 against the page | `#fe9a00` on `#fafafa` | 2.05:1 | 3:1 | **fail** | chart.rs |
| chart_1 against a card | `#f54900` on `#ffffff` | 3.60:1 | 3:1 | pass | chart.rs in a card |
| chart_3 against a card | `#104e64` on `#ffffff` | 9.15:1 | 3:1 | pass | chart.rs in a card |

## Summary of failures, with values that would pass

The main text passes in both themes: the foreground on every fill, the primary, secondary and selected states, tooltips, the sidebar, destructive text in the dark theme, every callout and every syntax colour. The failures are secondary text in the sidebar, the boundaries that show controls, the dark focus ring, a few status colours and two chart colours per theme.

Each suggestion below keeps the colour's hue. It moves the lightness (or the opacity) only as far as needed to pass, with a little margin, against every fill the colour is drawn on.

### Must fix for AA

| Token | Theme | Now | Ratio now | Suggested | Ratio then | Why |
| --- | --- | --- | --- | --- | --- | --- |
| `muted-70` (sidebar row time) | dark | `#8b8b93` at 70% | 3.40:1 | `#8b8b93` at 86% | 4.55:1 | text (1.4.3) |
| `muted-70` (sidebar row time) | light | `#6b6b73` at 70% | 2.71:1 | `muted-foreground` itself, at 100% | 4.68:1 | text; 70% can't pass in light |
| focus ring (`ring` at 50%) | dark | `#9f9fa9` at 50% | 2.59–2.67:1 | `#9f9fa9` at 60% (`Colors::focus_ring`) | 3.15–3.31:1 on page, card and muted | focus indicator (2.4.13, 1.4.11) |
| `input` (a field's border, a checkbox's outline, the off switch's track) | dark | `#3a3a41` | 1.57–1.70:1 | `#656571` | 3.08:1 | the boundary that shows the control (1.4.11) |
| off switch track | dark | `input` at 80% | 1.48:1 | the new `input` at 100% (`switch.rs:125` draws it at 80%) | 3.08:1 | state of the control (1.4.11) |
| `input` (same uses) | light | `#d9d9de` | 1.35–1.41:1 | `#8f8f9e` | 3.05:1 | as above |
| `destructive` | light | `#c7362b` | 4.34:1 on its 10% fill | `#c1342a` | 4.57:1 | destructive button and badge text |
| `chart-1` | dark | `#1447e6` | 2.59–2.80:1 | `#2556ec` | 3.05:1 | chart series (1.4.11) |
| `chart-4` | light | `#ffb900` | 1.65:1 | `#bb8700` | 3.06:1 | chart series |
| `chart-5` | light | `#fe9a00` | 2.05:1 | `#cf7d00` | 3.05:1 | chart series |

Notes on these:
- **The field and checkbox outline (`input`)** is the largest visual change. In the dark theme a field's fill is `input` at 30% over the page (1.15:1), so its outline is what shows where the field is, and WCAG asks 3:1 for it. An alternative that keeps the quiet outline is a fill that reaches 3:1 against the page, but that's a bigger change to the look.
- **The light theme's `muted-70`** can't reach 4.5:1 below about 99% opacity, so it is simply `muted-foreground`.
- **The light theme's `chart-4` and `chart-5`** are both oranges. Darkened, they come close to each other, so a different hue for one of them may be better than these values.

### Fix where the apps use them (status colours)

These tokens aren't drawn by gpui-core's components, but the apps use them, so they're part of the same compliance answer.

| Token | Theme | Now | Ratio now | Suggested | Ratio then |
| --- | --- | --- | --- | --- | --- |
| `warning-text` on `warning-fill` | light | `#b45309` | 4.47:1 | `#b25209` | 4.56:1 |
| `attention` as text (`attention_foreground`) | light | `#a8701a` | 4.03:1 | `#9c6818` | 4.57:1 |
| `live` as text | light | `#1f8f5c` | 3.91:1 | `#1c8254` | 4.60:1 |
| `attention` as a status dot | light | `#d9a441` | 2.15:1 | `#ba8625` | 3.08:1 |

`live` and `attention` are fine as dots in the dark theme (3:1 and above). As text in the light theme they need the darker values, or a separate text token like `attention_foreground` that's used for all of it.

### Advisory: decorative lines

WCAG doesn't require 3:1 for lines that only decorate, such as dividers between rows, a card's edge, or a menu's or popover's edge. The content is identifiable without them, so these failures are listed for completeness, not as AA failures.

| Token | Theme | Now | Ratio | A value at 3:1 |
| --- | --- | --- | --- | --- |
| `border` | dark | `#28282d` | 1.31:1 | `#60606c` |
| `border` | light | `#e4e4e7` | 1.22:1 | `#8f8f9c` |
| `overlay-line` (menu edge) | both | foreground at 10% | 1.18–1.41:1 | (decorative) |

One exception: in the light theme the outline button's border is `border`. If an outline button has no other visible boundary (its fill is the page's colour), its border is what identifies it, and it should use the new `input` instead.

### Disabled controls

Disabled controls are drawn at 50% opacity: their text comes to 4.73:1 (dark) and 3.58:1 (light), and muted text to 2.28:1 and 2.02:1. WCAG exempts them (1.4.3). They still read as present, as they should.
