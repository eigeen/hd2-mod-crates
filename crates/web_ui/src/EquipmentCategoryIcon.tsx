import SvgIcon, { type SvgIconProps } from "@mui/material/SvgIcon";
import SportsMotorsportsOutlinedIcon from "@mui/icons-material/SportsMotorsportsOutlined";
import type { EquipmentCategory } from "./types";

export function EquipmentCategoryIcon({
  category,
  ...props
}: SvgIconProps & { category: EquipmentCategory }) {
  if (category === "Helmet") {
    return <SportsMotorsportsOutlinedIcon {...props} />;
  }

  return (
    <SvgIcon {...props} viewBox="0 0 24 24">
      <path d="M7 2h3v3h4V2h3l5 4v7h-4v9H6v-9H2V6l5-4Zm1 3L4 7v4h4v9h8v-9h4V7l-4-2v2H8V5Z" />
    </SvgIcon>
  );
}
