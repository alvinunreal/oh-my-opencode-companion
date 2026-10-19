use std::cell::RefCell;
use std::rc::Rc;

use super::{
    egui, open_companion_menu_with_monitors, render_companion_menu, CompanionMenuAction,
    PresetMenuAction, PresetScope, SessionInfo, MENU_OPEN_KEY, POPUP_H, POPUP_W,
};
use crate::state::CompanionPresetState;
use egui::{Event, PointerButton, ViewportCommand, ViewportId};

const LONG_PRESET_NAME: &str = "Deep research, implementation, and review across multiple projects";

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
        preset: Some(CompanionPresetState {
            current: Some(LONG_PRESET_NAME.into()),
            available: vec!["Balanced".into(), LONG_PRESET_NAME.into(), "Fast".into()],
            effective: Some(LONG_PRESET_NAME.into()),
            project: Some(LONG_PRESET_NAME.into()),
            global: Some("Balanced".into()),
            project_available: vec!["Balanced".into(), LONG_PRESET_NAME.into(), "Fast".into()],
            global_available: vec!["Balanced".into(), "Fast".into()],
            message: None,
            last_request_id: None,
            result_ok: None,
            last_scope: None,
        }),
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

fn observed_input(
    viewport_id: ViewportId,
    position_pixels: [f32; 2],
    size_pixels: [f32; 2],
    native_scale: f32,
    zoom: f32,
) -> egui::RawInput {
    let mut input = input(viewport_id, viewport_id == ViewportId::ROOT, vec![]);
    let pixels_per_point = native_scale * zoom;
    let rect = egui::Rect::from_min_size(
        egui::pos2(position_pixels[0], position_pixels[1]) / pixels_per_point,
        egui::vec2(size_pixels[0], size_pixels[1]) / pixels_per_point,
    );
    let viewport = input.viewports.get_mut(&viewport_id).unwrap();
    viewport.native_pixels_per_point = Some(native_scale);
    viewport.inner_rect = Some(rect);
    viewport.outer_rect = Some(rect);
    input.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, rect.size()));
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
    selected: Option<CompanionMenuAction>,
}

impl NativeFrame {
    fn button_containing(&self, text_bounds: egui::Rect) -> egui::Rect {
        self.child_output
            .shapes
            .iter()
            .filter_map(|clipped| {
                if let egui::epaint::Shape::Rect(rect) = &clipped.shape {
                    if rect.rect.contains_rect(text_bounds) {
                        return Some(rect.rect);
                    }
                }
                None
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()))
            .expect("painted button around text")
    }

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
        let commands = self.commands();
        assert!(
            !commands.iter().any(|command| matches!(
                command,
                ViewportCommand::Visible(true) | ViewportCommand::Focus
            )),
            "revealed before geometry was ready: {commands:?}"
        );
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
        self.frame_with_root(input_for_root(), input)
    }

    fn frame_with_root(
        &self,
        mut root_input: egui::RawInput,
        mut input: egui::RawInput,
    ) -> NativeFrame {
        input.viewports.insert(
            ViewportId::ROOT,
            root_input.viewports[&ViewportId::ROOT].clone(),
        );
        root_input
            .viewports
            .insert(popup_id(), input.viewports[&popup_id()].clone());
        *self.next_input.borrow_mut() = Some(input);
        let target = target();
        let mut selected = None;
        let root_output = self.ctx.run(root_input, |ctx| {
            selected =
                render_companion_menu(ctx, std::slice::from_ref(&target), &[], &target, false);
        });
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
            selected,
        }
    }

    fn ready(&self) {
        open_companion_menu_with_monitors(&self.ctx, egui::pos2(20.0, 20.0), None);
        self.frame(input(popup_id(), false, vec![]));
        self.frame(input(popup_id(), true, vec![]));
    }

    fn open_on_monitors(&self, root: egui::RawInput, zoom: f32, monitors: &[[f32; 4]]) {
        self.ctx.set_zoom_factor(zoom);
        // Apply egui's pending zoom before submitting coordinates expressed
        // at that zoom, as egui-winit does on the next input frame.
        let _ = self.ctx.run(root.clone(), |_| {});
        let output = self.ctx.run(root, |ctx| {
            open_companion_menu_with_monitors(ctx, egui::pos2(20.0, 20.0), Some(monitors));
        });
        assert_root_untouched(&output);
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
fn native_popup_waits_for_window_geometry_before_reveal() {
    native_test(|| {
        let popup = NativePopup::new();
        let opened = popup.ctx.run(input_for_root(), |ctx| {
            open_companion_menu_with_monitors(
                ctx,
                egui::pos2(20.0, 20.0),
                Some(&[[0.0, 0.0, 1920.0, 1040.0]]),
            );
        });
        assert_root_untouched(&opened);

        // Native creation and movement may report their previous rectangle
        // for another frame. Painting that rectangle must not reveal it.
        let mut unsettled = input(popup_id(), false, vec![]);
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(1280.0, 960.0));
        unsettled.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, rect.size()));
        let viewport = unsettled.viewports.get_mut(&popup_id()).unwrap();
        viewport.inner_rect = Some(rect);
        viewport.outer_rect = Some(rect);
        popup.frame(unsettled.clone()).assert_no_reveal();
        let correcting = popup.frame(unsettled);
        correcting.assert_no_reveal();
        assert_eq!(
            correcting.commands(),
            vec![
                ViewportCommand::InnerSize(egui::vec2(352.0, 396.0)),
                ViewportCommand::OuterPosition(egui::pos2(320.0, 220.0)),
            ]
        );
        let ready = popup.frame(observed_input(
            popup_id(),
            [320.0, 220.0],
            [352.0, 396.0],
            1.0,
            1.0,
        ));
        assert!(!ready.child_output.shapes.is_empty());
        assert_eq!(
            ready.commands(),
            vec![ViewportCommand::Visible(true), ViewportCommand::Focus]
        );
    });
}

