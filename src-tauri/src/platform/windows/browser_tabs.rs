//! A browser's tabs on Windows, read with UI Automation from each window's own views: the address bar (the tab
//! in front) and the tab strip (every tab's title, so a call in a tab that isn't in front is seen too). The answer
//! is the URLs when one of them is a call, else the titles when one of them is a call (`title_call`), and
//! otherwise Unknown (the Browser rule, Ask, applies): "no call tab seen" never means "no call". It's only asked
//! about a browser that is using the mic. There's no permission to ask for on Windows, so never Denied.

use crate::calls::tabs::{title_call, web_call, Family, Tabs};
use std::time::{Duration, Instant};
use ::windows::core::{Interface, BOOL, PWSTR};
use ::windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use ::windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use ::windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use ::windows::Win32::UI::Accessibility::{
    CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationValuePattern, TreeScope_Children, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_NamePropertyId, UIA_TabItemControlTypeId, UIA_ValuePatternId,
};
use ::windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible};

/// The answer for the addresses and tab titles seen: they count only when one is a call (see the module doc).
pub fn windows_result(urls: Vec<String>, titles: Vec<String>) -> Tabs {
    if urls.iter().any(|u| web_call(u).is_some()) {
        Tabs::Urls(urls)
    } else if titles.iter().any(|t| title_call(t).is_some()) {
        Tabs::Titles(titles)
    } else {
        Tabs::Unknown
    }
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
/// How deep and how many elements the search looks at (the address bar and the tabs sit 8 to 10 levels down; an
/// Edge window has about 250 elements outside web content, and each tab adds one).
const MAX_DEPTH: usize = 12;
const MAX_NODES: usize = 1000;
/// A window that shows neither an address bar nor a tab after this many elements is no browser window (an
/// installed web app, DevTools, a picture-in-picture or a popup): give up on it early.
const NOTHING_AFTER: usize = 300;
/// The most one window may take of the budget, so a slow window leaves time for the others.
const PER_WINDOW: Duration = Duration::from_millis(150);
/// Cap on any single UI Automation call, so a hung browser can't hold the calls loop.
const CALL_TIMEOUT_MS: u32 = 120;
/// Connecting to a browser's UI Automation provider the first time can take longer than one call.
const CONNECT_TIMEOUT_MS: u32 = 150;

/// An element as the search sees it.
struct Element<N> {
    node: N,
    class: String,
    /// A tab's accessible name: its title, plus Chromium's notes (" - Memory usage - 22.4 MB"). Empty otherwise.
    name: String,
    document: bool,
    tab_item: bool,
}

/// What the search found under one window: the address bar, and the title of every tab in the tab strip.
struct Found<N> {
    omnibox: Option<N>,
    titles: Vec<String>,
}

/// Breadth-first walk of a browser window's own views for the address bar and the tabs. Web content (documents
/// and Chromium's render widget host) is never entered, so a page can't slow it down or fake an address bar or a
/// tab; the address bar and the tabs aren't entered either (their children are buttons). It stops after the level
/// where it has both the address bar and some tabs, or when bounded by depth,
/// element count and `out_of_time`, keeping what it found so far; `children` gives `None` for an element that
/// can't be read.
fn scan<N>(root: N, mut children: impl FnMut(&N) -> Option<Vec<Element<N>>>, out_of_time: impl Fn() -> bool) -> Found<N> {
    let mut found = Found { omnibox: None, titles: Vec::new() };
    let mut level = vec![root];
    let mut seen = 0;
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for node in &level {
            let nothing_yet = found.omnibox.is_none() && found.titles.is_empty();
            if out_of_time() || seen >= MAX_NODES || (nothing_yet && seen >= NOTHING_AFTER) {
                return found;
            }
            for child in children(node).unwrap_or_default() {
                seen += 1;
                if child.class == OMNIBOX_CLASS {
                    found.omnibox.get_or_insert(child.node);
                } else if child.tab_item {
                    if !child.name.trim().is_empty() {
                        found.titles.push(child.name);
                    }
                } else if !child.document && child.class != "Chrome_RenderWidgetHostHWND" {
                    next.push(child.node);
                }
            }
        }
        // Both found: the tabs are siblings and this level is done, so nothing deeper is needed (it's most of
        // the tree, and every element costs a round trip).
        if next.is_empty() || (found.omnibox.is_some() && !found.titles.is_empty()) {
            break;
        }
        level = next;
    }
    found
}

/// Chromium's top-level browser frames (not tooltips, menus or the hidden message windows).
fn is_browser_frame(class: &str) -> bool {
    class == "Chrome_WidgetWin_1"
}

