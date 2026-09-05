import { useMemo, useState } from "react";
import { MigratorApp, type PlatformViewExtras, type ToolMode } from "@hd2-mod-tools/migrator-app";
import { AppUpdateButton, AppUpdateDialog } from "./AppUpdateDialog";
import { DesktopSharedHeadMergeTool } from "./DesktopSharedHeadMergeTool";
import { DesktopSharedManualMergeTool } from "./DesktopSharedManualMergeTool";
import { DesktopSetupPanels, desktopPlatform } from "./desktopPlatform";
import { useAppUpdate } from "./useAppUpdate";

export default function App() {
  const [toolMode, setToolMode] = useState<ToolMode>("migrate");
  const appUpdate = useAppUpdate();
  const extras = useMemo<PlatformViewExtras>(() => ({
    headerAction: <AppUpdateButton controller={appUpdate} />,
    overlay: (taskBusy) => (
      <AppUpdateDialog controller={appUpdate} taskBusy={taskBusy} />
    ),
  }), [appUpdate]);

  return (
    <MigratorApp
      HeadMergeTool={DesktopSharedHeadMergeTool}
      MergeTool={DesktopSharedManualMergeTool}
      SetupPanels={DesktopSetupPanels}
      extras={extras}
      layout="desktop"
      onToolModeChange={setToolMode}
      platform={desktopPlatform}
      toolMode={toolMode}
    />
  );
}