const THREE_WORK_AREAS: [[f32; 4]; 3] = [
    [-1920.0, -160.0, 0.0, 1040.0],
    [0.0, 0.0, 1920.0, 1040.0],
    [1920.0, -120.0, 4480.0, 1320.0],
];

#[test]
fn native_popup_reopening_hides_the_existing_child_before_moving_it() {
    native_test(|| {
        let popup = NativePopup::new();
        popup.ready();
        let root = observed_input(
            ViewportId::ROOT,
            [-1850.0, 110.0],
            [150.0, 150.0],
            1.25,
            1.0,
        );
        popup.frame_with_root(root.clone(), input(popup_id(), true, vec![]));
        // Reopen without an intervening closed frame: egui reuses the HWND,
        // whose last builder.active=false makes its visible=false patch a no-op.
        open_companion_menu_with_monitors(
            &popup.ctx,
            egui::pos2(20.0, 20.0),
            Some(&THREE_WORK_AREAS),
        );
        // The old HWND can still report focus until the explicit hide runs.
        let reopening = popup.frame_with_root(root.clone(), input(popup_id(), true, vec![]));
        assert!(
            is_open(&popup.ctx),
            "reopening must reset the old focus state"
        );
        assert_eq!(reopening.builder.visible, Some(false));
        assert_eq!(
            reopening.commands(),
            vec![
                ViewportCommand::Visible(false),
                ViewportCommand::OuterPosition(egui::pos2(-1825.0, 135.0)),
            ]
        );
        let ready = popup.frame_with_root(
            root.clone(),
            observed_input(popup_id(), [-1825.0, 135.0], [440.0, 495.0], 1.25, 1.0),
        );
        assert!(
            is_open(&popup.ctx),
            "the hide's focus loss must not dismiss the reopening menu"
        );
        assert_eq!(
            ready.commands(),
            vec![ViewportCommand::Visible(true), ViewportCommand::Focus]
        );

        // Once shown and focused, an ordinary focus loss must still dismiss.
        let mut focused = observed_input(popup_id(), [-1825.0, 135.0], [440.0, 495.0], 1.25, 1.0);
        focused.focused = true;
        focused.viewports.get_mut(&popup_id()).unwrap().focused = Some(true);
        popup.frame_with_root(root.clone(), focused);
        popup.frame_with_root(
            root,
            observed_input(popup_id(), [-1825.0, 135.0], [440.0, 495.0], 1.25, 1.0),
        );
        assert!(
            !is_open(&popup.ctx),
            "a shown popup must dismiss after losing focus"
        );
    });
}