/// The address of the tab in front of each of `app`'s windows and the titles of all their tabs; see the module
/// doc for what the answer means.
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
    match unsafe { read_windows(&windows, started) } {
        Ok((urls, titles)) => windows_result(urls, titles),
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

/// The address bar text and the tab titles of each window, within the time budget. Every UI Automation call is
/// capped at 250 ms (`IUIAutomation2` timeouts), and the budget is checked before each window and each step of
/// the search.
unsafe fn read_windows(windows: &[HWND], started: Instant) -> ::windows::core::Result<(Vec<String>, Vec<String>)> {
    let uia: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)? };
    if let Ok(uia2) = uia.cast::<IUIAutomation2>() {
        let _ = unsafe { uia2.SetConnectionTimeout(CONNECT_TIMEOUT_MS) };
        let _ = unsafe { uia2.SetTransactionTimeout(CALL_TIMEOUT_MS) };
    }
    let all = unsafe { uia.CreateTrueCondition()? };
    // One round trip per element: its children, with their class name, control type and name.
    let cache = unsafe { uia.CreateCacheRequest()? };
    unsafe {
        cache.AddProperty(UIA_ClassNamePropertyId)?;
        cache.AddProperty(UIA_ControlTypePropertyId)?;
        cache.AddProperty(UIA_NamePropertyId)?;
    }
    let out_of_time = || started.elapsed() > BUDGET;
    let children = |e: &IUIAutomationElement| -> Option<Vec<Element<IUIAutomationElement>>> {
        let list = unsafe { e.FindAllBuildCache(TreeScope_Children, &all, &cache) }.ok()?;
        let n = unsafe { list.Length() }.ok()?;
        Some(
            (0..n)
                .filter_map(|i| unsafe { list.GetElement(i) }.ok())
                .map(|c| {
                    let kind = unsafe { c.CachedControlType() }.ok();
                    let tab_item = kind == Some(UIA_TabItemControlTypeId);
                    Element {
                        class: unsafe { c.CachedClassName() }.map(|b| b.to_string()).unwrap_or_default(),
                        // Only a tab's name is kept; the other names stay in the cache.
                        name: if tab_item { unsafe { c.CachedName() }.map(|b| b.to_string()).unwrap_or_default() } else { String::new() },
                        document: kind == Some(UIA_DocumentControlTypeId),
                        tab_item,
                        node: c,
                    }
                })
                .collect(),
        )
    };
    let (mut urls, mut titles) = (Vec::new(), Vec::new());
    for &hwnd in windows {
        if out_of_time() {
            break;
        }
        let Ok(window) = (unsafe { uia.ElementFromHandle(hwnd) }) else { continue };
        // This window's share: whatever is left of the budget, at most PER_WINDOW.
        let window_started = Instant::now();
        let found = scan(window, children, || out_of_time() || window_started.elapsed() > PER_WINDOW);
        titles.extend(found.titles);
        let Some(omnibox) = found.omnibox else { continue };
        let Ok(value) = (unsafe { omnibox.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }) else { continue };
        if let Ok(text) = unsafe { value.CurrentValue() } {
            let text = text.to_string().trim().to_string();
            if !text.is_empty() {
                urls.push(text);
            }
        }
    }
    Ok((urls, titles))
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
        assert_eq!(windows_result(seen.clone(), vec![]), Tabs::Urls(seen.clone()));
        assert_eq!(windows_result(urls(&["youtube.com/watch?v=1", "meet.google.com"]), vec![]), Tabs::Unknown);
        assert_eq!(windows_result(vec![], vec![]), Tabs::Unknown);
        // The address in front wins over the titles.
        assert_eq!(windows_result(seen.clone(), urls(&["Zoom Meeting"])), Tabs::Urls(seen));
    }

    #[test]
    fn titles_count_only_when_one_is_a_call() {
        let titles = urls(&["Example Domain - Memory usage - 22.4 MB", "Meet - abc-defg-hij"]);
        assert_eq!(windows_result(urls(&["https://example.com/"]), titles.clone()), Tabs::Titles(titles));
        assert_eq!(windows_result(urls(&["https://example.com/"]), urls(&["Google Meet", "Inbox"])), Tabs::Unknown);
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

    /// A small fake element tree: (id, class, name, document?, tab item?, children).
    type Node = (u32, &'static str, &'static str, bool, bool, Vec<u32>);
    fn tree() -> Vec<Node> {
        vec![
            (0, "Chrome_WidgetWin_1", "", false, false, vec![1, 2]),
            (1, "Chrome_RenderWidgetHostHWND", "", false, false, vec![10]), // web content host: never entered
            (10, "", "", true, false, vec![11, 13]),
            (11, OMNIBOX_CLASS, "", false, false, vec![]), // a page can't fake the address bar
            (13, "", "Meet - fake-page-tab", false, true, vec![]), // or a tab
            (2, "BrowserView", "", false, false, vec![3, 4, 20, 30]),
            (3, "", "", true, false, vec![12]), // a document elsewhere: not entered either
            (12, OMNIBOX_CLASS, "", false, false, vec![]),
            (4, "TopContainerView", "", false, false, vec![5]),
            (5, "ToolbarView", "", false, false, vec![6]),
            (6, "LocationBarView", "", false, false, vec![7]),
            (7, OMNIBOX_CLASS, "", false, false, vec![8]),
            (8, "", "", false, false, vec![]), // inside the address bar: not entered
            (20, "EdgeTabStripRegionView", "Tab bar", false, false, vec![21]),
            (21, "EdgeTabStrip", "", false, false, vec![22, 23, 24]),
            (22, "EdgeTab", "Example Domain - Memory usage - 22.4 MB", false, true, vec![25]),
            (23, "EdgeTab", "Meet - abc-defg-hij", false, true, vec![]),
            (24, "EdgeTab", "  ", false, true, vec![]),
            (25, "AlertIndicatorButton", "Mute tab", false, false, vec![]), // inside a tab: not entered
            // A deep branch (bookmarks, side panels): below the level where both were found, never read.
            (30, "SidePanel", "", false, false, vec![31]),
            (31, "", "", false, false, vec![32]),
            (32, "", "", false, false, vec![33]),
            (33, "", "", false, false, vec![34]),
            (34, "", "", false, false, vec![35]),
            (35, "", "", false, false, vec![]),
        ]
    }

    fn kids<'a>(t: &'a [Node], visited: &'a std::cell::RefCell<Vec<u32>>) -> impl FnMut(&u32) -> Option<Vec<Element<u32>>> + 'a {
        move |n| {
            visited.borrow_mut().push(*n);
            let (.., children) = t.iter().find(|(id, ..)| id == n)?;
            Some(
                children
                    .iter()
                    .map(|c| t.iter().find(|(id, ..)| id == c).unwrap())
                    .map(|(id, class, name, doc, tab, _)| Element { node: *id, class: class.to_string(), name: name.to_string(), document: *doc, tab_item: *tab })
                    .collect(),
            )
        }
    }

    fn plain(node: u32, class: &str) -> Element<u32> {
        Element { node, class: class.into(), name: String::new(), document: false, tab_item: false }
    }

    #[test]
    fn the_address_bar_and_tabs_are_found_without_entering_web_content() {
        let t = tree();
        let visited = std::cell::RefCell::new(vec![]);
        let found = scan(0, kids(&t, &visited), || false);
        assert_eq!(found.omnibox, Some(7));
        assert_eq!(found.titles, urls(&["Example Domain - Memory usage - 22.4 MB", "Meet - abc-defg-hij"]));
        let v = visited.borrow();
        assert!(v.contains(&32));
        for never in [1, 10, 3, 7, 22, 23, 33] {
            assert!(!v.contains(&never), "entered {never}: {v:?}");
        }
    }

    #[test]
    fn the_search_stops_at_the_deadline_and_on_unreadable_elements() {
        let t = tree();
        let visited = std::cell::RefCell::new(vec![]);
        let found = scan(0, kids(&t, &visited), || true);
        assert!(found.omnibox.is_none() && found.titles.is_empty());
        let found = scan(0u32, |_| None, || false);
        assert!(found.omnibox.is_none() && found.titles.is_empty());
    }

    #[test]
    fn the_search_keeps_what_it_found_when_time_runs_out() {
        let t = tree();
        let visited = std::cell::RefCell::new(vec![]);
        // Time runs out after a few elements: the tab strip (shallower) is read, the address bar isn't reached.
        let steps = std::cell::Cell::new(0);
        let found = scan(0, kids(&t, &visited), || {
            steps.set(steps.get() + 1);
            steps.get() > 7
        });
        assert_eq!(found.omnibox, None);
        assert_eq!(found.titles.len(), 2);
    }

    #[test]
    fn the_search_is_bounded_in_depth_and_size() {
        // A chain deeper than MAX_DEPTH with the omnibox at the bottom.
        let deep = |n: &u32| Some(vec![plain(n + 1, if *n as usize == MAX_DEPTH + 5 { OMNIBOX_CLASS } else { "" })]);
        assert_eq!(scan(0u32, deep, || false).omnibox, None);
        // A huge flat toolbar: gives up after MAX_NODES elements.
        let mut calls = 0;
        let wide = |n: &u32| {
            calls += 1;
            Some((0..50).map(|i| plain(n * 50 + i + 1, "")).collect())
        };
        assert_eq!(scan(0u32, wide, || false).omnibox, None);
        assert!(calls * 50 <= NOTHING_AFTER + 50, "a window with nothing to find stops early: {calls}");
        // With something found (a tab strip), the walk goes on to MAX_NODES looking for the address bar.
        let mut calls = 0;
        let tabs_then_wide = |n: &u32| {
            calls += 1;
            let tab = *n == 0;
            Some((0..50).map(|i| Element { node: n * 50 + i + 1, class: String::new(), name: if tab && i == 0 { "Meet - abc-defg-hij".into() } else { String::new() }, document: false, tab_item: tab && i == 0 }).collect())
        };
        let found = scan(0u32, tabs_then_wide, || false);
        assert_eq!(found.titles.len(), 1);
        assert!(calls * 50 > NOTHING_AFTER && calls * 50 <= MAX_NODES + 50, "{calls}");
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
            let t = Instant::now();
            let seen = unsafe { read_windows(&mine, Instant::now()) }.map(|(u, titles)| (u, titles.len()));
            let own = t.elapsed();
            let t = Instant::now();
            let result = tab_urls("msedge.exe", Family::Chromium);
            let took = t.elapsed();
            println!("{url}: new windows {}, their address bar and tab count {seen:?} in {own:?}; tab_urls -> {} in {took:?}", mine.len(), summary(&result));
            close(&mine);
        }
    }

    fn summary(result: &Tabs) -> String {
        match result {
            Tabs::Urls(u) => format!("Urls({} addresses, Meet among them: {})", u.len(), u.iter().any(|u| u.contains("meet.google.com/abc-defg-hij"))),
            Tabs::Titles(t) => format!("Titles({} tabs, the test call among them: {})", t.len(), t.iter().any(|t| t.starts_with("Meet - abc-defg-hij"))),
            other => format!("{other:?}"),
        }
    }

    /// Live check of a call tab that isn't in front. Starts a separate Edge with its own new profile in a temp
    /// folder (so no tab ever opens in the user's windows), opens a local page titled like a Meet call, then
    /// example.com in front of it, and expects `Titles`. Closes that Edge and deletes its folder afterwards. Run with
    /// `cargo test background_call_tab_live_smoke -- --ignored --nocapture --test-threads=1`.
    #[test]
    #[ignore]
    fn background_call_tab_live_smoke() {
        use ::windows::Win32::Foundation::WPARAM;
        use ::windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        super::super::com::init();
        let dir = std::env::temp_dir().join(format!("talaan-tab-smoke-{}", std::process::id()));
        let profile = dir.join("profile");
        std::fs::create_dir_all(&profile).unwrap();
        let page = dir.join("call.html");
        std::fs::write(&page, "<!doctype html><title>Meet - abc-defg-hij</title><p>Pretend call</p>").unwrap();
        let profile_arg = format!("--user-data-dir={}", profile.display());
        let edge = |args: &[&str]| {
            std::process::Command::new("cmd").args(["/C", "start", "", "msedge", &profile_arg, "--no-first-run", "--no-default-browser-check"]).args(args).status().expect("start Edge");
        };
        let before = browser_windows("msedge.exe");
        edge(&["--new-window", &format!("file:///{}", page.display().to_string().replace('\\', "/"))]);
        std::thread::sleep(Duration::from_secs(5));
        edge(&["https://example.com"]); // a new tab in front, in this profile's window
        std::thread::sleep(Duration::from_secs(4));
        let mine: Vec<HWND> = browser_windows("msedge.exe").into_iter().filter(|h| !before.contains(h)).collect();

        let mut runs = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            let r = unsafe { read_windows(&mine, Instant::now()) }.map(|(u, titles)| windows_result(u, titles));
            runs.push((t.elapsed(), r.map(|r| summary(&r))));
        }
        let t = Instant::now();
        let all = tab_urls("msedge.exe", Family::Chromium);
        println!("new windows {}; own windows: {runs:?}; tab_urls over every Edge window -> {} in {:?}", mine.len(), summary(&all), t.elapsed());

        for &h in &mine {
            let _ = unsafe { PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        std::thread::sleep(Duration::from_secs(3));
        // Edge may keep that profile's processes alive in the background: stop only the ones using our folder.
        let script = format!("Get-CimInstance Win32_Process -Filter \"name='msedge.exe'\" | Where-Object {{ $_.CommandLine -like '*{}*' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}", profile.display());
        let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).status();
        std::thread::sleep(Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(runs.iter().all(|(took, r)| *took < BUDGET && r.as_ref().is_ok_and(|s| s.starts_with("Titles") && s.ends_with("true)"))), "{runs:?}");
    }
}
