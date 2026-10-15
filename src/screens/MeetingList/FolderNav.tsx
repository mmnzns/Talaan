import { Folder as FolderIcon, MoreHorizontal } from "lucide-react";
import { useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Folder } from "../../data/types";
import { Chip, Field, MenuItem, Popover, cn } from "../../ui";

interface Props {
  folders: Folder[];
  tags: string[];
  total: number;
  folderId?: string;
  tag?: string;
  onFolder: (id?: string) => void;
  onTag: (tag?: string) => void;
}

export function FolderNav({ folders, tags, total, folderId, tag, onFolder, onTag }: Props) {
  const store = useStore();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState<string>();

  const commit = async () => {
    const trimmed = name.trim();
    setAdding(false);
    setName("");
    if (trimmed) onFolder((await store.createFolder(trimmed)).id);
  };

  const row = (active: boolean) =>
    cn(
      "flex w-full cursor-pointer items-center gap-2 rounded-control border-0 px-2 py-1.5 text-left font-[inherit] text-[13px]",
      active ? "bg-strong font-semibold text-strong-fg" : "bg-transparent text-ink hover:bg-hover",
    );

  return (
    <div className="flex flex-col gap-px border-b-[1.5px] border-line px-2 pb-2">
      <button type="button" className={row(!folderId)} onClick={() => onFolder(undefined)} aria-current={!folderId ? "true" : undefined}>
        <span className="flex-1">All meetings</span>
        <span className={cn("font-mono text-[11px]", !folderId ? "text-strong-fg" : "text-subtle")}>{total}</span>
      </button>
      {folders.map((f) =>
        renaming === f.id ? (
          <div key={f.id} className="px-1 py-1">
            <NameField
              initial={f.name}
              onDone={(next) => {
                setRenaming(undefined);
                if (next && next !== f.name) store.renameFolder(f.id, next);
              }}
            />
          </div>
        ) : (
          <div key={f.id} className="group relative flex items-center">
            <button type="button" className={row(folderId === f.id)} onClick={() => onFolder(f.id)} aria-current={folderId === f.id ? "true" : undefined}>
              <FolderIcon size={14} aria-hidden="true" className={folderId === f.id ? "" : "text-subtle"} />
              <span className="flex-1">{f.name}</span>
              <span className={cn("font-mono text-[11px] group-hover:invisible", folderId === f.id ? "text-strong-fg" : "text-subtle")}>{f.count}</span>
            </button>
            <div className="absolute right-1 hidden group-focus-within:flex group-hover:flex">
              <FolderMenu
                active={folderId === f.id}
                name={f.name}
                onRename={() => setRenaming(f.id)}
                onDelete={() => {
                  if (folderId === f.id) onFolder(undefined);
                  store.deleteFolder(f.id);
                }}
              />
            </div>
          </div>
        ),
      )}
      {adding ? (
        <div className="px-1 py-1">
          <Field
            label="Folder name"
            autoFocus
            placeholder="Folder name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") commit();
              if (e.key === "Escape") {
                setName("");
                setAdding(false);
              }
            }}
          />
        </div>
      ) : (
        <button type="button" className={cn(row(false), "text-subtle")} onClick={() => setAdding(true)}>
          + New folder
        </button>
      )}
      {tags.length > 0 && (
        <div className="flex flex-wrap gap-1.5 px-2 pb-0.5 pt-1.5">
          {tags.map((t) => (
            <Chip key={t} tone={tag === t ? "strong" : "default"} onClick={() => onTag(tag === t ? undefined : t)} aria-pressed={tag === t}>
              #{t}
            </Chip>
          ))}
        </div>
      )}
    </div>
  );
}

function NameField({ initial, onDone }: { initial: string; onDone: (name?: string) => void }) {
  const [value, setValue] = useState(initial);
  return (
    <Field
      label="Folder name"
      autoFocus
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => onDone(value.trim() || undefined)}
      onKeyDown={(e) => {
        if (e.key === "Enter") onDone(value.trim() || undefined);
        if (e.key === "Escape") onDone(undefined);
      }}
    />
  );
}

/** Rename / Delete for one folder. Delete asks once more, in place. */
function FolderMenu({ active, name, onRename, onDelete }: { active: boolean; name: string; onRename: () => void; onDelete: () => void }) {
  const [confirming, setConfirming] = useState(false);
  return (
    <Popover
      align="left"
      trigger={({ toggle }) => (
        <button
          type="button"
          aria-label={`${name} options`}
          onClick={() => {
            setConfirming(false);
            toggle();
          }}
          className={cn("flex size-6 cursor-pointer items-center justify-center rounded-control border-0 bg-transparent", active ? "text-strong-fg" : "text-subtle hover:bg-hover")}
        >
          <MoreHorizontal size={14} aria-hidden="true" />
        </button>
      )}
    >
      {(close) =>
        confirming ? (
          <div className="flex flex-col gap-1.5 p-1">
            <span className="text-[13px]">Delete “{name}”? Its meetings stay, just outside a folder.</span>
            <MenuItem
              className="font-semibold text-danger"
              onClick={() => {
                close();
                onDelete();
              }}
            >
              Delete folder
            </MenuItem>
          </div>
        ) : (
          <>
            <MenuItem
              onClick={() => {
                close();
                onRename();
              }}
            >
              Rename
            </MenuItem>
            <MenuItem className="text-danger" onClick={() => setConfirming(true)}>
              Delete…
            </MenuItem>
          </>
        )
      }
    </Popover>
  );
}
