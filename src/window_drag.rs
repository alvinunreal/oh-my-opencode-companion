fn packed_screen_position(x: i32, y: i32) -> isize {
    ((x as u16 as u32) | ((y as u16 as u32) << 16)) as isize
}

/// Repair the caption-drag message posted by winit 0.30.13.
///
/// # Safety
/// `message` must be null or point to the valid `MSG` supplied by winit's
/// message hook on its event-loop thread.
#[cfg(target_os = "windows")]
pub(crate) unsafe fn dispatch_caption_drag(message: *const std::ffi::c_void) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, HTCAPTION, MSG, WM_NCLBUTTONDOWN,
    };

    let Some(message) = (unsafe { message.cast::<MSG>().as_ref() }) else {
        return false;
    };
    if message.hwnd.is_null()
        || message.message != WM_NCLBUTTONDOWN
        || message.wParam != HTCAPTION as usize
    {
        return false;
    }

    // winit posts a stack POINTS address as lParam, but WM_NCLBUTTONDOWN
    // expects packed signed coordinates. MSG.pt is the screen cursor position
    // at posting; never dereference the original lParam or change its MSG.
    // https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-nclbuttondown
    // https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-msg
    let mut corrected = *message;
    corrected.lParam = packed_screen_position(message.pt.x, message.pt.y);
    // Dispatch directly, then suppress the original. Reposting would send the
    // replacement through this hook again. winit retains its drag bookkeeping.
    unsafe { DispatchMessageW(&corrected) };
    true
}

#[cfg(test)]
mod tests {
    use super::packed_screen_position;

    #[test]
    fn caption_drag_coordinates_preserve_signed_monitor_positions() {
        for (x, y, expected) in [
            (-1920, 100, 0x0064_F880_u32),
            (100, -1080, 0xFBC8_0064),
            (-1920, -1080, 0xFBC8_F880),
            (3840, 240, 0x00F0_0F00),
            (-32768, -32768, 0x8000_8000),
            (32767, 32767, 0x7FFF_7FFF),
            (-32768, 32767, 0x7FFF_8000),
            (32767, -32768, 0x8000_7FFF),
        ] {
            let packed = packed_screen_position(x, y);
            assert_eq!(packed as u32, expected, "screen position ({x}, {y})");
            // Match the signed GET_X_LPARAM / GET_Y_LPARAM interpretation.
            assert_eq!((packed as i16) as i32, x);
            assert_eq!(((packed >> 16) as i16) as i32, y);
        }
    }

    #[cfg(target_os = "windows")]
    mod native {
        use std::ptr::{null, null_mut};

        use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            GetWindowLongPtrW, SetWindowLongPtrW, GWLP_USERDATA, GWLP_WNDPROC, HTCAPTION, HTLEFT,
            HWND_MESSAGE, MSG, WM_LBUTTONDOWN, WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WNDPROC,
        };

        use super::super::dispatch_caption_drag;

        #[derive(Default)]
        struct WindowState {
            previous_proc: WNDPROC,
            received: Vec<(WPARAM, LPARAM)>,
        }

        unsafe extern "system" fn record_caption(
            hwnd: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            let state = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
            if state.is_null() {
                return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            }
            if message == WM_NCLBUTTONDOWN {
                // Consume the message so a headless test never enters the
                // system's interactive drag loop. Do not panic across FFI.
                unsafe { (*state).received.push((wparam, lparam)) };
                return 0;
            }
            let previous_proc = unsafe { (*state).previous_proc };
            unsafe { CallWindowProcW(previous_proc, hwnd, message, wparam, lparam) }
        }

        struct TestWindow {
            hwnd: HWND,
            // Own this allocation as a raw pointer while the native callback
            // can access it; moving TestWindow must not move or reborrow a Box.
            state: *mut WindowState,
        }

        impl TestWindow {
            fn new() -> Self {
                // The predefined STATIC class needs no registration. A
                // message-only window is invisible and needs no display.
                let hwnd = unsafe {
                    CreateWindowExW(
                        0,
                        windows_sys::core::w!("STATIC"),
                        null(),
                        0,
                        0,
                        0,
                        1,
                        1,
                        HWND_MESSAGE,
                        null_mut(),
                        null_mut(),
                        null(),
                    )
                };
                assert!(!hwnd.is_null(), "create message-only test window");
                let window = Self {
                    hwnd,
                    state: Box::into_raw(Box::default()),
                };
                let state = window.state as isize;
                unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, state) };
                assert_eq!(unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) }, state);
                let previous = unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_WNDPROC, record_caption as *const () as isize)
                };
                assert_ne!(previous, 0, "subclass test window");
                // SetWindowLongPtrW returns the previous native WNDPROC.
                unsafe {
                    (*window.state).previous_proc = std::mem::transmute::<isize, WNDPROC>(previous);
                }
                window
            }

            fn message(&self) -> MSG {
                MSG {
                    hwnd: self.hwnd,
                    message: WM_NCLBUTTONDOWN,
                    wParam: HTCAPTION as usize,
                    // Intentionally not a valid pointer; the hook must use pt.
                    lParam: 1,
                    time: 123,
                    pt: POINT { x: -1920, y: -1080 },
                }
            }
        }

        impl Drop for TestWindow {
            fn drop(&mut self) {
                unsafe {
                    if let Some(previous) = (*self.state).previous_proc {
                        SetWindowLongPtrW(self.hwnd, GWLP_WNDPROC, previous as *const () as isize);
                    }
                    SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                    DestroyWindow(self.hwnd);
                    // No native callback can access the allocation now.
                    // Recreate its sole Box owner only after native teardown.
                    drop(Box::from_raw(self.state));
                }
            }
        }

        #[test]
        fn caption_drag_hook_dispatches_corrected_coordinates_exactly_once() {
            let window = TestWindow::new();
            let message = window.message();
            let handled = unsafe { dispatch_caption_drag((&message as *const MSG).cast()) };
            // This is winit's dispatch contract: a false hook result lets the
            // original message through, which would produce a second delivery.
            if !handled {
                unsafe { DispatchMessageW(&message) };
            }
            assert!(handled);
            // Dispatch is synchronous; no callback is accessing state here.
            assert_eq!(
                unsafe { &(*window.state).received },
                &vec![(HTCAPTION as usize, 0xFBC8_F880_u32 as isize)]
            );
            assert_eq!(message.lParam, 1, "the original MSG must remain untouched");
        }

        #[test]
        fn caption_drag_hook_leaves_other_messages_and_thread_messages_alone() {
            let window = TestWindow::new();
            for (id, hit, hwnd) in [
                (WM_LBUTTONDOWN, HTCAPTION, window.hwnd),
                (WM_NCLBUTTONUP, HTCAPTION, window.hwnd),
                (WM_NCLBUTTONDOWN, HTLEFT, window.hwnd),
                (WM_NCLBUTTONDOWN, HTCAPTION, null_mut()),
            ] {
                let message = MSG {
                    hwnd,
                    message: id,
                    wParam: hit as usize,
                    ..window.message()
                };
                assert!(!unsafe { dispatch_caption_drag((&message as *const MSG).cast()) });
                assert_eq!(message.lParam, 1);
            }
            assert!(!unsafe { dispatch_caption_drag(null()) });
            assert!(
                unsafe { (&(*window.state).received).is_empty() },
                "pass-through messages must not be dispatched by the hook"
            );
        }
    }
}
