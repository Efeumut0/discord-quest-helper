#![windows_subsystem = "windows"]

pub fn is_turkish() -> bool {
    unsafe {
        let lang_id = windows::Win32::Globalization::GetUserDefaultUILanguage();
        (lang_id & 0x3FF) == 0x1F // LANG_TURKISH
    }
}

pub fn loc<'a>(en: &'a str, tr: &'a str) -> &'a str {
    if is_turkish() {
        tr
    } else {
        en
    }
}

use std::env;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, GetStockObject, SetBkColor, SetBkMode, SetDCBrushColor,
    SetTextColor, ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, DC_BRUSH, DEFAULT_CHARSET,
    DEFAULT_PITCH, FONT_WEIGHT, FW_BOLD, FW_MEDIUM, HDC, HFONT, OUT_DEFAULT_PRECIS, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::{
    FindResourceW, GetModuleHandleW, LoadResource, LockResource, SizeofResource,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrA, KillTimer, LoadCursorW, LoadImageW, PostQuitMessage, RegisterClassW,
    SendMessageA, SetCursor, SetTimer, SetWindowLongPtrA, SetWindowTextW, ShowWindow,
    TranslateMessage, CW_USEDEFAULT, GWL_EXSTYLE, HICON, IDC_ARROW, IMAGE_FLAGS, IMAGE_ICON, MSG,
    STM_SETIMAGE, SW_HIDE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_SETCURSOR, WM_SETFONT, WM_TIMER, WNDCLASSW, WS_CAPTION,
    WS_CHILD, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
};

mod activity;
mod tray;
use tray::create_tray_icon;

const WIDTH: i32 = 460;
const HEIGHT: i32 = 380;
const BACKGROUND: COLORREF = COLORREF(0x001F160F);
const PANEL: COLORREF = COLORREF(0x002D2118);
const TEXT: COLORREF = COLORREF(0x00F7F5F2);
const RT_RCDATA_ID: usize = 10;
const ACTIVITY_TITLE_RESOURCE_ID: usize = 101;
const STATUS_TIMER_ID: usize = 1;

static BACKGROUND_BRUSH: AtomicIsize = AtomicIsize::new(0);
static STATUS_LABEL: AtomicIsize = AtomicIsize::new(0);
static RPC_LABEL: AtomicIsize = AtomicIsize::new(0);
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

#[derive(Debug)]
struct Config {
    title: String,
    start_minimized: bool,
    title_from_args: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            title: "Discord Quest Helper".to_string(),
            start_minimized: false,
            title_from_args: false,
        }
    }
}

fn read_embedded_string(resource_id: usize) -> Option<String> {
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        let resource = FindResourceW(
            Some(module),
            PCWSTR(resource_id as *const u16),
            PCWSTR(RT_RCDATA_ID as *const u16),
        );
        if resource.0.is_null() {
            return None;
        }

        let size = SizeofResource(Some(module), resource);
        if size == 0 {
            return None;
        }

        let resource_data = LoadResource(Some(module), resource).ok()?;
        let ptr = LockResource(resource_data);
        if ptr.is_null() {
            return None;
        }

        let bytes = std::slice::from_raw_parts(ptr as *const u8, size as usize);
        let value = String::from_utf8_lossy(bytes);
        let value = value.trim().trim_start_matches('\u{feff}').trim();

        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    }
}

fn parse_args() -> Config {
    let args: Vec<String> = env::args().collect();
    let mut config = Config::default();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--title" => {
                if i + 1 < args.len() {
                    config.title = args[i + 1].clone();
                    config.title_from_args = true;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--tray" => {
                config.start_minimized = true;
                i += 1;
            }
            _ => i += 1,
        }
    }

    if !config.title_from_args {
        if let Some(title) = read_embedded_string(ACTIVITY_TITLE_RESOURCE_ID) {
            config.title = title;
        }
    }

    config
}

fn create_font(height: i32, weight: FONT_WEIGHT) -> HFONT {
    let font_name = wide_null("Segoe UI");
    unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(font_name.as_ptr()),
        )
    }
}

struct LabelSpec<'a> {
    text: &'a str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    centered: bool,
}

fn update_status_label() {
    let rpc_label = RPC_LABEL.load(Ordering::Relaxed);
    if rpc_label != 0 {
        let rpc_text = wide_null(activity::status_text());
        unsafe {
            let _ = SetWindowTextW(HWND(rpc_label as *mut c_void), PCWSTR(rpc_text.as_ptr()));
        }
    }
    let label = STATUS_LABEL.load(Ordering::Relaxed);
    if label == 0 {
        return;
    }
    let elapsed = STARTED_AT.get_or_init(Instant::now).elapsed().as_secs();
    let status_text = format!(
        "{} {:02}:{:02}",
        loc("Process uptime:", "İşlem süresi:"),
        elapsed / 60,
        elapsed % 60
    );
    let wide_text = wide_null(&status_text);
    unsafe {
        let _ = SetWindowTextW(HWND(label as *mut c_void), PCWSTR(wide_text.as_ptr()));
    }
}

