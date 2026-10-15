import { MoreHorizontal } from "lucide-react";
import { useState } from "react";
import { useStore } from "../../data/StoreProvider";
import type { Folder } from "../../data/types";
import { useT } from "../../lib/i18n/I18nProvider";
import { Chip, Field, MenuItem, Popover } from "../../ui";

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
  const tr = useT();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [renaming, setRenaming] = useState<string>();

  const commit = async () => {
    const trimmed = name.trim();
    setAdding(false);
    setName("");
    if (trimmed) onFolder((await store.createFolder(trimmed)).id);
  };

  return (
    <div className="flex flex-col gap-2 border-b border-line px-[18px] pb-[18px] pt-2.5">
      <div className="flex flex-wrap items-center gap-2" role="group" aria-label={tr("shell.foldersAndTags")}>
        <Chip tone={!folderId && !tag ? "strong" : "default"} onClick={() => { onFolder(undefined); onTag(undefined); }} aria-pressed={!folderId && !tag}>
          {tr("shell.all", { count: total })}
        </Chip>
        {folders.map((f) =>
          renaming === f.id ? (
            <span key={f.id} className="w-40">
              <NameField
                initial={f.name}
                onDone={(next) => {
                  setRenaming(undefined);
                  if (next && next !== f.name) store.renameFolder(f.id, next);
                }}
              />
            </span>
          ) : (
            <span key={f.id} className="inline-flex items-center">
              <Chip tone={folderId === f.id ? "strong" : "default"} onClick={() => onFolder(folderId === f.id ? undefined : f.id)} aria-pressed={folderId === f.id}>
                {f.name} · {f.count}
              </Chip>
              {folderId === f.id && (
                <FolderMenu
                  name={f.name}
                  onRename={() => setRenaming(f.id)}
                  onDelete={() => {
                    onFolder(undefined);
                    store.deleteFolder(f.id);
                  }}
                />
              )}
            </span>
          ),
        )}
        {tags.map((t) => (
          <Chip key={t} tone={tag === t ? "strong" : "default"} onClick={() => onTag(tag === t ? undefined : t)} aria-pressed={tag === t}>
            #{t}
          </Chip>
        ))}
        {!adding && (
          <Chip onClick={() => setAdding(true)} className="border-dashed">
            {tr("shell.addFolder")}
          </Chip>
        )}
      </div>
      {adding && (
        <Field
          label={tr("shell.folderName")}
          autoFocus
          placeholder={tr("shell.folderName")}
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
      )}
    </div>
  );
}

function NameField({ initial, onDone }: { initial: string; onDone: (name?: string) => void }) {
  const tr = useT();
  const [value, setValue] = useState(initial);
  return (
    <Field
      label={tr("shell.folderName")}
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
function FolderMenu({ name, onRename, onDelete }: { name: string; onRename: () => void; onDelete: () => void }) {
  const tr = useT();
  const [confirming, setConfirming] = useState(false);
  return (
    <Popover
      align="left"
      trigger={({ toggle }) => (
        <button
          type="button"
          aria-label={tr("shell.folderOptions", { name })}
          onClick={() => {
            setConfirming(false);
            toggle();
          }}
          className="flex size-6 cursor-pointer items-center justify-center rounded-control border-0 bg-transparent text-subtle hover:bg-hover hover:text-ink"
        >
          <MoreHorizontal size={14} aria-hidden="true" />
        </button>
      )}
    >
      {(close) =>
        confirming ? (
          <div className="flex flex-col gap-1.5 p-1">
            <span className="text-[13px]">{tr("shell.deleteFolderAsk", { name })}</span>
            <MenuItem
              className="font-semibold text-danger"
              onClick={() => {
                close();
                onDelete();
              }}
            >
              {tr("shell.deleteFolder")}
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
              {tr("shell.rename")}
            </MenuItem>
            <MenuItem className="text-danger" onClick={() => setConfirming(true)}>
              {tr("shell.deleteMenu")}
            </MenuItem>
          </>
        )
      }
    </Popover>
  );
}
