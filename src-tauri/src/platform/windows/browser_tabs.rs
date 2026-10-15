//! A browser's tab URLs on Windows, read with UI Automation from each window's address bar. Only the tab in
//! front of each window is visible this way, so "no call tab seen" doesn't mean "no call": the answer is a list
//! of URLs only when one of them is a call, and otherwise Unknown (the Browser rule, Ask, applies). There's no
//! permission to ask for on Windows, so never Denied.

use crate::calls::tabs::{web_call, Family, Tabs};
use std::time::{Duration, Instant};
use ::windows::core::{BOOL, BSTR, PWSTR};
use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use ::windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use ::windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use ::windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR};
use ::windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants, UIA_ClassNamePropertyId, UIA_ValuePatternId};
use ::windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};

/// The answer for the URLs seen: they count only when one is a call (see the module doc).
pub fn windows_result(urls: Vec<String>) -> Tabs {
    if urls.iter().any(|u| web_call(u).is_some()) { Tabs::Urls(urls) } else { Tabs::Unknown }
}

/// Whether a process image belongs to the browser `app` names: an exe file name (`msedge.exe`), or a packaged
/// app's family-name prefix (`TheBrowserCompany.Arc_`), which is a folder in the image path.
pub fn is_app(image_path: &str, app: &str) -> bool {
    if app.ends_with('_') {
        return image_path.split(['\\', '/']).any(|dir| dir.len() > app.len() && dir.get(..app.len()).is_some_and(|p| p.eq_ignore_ascii_case(app)));
    }
    image_path.rsplit(['\\', '/']).next().is_some_and(|exe| !exe.is_empty() && exe.eq_ignore_ascii_case(app))
}

/// Stop looking after this long: UI Automation can be slow on a busy browser.
const BUDGET: Duration = Duration::from_millis(300);
/// Chromium's address bar (a native view; the class name doesn't change with the UI language).
const OMNIBOX_CLASS: &str = "OmniboxViewViews";

/// The address of the tab in front of each of `app`'s windows; see the module doc for what the answer means.
pub fn tab_urls(app: &str, family: Family) -> Tabs {
    if family != Family::Chromium {
        return Tabs::Unknown;
    }
    let started = Instant::now();
    super::com::init();
    let windows = browser_windows(app);
    if windows.is_empty() {
        return Tabs::Unknown;
    }
    match unsafe { omnibox_urls(&windows, started) } {
        Ok(urls) => windows_result(urls),
        Err(_) => Tabs::Unknown,
    }
}

/// Visible top-level windows whose process is `app`.
fn browser_windows(app: &str) -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<(HWND, u32)>) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            let mut pid = 0u32;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            if pid != 0 {
                found.push((hwnd, pid));
            }
        }
        true.into()
    }
    let mut all: Vec<(HWND, u32)> = Vec::new();
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut all as *mut _ as isize)) };
    let mut checked: Vec<(u32, bool)> = Vec::new();
    all.into_iter()
        .filter(|(_, pid)| match checked.iter().find(|(p, _)| p == pid) {
            Some((_, ok)) => *ok,
            None => {
                let ok = image_path(*pid).is_some_and(|p| is_app(&p, app));
                checked.push((*pid, ok));
                ok
            }
        })
        .map(|(hwnd, _)| hwnd)
        .collect()
}

fn image_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(handle);
        ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// The address bar text of each window, within the time budget.
