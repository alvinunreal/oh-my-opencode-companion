use std::cell::RefCell;
use std::rc::Rc;

use super::{
    egui, open_companion_menu, render_companion_menu, SessionInfo, MENU_OPEN_KEY, POPUP_H, POPUP_W,
};
use egui::{Event, PointerButton, ViewportCommand, ViewportId};

fn popup_id() -> ViewportId {
    ViewportId::from_hash_of("companion-controls")
}

fn target() -> SessionInfo {
    SessionInfo {
        session_id: "popup-target".into(),
        cwd: "/popup-target".into(),
        active_agents: vec!["fixer".into()],
        active_agent_details: Vec::new(),
        active_agent: None,
        status: "busy".into(),
        attention_seq: 0,
        pid: None,
        config: None,
        preset: None,
    }
}

fn input(viewport_id: ViewportId, focused: bool, events: Vec<Event>) -> egui::RawInput {
    let size = egui::vec2(POPUP_W, POPUP_H);
    let mut input = egui::RawInput {
        viewport_id,
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        focused,
        events,
        ..Default::default()
    };
    for id in [ViewportId::ROOT, popup_id()] {
        input.viewports.insert(
            id,
            egui::ViewportInfo {
                parent: (id != ViewportId::ROOT).then_some(ViewportId::ROOT),
                native_pixels_per_point: Some(1.0),
                inner_rect: Some(egui::Rect::from_min_size(egui::pos2(300.0, 200.0), size)),
                outer_rect: Some(egui::Rect::from_min_size(egui::pos2(300.0, 200.0), size)),
                focused: Some(focused),
                ..Default::default()
            },
        );
    }
    input
}

fn render(ctx: &egui::Context) {
    let target = target();
    assert!(
        render_companion_menu(ctx, std::slice::from_ref(&target), &[], &target, false).is_none()
    );
}

fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<bool>(egui::Id::new(MENU_OPEN_KEY))
            .unwrap_or(false)
    })
}

fn assert_root_untouched(output: &egui::FullOutput) {
    if let Some(root) = output.viewport_output.get(&ViewportId::ROOT) {
        assert!(
            !root.commands.iter().any(|command| matches!(
                command,
                ViewportCommand::Visible(_)
                    | ViewportCommand::Focus
                    | ViewportCommand::StartDrag
                    | ViewportCommand::OuterPosition(_)
                    | ViewportCommand::InnerSize(_)
                    | ViewportCommand::MinInnerSize(_)
                    | ViewportCommand::MaxInnerSize(_)
            )),
            "popup changed the animation viewport: {:?}",
            root.commands
        );
    }
}

struct NativeFrame {
    builder: egui::ViewportBuilder,
    child_output: egui::FullOutput,
    root_output: egui::FullOutput,
}

impl NativeFrame {
    fn commands(&self) -> Vec<ViewportCommand> {
        // egui 0.29 normally drains child commands when the outer root frame
        // ends. Inspect both outputs, just as the native integration does.
        [&self.child_output, &self.root_output]
            .into_iter()
            .filter_map(|output| output.viewport_output.get(&popup_id()))
            .flat_map(|output| output.commands.iter().cloned())
            .collect()
    }

    fn assert_no_reveal(&self) {
        assert!(!self.commands().iter().any(|command| matches!(
            command,
            ViewportCommand::Visible(_) | ViewportCommand::Focus
        )));
    }
}

struct NativePopup {
    ctx: egui::Context,
    next_input: Rc<RefCell<Option<egui::RawInput>>>,
    rendered: Rc<RefCell<Option<(egui::ViewportBuilder, egui::FullOutput)>>>,
}

impl NativePopup {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.set_embed_viewports(false);
        let next_input = Rc::new(RefCell::new(None::<egui::RawInput>));
        let rendered = Rc::new(RefCell::new(None));
        let callback_input = Rc::clone(&next_input);
        let callback_output = Rc::clone(&rendered);
        egui::Context::set_immediate_viewport_renderer(move |ctx, mut viewport| {
            assert_eq!(viewport.ids.this, popup_id());
            assert_eq!(viewport.ids.parent, ViewportId::ROOT);
            let input = callback_input.borrow_mut().take().expect("child input");
            assert_eq!(input.viewport_id, viewport.ids.this);
            // Exercise the real immediate callback under the child viewport,
            // with the same nested Context::run contract as eframe.
            let output = ctx.run(input, |ctx| (viewport.viewport_ui_cb)(ctx));
            assert_root_untouched(&output);
            assert!(callback_output
                .borrow_mut()
                .replace((viewport.builder, output))
                .is_none());
        });
        Self {
            ctx,
            next_input,
            rendered,
        }
    }

    fn frame(&self, input: egui::RawInput) -> NativeFrame {
        *self.next_input.borrow_mut() = Some(input);
        let root_output = self.ctx.run(input_for_root(), render);
        assert_root_untouched(&root_output);
        let (builder, child_output) = self
            .rendered
            .borrow_mut()
            .take()
            .expect("native popup render");
        NativeFrame {
            builder,
            child_output,
            root_output,
        }
    }

    fn ready(&self) {
        open_companion_menu(&self.ctx, egui::pos2(20.0, 20.0));
        self.frame(input(popup_id(), false, vec![]));
        self.frame(input(popup_id(), true, vec![]));
    }
}

fn input_for_root() -> egui::RawInput {
    input(ViewportId::ROOT, true, vec![])
}

