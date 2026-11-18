// Markdown drawn as text: headings, emphasis, lists, links and code, selectable and copied
// as you'd expect.
div().w(rems(28.)).child(TextView::markdown(
    "notes",
    "### Release notes\n\nThis version adds **Toggle Group** and fixes the *Select* list on narrow windows.\n\n- Drag to reorder rows\n- `⌘K` opens the command palette\n- See [the changelog](https://example.com) for more",
))
