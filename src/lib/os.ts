// Which OS the app is running on, from the webview's user agent (WebView2 says "Windows NT", WKWebView doesn't).
// The one place for OS checks in the frontend; fold into the platform string table when branches merge.
const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
export const onWindows = /windows/i.test(ua);
export const onMac = !onWindows && /mac/i.test(ua);
