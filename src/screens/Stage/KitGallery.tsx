import { useState } from "react";
import { Button, Checkbox, Chip, Field, Label, Radio, SectionHeader, Select, Sheet, SpeakerTag, Switch, Timestamp } from "../../ui";

export function KitGallery() {
  const [sheet, setSheet] = useState(false);
  const [radio, setRadio] = useState("a");
  return (
    <div className="flex flex-col gap-6">
      <SectionHeader title="Buttons" hint="default · raised · primary · strong · danger" />
      <div className="flex flex-wrap items-center gap-3">
        <Button>Default</Button>
        <Button raised>Raised</Button>
        <Button variant="primary">Primary</Button>
        <Button variant="strong">Toggled</Button>
        <Button variant="danger">Discard</Button>
        <Button disabled>Disabled</Button>
      </div>
      <SectionHeader title="Chips and times" />
      <div className="flex flex-wrap items-center gap-2">
        <Chip>Zoom</Chip>
        <Chip tone="strong">Work</Chip>
        <Chip tone="gold">Edited</Chip>
        <Chip onClick={() => {}}>+ tag</Chip>
        <Timestamp at={754_000} onJump={() => {}} />
        <Label>00:42 / 42:10</Label>
      </div>
      <SectionHeader title="Speakers" />
      <div className="flex w-[360px] gap-2">
        <SpeakerTag speaker="me" name="Me" className="w-14" />
        <SpeakerTag speaker="them:0" name="Them" className="w-14" />
        <SpeakerTag speaker="them:1" name="Dana Reyes" className="w-14" />
        <SpeakerTag speaker="them:2" name="Priya Shah" className="w-14" />
        <SpeakerTag speaker="them:3" name="Sam Okafor" className="w-14" />
        <SpeakerTag speaker="them:4" name="Lee Wu" className="w-14" />
      </div>
      <SectionHeader title="Form controls" />
      <div className="flex flex-wrap items-center gap-4">
        <Field label="Search" placeholder="Search meetings" wrapperClassName="w-60" />
        <Select label="Language" defaultValue="en">
          <option value="en">English</option>
          <option value="es">Spanish</option>
        </Select>
        <Switch aria-label="Example switch" defaultChecked />
        <Switch aria-label="Example switch off" />
        <Checkbox aria-label="Example checkbox" defaultChecked />
        <Checkbox aria-label="Example checkbox off" />
        {["a", "b"].map((v) => (
          <button key={v} type="button" role="radio" aria-checked={radio === v} onClick={() => setRadio(v)} className="flex cursor-pointer items-center gap-2 border-0 bg-transparent p-0 font-[inherit] text-[13px] text-ink">
            <Radio checked={radio === v} />
            Option {v.toUpperCase()}
          </button>
        ))}
      </div>
      <SectionHeader title="Sheet" />
      <div>
        <Button onClick={() => setSheet(true)}>Open sheet</Button>
        <Sheet open={sheet} title="Example" onClose={() => setSheet(false)}>
          <div className="flex flex-col gap-3 p-5">
            <h2 className="m-0 text-lg font-extrabold">Example sheet</h2>
            <p className="m-0 text-[13px] text-subtle">Closes on Escape or a click outside.</p>
            <Button onClick={() => setSheet(false)}>Close</Button>
          </div>
        </Sheet>
      </div>
    </div>
  );
}
