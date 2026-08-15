use crate::archive::StreamToc;
use crate::io::DataSource;
use crate::migrator::mode_a_web::{
    self, MigrationArchiveCache, PreparedMigration, PreparedMigrationOptions, PreparedTarget,
};
use crate::web::equipment::{EquipmentCategory, WebMigrationMapping};
use crate::web::migration::UnmatchedUnitPolicy;
use std::sync::Arc;

pub(super) struct MigrationExecutor<'a, S: DataSource + ?Sized> {
    archive_cache: MigrationArchiveCache,
    no_padding: bool,
    original: &'a StreamToc,
    prepared: Vec<PreparedEntry>,
    progress: Option<&'a dyn mode_a_web::WebProgress>,
    source: &'a S,
}

struct PreparedEntry {
    category: EquipmentCategory,
    migration: Arc<PreparedMigration>,
    source_hash: String,
}

pub(super) struct PreparedWork {
    migration: Arc<PreparedMigration>,
    target: PreparedTarget,
}

impl PreparedWork {
    #[cfg(not(target_family = "wasm"))]
    pub(super) fn compute(
        self,
        progress: Option<&(dyn mode_a_web::WebProgress + Sync)>,
    ) -> crate::Result<mode_a_web::WebTargetResult> {
        self.migration
            .compute_prepared_target(self.target, progress.map(as_web_progress))
    }
}

#[cfg(not(target_family = "wasm"))]
pub(super) fn as_web_progress(
    progress: &(dyn mode_a_web::WebProgress + Sync),
) -> &dyn mode_a_web::WebProgress {
    progress
}

impl<'a, S: DataSource + ?Sized> MigrationExecutor<'a, S> {
    pub(super) async fn new(
        original: &'a StreamToc,
        source: &'a S,
        progress: Option<&'a dyn mode_a_web::WebProgress>,
        no_padding: bool,
    ) -> crate::Result<Self> {
        Ok(Self {
            archive_cache: MigrationArchiveCache::open(source).await?,
            no_padding,
            original,
            prepared: Vec::new(),
            progress,
            source,
        })
    }

    pub(super) async fn migrate(
        &mut self,
        mapping: &WebMigrationMapping,
    ) -> crate::Result<mode_a_web::WebTargetResult> {
        let work = self.prepare_work(mapping, self.progress).await?;
        work.migration
            .compute_prepared_target(work.target, self.progress)
    }

    #[cfg(not(target_family = "wasm"))]
    pub(super) async fn prepare_parallel_work(
        &mut self,
        mapping: &WebMigrationMapping,
        progress: Option<&(dyn mode_a_web::WebProgress + Sync)>,
    ) -> crate::Result<PreparedWork> {
        self.prepare_work(mapping, progress.map(as_web_progress))
            .await
    }

    async fn prepare_work(
        &mut self,
        mapping: &WebMigrationMapping,
        progress: Option<&dyn mode_a_web::WebProgress>,
    ) -> crate::Result<PreparedWork> {
        let prepared_index = self.prepared_index(mapping).await?;
        let migration = Arc::clone(&self.prepared[prepared_index].migration);
        let target = migration
            .prepare_target(
                self.source,
                &mut self.archive_cache,
                &mapping.target_hash,
                progress,
            )
            .await?;
        Ok(PreparedWork { migration, target })
    }

    async fn prepared_index(&mut self, mapping: &WebMigrationMapping) -> crate::Result<usize> {
        if let Some(index) = self.find_prepared(mapping) {
            return Ok(index);
        }
        let migration = PreparedMigration::new(
            self.original,
            self.source,
            &mut self.archive_cache,
            PreparedMigrationOptions {
                category: mapping.category.as_str(),
                no_padding: self.no_padding,
                source_hash: &mapping.source_hash,
                unmatched_unit_policy: UnmatchedUnitPolicy::Keep,
            },
        )
        .await?;
        self.prepared.push(PreparedEntry {
            category: mapping.category,
            migration: Arc::new(migration),
            source_hash: mapping.source_hash.clone(),
        });
        Ok(self.prepared.len() - 1)
    }

    fn find_prepared(&self, mapping: &WebMigrationMapping) -> Option<usize> {
        self.prepared.iter().position(|entry| {
            entry.category == mapping.category && entry.source_hash == mapping.source_hash
        })
    }
}
