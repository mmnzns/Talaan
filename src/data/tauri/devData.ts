import { seedFolders, seedMeetings } from "../mock/seed";
import { call } from "./invoke";

/** Dev builds only: writes the M1 sample meetings into the real library so the click-through works on disk. */
export async function loadSampleData(): Promise<void> {
  await call("dev_seed", { meetings: seedMeetings(new Date()), folders: seedFolders });
}

/** Dev builds only: removes every meeting and folder (settings stay). */
export async function wipeLibrary(): Promise<void> {
  await call("dev_wipe");
}
