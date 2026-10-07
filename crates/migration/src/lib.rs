use sea_orm_migration::prelude::*;
mod m20261006_000001_course_revisions;
mod m20261006_000002_sessions;
mod m20261006_000003_identity;
mod m20261006_000004_profile;
mod m20261006_000005_learning;
mod m20261006_000006_reviews;
mod m20261006_000007_saved;
mod m20261006_000008_releases;
mod m20261006_000009_media;
mod m20261006_000010_audio;
mod m20261007_000011_editorial;
mod m20261007_000012_author_import;
mod m20261007_000013_account_admin;
mod m20261007_000014_character_voices;
mod m20261007_000015_token_admin;
mod m20261007_000016_asset_admin;
mod m20261007_000017_recording_admin;
mod m20261007_000018_voice_reference_grants;
mod m20261007_000019_voice_jobs;
mod m20261007_000020_voice_auditions;
mod m20261007_000021_system_auditions;
mod m20261007_000022_course_speech_plans;
mod m20261007_000023_course_speech_clips;
mod m20261007_000024_speech_alignments;
mod m20261007_000025_speech_packages;
mod m20261007_000026_lesson_audio_reviews;
mod m20261007_000027_direct_publication;
mod m20261008_000028_product_settings;
mod m20261008_000029_product_memberships;
mod m20261008_000030_identity_product_scope;
mod m20261008_000031_learning_content_locks;
pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261006_000001_course_revisions::Migration),
            Box::new(m20261006_000002_sessions::Migration),
            Box::new(m20261006_000003_identity::Migration),
            Box::new(m20261006_000004_profile::Migration),
            Box::new(m20261006_000005_learning::Migration),
            Box::new(m20261006_000006_reviews::Migration),
            Box::new(m20261006_000007_saved::Migration),
            Box::new(m20261006_000008_releases::Migration),
            Box::new(m20261006_000009_media::Migration),
            Box::new(m20261006_000010_audio::Migration),
            Box::new(m20261007_000011_editorial::Migration),
            Box::new(m20261007_000012_author_import::Migration),
            Box::new(m20261007_000013_account_admin::Migration),
            Box::new(m20261007_000014_character_voices::Migration),
            Box::new(m20261007_000015_token_admin::Migration),
            Box::new(m20261007_000016_asset_admin::Migration),
            Box::new(m20261007_000017_recording_admin::Migration),
            Box::new(m20261007_000018_voice_reference_grants::Migration),
            Box::new(m20261007_000019_voice_jobs::Migration),
            Box::new(m20261007_000020_voice_auditions::Migration),
            Box::new(m20261007_000021_system_auditions::Migration),
            Box::new(m20261007_000022_course_speech_plans::Migration),
            Box::new(m20261007_000023_course_speech_clips::Migration),
            Box::new(m20261007_000024_speech_alignments::Migration),
            Box::new(m20261007_000025_speech_packages::Migration),
            Box::new(m20261007_000026_lesson_audio_reviews::Migration),
            Box::new(m20261007_000027_direct_publication::Migration),
            Box::new(m20261008_000028_product_settings::Migration),
            Box::new(m20261008_000029_product_memberships::Migration),
            Box::new(m20261008_000030_identity_product_scope::Migration),
            Box::new(m20261008_000031_learning_content_locks::Migration),
        ]
    }
}
