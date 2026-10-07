//! Operator responses deliberately exclude author sources and grading rules.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminOverview {
    pub generation: String,
    pub active_release: Option<String>,
    pub lessons: Vec<AdminLesson>,
    pub releases: Vec<AdminRelease>,
    pub lesson_next: Option<AdminLessonCursor>,
    pub release_next: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminLessonCursor {
    pub id: String,
    pub revision: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminLesson {
    pub id: String,
    pub revision: u32,
    pub title: String,
    pub level: String,
    pub unit: String,
    pub published: bool,
    pub withdrawn: bool,
    pub approved: bool,
    pub content_approved: bool,
    pub audio_required: bool,
    pub audio_accepted: bool,
    pub review_version: u32,
    pub review_note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRelease {
    pub id: String,
    pub lesson_count: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminReviewRequest {
    pub version: u32,
    pub approved: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminActivateRequest {
    pub release_id: String,
    pub generation: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminWithdrawRequest {
    pub generation: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminDocumentRequest {
    pub document: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminImportResult {
    pub lesson_id: String,
    pub revision: u32,
}

/// Read-only source preflight; does not certify registered media or publishing.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminDocumentCheck {
    pub valid: bool,
    pub issue: Option<AdminDocumentIssue>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminDocumentIssue {
    pub pointer: String,
    pub line: u32,
    pub column: u32,
    pub message_zh: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminHistory {
    pub items: Vec<AdminHistoryItem>,
    pub next: Option<AdminHistoryCursor>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminHistoryCursor {
    pub before_time: String,
    pub before_key: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminHistoryItem {
    pub key: String,
    pub action: String,
    pub target: String,
    pub actor: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAccounts {
    pub items: Vec<AdminAccount>,
    pub next_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAccount {
    pub id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AdminTokenKind {
    Invite,
    Reset,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminTokenRequest {
    pub email: String,
    pub kind: AdminTokenKind,
    pub operator: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminTokenResult {
    pub token: String,
    pub email: String,
    pub kind: AdminTokenKind,
    pub expires_in_seconds: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AdminAccountRole {
    Learner,
    Operator,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRoleRequest {
    pub expected_role: AdminAccountRole,
    pub role: AdminAccountRole,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSessions {
    pub account: AdminAccount,
    pub items: Vec<AdminSession>,
    pub next_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSession {
    // A SHA-256 record identifier, never a session cookie or reusable login token.
    pub id: String,
    pub expires_at: String,
    pub current: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRevokeSessionRequest {
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRevokeSessionResult {
    pub current: bool,
}

/// Private authoring data, never embedded in public lesson snapshots.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterVoiceProfile {
    pub personality: String,
    pub speaking_style: String,
    pub default_emotion: String,
    pub provider: String,
    pub model: String,
    pub voice_id: String,
    pub voice_kind: String,
    pub locale: String,
    pub rate: f64,
    pub reference_audio: Option<CharacterVoiceReference>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterVoiceReference {
    pub asset_id: String,
    pub revision: u32,
    pub transcript: String,
    // Provenance and consent for cloning, separate from ordinary playback rights.
    pub cloning_permission: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminCharacterVoices {
    pub items: Vec<AdminCharacterVoice>,
    pub next_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminCharacterVoice {
    pub character: crate::Character,
    pub avatar_revision: u32,
    pub voice_revision: u32,
    pub profile: Option<CharacterVoiceProfile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechVoice {
    pub character_id: String,
    pub character_revision: u32,
    pub voice_revision: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechSelection {
    pub voices: Vec<AdminSpeechVoice>,
    pub knowledge_narrator: AdminSpeechVoice,
    pub emotions: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPreviewRequest {
    pub lesson_id: String,
    pub lesson_revision: u32,
    pub selection: AdminSpeechSelection,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPlanRequest {
    pub id: String,
    pub preview: AdminSpeechPreviewRequest,
    pub expected_plan_hash: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechTarget {
    pub pointer: String,
    pub entry_id: String,
    pub text: String,
    pub voice: AdminSpeechVoice,
    pub emotion: String,
    pub generation_key: String,
    pub word_count: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPlan {
    pub id: Option<String>,
    pub lesson_id: String,
    pub lesson_revision: u32,
    pub source_hash: String,
    pub plan_hash: String,
    pub request_count: u32,
    pub total_request_characters: u32,
    pub selection: AdminSpeechSelection,
    pub voices: Vec<AdminCharacterVoice>,
    pub targets: Vec<AdminSpeechTarget>,
    pub created_at: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPlans {
    pub items: Vec<AdminSpeechPlan>,
    pub next: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechOptions {
    pub lesson: crate::PublicLesson,
    pub voices: Vec<AdminCharacterVoice>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminCharacterVoiceRequest {
    pub character_id: String,
    pub character_revision: u32,
    pub expected_voice_revision: u32,
    pub profile: CharacterVoiceProfile,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminPendingTokens {
    pub items: Vec<AdminPendingToken>,
    pub next_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminPendingToken {
    // An opaque management identifier, neither a token nor its authentication hash.
    pub id: String,
    pub email: String,
    pub kind: AdminTokenKind,
    pub role: AdminAccountRole,
    pub expires_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRevokeTokenRequest {
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAssets {
    pub items: Vec<AdminAsset>,
    pub next: Option<AdminAssetCursor>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAssetCursor {
    pub asset_id: String,
    pub revision: u32,
}
/// Explicit private projection: never return local import filenames or the raw provenance blob.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAsset {
    pub asset: crate::MediaAsset,
    pub source: String,
    pub license: String,
    pub creator: String,
    pub rights_confirmed: bool,
    pub byte_size: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAssetUpload {
    pub asset_id: String,
    pub revision: u32,
    pub mime_type: String,
    pub alt_zh: String,
    pub credit_zh: String,
    pub source: String,
    pub license: String,
    pub creator: String,
    pub rights_confirmed: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminCharacterRequest {
    pub character_id: String,
    pub expected_revision: u32,
    pub display_name: String,
    pub avatar_id: String,
    pub avatar_revision: u32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRecordings {
    pub items: Vec<AdminRecording>,
    pub next: Option<AdminAssetCursor>,
}
/// Private projection deliberately excludes source filesystem paths.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRecording {
    pub asset: crate::AudioAsset,
    pub source: String,
    pub license: String,
    pub creator: String,
    pub rights_confirmed: bool,
    pub byte_size: u32,
    pub sample_rate: u32,
    pub channels: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminRecordingUpload {
    pub asset_id: String,
    pub revision: u32,
    pub mime_type: String,
    pub credit_zh: String,
    pub source: String,
    pub license: String,
    pub creator: String,
    pub rights_confirmed: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminReferenceGrantRequest {
    pub character_id: String,
    pub character_revision: u32,
    pub voice_revision: u32,
    pub single_speaker_confirmed: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminReferenceGrant {
    pub id: String,
    pub character_id: String,
    pub character_revision: u32,
    pub voice_revision: u32,
    pub asset_id: String,
    pub asset_revision: u32,
    pub model: String,
    pub created_at: String,
    pub expires_at: String,
    pub revoked: bool,
    pub read_count: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminReferenceGrants {
    pub items: Vec<AdminReferenceGrant>,
    pub next: Option<String>,
}
/// Bearer path is returned once, never in list/history responses or persisted in the browser.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminReferenceGrantResult {
    pub grant: AdminReferenceGrant,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminVoiceJobRequest {
    pub grant_id: String,
    pub token: String,
    pub cost_confirmed: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminVoiceJobCheck {
    pub expected_version: u32,
    pub voice_id: Option<String>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum VoiceJobStatus {
    Submitted,
    Unknown,
    Failed,
    Processing,
    Checking,
    Ready,
    Unavailable,
    ModelMismatch,
    CheckFailed,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminVoiceJob {
    pub id: String,
    pub grant_id: String,
    pub character_id: String,
    pub character_revision: u32,
    pub voice_revision: u32,
    pub model: String,
    pub prefix: String,
    pub version: u32,
    pub status: VoiceJobStatus,
    pub voice_id: Option<String>,
    pub request_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminVoiceJobs {
    pub items: Vec<AdminVoiceJob>,
    pub next: Option<String>,
    pub configured: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSystemAuditionCandidate {
    pub character_id: String,
    pub character_revision: u32,
    pub expected_voice_revision: u32,
    pub profile: CharacterVoiceProfile,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAuditionRequest {
    pub id: String,
    pub clone_job_id: Option<String>,
    pub expected_clone_version: Option<u32>,
    pub candidate: Option<AdminSystemAuditionCandidate>,
    pub text: String,
    pub emotion: String,
    pub cost_confirmed: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAuditionReview {
    pub accepted: bool,
    pub heard: bool,
    pub expected_voice_revision: u32,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAudition {
    pub id: String,
    pub clone_job_id: Option<String>,
    pub profile: CharacterVoiceProfile,
    pub character_id: String,
    pub character_revision: u32,
    pub base_voice_revision: u32,
    pub voice_id: String,
    pub text: String,
    pub emotion: String,
    pub status: String,
    pub duration_ms: Option<u32>,
    pub request_id: Option<String>,
    pub accepted: Option<bool>,
    pub applied_voice_revision: Option<u32>,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAuditions {
    pub items: Vec<AdminAudition>,
    pub next: Option<String>,
    pub configured: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechClipRequest {
    pub id: String,
    pub plan_id: String,
    pub generation_key: String,
    pub expected_plan_hash: String,
    pub expected_previous_id: Option<String>,
    pub cost_confirmed: bool,
    pub retry_unknown_confirmed: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechClip {
    pub id: String,
    pub plan_id: String,
    pub generation_key: String,
    pub reused_from: Option<String>,
    pub status: String,
    pub duration_ms: Option<u32>,
    pub request_id: Option<String>,
    pub accepted: Option<bool>,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechClips {
    pub items: Vec<AdminSpeechClip>,
    pub configured: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechClipReview {
    pub heard: bool,
    pub accepted: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAlignmentImport {
    pub id: String,
    pub plan_id: String,
    pub expected_plan_hash: String,
    pub report_json: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAlignmentWord {
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub start_ms: Option<u32>,
    pub end_ms: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAlignmentClip {
    pub clip_id: String,
    pub generation_key: String,
    pub text: String,
    pub duration_ms: u32,
    pub issues: Vec<String>,
    pub words: Vec<AdminAlignmentWord>,
    pub accepted: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAlignment {
    pub id: String,
    pub plan_id: String,
    pub plan_hash: String,
    pub report_hash: String,
    pub clips: Vec<AdminAlignmentClip>,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAlignments {
    pub items: Vec<AdminAlignmentSummary>,
    pub next: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAlignmentSummary {
    pub id: String,
    pub plan_id: String,
    pub plan_hash: String,
    pub report_hash: String,
    pub clip_count: u32,
    pub accepted_count: u32,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAlignmentReview {
    pub expected_report_hash: String,
    pub heard: bool,
    pub timings_checked: bool,
    pub accepted: bool,
    pub words: Vec<AdminAlignmentWord>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPackageRequest {
    pub expected_report_hash: String,
    pub lesson_revision: u32,
    pub gap_ms: u32,
    pub rights_confirmed: bool,
    pub source: String,
    pub license: String,
    pub creator: String,
    pub credit_zh: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPackageImport {
    pub id: String,
    pub package: AdminSpeechPackageRequest,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminAutomaticSpeechPackageRequest {
    pub report_json: String,
    pub package: AdminSpeechPackageRequest,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPackageResult {
    pub id: String,
    pub lesson_id: String,
    pub revision: u32,
    pub recording_count: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminSpeechPackageResults {
    pub items: Vec<AdminSpeechPackageResult>,
    pub next: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminLessonAudioReview {
    pub expected_lesson_hash: String,
    pub version: u32,
    pub accepted: bool,
    pub heard: bool,
    pub reason: String,
}

/// Explicit owner authorization; evidence is bounded and never a hearing declaration.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminDirectPublication {
    pub expected_lesson_hash: String,
    pub reason: String,
    #[ts(type = "Record<string, unknown>")]
    pub evidence: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdminLessonAudioStatus {
    pub published: bool,
    pub required: bool,
    pub lesson_hash: String,
    pub version: u32,
    pub accepted: bool,
    // Publication authorization is distinct from a human hearing declaration.
    pub direct_authorized: bool,
    pub reason: String,
    pub actor: Option<String>,
}
