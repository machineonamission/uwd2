use std::cell::RefCell;
use std::fs;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, DeleteObject, DrawTextW, FillRect, FrameRect, GetSysColor, GetSysColorBrush,
    InvalidateRect, SetBkMode, SetTextColor, UpdateWindow, COLOR_WINDOW, COLOR_WINDOWTEXT,
    DT_CENTER, DT_SINGLELINE, DT_VCENTER, HBRUSH, HDC, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, GetWindowTextW,
    MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassW, SetWindowTextW, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HMENU, MB_ICONERROR,
    MB_ICONINFORMATION, MB_OK, MESSAGEBOX_STYLE, MSG, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_COMMAND, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_ERASEBKGND, WNDCLASSW, WS_CAPTION,
    WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::startup;

const ID_STARTUP: usize = 1003;
const ID_THEME: usize = 1004;
const ID_REMOVE_NOW: usize = 1005;
const WM_APP_REMOVE_NOW_COMPLETE: u32 = 0x8001;

struct UiState {
    window: HWND,
    status: HWND,
    detail: HWND,
    startup: HWND,
    now_status: HWND,
    now_detail: HWND,
    remove_now: HWND,
    theme: HWND,
    dark_mode: bool,
    dark_brush: HBRUSH,
    startup_enabled: bool,
    operation_running: bool,
}

thread_local! {
    static UI_STATE: RefCell<Option<UiState>> = RefCell::new(None);
}

pub fn run() -> Result<(), String> {
    unsafe {
        let module = GetModuleHandleW(PCWSTR::null())
            .map_err(|err| format!("Could not start the UWD2 window: {err}"))?;
        let instance = HINSTANCE(module.0);
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize),
            lpszClassName: w!("UWD2Window"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err("Could not register the UWD2 window.".into());
        }

        let window = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("UWD2Window"),
            w!("Universal Watermark Disabler 2"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            510,
            510,
            HWND::default(),
            HMENU::default(),
            instance,
            None,
        );
        if window.0 == 0 {
            return Err("Could not create the UWD2 window.".into());
        }

        create_control(
            window,
            instance,
            w!("STATIC"),
            "Universal Watermark Disabler 2",
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            22,
            440,
            34,
        );
        create_control(
            window,
            instance,
            w!("STATIC"),
            "Removes the Insider watermark.",
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            58,
            300,
            24,
        );
        let theme_path = crate::constants::config_dir().join("ui-theme");
        let dark_mode = fs::read_to_string(&theme_path)
            .ok()
            .as_deref()
            .map(str::trim)
            == Some("dark");
        let theme_text = if dark_mode {
            "Use light mode"
        } else {
            "Use dark mode"
        };
        let theme = create_control(
            window,
            instance,
            w!("BUTTON"),
            theme_text,
            owner_draw_button_style(),
            Some(ID_THEME),
            340,
            50,
            135,
            34,
        );
        let startup_state = startup::is_enabled();
        let startup_enabled = startup_state.as_ref().copied().unwrap_or(false);
        let status_text = match &startup_state {
            Ok(true) => "Startup is enabled",
            Ok(false) => "Startup is disabled",
            Err(_) => "Could not read startup setting",
        };
        let now_status = create_control(
            window,
            instance,
            w!("STATIC"),
            "Current session: ready",
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            108,
            440,
            28,
        );
        let now_detail = create_control(
            window,
            instance,
            w!("STATIC"),
            "This changes the current Explorer session; startup is controlled separately.",
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            137,
            440,
            34,
        );
        let remove_now_button = create_control(
            window,
            instance,
            w!("BUTTON"),
            "Remove watermark now",
            owner_draw_button_style(),
            Some(ID_REMOVE_NOW),
            150,
            177,
            210,
            42,
        );
        let startup_error_path = crate::constants::config_dir().join("last-startup-error.txt");
        let last_startup_error = fs::read_to_string(&startup_error_path).ok();
        if last_startup_error.is_some() {
            let _ = fs::remove_file(&startup_error_path);
        }
        let initial_detail = if let Some(err) = &last_startup_error {
            format!("Last automatic start failed: {err}")
        } else if let Err(err) = &startup_state {
            err.clone()
        } else if startup_enabled {
            "UWD2 will apply the watermark change when you sign in.".to_owned()
        } else {
            "UWD2 will not run automatically when you sign in.".to_owned()
        };
        let detail = create_control(
            window,
            instance,
            w!("STATIC"),
            &initial_detail,
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            274,
            440,
            48,
        );
        let startup_text = if startup_enabled {
            "Disable at startup"
        } else {
            "Enable at startup"
        };
        let startup_button = create_control(
            window,
            instance,
            w!("BUTTON"),
            startup_text,
            owner_draw_button_style(),
            Some(ID_STARTUP),
            160,
            334,
            190,
            42,
        );
        let status = create_control(
            window,
            instance,
            w!("STATIC"),
            status_text,
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            244,
            440,
            28,
        );
        create_control(
            window,
            instance,
            w!("STATIC"),
            "Startup applies at sign-in. Remove now applies to this session only.",
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            398,
            450,
            28,
        );
        create_control(
            window,
            instance,
            w!("STATIC"),
            "For Insider build watermarks only. It does not remove the Activate Windows notice.",
            WS_CHILD | WS_VISIBLE,
            None,
            28,
            428,
            450,
            38,
        );

        let dark_brush = CreateSolidBrush(COLORREF(rgb(24, 27, 33)));
        UI_STATE.with(|state| {
            *state.borrow_mut() = Some(UiState {
                window,
                status,
                detail,
                startup: startup_button,
                now_status,
                now_detail,
                remove_now: remove_now_button,
                theme,
                dark_mode,
                dark_brush,
                startup_enabled,
                operation_running: false,
            })
        });

        set_titlebar_theme(window, dark_mode);
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);

        let mut message = MSG::default();
        while GetMessageW(&mut message, HWND::default(), 0, 0).0 > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        let _ = DeleteObject(dark_brush);
    }
    Ok(())
}

