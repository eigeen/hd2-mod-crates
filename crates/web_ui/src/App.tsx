import { useCallback, useMemo, useState } from "react";
import { MigratorApp, type SetupPanelsProps } from "@hd2-mod-tools/migrator-app";
import type { PatchFiles, TranslationKey } from "@hd2-mod-tools/migrator-ui";
import { FrequentlyAskedQuestions } from "./FrequentlyAskedQuestions";
import { WebHeadArmorMergeTool } from "./WebHeadArmorMergeTool";
import { WebManualMergeTool } from "./WebManualMergeTool";
import type { GameDirSelection } from "./GameDataDirPanel";
import { useToolModeRoute } from "./toolModeRoute";
import { WebSetupPanels, webPlatform, webViewExtras } from "./webPlatform";

export default function App() {
  const [toolMode, navigateToToolMode] = useToolModeRoute();
  const [faqOpen, setFaqOpen] = useState(false);
  const [faqAttention, setFaqAttention] = useState<TranslationKey | null>(null);
  const openDirectoryHelp = useCallback(() => {
    setFaqAttention("faq.workaroundQuestion");
    setFaqOpen(true);
  }, []);
  const SetupPanels = useMemo(
    () => function WebSetup(props: SetupPanelsProps<PatchFiles, GameDirSelection>) {
      return <WebSetupPanels {...props} onDirectoryAccessAborted={openDirectoryHelp} />;
    },
    [openDirectoryHelp],
  );
  const extras = useMemo(() => ({
    ...webViewExtras,
    footer: (
      <FrequentlyAskedQuestions
        attentionQuestion={faqAttention}
        onOpenChange={(open) => {
          setFaqOpen(open);
          if (!open) setFaqAttention(null);
        }}
        open={faqOpen}
      />
    ),
  }), [faqAttention, faqOpen]);

  return (
    <MigratorApp
      HeadMergeTool={WebHeadArmorMergeTool}
      MergeTool={WebManualMergeTool}
      SetupPanels={SetupPanels}
      extras={extras}
      layout="web"
      onToolModeChange={navigateToToolMode}
      platform={webPlatform}
      toolMode={toolMode}
    />
  );
}