#[test]
fn native_popup_uses_child_scale_and_waits_for_move_resize_and_canvas() {
    native_test(|| {
        let popup = NativePopup::new();
        let root = observed_input(ViewportId::ROOT, [3975.0, 775.0], [300.0, 300.0], 2.0, 1.25);
        popup.open_on_monitors(root.clone(), 1.25, &THREE_WORK_AREAS);

        // Root scale 2x plus 125% zoom freezes the click at (4025,825)
        // desktop pixels. Creation may still report a size calculated for a
        // 1.5x primary monitor while the child observes a 1x native scale.
        let initial = observed_input(popup_id(), [-1500.0, 200.0], [660.0, 743.0], 1.0, 1.25);
        for frame_index in 0..2 {
            let frame = popup.frame_with_root(root.clone(), initial.clone());
            assert_eq!(
                frame.builder.position, None,
                "primary-monitor scaling must not reinterpret the root anchor"
            );
            assert_eq!(frame.builder.min_inner_size, None);
            assert_eq!(frame.builder.active, Some(false));
            frame.assert_no_reveal();
            let mut expected = vec![
                ViewportCommand::InnerSize(egui::vec2(352.0, 396.0)),
                ViewportCommand::OuterPosition(egui::pos2(3220.0, 652.0)),
            ];
            if frame_index == 0 {
                expected.insert(0, ViewportCommand::Visible(false));
            }
            assert_eq!(frame.commands(), expected);
        }

        // The move reaches the right monitor and changes native scale to
        // 2x. Its 880x990 popup now needs a different edge clamp: (3580,310).
        // Moving the root meanwhile must not change the frozen click.
        let moved_root =
            observed_input(ViewportId::ROOT, [100.0, 100.0], [225.0, 225.0], 1.5, 1.25);
        let moved = popup.frame_with_root(
            moved_root.clone(),
            observed_input(popup_id(), [4025.0, 815.0], [660.0, 743.0], 2.0, 1.25),
        );
        moved.assert_no_reveal();
        assert_eq!(
            moved.commands(),
            vec![
                ViewportCommand::InnerSize(egui::vec2(352.0, 396.0)),
                ViewportCommand::OuterPosition(egui::pos2(1432.0, 124.0)),
            ]
        );

        let positioned = popup.frame_with_root(
            moved_root.clone(),
            observed_input(popup_id(), [3580.0, 310.0], [660.0, 743.0], 2.0, 1.25),
        );
        positioned.assert_no_reveal();
        assert_eq!(
            positioned.commands(),
            vec![ViewportCommand::InnerSize(egui::vec2(352.0, 396.0))]
        );

        let settled = observed_input(popup_id(), [3580.0, 310.0], [880.0, 990.0], 2.0, 1.25);
        let mut stale_canvas = settled.clone();
        stale_canvas.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(264.0, 297.2),
        ));
        popup
            .frame_with_root(moved_root.clone(), stale_canvas)
            .assert_no_reveal();
        let revealed = popup.frame_with_root(moved_root.clone(), settled);
        assert!(!revealed.child_output.shapes.is_empty());
        assert_eq!(
            revealed.commands(),
            vec![ViewportCommand::Visible(true), ViewportCommand::Focus]
        );

        // A header drag is free to move the shown popup to another monitor.
        // Neither a child DPI change nor root movement may re-anchor it.
        let dragged = popup.frame_with_root(
            moved_root,
            observed_input(popup_id(), [-1800.0, 50.0], [440.0, 495.0], 1.0, 1.25),
        );
        assert!(
            dragged.commands().is_empty(),
            "dragged popup was repositioned: {:?}",
            dragged.commands()
        );
    });
}

#[test]
fn native_popup_places_on_three_monitors_and_respects_taskbar_work_areas() {
    native_test(|| {
        // Expectations are physical rectangles, calculated independently of
        // the production clamp. Include both halves of the right monitor.
        let cases = [
            ([-1865.0, 85.0], 1.25, [-1840.0, 110.0], [440.0, 495.0]),
            ([-35.0, 995.0], 1.25, [-450.0, 535.0], [440.0, 495.0]),
            ([1870.0, 1010.0], 1.5, [1380.0, 434.0], [528.0, 594.0]),
            ([2760.0, 160.0], 2.0, [2800.0, 200.0], [704.0, 792.0]),
            ([3760.0, 460.0], 2.0, [3760.0, 500.0], [704.0, 792.0]),
            ([4430.0, 1360.0], 2.0, [3760.0, 512.0], [704.0, 792.0]),
        ];
        for (root_position, scale, expected_position, expected_size) in cases {
            let popup = NativePopup::new();
            let root = observed_input(ViewportId::ROOT, root_position, [240.0, 240.0], scale, 1.0);
            popup.open_on_monitors(root.clone(), 1.0, &THREE_WORK_AREAS);
            let first = popup.frame_with_root(
                root.clone(),
                observed_input(popup_id(), [120.0, 80.0], [528.0, 594.0], 1.5, 1.0),
            );
            assert_eq!(first.builder.visible, Some(false));
            assert_eq!(first.builder.position, None);
            first.assert_no_reveal();
            let ready = popup.frame_with_root(
                root,
                observed_input(popup_id(), expected_position, expected_size, scale, 1.0),
            );
            assert_eq!(
                ready.commands(),
                vec![ViewportCommand::Visible(true), ViewportCommand::Focus],
                "root position {root_position:?}"
            );
        }
    });
}