fn owner_draw_button_style() -> WINDOW_STYLE {
    WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(0x0000000B)
}

pub fn show_message(title: &str, message: &str, is_error: bool) {
    let title = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let message = message
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let icon = if is_error {
        MB_ICONERROR
    } else {
        MB_ICONINFORMATION
    };
    unsafe {
        let _ = MessageBoxW(
            HWND::default(),
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MESSAGEBOX_STYLE(MB_OK.0 | icon.0),
        );
    }
}

unsafe fn create_control(
    parent: HWND,
    instance: HINSTANCE,
    class: PCWSTR,
    text: &str,
    style: WINDOW_STYLE,
    id: Option<usize>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> HWND {
    let text = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let menu = id.map(|id| HMENU(id as isize)).unwrap_or_default();
    let control = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        PCWSTR(text.as_ptr()),
        style,
        x,
        y,
        width,
        height,
        parent,
        menu,
        instance,
        None,
    );
    assert!(control.0 != 0, "could not create a UWD2 control");
    control
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_COMMAND => {
            match wparam.0 & 0xffff {
                ID_STARTUP => toggle_startup(),
                ID_THEME => toggle_theme(),
                ID_REMOVE_NOW => remove_now(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_APP_REMOVE_NOW_COMPLETE => {
            complete_remove_now(lparam);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let hdc = HDC(wparam.0 as isize);
            UI_STATE.with(|ui_state| {
                let ui_state = ui_state.borrow();
                let Some(state) = ui_state.as_ref() else {
                    return;
                };
                let mut rect = RECT::default();
                if GetClientRect(window, &mut rect).is_ok() {
                    let _ = FillRect(hdc, &rect, background_brush(state));
                }
            });
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as isize);
            UI_STATE.with(|ui_state| {
                let ui_state = ui_state.borrow();
                let Some(state) = ui_state.as_ref() else {
                    return LRESULT(0);
                };
                unsafe {
                    let text_color = if state.dark_mode {
                        COLORREF(rgb(237, 241, 247))
                    } else {
                        COLORREF(GetSysColor(COLOR_WINDOWTEXT))
                    };
                    let _ = SetTextColor(hdc, text_color);
                    let _ = SetBkMode(hdc, TRANSPARENT);
                }
                LRESULT(background_brush(state).0)
            })
        }
        WM_DRAWITEM => {
            if draw_button(lparam) {
                LRESULT(1)
            } else {
                DefWindowProcW(window, message, wparam, lparam)
            }
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

unsafe fn draw_button(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    let item = &*(lparam.0 as *const DRAWITEMSTRUCT);
    UI_STATE.with(|ui_state| {
        let ui_state = ui_state.borrow();
        let Some(state) = ui_state.as_ref() else {
            return false;
        };
        if ![ID_STARTUP, ID_THEME, ID_REMOVE_NOW].contains(&(item.CtlID as usize)) {
            return false;
        }

        let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
        let focused = item.itemState.0 & ODS_FOCUS.0 != 0;
        let (fill, border, text) = if state.dark_mode {
            (
                if selected {
                    rgb(58, 69, 84)
                } else {
                    rgb(43, 50, 61)
                },
                rgb(79, 91, 108),
                rgb(237, 241, 247),
            )
        } else {
            (
                if selected {
                    rgb(218, 228, 240)
                } else {
                    rgb(239, 243, 248)
                },
                rgb(188, 198, 210),
                rgb(32, 38, 46),
            )
        };
        unsafe {
            let fill_brush = CreateSolidBrush(COLORREF(fill));
            let border_brush =
                CreateSolidBrush(COLORREF(if focused { rgb(65, 132, 220) } else { border }));
            let _ = FillRect(item.hDC, &item.rcItem, fill_brush);
            let _ = FrameRect(item.hDC, &item.rcItem, border_brush);
            let _ = DeleteObject(fill_brush);
            let _ = DeleteObject(border_brush);
            let _ = SetBkMode(item.hDC, TRANSPARENT);
            let _ = SetTextColor(item.hDC, COLORREF(text));

            let mut text_buffer = [0u16; 128];
            let text_len = GetWindowTextW(item.hwndItem, &mut text_buffer) as usize;
            let text_len = text_len.min(text_buffer.len());
            let mut text_rect = item.rcItem;
            let _ = DrawTextW(
                item.hDC,
                &mut text_buffer[..text_len],
                &mut text_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        true
    })
}

fn background_brush(state: &UiState) -> HBRUSH {
    if state.dark_mode {
        state.dark_brush
    } else {
        unsafe { GetSysColorBrush(COLOR_WINDOW) }
    }
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

unsafe fn set_titlebar_theme(window: HWND, dark_mode: bool) {
    let value = BOOL(if dark_mode { 1 } else { 0 });
    let _ = DwmSetWindowAttribute(
        window,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &value as *const _ as *const _,
        std::mem::size_of::<BOOL>() as u32,
    );
}

unsafe fn toggle_theme() {
    let outcome = UI_STATE.with(|ui_state| {
        let mut ui_state = ui_state.borrow_mut();
        let state = ui_state.as_mut()?;
        let dark_mode = !state.dark_mode;
        let theme_path = crate::constants::config_dir().join("ui-theme");
        let theme_value = if dark_mode { "dark" } else { "light" };
        let save_result = (|| -> std::io::Result<()> {
            if let Some(parent) = theme_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(theme_path, theme_value)
        })();
        if let Err(err) = save_result {
            return Some(Err((state.detail, err.to_string())));
        }

        state.dark_mode = dark_mode;
        Some(Ok((
            state.window,
            state.theme,
            dark_mode,
            [
                state.status,
                state.detail,
                state.startup,
                state.now_status,
                state.now_detail,
                state.remove_now,
                state.theme,
            ],
        )))
    });

    if let Some(outcome) = outcome {
        match outcome {
            Err((detail, err)) => {
                set_text(
                    detail,
                    &format!("Could not save the theme preference: {err}"),
                );
            }
            Ok((window, theme, dark_mode, controls)) => {
                set_text(
                    theme,
                    if dark_mode {
                        "Use light mode"
                    } else {
                        "Use dark mode"
                    },
                );
                set_titlebar_theme(window, dark_mode);
                let _ = InvalidateRect(window, None, BOOL(1));
                for control in controls {
                    let _ = InvalidateRect(control, None, BOOL(1));
                }
            }
        }
    }
}

unsafe fn toggle_startup() {
    let enabled = UI_STATE.with(|ui_state| {
        ui_state
            .borrow()
            .as_ref()
            .map(|state| !state.startup_enabled)
    });
    let Some(enabled) = enabled else {
        return;
    };

    let result = startup::set_enabled(enabled);
    let update = UI_STATE.with(|ui_state| {
        let mut ui_state = ui_state.borrow_mut();
        let state = ui_state.as_mut()?;
        if result.is_ok() {
            state.startup_enabled = enabled;
        }
        let (status_text, detail_text) = match &result {
            Ok(()) if enabled => (
                "Startup is enabled".to_owned(),
                "UWD2 will apply the change the next time you sign in. Explorer is unchanged now."
                    .to_owned(),
            ),
            Ok(()) => (
                "Startup is disabled".to_owned(),
                "UWD2 will not run automatically at sign-in. Explorer is unchanged now.".to_owned(),
            ),
            Err(err) => ("Could not update startup setting".to_owned(), err.clone()),
        };
        let button_text = if enabled {
            "Disable at startup"
        } else {
            "Enable at startup"
        };
        Some((
            state.status,
            state.detail,
            state.startup,
            status_text,
            detail_text,
            button_text,
        ))
    });

    if let Some((status, detail, button, status_text, detail_text, button_text)) = update {
        set_text(status, &status_text);
        if result.is_ok() {
            set_text(button, button_text);
        }
        set_text(detail, &detail_text);
    }
}

unsafe fn remove_now() {
    let pending = UI_STATE.with(|ui_state| {
        let mut ui_state = ui_state.borrow_mut();
        let state = ui_state.as_mut()?;
        if state.operation_running {
            return None;
        }
        state.operation_running = true;
        Some((
            state.window.0,
            state.remove_now,
            state.now_status,
            state.now_detail,
        ))
    });
    let Some((window, button, status, detail)) = pending else {
        return;
    };

    let _ = EnableWindow(button, BOOL(0));
    set_text(button, "Working…");
    set_text(status, "Removing watermark…");
    set_text(
        detail,
        "Working on the current Explorer session. First use may take a moment to download symbols.",
    );

    std::thread::spawn(move || {
        let result = crate::run_operation(true);
        let payload = Box::into_raw(Box::new(result)) as isize;
        if PostMessageW(
            HWND(window),
            WM_APP_REMOVE_NOW_COMPLETE,
            WPARAM::default(),
            LPARAM(payload),
        )
        .is_err()
        {
            drop(Box::from_raw(payload as *mut Result<bool, String>));
        }
    });
}

unsafe fn complete_remove_now(lparam: LPARAM) {
    if lparam.0 == 0 {
        return;
    }
    let result = *Box::from_raw(lparam.0 as *mut Result<bool, String>);
    let update = UI_STATE.with(|ui_state| {
        let mut ui_state = ui_state.borrow_mut();
        let state = ui_state.as_mut()?;
        state.operation_running = false;
        Some((state.remove_now, state.now_status, state.now_detail))
    });
    let Some((button, status, detail)) = update else {
        return;
    };
    let _ = EnableWindow(button, BOOL(1));
    set_text(button, "Remove watermark now");
    match result {
        Ok(true) => {
            set_text(status, "Watermark hidden for this session");
            set_text(
                detail,
                "The change applies now and lasts until Explorer restarts.",
            );
        }
        Ok(false) => {
            set_text(status, "Watermark was not changed");
            set_text(
                detail,
                "UWD2 reported that the current session is unchanged.",
            );
        }
        Err(err) => {
            set_text(status, "Could not remove the watermark");
            set_text(detail, &err);
        }
    }
}

unsafe fn set_text(window: HWND, text: &str) {
    let text = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let _ = SetWindowTextW(window, PCWSTR(text.as_ptr()));
}
