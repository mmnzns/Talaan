import { describeError } from "../../data/errors";
import { usingRealStorage } from "../../data";
import { loadSampleData, wipeLibrary } from "../../data/tauri/devData";
import { Button, Label, useToast } from "../../ui";

export function DevDataStage() {
  const toast = useToast();
  const run = (label: string, fn: () => Promise<void>) => fn().then(() => toast(label), (e) => toast(describeError(e)));

  return (
    <div className="flex max-w-xl flex-col gap-3">
      <h2 className="m-0 text-lg font-extrabold tracking-tight">Dev data</h2>
      {usingRealStorage ? (
        <>
          <p className="m-0 text-[13px] text-subtle">
            Talaan is reading and writing real files. Load the sample meetings to click through the app on disk, or wipe the library to start empty.
            Settings are kept. Debug builds only.
          </p>
          <div className="flex gap-2">
            <Button variant="primary" onClick={() => run("Sample meetings loaded", loadSampleData)}>
              Load sample meetings
            </Button>
            <Button variant="danger" onClick={() => run("Library wiped", wipeLibrary)}>
              Wipe library
            </Button>
          </div>
        </>
      ) : (
        <p className="m-0 text-[13px] text-subtle">
          You’re in a plain browser, so the sample meetings are always on and nothing is saved to disk. Run <Label>npm run tauri dev</Label> to use real storage.
        </p>
      )}
    </div>
  );
}
