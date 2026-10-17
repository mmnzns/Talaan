import { call } from "./tauri/invoke";

/** Check setup › Copy diagnostics and Open log folder. Rust builds the report (`commands/diagnostics.rs`). */
export interface DiagnosticsClient {
  /** The plain-text report: version, system, these Check setup lines, settings without personal values, the log. */
  report(setup: string[]): Promise<string>;
  /** Shows the log file in the file manager. */
  revealLog(): Promise<void>;
}

export class TauriDiagnosticsClient implements DiagnosticsClient {
  report(setup: string[]): Promise<string> {
    return call<string>("diagnostics_report", { setup });
  }

  revealLog(): Promise<void> {
    return call<void>("logs_reveal");
  }
}

/** The browser preview: a sample report, and no folder to open. */
export class MockDiagnosticsClient implements DiagnosticsClient {
  async report(setup: string[]): Promise<string> {
    return ["Talaan diagnostics (sample data)", "", "== Check setup ==", ...setup].join("\n");
  }

  async revealLog(): Promise<void> {}
}
