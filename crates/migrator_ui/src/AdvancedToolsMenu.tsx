import ExpandMoreIcon from "@mui/icons-material/ExpandMore";
import { Button, Menu, MenuItem } from "@mui/material";
import { useState } from "react";
import { useI18n } from "./i18n";

export interface AdvancedToolsItem<TMode extends string> {
  mode: TMode;
  label: string;
}

interface AdvancedToolsMenuProps<TMode extends string> {
  /** Currently active tool mode; the matching item is highlighted. */
  active: string;
  /** Secondary tools listed inside the dropdown. */
  items: AdvancedToolsItem<TMode>[];
  /** Called with the selected item's mode. */
  onSelect: (mode: TMode) => void;
}

/**
 * Toolbar dropdown for secondary tools (e.g. Merge Mods, Head Merge),
 * styled to match the adjacent MUI Tabs.
 */
export function AdvancedToolsMenu<TMode extends string>({
  active,
  items,
  onSelect,
}: AdvancedToolsMenuProps<TMode>) {
  const { t } = useI18n();
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const close = () => setAnchor(null);
  const activeItem = items.find((item) => item.mode === active);

  return (
    <>
      <Button
        aria-expanded={Boolean(anchor)}
        aria-haspopup="menu"
        className={activeItem ? "advancedToolsTab advancedToolsTab-active" : "advancedToolsTab"}
        endIcon={<ExpandMoreIcon />}
        onClick={(event) => setAnchor(event.currentTarget)}
      >
        {activeItem ? activeItem.label : t("mode.advancedFeatures")}
      </Button>
      <Menu anchorEl={anchor} open={Boolean(anchor)} onClose={close}>
        {items.map((item) => (
          <MenuItem
            key={item.mode}
            onClick={() => {
              onSelect(item.mode);
              close();
            }}
            selected={item.mode === active}
          >
            {item.label}
          </MenuItem>
        ))}
      </Menu>
    </>
  );
}
