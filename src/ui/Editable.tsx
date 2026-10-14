import { cn } from "./cn";

interface Props {
  text: string;
  editing: boolean;
  onCommit: (text: string) => void;
  className?: string;
}

/** Plain text that becomes click-to-type while `editing`. Commits on blur; Enter confirms, Escape cancels. */
export function Editable({ text, editing, onCommit, className }: Props) {
  if (!editing) return <span className={className}>{text}</span>;
  return (
    <span
      key={text}
      contentEditable="plaintext-only"
      suppressContentEditableWarning
      spellCheck
      role="textbox"
      aria-label="Edit text"
      onBlur={(e) => {
        const next = (e.currentTarget.textContent ?? "").trim();
        if (next && next !== text) onCommit(next);
        else e.currentTarget.textContent = text;
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          e.currentTarget.blur();
        }
        if (e.key === "Escape") {
          e.currentTarget.textContent = text;
          e.currentTarget.blur();
        }
      }}
      className={cn("rounded border border-dashed border-input bg-raised px-1 outline-none focus:border-edge", className)}
    >
      {text}
    </span>
  );
}
