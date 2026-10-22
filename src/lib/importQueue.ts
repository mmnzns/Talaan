/** Importing several audio files: one after another, each its own meeting, a failure never stops the rest. */

export type ImportOutcome = { ok: true; id: string } | { ok: false; message: string };

export interface ImportFailure {
  name: string;
  message: string;
  /** True when the file was never tried because its type isn't one Talaan imports. */
  unsupported?: boolean;
}

export interface QueueSnapshot {
  running: boolean;
  /** Files accepted in this run (grows when more are dropped while it runs). */
  total: number;
  /** Files finished, imported or failed. */
  finished: number;
  /** Name of the file being imported now. */
  current?: string;
  failures: ImportFailure[];
  /** Meeting ids made so far, in order. */
  imported: string[];
}

export interface BatchSummary {
  total: number;
  imported: string[];
  failures: ImportFailure[];
}

/** The file name from a Windows or Mac path. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

const extensionOf = (name: string) => (name.includes(".") ? name.slice(name.lastIndexOf(".") + 1).toLowerCase() : "");

/**
 * Splits dropped or picked paths into the ones Talaan can import and the ones it can't (named, never silently
 * skipped). The same path twice is imported once.
 */
export function partitionImports(paths: string[], extensions: string[]): { accepted: string[]; unsupported: string[] } {
  const allowed = new Set(extensions.map((e) => e.toLowerCase()));
  const accepted: string[] = [];
  const unsupported: string[] = [];
  const seen = new Set<string>();
  for (const path of paths) {
    if (seen.has(path)) continue;
    seen.add(path);
    (allowed.has(extensionOf(baseName(path))) ? accepted : unsupported).push(path);
  }
  return { accepted, unsupported };
}

interface Options {
  /** Imports one file and says how it went. Must not throw (a throw counts as a failure anyway). */
  importOne: (path: string, name: string) => Promise<ImportOutcome>;
  onChange: (snapshot: QueueSnapshot) => void;
  /** Called once when the queue runs dry. */
  onFinish: (summary: BatchSummary) => void;
}

const empty = (): QueueSnapshot => ({ running: false, total: 0, finished: 0, failures: [], imported: [] });

export class ImportQueue {
  private pending: string[] = [];
  private state = empty();
  private active = false;

  constructor(private readonly opts: Options) {}

  get snapshot(): QueueSnapshot {
    return this.state;
  }

  /** Adds files (and paths of files that can't be imported). Starts the run if it isn't running. */
  add(paths: string[], unsupported: string[] = []) {
    if (paths.length === 0 && unsupported.length === 0) return;
    if (!this.active) this.state = empty();
    this.pending.push(...paths);
    this.state = {
      ...this.state,
      total: this.state.total + paths.length + unsupported.length,
      finished: this.state.finished + unsupported.length,
      failures: [...this.state.failures, ...unsupported.map((p) => ({ name: baseName(p), message: "", unsupported: true }))],
    };
    if (this.active) return this.emit();
    this.active = true;
    void this.run();
  }

  private emit() {
    this.opts.onChange(this.state);
  }

  private async run() {
    this.state = { ...this.state, running: true };
    this.emit();
    for (let path = this.pending.shift(); path !== undefined; path = this.pending.shift()) {
      const name = baseName(path);
      this.state = { ...this.state, current: name };
      this.emit();
      let outcome: ImportOutcome;
      try {
        outcome = await this.opts.importOne(path, name);
      } catch (e) {
        outcome = { ok: false, message: e instanceof Error ? e.message : String(e) };
      }
      this.state = {
        ...this.state,
        finished: this.state.finished + 1,
        imported: outcome.ok ? [...this.state.imported, outcome.id] : this.state.imported,
        failures: outcome.ok ? this.state.failures : [...this.state.failures, { name, message: outcome.message }],
      };
      this.emit();
    }
    const { total, imported, failures } = this.state;
    this.state = { ...this.state, running: false, current: undefined };
    this.active = false;
    this.emit();
    this.opts.onFinish({ total, imported, failures });
  }
}
