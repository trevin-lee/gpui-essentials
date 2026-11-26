//! The Next.js site's live previews: any demo in the registry (docs-site/registry), drawn in
//! the middle of the page's one GPUI window.
//!
//! GPUI's web platform opens one window per page, in a canvas it adds to the page's body, and
//! can't open another or reopen it once closed. So the site starts the app once a visit
//! ([`stage`]), moves that canvas into whichever preview is live, and has the window draw that
//! preview's demo ([`mount`]); the other previews show their pictures.

use std::cell::RefCell;

use docs_site_registry::Demo;
use gpui::*;
use gpui_core::*;
use wasm_bindgen::prelude::*;

/// The window's view: one demo, or nothing.
struct Stage {
    demo: Option<Staged>,
}

enum Staged {
    /// An element, built again each frame.
    Element(fn(&mut Window, &mut App) -> AnyElement),
    /// A view with state of its own, built when mounted.
    View(AnyView),
}

impl Render for Stage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let demo = match &self.demo {
            Some(Staged::Element(render)) => Some(render(window, cx)),
            Some(Staged::View(view)) => Some(view.clone().into_any_element()),
            None => None,
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(rems(2.))
            .bg(theme.background)
            .text_color(theme.foreground)
            .children(demo)
    }
}

/// The running app's window and its stage, once [`stage`] has opened them.
struct Live {
    app: AsyncApp,
    window: AnyWindowHandle,
    stage: Entity<Stage>,
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

/// Starts the app, in the dark theme or the light one, with an empty stage. GPUI adds its
/// canvas to the end of the page's body; once the window is open, `ready` is called with no
/// argument, or with what went wrong (a browser with neither WebGPU nor WebGL2).
#[wasm_bindgen]
pub fn stage(dark: bool, ready: js_sys::Function) {
    gpui_platform::web_init();
    // Every Lucide icon, so any demo's icons draw: about 85 KB more, gzipped.
    gpui_platform::single_threaded_web()
        .with_assets(Icons)
        .run(move |cx| {
            crate::setup(cx, dark);
            let opened = open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| Stage { demo: None })
            });
            let error = match opened {
                Ok((window, stage)) => {
                    let app = cx.to_async();
                    LIVE.with(|live| *live.borrow_mut() = Some(Live { app, window, stage }));
                    JsValue::UNDEFINED
                }
                Err(e) => JsValue::from_str(&format!("{e:#}")),
            };
            // Later, outside this update: the page's `ready` may call `mount` straight away.
            cx.spawn(async move |_| {
                let _ = ready.call1(&JsValue::NULL, &error);
            })
            .detach();
        });
}

/// Runs `f` with the app and its stage, if [`stage`] has opened them.
fn with_live<R>(f: impl FnOnce(&Live, &mut App) -> R) -> Option<R> {
    LIVE.with(|live| {
        let live = live.borrow();
        let live = live.as_ref()?;
        Some(live.app.update(|cx| f(live, cx)))
    })
}

/// Has the stage draw `variant` of `component` (their slugs), with fresh state. Returns
/// whether the registry has it and the stage is open.
#[wasm_bindgen]
pub fn mount(component: &str, variant: &str) -> bool {
    let Some(demo) = docs_site_registry::components()
        .into_iter()
        .find(|c| c.slug == component)
        .and_then(|c| c.variants.into_iter().find(|v| v.slug == variant))
        .map(|v| v.demo)
    else {
        return false;
    };
    with_live(|live, cx| {
        live.window
            .update(cx, |_, window, cx| {
                live.stage.update(cx, |stage, cx| {
                    stage.demo = Some(match demo {
                        Demo::Element(render) => Staged::Element(render),
                        Demo::View(build) => Staged::View(build(window, cx)),
                    });
                    cx.notify();
                });
            })
            .is_ok()
    })
    .unwrap_or(false)
}

/// Empties the stage, dropping the demo's state: while the canvas is put away.
#[wasm_bindgen]
pub fn unmount() {
    with_live(|live, cx| {
        live.stage.update(cx, |stage, cx| {
            stage.demo = None;
            cx.notify();
        });
    });
}

/// Draws in the dark theme or the light one, as the page does.
#[wasm_bindgen]
pub fn theme(dark: bool) {
    with_live(|_, cx| {
        Theme::set(crate::theme(dark), cx);
        cx.refresh_windows();
    });
}
