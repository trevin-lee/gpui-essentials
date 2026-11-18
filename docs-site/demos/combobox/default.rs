// A button showing the option chosen that opens a search over the options, filtered as you
// type. For a short list with no search, use a Select.
pub struct Example {
    framework: Option<SharedString>,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { framework: None }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Combobox::new("framework")
            .accessibility_label("Framework")
            .placeholder("Select framework…")
            .search_placeholder("Search framework…")
            .empty_text("No framework found.")
            .options([
                ("next", "Next.js"),
                ("svelte", "SvelteKit"),
                ("nuxt", "Nuxt.js"),
                ("remix", "Remix"),
                ("astro", "Astro"),
            ])
            .selected(self.framework.clone())
            .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                this.framework = Some(id.clone());
                cx.notify();
            }))
    }
}
