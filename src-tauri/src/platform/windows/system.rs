//! What the log and the diagnostics say about this PC: the Windows version and the processor.

use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

/// "Windows 11 Pro 24H2 (build 26100)". Windows 11 still says "Windows 10" in ProductName, so the build decides.
pub fn os_version() -> String {
    let Ok(k) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") else {
        return "Windows".into();
    };
    let product: String = k.get_value("ProductName").unwrap_or_else(|_| "Windows".into());
    let display: String = k.get_value("DisplayVersion").unwrap_or_default();
    let build: String = k.get_value("CurrentBuild").unwrap_or_default();
    describe(&product, &display, &build)
}

fn describe(product: &str, display: &str, build: &str) -> String {
    let eleven = build.parse::<u32>().is_ok_and(|b| b >= 22000);
    let product = if eleven { product.replacen("Windows 10", "Windows 11", 1) } else { product.to_string() };
    let mut s = product;
    if !display.is_empty() {
        s.push(' ');
        s.push_str(display);
    }
    if !build.is_empty() {
        s.push_str(&format!(" (build {build})"));
    }
    s
}

/// The processor's name, as Windows reports it.
pub fn cpu_name() -> String {
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0")
        .and_then(|k| k.get_value::<String, _>("ProcessorNameString"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_is_named_by_its_build() {
        assert_eq!(describe("Windows 10 Pro", "24H2", "26100"), "Windows 11 Pro 24H2 (build 26100)");
        assert_eq!(describe("Windows 10 Home", "22H2", "19045"), "Windows 10 Home 22H2 (build 19045)");
        assert_eq!(describe("Windows", "", ""), "Windows");
    }

    #[test]
    fn this_pc_answers() {
        assert!(os_version().starts_with("Windows"));
        let _ = cpu_name();
    }
}
