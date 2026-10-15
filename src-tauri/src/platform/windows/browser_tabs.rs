//! A browser's tab URLs on Windows, read with UI Automation from each window's address bar. Only the tab in
//! front of each window is visible this way, so "no call tab seen" doesn't mean "no call": the answer is a list
//! of URLs only when one of them is a call, and otherwise Unknown (the Browser rule, Ask, applies). There's no
//! permission to ask for on Windows, so never Denied.

use crate::calls::tabs::{web_call, Family, Tabs};
use std::time::{Duration, Instant};
use ::windows::core::{Interface, BOOL, PWSTR};
use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use ::windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use ::windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use ::windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationValuePattern, TreeScope_Children, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_ValuePatternId,
};
use ::windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible};

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
/// How deep and how many elements the address-bar search looks at (it sits about 6 levels down).
const MAX_DEPTH: usize = 12;
const MAX_NODES: usize = 600;
/// Cap on any single UI Automation call, so a hung browser can't hold the calls loop.
const CALL_TIMEOUT_MS: u32 = 250;

/// An element as the search sees it.
struct Element<N> {
    node: N,
    class: String,
    document: bool,
}

/// Breadth-first search for the address bar under a browser window. Web content (documents and Chromium's render
/// widget host) is never entered, so a page can't slow it down or fake an address bar. Bounded by depth, element
/// count and `out_of_time`; `children` gives `None` for an element that can't be read.
fn find_omnibox<N>(root: N, mut children: impl FnMut(&N) -> Option<Vec<Element<N>>>, out_of_time: impl Fn() -> bool) -> Option<N> {
    let mut level = vec![root];
    let mut seen = 0;
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for node in &level {
            if out_of_time() || seen >= MAX_NODES {
                return None;
            }
            for child in children(node).unwrap_or_default() {
                seen += 1;
                if child.class == OMNIBOX_CLASS {
                    return Some(child.node);
                }
                if !child.document && child.class != "Chrome_RenderWidgetHostHWND" {
                    next.push(child.node);
                }
            }
        }
        if next.is_empty() {
            return None;
        }
        level = next;
    }
    None
}

/// Chromium's top-level browser frames (not tooltips, menus or the hidden message windows).
fn is_browser_frame(class: &str) -> bool {
    class == "Chrome_WidgetWin_1"
}

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

