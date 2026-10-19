/// Returns the primary screen's logical size.
pub fn primary_size() -> [f32; 2] {
    platform_size().unwrap_or([1440.0, 900.0])
}

#[cfg(target_os = "macos")]
fn platform_size() -> Option<[f32; 2]> {
    let out = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"Finder\" to get bounds of window of desktop",
        ])
        .output()
        .ok()?;
    let s = String::from_utf8(out.stdout).ok()?;
    let ns: Vec<f32> = s
        .trim()
        .split(", ")
        .filter_map(|p| p.parse().ok())
        .collect();
    (ns.len() == 4).then(|| [ns[2], ns[3]])
}

#[cfg(not(target_os = "macos"))]
fn platform_size() -> Option<[f32; 2]> {
    None
}

#[cfg(target_os = "windows")]
pub fn monitor_rects_physical() -> Option<Vec<[f32; 4]>> {
    enumerate_monitor_rects(false)
}

/// Usable desktop bounds, excluding taskbars and other reserved monitor edges.
#[cfg(target_os = "windows")]
pub fn monitor_work_areas_physical() -> Option<Vec<[f32; 4]>> {
    enumerate_monitor_rects(true)
}

#[cfg(target_os = "windows")]
fn enumerate_monitor_rects(work_areas: bool) -> Option<Vec<[f32; 4]>> {
    use windows_sys::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };

    struct MonitorRects {
        rects: Vec<[f32; 4]>,
        work_areas: bool,
    }

    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _hdc: HDC,
        rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        if rect.is_null() {
            return 0;
        }
        // EnumDisplayMonitors invokes this synchronously while the collection lives.
        let monitors = unsafe { &mut *(data as *mut MonitorRects) };
        let mut rect = unsafe { *rect };
        if monitors.work_areas {
            // MONITORINFO contains only integer fields and RECTs; zero is valid
            // initialization, with cbSize required by GetMonitorInfoW.
            let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
                // A display may disappear during enumeration. Keep other displays.
                return 1;
            }
            rect = info.rcWork;
        }
        monitors.rects.push([
            rect.left as f32,
            rect.top as f32,
            rect.right as f32,
            rect.bottom as f32,
        ]);
        1
    }

    let mut monitors = MonitorRects {
        rects: Vec::new(),
        work_areas,
    };
    let ok = unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(collect),
            (&mut monitors as *mut MonitorRects) as LPARAM,
        )
    };
    (ok != 0 && !monitors.rects.is_empty()).then_some(monitors.rects)
}