fn set_font(hwnd: HWND, font: HFONT) {
    unsafe {
        let _ = SendMessageA(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
    }
}

fn create_label(parent_hwnd: HWND, instance: HINSTANCE, spec: LabelSpec) -> Option<HWND> {
    unsafe {
        let class_name = wide_null("STATIC");
        let window_text = wide_null(spec.text);
        let mut style = WS_CHILD | WS_VISIBLE;
        if spec.centered {
            style |= WINDOW_STYLE(0x00000001); // SS_CENTER
        }

        let label_hwnd = CreateWindowExW(
            WS_EX_TRANSPARENT,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_text.as_ptr()),
            style,
            spec.x,
            spec.y,
            spec.w,
            spec.h,
            Some(parent_hwnd),
            None,
            Some(instance),
            None,
        );

        match label_hwnd {
            Ok(hwnd) if !hwnd.0.is_null() => Some(hwnd),
            _ => None,
        }
    }
}

#[allow(clippy::manual_dangling_ptr)]
fn make_int_resource(resource_id: u16) -> PCWSTR {
    PCWSTR(resource_id as usize as *const u16)
}

fn load_icon_handle(instance: HINSTANCE, size: i32) -> Option<HANDLE> {
    unsafe {
        LoadImageW(
            Some(instance),
            make_int_resource(1),
            IMAGE_ICON,
            size,
            size,
            IMAGE_FLAGS(0),
        )
        .ok()
    }
}

fn create_icon(parent_hwnd: HWND, instance: HINSTANCE, x: i32, y: i32, size: i32) -> Option<HWND> {
    unsafe {
        let class_name = wide_null("STATIC");
        let window_text = wide_null("");
        let icon_hwnd = CreateWindowExW(
            WS_EX_TRANSPARENT,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x00000003 | 0x00000200), // SS_ICON | SS_CENTERIMAGE
            x,
            y,
            size,
            size,
            Some(parent_hwnd),
            None,
            Some(instance),
            None,
        );

        let hwnd = match icon_hwnd {
            Ok(hwnd) if !hwnd.0.is_null() => hwnd,
            _ => return None,
        };

        if let Some(icon) = load_icon_handle(instance, size) {
            let _ = SendMessageA(
                hwnd,
                STM_SETIMAGE,
                WPARAM(IMAGE_ICON.0 as usize),
                LPARAM(icon.0 as isize),
            );
        }

        Some(hwnd)
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), STATUS_TIMER_ID);
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == STATUS_TIMER_ID {
                update_status_label();
                return LRESULT(0);
            }

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETCURSOR => {
            const HTCLIENT_CODE: isize = 1;

            if (lparam.0 & 0xffff) == HTCLIENT_CODE {
                if let Ok(cursor) = LoadCursorW(None, IDC_ARROW) {
                    let _ = SetCursor(Some(cursor));
                    return LRESULT(1);
                }
            }

            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as *mut _);
            SetTextColor(hdc, TEXT);
            SetBkColor(hdc, BACKGROUND);
            SetBkMode(hdc, TRANSPARENT);
            LRESULT({
                let handle = BACKGROUND_BRUSH.load(Ordering::Relaxed);
                if handle != 0 {
                    handle
                } else {
                    SetDCBrushColor(hdc, PANEL);
                    GetStockObject(DC_BRUSH).0 as isize
                }
            })
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn create_native_window(title: &str) -> Result<(HWND, HINSTANCE), Box<dyn std::error::Error>> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = wide_null("DiscordQuestRunner");
        let window_title = wide_null(&format!("{title} - Discord Quest Helper"));
        let brush = CreateSolidBrush(BACKGROUND);
        BACKGROUND_BRUSH.store(brush.0 as isize, Ordering::Relaxed);

        let icon_handle = load_icon_handle(HINSTANCE(instance.0), 32);

        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(instance.0),
            hIcon: icon_handle.map(|icon| HICON(icon.0)).unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hbrBackground: brush,
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WIDTH,
            HEIGHT,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )?;

        apply_dark_title_bar(hwnd);

        Ok((hwnd, HINSTANCE(instance.0)))
    }
}