#[test]
fn native_popup_caps_its_size_on_a_small_high_dpi_work_area() {
    native_test(|| {
        let popup = NativePopup::new();
        let root = observed_input(ViewportId::ROOT, [710.0, 510.0], [300.0, 300.0], 2.0, 1.25);
        popup.open_on_monitors(root.clone(), 1.25, &[[0.0, 0.0, 1920.0, 1400.0]]);
        let wide_input = observed_input(popup_id(), [760.0, 390.0], [880.0, 990.0], 2.0, 1.25);
        popup.frame_with_root(root.clone(), wide_input.clone());
        let wide = popup.frame_with_root(root.clone(), wide_input.clone());
        let wide_combo_position = wide
            .child_output
            .shapes
            .iter()
            .find_map(|clipped| {
                if let egui::epaint::Shape::Text(text) = &clipped.shape {
                    if text.galley.text() == LONG_PRESET_NAME {
                        return Some(text.galley.rect.translate(text.pos.to_vec2()).center());
                    }
                }
                None
            })
            .expect("wide preset control");
        // Open and close the catalog at normal width first. Its cached Area
        // must be able to shrink when this same native popup is reopened.
        for _ in 0..2 {
            for pressed in [true, false] {
                let mut click = wide_input.clone();
                click.events = vec![
                    Event::PointerMoved(wide_combo_position),
                    Event::PointerButton {
                        pos: wide_combo_position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ];
                popup.frame_with_root(root.clone(), click);
            }
            popup.frame_with_root(root.clone(), wide_input.clone());
        }
        open_companion_menu_with_monitors(
            &popup.ctx,
            egui::pos2(20.0, 20.0),
            Some(&[[0.0, 0.0, 800.0, 600.0]]),
        );
        // At 2.5 effective pixels/point the normal 880x990 popup cannot fit.
        // Keep 20 physical pixels on every side, yielding 304x224 UI points.
        let first = popup.frame_with_root(
            root.clone(),
            observed_input(popup_id(), [100.0, 100.0], [880.0, 990.0], 2.0, 1.25),
        );
        assert_eq!(first.builder.min_inner_size, None);
        first.assert_no_reveal();
        assert_eq!(
            first.commands(),
            vec![
                ViewportCommand::Visible(false),
                ViewportCommand::InnerSize(egui::vec2(304.0, 224.0)),
                ViewportCommand::OuterPosition(egui::pos2(8.0, 8.0)),
            ]
        );
        let ready_input = observed_input(popup_id(), [20.0, 20.0], [760.0, 560.0], 2.0, 1.25);
        let ready = popup.frame_with_root(root.clone(), ready_input.clone());
        assert_eq!(
            ready.commands(),
            vec![ViewportCommand::Visible(true), ViewportCommand::Focus]
        );

        let mut scroll = ready_input.clone();
        scroll.events = vec![
            Event::PointerMoved(egui::pos2(286.0, 190.0)),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -10000.0),
                modifiers: egui::Modifiers::NONE,
            },
        ];
        popup.frame_with_root(root.clone(), scroll);
        let scrolled = popup.frame_with_root(root.clone(), ready_input.clone());
        let (selected_text, selected_clip) = scrolled
            .child_output
            .shapes
            .iter()
            .find_map(|clipped| {
                if let egui::epaint::Shape::Text(text) = &clipped.shape {
                    if text.galley.text() == LONG_PRESET_NAME {
                        return Some((
                            text.galley.rect.translate(text.pos.to_vec2()),
                            clipped.clip_rect,
                        ));
                    }
                }
                None
            })
            .expect("selected preset must be visible after scrolling");
        let combo_button = scrolled.button_containing(selected_text);
        assert!(
            selected_clip.contains_rect(selected_text),
            "long selected preset is clipped: {selected_text:?}, clip={selected_clip:?}"
        );
        assert!(
            combo_button.left() >= 12.0 && combo_button.right() <= 292.0,
            "preset combo exceeds the capped popup's content width: {combo_button:?}"
        );
        for label in ["Open project folder", "Copy path", "Quit"] {
            let (bounds, clip_rect) = scrolled
                .child_output
                .shapes
                .iter()
                .find_map(|clipped| {
                    if let egui::epaint::Shape::Text(text) = &clipped.shape {
                        if text.galley.text() == label {
                            return Some((
                                text.galley.rect.translate(text.pos.to_vec2()),
                                clipped.clip_rect,
                            ));
                        }
                    }
                    None
                })
                .unwrap_or_else(|| {
                    panic!("footer action {label:?} is not visible after scrolling")
                });
            assert!(
                clip_rect.contains_rect(bounds),
                "footer action {label:?} is clipped: {bounds:?}, clip={clip_rect:?}"
            );
        }

        // The selected text can truncate, but the opened catalog must remain
        // readable and return the complete preset value when clicked.
        let combo_position = combo_button.center();
        for pressed in [true, false] {
            let mut click = ready_input.clone();
            click.events = vec![
                Event::PointerMoved(combo_position),
                Event::PointerButton {
                    pos: combo_position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            let frame = popup.frame_with_root(root.clone(), click);
            assert!(frame.selected.is_none());
        }
        let catalog = popup.frame_with_root(root.clone(), ready_input.clone());
        let (choice_bounds, choice_clip) = catalog
            .child_output
            .shapes
            .iter()
            .rev()
            .find_map(|clipped| {
                if let egui::epaint::Shape::Text(text) = &clipped.shape {
                    if text.galley.text() == LONG_PRESET_NAME && !text.galley.elided {
                        return Some((
                            text.galley.rect.translate(text.pos.to_vec2()),
                            clipped.clip_rect,
                        ));
                    }
                }
                None
            })
            .expect("the opened catalog must show the full long preset name");
        assert!(
            ready_input
                .screen_rect
                .unwrap()
                .contains_rect(choice_bounds),
            "preset choice exceeds the capped canvas: {choice_bounds:?}"
        );
        assert!(
            choice_clip.contains_rect(choice_bounds),
            "preset choice is clipped: {choice_bounds:?}, clip={choice_clip:?}"
        );
        let choice_button = catalog.button_containing(choice_bounds);
        assert!(
            ready_input
                .screen_rect
                .unwrap()
                .contains_rect(choice_button),
            "cached preset row exceeds the capped canvas: {choice_button:?}"
        );
        for pressed in [true, false] {
            let mut click = ready_input.clone();
            click.events = vec![
                Event::PointerMoved(choice_bounds.center()),
                Event::PointerButton {
                    pos: choice_bounds.center(),
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            let frame = popup.frame_with_root(root.clone(), click);
            if pressed {
                assert!(frame.selected.is_none());
            } else {
                assert_eq!(
                    frame.selected,
                    Some(CompanionMenuAction::SelectPreset(PresetMenuAction {
                        scope: PresetScope::Project,
                        preset: Some(LONG_PRESET_NAME.into()),
                        inherit: false,
                    }))
                );
            }
        }

        // The top-right Close remains reachable at the capped width.
        for pressed in [true, false] {
            let mut close = ready_input.clone();
            close.events = vec![
                Event::PointerMoved(egui::pos2(274.0, 26.0)),
                Event::PointerButton {
                    pos: egui::pos2(274.0, 26.0),
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            let frame = popup.frame_with_root(root.clone(), close);
            assert!(!frame.commands().contains(&ViewportCommand::StartDrag));
        }
        assert!(!is_open(&popup.ctx));
    });
}

#[test]
fn native_popup_rounding_and_missing_monitor_data_do_not_prevent_reveal() {
    native_test(|| {
        let popup = NativePopup::new();
        let root = observed_input(ViewportId::ROOT, [3600.0, 100.0], [300.0, 300.0], 2.0, 1.25);
        popup.open_on_monitors(root.clone(), 1.25, &[]);
        let first = popup.frame_with_root(
            root.clone(),
            observed_input(popup_id(), [100.0, 100.0], [440.0, 495.0], 1.0, 1.25),
        );
        assert_eq!(first.builder.position, None);
        assert_eq!(
            first.commands(),
            vec![
                ViewportCommand::Visible(false),
                ViewportCommand::OuterPosition(egui::pos2(2920.0, 120.0))
            ]
        );
        first.assert_no_reveal();
        let ready = popup.frame_with_root(
            root,
            observed_input(popup_id(), [3650.5, 150.5], [879.5, 990.5], 2.0, 1.25),
        );
        assert_eq!(
            ready.commands(),
            vec![ViewportCommand::Visible(true), ViewportCommand::Focus]
        );
    });
}

#[test]
fn native_popup_renders_hidden_then_reveals_once_on_each_open() {
    native_test(|| {
        let popup = NativePopup::new();
        for _ in 0..2 {
            open_companion_menu_with_monitors(&popup.ctx, egui::pos2(20.0, 20.0), None);
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
                assert!(later.commands().is_empty());
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
    open_companion_menu_with_monitors(&ctx, egui::pos2(20.0, 20.0), Some(&THREE_WORK_AREAS));
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
