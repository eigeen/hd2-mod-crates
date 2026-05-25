import CheckIcon from "@mui/icons-material/Check";
import LanguageIcon from "@mui/icons-material/Language";
import { IconButton, ListItemIcon, Menu, MenuItem, Tooltip } from "@mui/material";
import { useState } from "react";
import { useI18n, type LanguageCode } from "./i18n";

export function LanguageMenu() {
  const { language, languages, setLanguage, t } = useI18n();
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const close = () => setAnchor(null);

  const selectLanguage = (nextLanguage: LanguageCode) => {
    setLanguage(nextLanguage);
    close();
  };

  return (
    <>
      <Tooltip arrow placement="top" title={t("language.menu")}>
        <IconButton
          aria-label={t("language.menu")}
          className="languageMenuBtn"
          onClick={(event) => setAnchor(event.currentTarget)}
          size="small"
        >
          <LanguageIcon fontSize="small" />
        </IconButton>
      </Tooltip>
      <Menu anchorEl={anchor} open={Boolean(anchor)} onClose={close}>
        {languages.map((item) => (
          <MenuItem key={item} onClick={() => selectLanguage(item)} selected={item === language}>
            <ListItemIcon className="languageMenuCheck">
              {item === language ? <CheckIcon fontSize="small" /> : null}
            </ListItemIcon>
            {t(`language.${item}`)}
          </MenuItem>
        ))}
      </Menu>
    </>
  );
}
