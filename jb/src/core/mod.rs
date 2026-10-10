pub mod curl;
pub mod ffmpeg;

use unicode_width::UnicodeWidthStr;

pub fn align_left(s: &str, width: usize) -> String {
    let w = UnicodeWidthStr::width(s);
    let padding = width.saturating_sub(w);
    format!("{}{:<padding$}", s, "", padding = padding)
}

pub fn padding_left(s: &str, count: usize) -> String {
    let mut padding = String::with_capacity(count + 1);
    padding.push('\n');
    padding.extend(std::iter::repeat_n(" ", count));
    s.replace('\n', &padding)
}

#[cfg(unix)]
pub fn terminal_width() -> Option<u16> {
    use rustix::termios::tcgetwinsize;

    let size = tcgetwinsize(std::io::stdout()).ok()?;
    Some(size.ws_col)
}

#[cfg(windows)]
pub fn terminal_width() -> Option<u16> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        CONSOLE_SCREEN_BUFFER_INFO, COORD, GetConsoleScreenBufferInfo, SMALL_RECT,
    };

    let handle =
        unsafe { BorrowedHandle::borrow_raw(GetStdHandle(STD_OUTPUT_HANDLE) as RawHandle) };
    // convert between windows_sys::Win32::Foundation::HANDLE and std::os::windows::raw::HANDLE
    let hand = handle.as_handle().as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;

    if hand == INVALID_HANDLE_VALUE {
        return None;
    }

    let zc = COORD { X: 0, Y: 0 };
    let mut csbi = CONSOLE_SCREEN_BUFFER_INFO {
        dwSize: zc,
        dwCursorPosition: zc,
        wAttributes: 0,
        srWindow: SMALL_RECT {
            Left: 0,
            Top: 0,
            Right: 0,
            Bottom: 0,
        },
        dwMaximumWindowSize: zc,
    };
    if unsafe { GetConsoleScreenBufferInfo(hand, &mut csbi) } == 0 {
        return None;
    }

    Some((csbi.srWindow.Right - csbi.srWindow.Left + 1) as u16)
}
