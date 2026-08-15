use crate::archive::StreamToc;
use crate::io::DataSource;
use crate::migrator::mode_a_web::{
    self, MigrationArchiveCache, PreparedMigration, PreparedMigrationOptions,
};
use crate::web::equipment::{EquipmentCategory, WebMigrationMapping};
use crate::web::migration::UnmatchedUnitPolicy;

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
    migration: PreparedMigration,
    source_hash: String,
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
        let prepared_index = self.prepared_index(mapping).await?;
        let prepared = &self.prepared[prepared_index].migration;
        prepared
            .migrate_target(
                self.source,
                &mut self.archive_cache,
                &mapping.target_hash,
                self.progress,
            )
            .await
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
            migration,
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
