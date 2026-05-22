//! Indicatif-backed implementation of the migrator progress sink.

use hd2_migrator_io::migrator::ProgressSink;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct IndicatifProgress {
    multi: MultiProgress,
    overall: ProgressBar,
    bars: Mutex<HashMap<String, ProgressBar>>,
}

impl IndicatifProgress {
    pub fn new(total_targets: u64) -> Self {
        let multi = MultiProgress::new();
        crate::cli::logging::attach_progress(multi.clone());
        let overall = multi.add(ProgressBar::new(total_targets));
        overall.set_style(
            ProgressStyle::with_template("[{elapsed_precise}] {bar:40} {pos}/{len} {msg}")
                .expect("style"),
        );
        Self {
            multi,
            overall,
            bars: Mutex::new(HashMap::new()),
        }
    }

    pub fn finish(&self) {
        self.overall.finish_with_message("done");
        crate::cli::logging::detach_progress();
    }
}

impl ProgressSink for IndicatifProgress {
    fn target_started(&self, name: &str) {
        let bar = self.multi.add(ProgressBar::new_spinner());
        bar.set_style(
            ProgressStyle::with_template("  {prefix:.cyan} {spinner} {wide_msg}").expect("style"),
        );
        bar.set_prefix(name.to_string());
        bar.enable_steady_tick(std::time::Duration::from_millis(100));
        self.bars
            .lock()
            .expect("lock poisoned")
            .insert(name.to_string(), bar);
    }

    fn stage(&self, name: &str, stage: &str) {
        if let Some(bar) = self.bars.lock().expect("lock poisoned").get(name) {
            bar.set_message(stage.to_string());
        }
    }

    fn target_finished(&self, name: &str) {
        if let Some(bar) = self.bars.lock().expect("lock poisoned").remove(name) {
            bar.finish_and_clear();
        }
        self.overall.inc(1);
    }
}