fn apply_dark_title_bar(hwnd: HWND) {
    unsafe {
        let enabled: i32 = 1;
        let caption_color: u32 = 0x00202020;
        let text_color: u32 = 0x00FFFFFF;

        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &enabled as *const _ as *const c_void,
            std::mem::size_of_val(&enabled) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &caption_color as *const _ as *const c_void,
            std::mem::size_of_val(&caption_color) as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR,
            &text_color as *const _ as *const c_void,
            std::mem::size_of_val(&text_color) as u32,
        );
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn main() {
    let config = parse_args();
    let _ = STARTED_AT.set(Instant::now());
    activity::start(read_embedded_string(102));

    let tray_menu = tray_icon::menu::Menu::new();
    let quit_i = tray_icon::menu::MenuItem::new("Quit", true, None);
    let show_i = tray_icon::menu::MenuItem::new("Show", true, None);
    let hide_i = tray_icon::menu::MenuItem::new("Minimize to tray", true, None);

    let tray_menu_ready = tray_menu
        .append_items(&[
            &show_i,
            &hide_i,
            &tray_icon::menu::PredefinedMenuItem::separator(),
            &quit_i,
        ])
        .is_ok();

    let tray = if tray_menu_ready {
        create_tray_icon(tray_menu, &config.title)
    } else {
        None
    };
    let (hwnd, instance) = match create_native_window(&config.title) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Failed to create window: {}", e);
            return;
        }
    };

    let eyebrow_font = create_font(15, FW_MEDIUM);
    let title_font = create_font(28, FW_BOLD);
    let status_font = create_font(17, FW_MEDIUM);
    let footer_font = create_font(13, FW_MEDIUM);

    if let Some(label) = create_label(
        hwnd,
        instance,
        LabelSpec {
            text: "Discord Quest Helper",
            x: 0,
            y: 34,
            w: WIDTH,
            h: 24,
            centered: true,
        },
    ) {
        set_font(label, eyebrow_font);
    }

    let _icon = create_icon(hwnd, instance, (WIDTH - 112) / 2, 92, 112);

    if let Some(label) = create_label(
        hwnd,
        instance,
        LabelSpec {
            text: &config.title,
            x: 30,
            y: 220,
            w: WIDTH - 60,
            h: 42,
            centered: true,
        },
    ) {
        set_font(label, title_font);
    }

    if let Some(label) = create_label(
        hwnd,
        instance,
        LabelSpec {
            text: activity::status_text(),
            x: 30,
            y: 266,
            w: WIDTH - 60,
            h: 28,
            centered: true,
        },
    ) {
        RPC_LABEL.store(label.0 as isize, Ordering::Relaxed);
        set_font(label, status_font);
    }

    if let Some(label) = create_label(
        hwnd,
        instance,
        LabelSpec {
            text: "",
            x: 30,
            y: 308,
            w: WIDTH - 60,
            h: 22,
            centered: true,
        },
    ) {
        STATUS_LABEL.store(label.0 as isize, Ordering::Relaxed);
        set_font(label, footer_font);
        update_status_label();
    }

    unsafe {
        let _ = SetTimer(Some(hwnd), STATUS_TIMER_ID, 1000, None);
    }

    unsafe {
        if config.start_minimized && tray.is_some() {
            let ex_style = GetWindowLongPtrA(hwnd, GWL_EXSTYLE);
            let new_ex_style = (ex_style & !(WS_EX_APPWINDOW.0 as isize))
                | WS_EX_TOOLWINDOW.0 as isize
                | WS_EX_TRANSPARENT.0 as isize
                | WS_EX_LAYERED.0 as isize;

            SetWindowLongPtrA(hwnd, GWL_EXSTYLE, new_ex_style);
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }

        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 == 0 || ret.0 == -1 {
                break;
            }

            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);

            while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                if event.id == quit_i.id() {
                    let _ = DestroyWindow(hwnd);
                }

                if event.id == show_i.id() {
                    let ex_style = GetWindowLongPtrA(hwnd, GWL_EXSTYLE);
                    let new_ex_style = (ex_style
                        & !(WS_EX_TOOLWINDOW.0 as isize
                            | WS_EX_TRANSPARENT.0 as isize
                            | WS_EX_LAYERED.0 as isize))
                        | WS_EX_APPWINDOW.0 as isize;

                    SetWindowLongPtrA(hwnd, GWL_EXSTYLE, new_ex_style);
                    let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                }

                if event.id == hide_i.id() && tray.is_some() {
                    let ex_style = GetWindowLongPtrA(hwnd, GWL_EXSTYLE);
                    let new_ex_style = (ex_style & !(WS_EX_APPWINDOW.0 as isize))
                        | WS_EX_TOOLWINDOW.0 as isize
                        | WS_EX_TRANSPARENT.0 as isize
                        | WS_EX_LAYERED.0 as isize;

                    SetWindowLongPtrA(hwnd, GWL_EXSTYLE, new_ex_style);
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
    }
}
