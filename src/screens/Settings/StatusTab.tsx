import type { MessageKey } from "../../lib/i18n";
import { useT } from "../../lib/i18n/I18nProvider";
import { onWindows } from "../../lib/platform";
import { Switch } from "../../ui";
import { Group, Row } from "./parts";
import { SetupCheck } from "./SetupCheck";
import type { TabProps } from "./tabProps";

/** A Mac has a graphics chip; a Windows PC that Talaan can speed up has a graphics card. */
export const gpuTitle = (windows: boolean): MessageKey => (windows ? "settings.gpu_title" : "settings.gpu_title_mac");

/** Settings › Status, the first tab: everything a call needs (Check setup), the graphics card switch beside the
 *  Graphics chip check (on Windows it speeds up speech and summaries, so it belongs to neither of their tabs), then
 *  Diagnostics for a bug report. */
export function StatusTab({ settings: s, update }: TabProps) {
  const t = useT();
  const title = t(gpuTitle(onWindows));
  return (
    <div className="flex flex-col gap-6">
      <SetupCheck gpuKey={s.useGpu}>
        <Group>
          <Row title={title} desc={t(onWindows ? "settings.gpu_desc_windows" : "settings.gpu_desc")}>
            <Switch aria-label={title} checked={s.useGpu} onChange={(v) => update({ useGpu: v })} />
          </Row>
        </Group>
      </SetupCheck>
    </div>
  );
}
