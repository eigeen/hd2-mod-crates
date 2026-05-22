import ClearAllIcon from "@mui/icons-material/ClearAll";
import SearchIcon from "@mui/icons-material/Search";
import SelectAllIcon from "@mui/icons-material/SelectAll";
import {
  Alert,
  Box,
  Button,
  Checkbox,
  CircularProgress,
  InputAdornment,
  Stack,
  TextField,
  Typography,
} from "@mui/material";
import { useMemo } from "react";
import type { MigrationTargetOption } from "./types";

interface TargetPickerProps {
  errorText: string;
  loading: boolean;
  onClear: () => void;
  onQueryChange: (value: string) => void;
  onSelectAll: () => void;
  onToggle: (hash: string) => void;
  options: MigrationTargetOption[];
  query: string;
  selectedHashes: string[];
}

function TargetPicker(props: TargetPickerProps) {
  const visibleOptions = useMemo(
    () => filteredOptions(props.options, props.query),
    [props.options, props.query],
  );
  const selectedSet = useMemo(
    () => new Set(props.selectedHashes),
    [props.selectedHashes],
  );

  return (
    <Box className="targetPanel">
      <TargetPanelHeader
        count={props.options.length}
        onClear={props.onClear}
        onSelectAll={props.onSelectAll}
        selectedCount={props.selectedHashes.length}
      />
      <TextField
        fullWidth
        label="Search targets"
        onChange={(event) => props.onQueryChange(event.target.value)}
        placeholder="armor name or hash"
        size="small"
        slotProps={{ input: { startAdornment: searchAdornment() } }}
        value={props.query}
      />
      <TargetListState errorText={props.errorText} loading={props.loading} />
      {!props.loading && !props.errorText && (
        <Box className="targetList">
          {visibleOptions.map((option) => (
            <TargetRow
              key={option.hash}
              onToggle={props.onToggle}
              option={option}
              selected={selectedSet.has(option.hash)}
            />
          ))}
          {visibleOptions.length === 0 && <EmptyTargets />}
        </Box>
      )}
    </Box>
  );
}

interface TargetPanelHeaderProps {
  count: number;
  onClear: () => void;
  onSelectAll: () => void;
  selectedCount: number;
}

function TargetPanelHeader(props: TargetPanelHeaderProps) {
  return (
    <Stack className="targetPanelHeader" direction="row" spacing={1}>
      <Box>
        <Typography className="targetPanelTitle">Targets</Typography>
        <Typography className="targetPanelMeta">
          {props.selectedCount} / {props.count} selected
        </Typography>
      </Box>
      <Box className="toolbarSpacer" />
      <Button onClick={props.onSelectAll} startIcon={<SelectAllIcon />} variant="outlined">
        Select all
      </Button>
      <Button onClick={props.onClear} startIcon={<ClearAllIcon />} variant="outlined">
        Clear
      </Button>
    </Stack>
  );
}

interface TargetListStateProps {
  errorText: string;
  loading: boolean;
}

function TargetListState({ errorText, loading }: TargetListStateProps) {
  if (loading) {
    return (
      <Stack className="targetLoading" direction="row" spacing={1}>
        <CircularProgress size={18} />
        <Typography>Loading targets</Typography>
      </Stack>
    );
  }
  if (errorText) {
    return <Alert severity="error">{errorText}</Alert>;
  }
  return null;
}

interface TargetRowProps {
  onToggle: (hash: string) => void;
  option: MigrationTargetOption;
  selected: boolean;
}

function TargetRow({ onToggle, option, selected }: TargetRowProps) {
  return (
    <Box className="targetRow" onClick={() => onToggle(option.hash)}>
      <Checkbox
        checked={selected}
        onChange={() => onToggle(option.hash)}
        onClick={(event) => event.stopPropagation()}
        tabIndex={-1}
      />
      <Box className="targetRowText">
        <Typography className="targetName">{option.name}</Typography>
        <Typography className="targetHash">{option.hash}</Typography>
      </Box>
    </Box>
  );
}

function EmptyTargets() {
  return <Typography className="targetEmpty">No targets match the search.</Typography>;
}

function filteredOptions(options: MigrationTargetOption[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return options;
  }
  return options.filter((option) => targetMatchesQuery(option, normalized));
}

function targetMatchesQuery(option: MigrationTargetOption, query: string) {
  return option.name.toLowerCase().includes(query) || option.hash.toLowerCase().includes(query);
}

function searchAdornment() {
  return (
    <InputAdornment position="start">
      <SearchIcon fontSize="small" />
    </InputAdornment>
  );
}

export default TargetPicker;
