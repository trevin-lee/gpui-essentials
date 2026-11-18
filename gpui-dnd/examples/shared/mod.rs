//! What the examples share: a window in gpui-core's theme, and a picture of it taken offscreen
//! after a scripted drag (`--shot out.png --scene <name>`, with `--features shots`).

#![allow(dead_code)]

use std::path::PathBuf;

use gpui::*;

pub struct Args {
    pub shot: Option<PathBuf>,
    pub scene: String,
}

pub fn args() -> Args {
    let mut out = Args {
        shot: None,
        scene: String::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shot" => out.shot = it.next().map(PathBuf::from),
            "--scene" => out.scene = it.next().unwrap_or_default(),
            _ => {}
        }
    }
    out
}

/// A step of a scripted drag, for a picture.
#[derive(Clone, Copy)]
pub enum Step {
    Press(Point<Pixels>),
    Move(Point<Pixels>),
    Release(Point<Pixels>),
    Key(&'static str),
}

actions!(dnd_example, [Quit]);

/// Opens `build`'s view in a window of `size` inside gpui-core's `Root`; with `--shot`, plays
/// `script(scene)` and saves a picture of the window instead of showing it.
pub fn run<V: Render>(
    title: &'static str,
    size: Size<Pixels>,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
    script: impl Fn(&str) -> Vec<Step> + 'static,
) {
    let args = args();
    gpui_platform::application()
        .with_assets(gpui_core::Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            let shooting = args.shot.is_some();
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(title.into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
                // A picture doesn't take the keyboard or come forward, and isn't shown at all.
                focus: !shooting,
                show: !shooting,
                ..Default::default()
            };
            let (window, _) = gpui_core::open_window(options, cx, build).expect("the window");
            if !shooting {
                cx.activate(true);
            }
            #[cfg(feature = "shots")]
            if let Some(path) = args.shot.clone() {
                shots::shot(window, path, script(&args.scene), cx);
            }
            let _ = (window, &script);
        });
}

#[cfg(feature = "shots")]
mod shots {
    use super::*;
    use std::time::Duration;

    async fn wait(cx: &mut AsyncApp, ms: u64) {
        cx.background_executor()
            .timer(Duration::from_millis(ms))
            .await;
    }

    /// Draws the window now, more than once, letting slides and timers run between: a window
    /// that isn't shown gets no frames from the display.
    async fn settle(window: AnyWindowHandle, cx: &mut AsyncApp) {
        for _ in 0..8 {
            let _ = cx.update_window(window, |_, window, cx| {
                window.simulate_next_frame(cx);
                window.refresh();
                let _ = window.draw(cx);
            });
            wait(cx, 40).await;
        }
    }

    fn send(window: AnyWindowHandle, cx: &mut AsyncApp, event: PlatformInput) {
        let _ = cx.update_window(window, |_, window, cx| {
            window.dispatch_event(event, cx);
        });
    }

    pub fn shot(window: AnyWindowHandle, path: PathBuf, steps: Vec<Step>, cx: &mut App) {
        cx.spawn(async move |cx| {
            wait(cx, 300).await;
            settle(window, cx).await;
            let mut held = false;
            for step in steps {
                match step {
                    Step::Press(at) => {
                        send(
                            window,
                            cx,
                            PlatformInput::MouseMove(MouseMoveEvent {
                                position: at,
                                pressed_button: None,
                                modifiers: Modifiers::none(),
                            }),
                        );
                        send(
                            window,
                            cx,
                            PlatformInput::MouseDown(MouseDownEvent {
                                button: MouseButton::Left,
                                position: at,
                                modifiers: Modifiers::none(),
                                click_count: 1,
                                first_mouse: false,
                            }),
                        );
                        held = true;
                    }
                    Step::Move(at) => send(
                        window,
                        cx,
                        PlatformInput::MouseMove(MouseMoveEvent {
                            position: at,
                            pressed_button: held.then_some(MouseButton::Left),
                            modifiers: Modifiers::none(),
                        }),
                    ),
                    Step::Release(at) => {
                        send(
                            window,
                            cx,
                            PlatformInput::MouseUp(MouseUpEvent {
                                button: MouseButton::Left,
                                position: at,
                                modifiers: Modifiers::none(),
                                click_count: 1,
                            }),
                        );
                        held = false;
                    }
                    Step::Key(key) => send(
                        window,
                        cx,
                        PlatformInput::KeyDown(KeyDownEvent {
                            keystroke: Keystroke::parse(key).expect("a key"),
                            is_held: false,
                            prefer_character_input: false,
                        }),
                    ),
                }
                settle(window, cx).await;
            }
            let _ = cx.update_window(window, |_, window, cx| {
                window.refresh();
                let _ = window.draw(cx);
                match window.render_to_image() {
                    Ok(img) => {
                        if let Err(e) = img.save(&path) {
                            eprintln!("the picture wasn't saved: {e}");
                        }
                    }
                    Err(e) => eprintln!("the window didn't render to a picture: {e}"),
                }
                cx.quit();
            });
        })
        .detach();
    }
}

/// Moves in a few steps from `from` to `to`, as a hand does.
pub fn glide(from: Point<Pixels>, to: Point<Pixels>) -> impl Iterator<Item = Step> {
    (1..=6).map(move |i| Step::Move(from + (to - from) * (i as f32 / 6.)))
}

/// A mouse drag from `from` to `to`, held there (not let go).
pub fn hold(from: Point<Pixels>, to: Point<Pixels>) -> Vec<Step> {
    let mut steps = vec![Step::Press(from), Step::Move(from + point(px(8.), px(0.)))];
    steps.extend(glide(from + point(px(8.), px(0.)), to));
    steps
}