fn native_test(test: impl FnOnce() + Send + 'static) {
    // The egui renderer is thread-local and has no getter. A dedicated thread
    // drops it even on assertion failure, without altering another test's host.
    if let Err(panic) = std::thread::spawn(test).join() {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn native_popup_renders_hidden_then_reveals_once_on_each_open() {
    native_test(|| {
        let popup = NativePopup::new();
        for _ in 0..2 {
            open_companion_menu(&popup.ctx, egui::pos2(20.0, 20.0));
            let first = popup.frame(input(popup_id(), false, vec![]));
            assert_eq!(first.builder.visible, Some(false));
            assert_eq!(first.builder.active, Some(false));
            assert!(
                !first.child_output.shapes.is_empty(),
                "hidden frame must render the menu"
            );
            first.assert_no_reveal();
            assert!(
                is_open(&popup.ctx),
                "reopening must forget the previous focus"
            );

            let second = popup.frame(input(popup_id(), false, vec![]));
            assert_eq!(second.builder.visible, None);
            assert_eq!(second.builder.active, Some(false));
            assert_eq!(
                second.commands(),
                vec![ViewportCommand::Visible(true), ViewportCommand::Focus]
            );

            for _ in 0..2 {
                let later = popup.frame(input(popup_id(), true, vec![]));
                assert_eq!(later.builder.visible, None);
                assert_eq!(later.builder.active, Some(false));
                later.assert_no_reveal();
            }

            let mut close = input(popup_id(), true, vec![]);
            close
                .viewports
                .get_mut(&popup_id())
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            popup.frame(close).assert_no_reveal();
            assert!(!is_open(&popup.ctx));
            let closed = popup.ctx.run(input_for_root(), render);
            assert_root_untouched(&closed);
            assert!(
                popup.rendered.borrow().is_none(),
                "closed popup must stop rendering"
            );
        }
    });
}

fn pointer(pos: egui::Pos2, button: PointerButton, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

#[test]
fn popup_header_starts_one_native_drag_only_for_primary_press() {
    native_test(|| {
        let popup = NativePopup::new();
        popup.ready();
        let header = egui::pos2(100.0, 26.0);
        for button in [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
        ] {
            let pressed = popup.frame(input(popup_id(), true, pointer(header, button, true)));
            let drags = pressed
                .commands()
                .iter()
                .filter(|command| **command == ViewportCommand::StartDrag)
                .count();
            assert_eq!(
                drags,
                usize::from(button == PointerButton::Primary),
                "button: {button:?}"
            );
            for dx in [10.0, 20.0] {
                let held = popup.frame(input(
                    popup_id(),
                    true,
                    vec![Event::PointerMoved(header + egui::vec2(dx, 0.0))],
                ));
                assert!(
                    !held.commands().contains(&ViewportCommand::StartDrag),
                    "holding a button must not restart the drag"
                );
            }
            let released = popup.frame(input(popup_id(), true, pointer(header, button, false)));
            assert!(!released.commands().contains(&ViewportCommand::StartDrag));
            assert!(is_open(&popup.ctx));
        }
    });
}

#[test]
fn popup_header_can_drag_again_when_native_drag_consumes_the_release() {
    native_test(|| {
        let popup = NativePopup::new();
        popup.ready();
        let header = egui::pos2(100.0, 26.0);
        let press = || {
            input(
                popup_id(),
                true,
                pointer(header, PointerButton::Primary, true),
            )
        };
        assert_eq!(
            popup.frame(press()).commands(),
            vec![ViewportCommand::StartDrag]
        );

        // Native dragging can consume the release. PointerGone alone must not
        // prevent the next real press from starting another drag (#1219).
        let gone = popup.frame(input(popup_id(), true, vec![Event::PointerGone]));
        assert!(!gone.commands().contains(&ViewportCommand::StartDrag));
        assert_eq!(
            popup.frame(press()).commands(),
            vec![ViewportCommand::StartDrag]
        );

        for dx in [10.0, 20.0] {
            let held = popup.frame(input(
                popup_id(),
                true,
                vec![Event::PointerMoved(header + egui::vec2(dx, 0.0))],
            ));
            assert!(!held.commands().contains(&ViewportCommand::StartDrag));
        }
    });
}

#[test]
fn popup_close_button_does_not_start_a_header_drag() {
    native_test(|| {
        let popup = NativePopup::new();
        popup.ready();
        // Center of the 28-point close button, outside the header handle.
        let close = egui::pos2(POPUP_W - 30.0, 26.0);
        let pressed = popup.frame(input(
            popup_id(),
            true,
            pointer(close, PointerButton::Primary, true),
        ));
        assert!(!pressed.commands().contains(&ViewportCommand::StartDrag));
        assert!(is_open(&popup.ctx));
        let released = popup.frame(input(
            popup_id(),
            true,
            pointer(close, PointerButton::Primary, false),
        ));
        assert!(!released.commands().contains(&ViewportCommand::StartDrag));
        assert!(!is_open(&popup.ctx), "close hit must close the popup");
    });
}

#[test]
fn embedded_popup_never_reveals_focuses_or_drags_the_root_window() {
    let ctx = egui::Context::default();
    ctx.set_embed_viewports(true);
    open_companion_menu(&ctx, egui::pos2(20.0, 20.0));
    let header = egui::pos2(100.0, 26.0);
    for events in [
        vec![],
        vec![],
        pointer(header, PointerButton::Primary, true),
        vec![Event::PointerMoved(header + egui::vec2(20.0, 0.0))],
        pointer(header, PointerButton::Primary, false),
    ] {
        let output = ctx.run(input(ViewportId::ROOT, true, events), render);
        assert_root_untouched(&output);
        assert!(is_open(&ctx));
    }
}
