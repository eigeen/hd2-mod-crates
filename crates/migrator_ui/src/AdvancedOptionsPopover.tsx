import ExpandMoreIcon from "@mui/icons-material/ExpandMore";
import HelpOutlineIcon from "@mui/icons-material/HelpOutlined";
import { Button, Checkbox, FormControlLabel, Popover, Tooltip } from "@mui/material";
import { useState, type ReactNode } from "react";

interface AdvancedOptionsPopoverProps {
  children: ReactNode;
  id: string;
  label: string;
}

/** Show infrequently used settings without adding another nested panel. */
export function AdvancedOptionsPopover(props: AdvancedOptionsPopoverProps) {
  const [anchor, setAnchor] = useState<HTMLButtonElement | null>(null);
  const open = Boolean(anchor);

  return (
    <>
      <Button
        aria-controls={open ? props.id : undefined}
        aria-expanded={open}
        aria-haspopup="dialog"
        endIcon={<ExpandMoreIcon className={`transition-transform ${open ? "rotate-180" : ""}`} />}
        onClick={(event) => {
          const button = event.currentTarget;
          setAnchor((current) => current ? null : button);
        }}
        variant="outlined"
      >
        {props.label}
      </Button>
      <Popover
        anchorEl={anchor}
        anchorOrigin={{ horizontal: "left", vertical: "top" }}
        id={props.id}
        onClose={() => setAnchor(null)}
        open={open}
        slotProps={{ paper: { sx: { backgroundImage: "none", borderRadius: 0 } } }}
        transformOrigin={{ horizontal: "left", vertical: "bottom" }}
      >
        <div
          aria-label={props.label}
          className="flex w-[min(30rem,calc(100vw-2rem))] flex-col gap-2 border border-hd2-border bg-hd2-pit p-3"
          role="dialog"
        >
          {props.children}
        </div>
      </Popover>
    </>
  );
}

interface AdvancedCheckboxOptionProps {
  checked: boolean;
  help: string;
  label: string;
  onChange: (checked: boolean) => void;
}

export function AdvancedCheckboxOption(props: AdvancedCheckboxOptionProps) {
  return (
    <div className="flex min-w-0 items-center gap-1">
      <FormControlLabel
        className="optionsControl min-w-0 flex-1"
        control={<Checkbox checked={props.checked} onChange={(event) => props.onChange(event.target.checked)} />}
        label={props.label}
      />
      <HelpHint title={props.help} />
    </div>
  );
}

interface AdvancedOptionRowProps {
  children: ReactNode;
  help?: string;
  label: string;
}

export function AdvancedOptionRow(props: AdvancedOptionRowProps) {
  return (
    <div className="flex flex-wrap items-center gap-2 border-t border-hd2-border pt-3">
      <span className="mr-auto text-xs text-hd2-muted">{props.label}</span>
      {props.children}
      {props.help && <HelpHint title={props.help} />}
    </div>
  );
}

export function HelpHint({ title }: { title: string }) {
  return (
    <Tooltip arrow placement="top" title={title}>
      <HelpOutlineIcon className="shrink-0 cursor-help text-hd2-faint hover:text-hd2-yellow" sx={{ fontSize: "1rem" }} />
    </Tooltip>
  );
}