unsafe fn omnibox_urls(windows: &[HWND], started: Instant) -> ::windows::core::Result<Vec<String>> {
    let uia: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)? };
    let mut class = VARIANT::default();
    unsafe {
        (*class.Anonymous.Anonymous).vt = VT_BSTR;
        (*class.Anonymous.Anonymous).Anonymous.bstrVal = std::mem::ManuallyDrop::new(BSTR::from(OMNIBOX_CLASS));
    }
    let condition = unsafe { uia.CreatePropertyCondition(UIA_ClassNamePropertyId, &class) };
    let _ = unsafe { VariantClear(&mut class) };
    let condition = condition?;
    let mut urls = Vec::new();
    for &hwnd in windows {
        if started.elapsed() > BUDGET {
            break;
        }
        let Ok(window) = (unsafe { uia.ElementFromHandle(hwnd) }) else { continue };
        let Ok(omnibox) = (unsafe { window.FindFirst(TreeScope_Descendants, &condition) }) else { continue };
        let Ok(value) = (unsafe { omnibox.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }) else { continue };
        if let Ok(text) = unsafe { value.CurrentValue() } {
            let text = text.to_string().trim().to_string();
            if !text.is_empty() {
                urls.push(text);
            }
        }
    }
    Ok(urls)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(u: &[&str]) -> Vec<String> {
        u.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn urls_count_only_when_one_is_a_call() {
        let seen = urls(&["github.com/talaan", "meet.google.com/abc-defg-hij", "youtube.com/watch?v=1"]);
        assert_eq!(windows_result(seen.clone()), Tabs::Urls(seen));
        assert_eq!(windows_result(urls(&["youtube.com/watch?v=1", "meet.google.com"])), Tabs::Unknown);
        assert_eq!(windows_result(vec![]), Tabs::Unknown);
    }

    #[test]
    fn processes_are_matched_by_exe_name_or_package_folder() {
        assert!(is_app(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe", "msedge.exe"));
        assert!(is_app(r"C:\Program Files\Google\Chrome\Application\CHROME.EXE", "chrome.exe"));
        assert!(!is_app(r"C:\Program Files\Google\Chrome\Application\chrome_proxy.exe", "chrome.exe"));
        assert!(!is_app(r"C:\Tools\notmsedge.exe", "msedge.exe"));
        assert!(is_app(r"C:\Program Files\WindowsApps\TheBrowserCompany.Arc_1.2.3.0_x64__ttt1ap7aakyb4\Arc.exe", "TheBrowserCompany.Arc_"));
        assert!(!is_app(r"C:\Program Files\WindowsApps\Other.App_1.0_x64__abc\Arc.exe", "TheBrowserCompany.Arc_"));
        assert!(!is_app("", "msedge.exe"));
    }

    /// Live check with Edge (always on Windows 11). Opens a NEW Edge window per URL, reads it, and closes only
    /// the windows it opened (found by comparing Edge's windows before and after); the user's other windows are
    /// left alone, and their addresses are never printed. Run with
    /// `cargo test tab_urls_live_smoke -- --ignored --nocapture --test-threads=1`.
    #[test]
    #[ignore]
    fn tab_urls_live_smoke() {
        use ::windows::Win32::Foundation::WPARAM;
        use ::windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        super::super::com::init();
        let open = |url: &str| -> Vec<HWND> {
            let before = browser_windows("msedge.exe");
            std::process::Command::new("cmd").args(["/C", "start", "", "msedge", "--new-window", url]).status().expect("start Edge");
            std::thread::sleep(Duration::from_secs(4));
            browser_windows("msedge.exe").into_iter().filter(|h| !before.contains(h)).collect()
        };
        let close = |windows: &[HWND]| {
            for &h in windows {
                let _ = unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) };
            }
            std::thread::sleep(Duration::from_secs(1));
        };

        for url in ["https://meet.google.com/abc-defg-hij", "https://example.com"] {
            let mine = open(url);
            let seen = unsafe { omnibox_urls(&mine, Instant::now()) };
            let t = Instant::now();
            let result = tab_urls("msedge.exe", Family::Chromium);
            let took = t.elapsed();
            let summary = match &result {
                Tabs::Urls(u) => format!("Urls({} addresses, Meet tab among them: {})", u.len(), u.iter().any(|u| u.contains("meet.google.com/abc-defg-hij"))),
                other => format!("{other:?}"),
            };
            println!("{url}: new windows {}, their address bar {:?}; tab_urls -> {summary} in {took:?}", mine.len(), seen);
            close(&mine);
        }
    }
}
