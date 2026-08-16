import AltRouteIcon from "@mui/icons-material/AltRoute";
import DeleteSweepOutlinedIcon from "@mui/icons-material/DeleteSweepOutlined";
import FileUploadOutlinedIcon from "@mui/icons-material/FileUploadOutlined";
import TuneIcon from "@mui/icons-material/Tune";
import {
  Checkbox,
  Divider,
  ListItemIcon,
  ListItemText,
  ListSubheader,
  Menu,
  MenuItem,
  Radio,
} from "@mui/material";
import { useI18n } from "./i18n";
import type { UnitBehaviorOptions, UnitMappingBehaviorKey } from "./types";
import {
  hasUnitBehavior,
  mappingIsEnabled,
  preferredConflictSource,
  resetUnitBehavior,
  resolvedUnitExport,
  setMappingsEnabled,
  setPreferredConflictSource,
  setUnitsExported,
} from "./unitBehavior";

export interface UnitMenuOutput {
  fileId: string;
  defaultExport: boolean;
}

export interface UnitMenuContext {
  anchor: { left: number; top: number };
  fileId: string;
  mappings: UnitMappingBehaviorKey[];
  outputs: UnitMenuOutput[];
  conflictTargetFileId: string | null;
  conflictSourceFileIds: string[];
}

interface UnitContextMenuProps {
  behavior: UnitBehaviorOptions;
  context: UnitMenuContext | null;
  onChange: (behavior: UnitBehaviorOptions) => void;
  onClose: () => void;
}

export function UnitContextMenu(props: UnitContextMenuProps) {
  const { t } = useI18n();
  const context = props.context;
  if (!context) return null;
  const outputIds = context.outputs.map((output) => output.fileId);
  const conversionEnabled = context.mappings.every((mapping) => mappingIsEnabled(props.behavior, mapping));
  const exportEnabled = context.outputs.every((output) => (
    resolvedUnitExport(props.behavior, output.fileId, output.defaultExport)
  ));
  const preferredSource = context.conflictTargetFileId
    ? preferredConflictSource(props.behavior, context.conflictTargetFileId)
    : null;
  const customized = hasUnitBehavior(
    props.behavior,
    context.mappings,
    outputIds,
    context.conflictTargetFileId,
  );

  return (
    <Menu
      anchorPosition={context.anchor}
      anchorReference="anchorPosition"
      onClose={props.onClose}
      open
      slotProps={{ paper: { sx: { minWidth: 300, maxWidth: 360 } } }}
    >
      <ListSubheader className="unitContextMenuHeader">
        <code>{context.fileId}</code>
      </ListSubheader>
      {context.mappings.length > 0 && (
        <MenuItem onClick={() => {
          props.onChange(setMappingsEnabled(props.behavior, context.mappings, !conversionEnabled));
          props.onClose();
        }}>
          <ListItemIcon><AltRouteIcon fontSize="small" /></ListItemIcon>
          <ListItemText primary={t("preview.unitConversion")} secondary={t("preview.unitConversionHelp")} />
          <Checkbox checked={conversionEnabled} edge="end" size="small" />
        </MenuItem>
      )}
      {context.outputs.length > 0 && (
        <MenuItem onClick={() => {
          props.onChange(setUnitsExported(props.behavior, context.outputs, !exportEnabled));
          props.onClose();
        }}>
          <ListItemIcon><FileUploadOutlinedIcon fontSize="small" /></ListItemIcon>
          <ListItemText primary={t("preview.unitExport")} secondary={t("preview.unitExportHelp")} />
          <Checkbox checked={exportEnabled} edge="end" size="small" />
        </MenuItem>
      )}
      {context.conflictTargetFileId && context.conflictSourceFileIds.length > 0 && <>
        <Divider />
        <ListSubheader className="unitContextMenuSection">{t("preview.unitConflict")}</ListSubheader>
        <MenuItem onClick={() => {
          props.onChange(setPreferredConflictSource(
            props.behavior,
            context.conflictTargetFileId!,
            null,
          ));
          props.onClose();
        }}>
          <ListItemIcon><TuneIcon fontSize="small" /></ListItemIcon>
          <ListItemText primary={t("preview.unitConflictDefault")} />
          <Radio checked={preferredSource === null} edge="end" size="small" />
        </MenuItem>
        {context.conflictSourceFileIds.map((sourceFileId) => (
          <MenuItem key={sourceFileId} onClick={() => {
            props.onChange(setPreferredConflictSource(
              props.behavior,
              context.conflictTargetFileId!,
              sourceFileId,
            ));
            props.onClose();
          }}>
            <ListItemIcon><AltRouteIcon fontSize="small" /></ListItemIcon>
            <ListItemText
              primary={t("preview.unitConflictPrefer")}
              secondary={<code>{sourceFileId}</code>}
            />
            <Radio checked={preferredSource === sourceFileId} edge="end" size="small" />
          </MenuItem>
        ))}
      </>}
      <Divider />
      <MenuItem disabled={!customized} onClick={() => {
        props.onChange(resetUnitBehavior(
          props.behavior,
          context.mappings,
          outputIds,
          context.conflictTargetFileId,
        ));
        props.onClose();
      }}>
        <ListItemIcon><DeleteSweepOutlinedIcon fontSize="small" /></ListItemIcon>
        <ListItemText primary={t("preview.unitReset")} />
      </MenuItem>
    </Menu>
  );
}