/// Visible top-level browser frames (`Chrome_WidgetWin_1`) whose process is `app`.
fn browser_windows(app: &str) -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut Vec<(HWND, u32)>) };
        let mut class = [0u16; 64];
        let len = unsafe { GetClassNameW(hwnd, &mut class) }.max(0) as usize;
        if unsafe { IsWindowVisible(hwnd) }.as_bool() && is_browser_frame(&String::from_utf16_lossy(&class[..len])) {
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

/// The address bar text of each window, within the time budget. Every UI Automation call is capped at 250 ms
/// (`IUIAutomation2` timeouts), and the budget is checked before each window and each step of the search.
unsafe fn omnibox_urls(windows: &[HWND], started: Instant) -> ::windows::core::Result<Vec<String>> {
    let uia: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
    if let Ok(uia2) = uia.cast::<IUIAutomation2>() {
        let _ = unsafe { uia2.SetConnectionTimeout(CALL_TIMEOUT_MS) };
        let _ = unsafe { uia2.SetTransactionTimeout(CALL_TIMEOUT_MS) };
    }
    let all = unsafe { uia.CreateTrueCondition()? };
    // One round trip per element: its children, with their class name and control type.
    let cache = unsafe { uia.CreateCacheRequest()? };
    unsafe {
        cache.AddProperty(UIA_ClassNamePropertyId)?;
        cache.AddProperty(UIA_ControlTypePropertyId)?;
    }
    let out_of_time = || started.elapsed() > BUDGET;
    let children = |e: &IUIAutomationElement| -> Option<Vec<Element<IUIAutomationElement>>> {
        let list = unsafe { e.FindAllBuildCache(TreeScope_Children, &all, &cache) }.ok()?;
        let n = unsafe { list.Length() }.ok()?;
        Some(
            (0..n)
                .filter_map(|i| unsafe { list.GetElement(i) }.ok())
                .map(|c| Element {
                    class: unsafe { c.CachedClassName() }.map(|b| b.to_string()).unwrap_or_default(),
                    document: unsafe { c.CachedControlType() }.is_ok_and(|t| t == UIA_DocumentControlTypeId),
                    node: c,
                })
                .collect(),
        )
    };
    let mut urls = Vec::new();
    for &hwnd in windows {
        if out_of_time() {
            break;
        }
        let Ok(window) = (unsafe { uia.ElementFromHandle(hwnd) }) else { continue };
        let Some(omnibox) = find_omnibox(window, children, out_of_time) else { continue };
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

    /// A small fake element tree: (id, class, document?, children).
    fn tree() -> Vec<(u32, &'static str, bool, Vec<u32>)> {
        vec![
            (0, "Chrome_WidgetWin_1", false, vec![1, 2]),
            (1, "Chrome_RenderWidgetHostHWND", false, vec![10]), // web content host: never entered
            (10, "", true, vec![11]),
            (11, OMNIBOX_CLASS, false, vec![]), // a page can't fake the address bar
            (2, "BrowserView", false, vec![3, 4]),
            (3, "", true, vec![12]), // a document elsewhere: not entered either
            (12, OMNIBOX_CLASS, false, vec![]),
            (4, "TopContainerView", false, vec![5]),
            (5, "ToolbarView", false, vec![6]),
            (6, "LocationBarView", false, vec![7]),
            (7, OMNIBOX_CLASS, false, vec![]),
        ]
    }

    fn kids(t: &[(u32, &'static str, bool, Vec<u32>)], visited: &std::cell::RefCell<Vec<u32>>) -> impl FnMut(&u32) -> Option<Vec<Element<u32>>> {
        move |n| {
            visited.borrow_mut().push(*n);
            let (_, _, _, children) = t.iter().find(|(id, ..)| id == n)?;
            Some(children.iter().map(|c| t.iter().find(|(id, ..)| id == c).unwrap()).map(|(id, class, doc, _)| Element { node: *id, class: class.to_string(), document: *doc }).collect())
        }
    }

    #[test]
    fn the_omnibox_is_found_without_entering_web_content() {
        let t = tree();
        let visited = std::cell::RefCell::new(vec![]);
        assert_eq!(find_omnibox(0, kids(&t, &visited), || false), Some(7));
        let v = visited.borrow();
        assert!(!v.contains(&1) && !v.contains(&10) && !v.contains(&3), "{v:?}");
    }

    #[test]
    fn the_search_stops_at_the_deadline_and_on_unreadable_elements() {
        let t = tree();
        let visited = std::cell::RefCell::new(vec![]);
        assert_eq!(find_omnibox(0, kids(&t, &visited), || true), None);
        assert_eq!(find_omnibox(0u32, |_| None, || false), None);
    }

    #[test]
    fn the_search_is_bounded_in_depth_and_size() {
        // A chain deeper than MAX_DEPTH with the omnibox at the bottom.
        let deep = |n: &u32| Some(vec![Element { node: n + 1, class: if *n as usize == MAX_DEPTH + 5 { OMNIBOX_CLASS.into() } else { String::new() }, document: false }]);
        assert_eq!(find_omnibox(0u32, deep, || false), None);
        // A huge flat toolbar: gives up after MAX_NODES elements.
        let mut calls = 0;
        let wide = |n: &u32| {
            calls += 1;
            Some((0..50).map(|i| Element { node: n * 50 + i + 1, class: String::new(), document: false }).collect())
        };
        assert_eq!(find_omnibox(0u32, wide, || false), None);
        assert!(calls * 50 <= MAX_NODES + 50, "{calls}");
    }

    #[test]
    fn only_browser_frames_are_searched() {
        assert!(is_browser_frame("Chrome_WidgetWin_1"));
        assert!(!is_browser_frame("Chrome_WidgetWin_0"));
        assert!(!is_browser_frame("Chrome_RenderWidgetHostHWND"));
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

        // The heavy page shows the search never walks web content (its timing should match example.com).
        for url in ["https://meet.google.com/abc-defg-hij", "https://example.com", "https://en.wikipedia.org/wiki/United_States"] {
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
